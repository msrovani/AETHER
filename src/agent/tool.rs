//! Contratos de ferramenta (`Tool`) do agente — P0 do roadmap.
//!
//! Uma tool declara **schema**, **affordance**, **pré-condições**, **custo** e
//! **risco**. O planner só considera o que passa nessas condições; o executor
//! cobra o orçamento e verifica o critério de sucesso. Sem esses campos, uma
//! tool não entra no [`crate::agent::registry::ToolRegistry`].

use std::fmt;
use std::sync::Arc;

use crate::core::certificate::Certificate as EngineCertificate;
use crate::core::knowledge_bridge_f1::ScopeId;
use crate::core::logic_pipeline::Clause3;

use super::{Claim, ExpertId, ExpertOutput};

/// Risco de uma tool (ordem importa: `High > Medium > Low`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    /// Somente leitura/local.
    Low,
    /// Efeito moderado (rede/arquivo em sandbox).
    Medium,
    /// Ação irreversível/privilegiada: exige HITL.
    High,
}

impl Risk {
    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Penalidade numérica usada pelo planner.
    pub const fn penalty(self) -> f64 {
        match self {
            Self::Low => 0.05,
            Self::Medium => 0.35,
            Self::High => 1.5,
        }
    }
}

/// Custo estimado de uma chamada.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cost {
    /// Tempo esperado em ms.
    pub ms: f32,
    /// Tokens/LLM equivalentes (0 para tools locais).
    pub tokens: u32,
}

impl Cost {
    /// Custo local sem tokens.
    pub const fn local(ms: f32) -> Self {
        Self { ms, tokens: 0 }
    }
}

/// Tipo grosseiro de objetivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GoalKind {
    /// Responder/consultar.
    Answer,
    /// Recuperar da memória.
    Retrieve,
    /// Transformar (ex.: QUBO).
    Transform,
    /// Verificar um resultado.
    Verify,
    /// Executar um efeito.
    Act,
}

impl GoalKind {
    /// Código estável.
    pub const fn code(self) -> u8 {
        match self {
            Self::Answer => 0,
            Self::Retrieve => 1,
            Self::Transform => 2,
            Self::Verify => 3,
            Self::Act => 4,
        }
    }
}

/// Critério verificável de sucesso.
#[derive(Clone, Debug, PartialEq)]
pub enum SuccessCriteria {
    /// Pelo menos uma claim com valor não vazio.
    NonEmpty,
    /// Claim de `(subject, relation)`; `value` opcional para conferência.
    Claim {
        /// Sujeito.
        subject: String,
        /// Relação.
        relation: String,
        /// Valor esperado (case-insensitive) ou `None` para qualquer um.
        value: Option<String>,
    },
    /// Certificado factível (`violations == 0`); não exige gap > 0.
    Certificate,
    /// Certificado coerente (`is_coherent`: gap > 0 e violations == 0).
    Coherent,
    /// Qualquer saída não vazia (tool de verificação).
    Any,
}

impl SuccessCriteria {
    /// Testa a saída.
    pub fn met(&self, outcome: &ToolOutcome) -> bool {
        match self {
            Self::NonEmpty => outcome
                .claims
                .iter()
                .any(|claim| !claim.value.trim().is_empty()),
            Self::Claim {
                subject,
                relation,
                value,
            } => outcome.claims.iter().any(|claim| {
                // subject vazio = wildcard (ex.: code.loop keep em qualquer path).
                (subject.is_empty() || claim.subject.eq_ignore_ascii_case(subject))
                    && claim.relation.eq_ignore_ascii_case(relation)
                    && value.as_ref().map_or(true, |want| {
                        claim.value.trim().eq_ignore_ascii_case(want.trim())
                    })
            }),
            Self::Certificate => outcome
                .certificate
                .as_ref()
                .is_some_and(|cert| cert.violations == 0 && cert.energy.is_finite()),
            Self::Coherent => outcome
                .certificate
                .as_ref()
                .is_some_and(|cert| cert.is_coherent()),
            Self::Any => !outcome.claims.is_empty() || outcome.certificate.is_some(),
        }
    }
}

/// Orçamento de uma execução.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Passos máximos.
    pub max_steps: usize,
    /// Tempo máximo em ms.
    pub max_ms: u64,
    /// Tokens máximos (0 = sem limite de tokens).
    pub max_tokens: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_steps: 4,
            max_ms: 2_000,
            max_tokens: 0,
        }
    }
}

/// Objetivo estruturado (texto só na borda).
#[derive(Clone, Debug)]
pub struct Goal {
    /// Identificador.
    pub id: u64,
    /// Tipo.
    pub kind: GoalKind,
    /// Sujeito (fato).
    pub subject: Option<String>,
    /// Relação (fato).
    pub relation: Option<String>,
    /// Consulta textual (memória) ou corpo de escrita fs.
    pub query: Option<String>,
    /// Variáveis (SAT/QUBO).
    pub n: Option<usize>,
    /// Cláusulas (SAT).
    pub clauses: Vec<Clause3>,
    /// Matriz QUBO.
    pub j: Vec<f64>,
    /// Campo QUBO.
    pub h: Vec<f64>,
    /// Caminho relativo à sandbox (`fs.read` / `fs.write`).
    pub path: Option<String>,
    /// URL (`http.get`).
    pub url: Option<String>,
    /// Nome MCP allowlisted (`mcp.call`).
    pub mcp_name: Option<String>,
    /// Critério de sucesso.
    pub success: SuccessCriteria,
    /// Orçamento.
    pub budget: Budget,
    /// Teto de risco aceitável sem HITL.
    pub risk_ceiling: Risk,
    /// Subgoals em sequência (P2). Vazio = goal único.
    pub pipeline: Vec<Goal>,
}

impl Goal {
    /// Consulta relacional.
    pub fn fact(subject: impl Into<String>, relation: impl Into<String>) -> Self {
        let subject = subject.into();
        let relation = relation.into();
        Self {
            id: 0,
            kind: GoalKind::Answer,
            success: SuccessCriteria::Claim {
                subject: subject.clone(),
                relation: relation.clone(),
                value: None,
            },
            subject: Some(subject),
            relation: Some(relation),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
            pipeline: Vec::new(),
        }
    }

    /// 3-SAT.
    pub fn satisfy(n: usize, clauses: Vec<Clause3>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Answer,
            subject: None,
            relation: None,
            query: None,
            n: Some(n),
            clauses,
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Certificate,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
            pipeline: Vec::new(),
        }
    }

    /// QUBO.
    pub fn optimize(n: usize, j: Vec<f64>, h: Vec<f64>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Transform,
            subject: None,
            relation: None,
            query: None,
            n: Some(n),
            clauses: Vec::new(),
            j,
            h,
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Coherent,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
            pipeline: Vec::new(),
        }
    }

    /// Memória durável.
    pub fn memory(query: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Retrieve,
            subject: None,
            relation: None,
            query: Some(query.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
            pipeline: Vec::new(),
        }
    }

    /// Leitura em sandbox (`fs.read`).
    pub fn fs_read(path: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Retrieve,
            subject: None,
            relation: Some("fs.read".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: Some(path.into()),
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// Escrita em sandbox (`fs.write`); corpo em `query`.
    pub fn fs_write(path: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("fs.write".into()),
            query: Some(body.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: Some(path.into()),
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Any,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// GET allowlisted (`http.get`).
    pub fn http_get(url: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Retrieve,
            subject: None,
            relation: Some("http.get".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: Some(url.into()),
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// Call MCP tipada (`mcp.call`).
    pub fn mcp_call(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("mcp.call".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: Some(name.into()),
            success: SuccessCriteria::Any,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.check` (K1).
    pub fn code_check(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            id: 0,
            kind: GoalKind::Verify,
            subject: None,
            relation: Some("code.check".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: Some(path.clone()),
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: path,
                relation: "code.check".into(),
                value: None,
            },
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.ast_grep` (K1).
    pub fn code_ast_grep(path: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Retrieve,
            subject: None,
            relation: Some("code.ast_grep".into()),
            query: Some(pattern.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: Some(path.into()),
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.test` (K1).
    pub fn code_test(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            id: 0,
            kind: GoalKind::Verify,
            subject: None,
            relation: Some("code.test".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: Some(path.clone()),
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: path,
                relation: "code.test".into(),
                value: Some("pass".into()),
            },
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.patch` com JSON `PatchProposal` (K2).
    pub fn code_patch(proposal_json: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("code.patch".into()),
            query: Some(proposal_json.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.generate` — brief → `PatchProposal` (K3; sem auto-merge).
    pub fn code_generate(brief: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Transform,
            subject: None,
            relation: Some("code.generate".into()),
            query: Some(brief.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.apply_proposal` — aplica proposal do blackboard (K4 bridge).
    pub fn code_apply_proposal() -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("code.apply_proposal".into()),
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.eval` — suíte T3 coder com generator declarado (K7).
    pub fn code_eval(query_json: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Verify,
            subject: None,
            relation: Some("code.eval".into()),
            query: Some(query_json.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: String::new(),
                relation: "code.eval".into(),
                value: Some("ok".into()),
            },
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `shell.exec` — build/shell sob HITL (K8; nunca default).
    ///
    /// `query_json`: `{"argv":["stub"],"hitl_token":"..."}`. Exige
    /// `risk_ceiling=High` + `AETHER_SHELL=hitl|stub` + token casando
    /// `AETHER_HITL_TOKEN`. Kill switch: `AETHER_SHELL_KILL=1`.
    pub fn shell_exec(query_json: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("shell.exec".into()),
            query: Some(query_json.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: String::new(),
                relation: "shell.exec".into(),
                value: Some("ok".into()),
            },
            budget: Budget::default(),
            risk_ceiling: Risk::High,
            pipeline: Vec::new(),
        }
    }

    /// `code.lm` — small code LM local offline (K6).
    pub fn code_lm(brief: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Transform,
            subject: None,
            relation: Some("code.lm".into()),
            query: Some(brief.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.grammar` — prosa/JSON → `PatchProposal` tipado (K5).
    pub fn code_grammar(raw: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Transform,
            subject: None,
            relation: Some("code.grammar".into()),
            query: Some(raw.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: String::new(),
                relation: "code.grammar".into(),
                value: Some("ok".into()),
            },
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// `code.loop` — generate→apply→test→keep|rollback (K4).
    pub fn code_loop(brief: impl Into<String>) -> Self {
        let brief = brief.into();
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: Some("code.loop".into()),
            query: Some(brief),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: String::new(),
                relation: "code.loop".into(),
                value: Some("keep".into()),
            },
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// Pesquisa externa A5 / T-NET (`research.query`).
    pub fn research(query: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Retrieve,
            subject: None,
            relation: Some("research.query".into()),
            query: Some(query.into()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Medium,
            pipeline: Vec::new(),
        }
    }

    /// Ajusta orçamento/teto.
    pub fn with_budget(mut self, budget: Budget, risk_ceiling: Risk) -> Self {
        self.budget = budget;
        self.risk_ceiling = risk_ceiling;
        self
    }

    /// Workflow sequencial (P2): executa subgoals em ordem, com blackboard.
    pub fn pipeline(steps: Vec<Goal>) -> Self {
        Self {
            id: 0,
            kind: GoalKind::Act,
            subject: None,
            relation: None,
            query: None,
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Any,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
            pipeline: steps,
        }
    }

    /// Cluster de competência (contexto do self-model).
    pub fn cluster(&self) -> u64 {
        let mut h = u64::from(self.kind.code()) | 1;
        for text in [
            self.subject.as_deref(),
            self.relation.as_deref(),
            self.query.as_deref(),
            self.path.as_deref(),
            self.url.as_deref(),
            self.mcp_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            for b in text.bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        }
        if let Some(n) = self.n {
            h ^= n as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }
}

/// Estado observável pelo planner (mínimo no P1).
#[derive(Clone, Debug, Default)]
pub struct WorldState {
    /// Escopo corrente.
    pub scope: ScopeId,
    /// Fatos carregados (cache).
    pub facts_loaded: usize,
    /// Hits de memória.
    pub memory_hits: usize,
    /// Snapshot do blackboard para poda de pré-condição (A5 / P6).
    pub claims: Vec<Claim>,
    /// Raiz da sandbox (A4); vazia = tools mundo inativas no planner.
    pub sandbox_root: String,
    /// Hosts HTTP allowlisted.
    pub http_hosts: Vec<String>,
    /// Nomes MCP allowlisted.
    pub mcp_allow: Vec<String>,
}

impl WorldState {
    /// `true` se existe claim com relação/valor (poda de candidatos).
    pub fn has_claim(&self, relation: &str, value: &str) -> bool {
        self.claims
            .iter()
            .any(|c| c.relation == relation && c.value == value)
    }
}

/// Fato inferido (quarentena): entra sem confiança humana nem corroboração.
pub const AUTHORITY_QUARANTINE: u8 = 96;

/// Fato corroborado por duas ou mais fontes independentes.
pub const AUTHORITY_CORROBORATED: u8 = 128;

/// Fato verificado por evidência física/certificado.
pub const AUTHORITY_VERIFIED: u8 = 200;

/// Aprovação humana explícita (HITL); só humano escreve/apaga.
pub const AUTHORITY_HITL: u8 = 255;

/// Fato com evidência física (certificado) dentro do workflow.
///
/// Só é produzido quando uma verificação válida (`violations=0`) confirma um
/// claim de solver; é o que autoriza `memory.write` a gravar em 200.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedFact {
    /// Sujeito.
    pub subject: String,
    /// Relação.
    pub relation: String,
    /// Valor verificado.
    pub value: String,
    /// Certificado que sustenta o fato.
    pub certificate: EngineCertificate,
}

/// Snapshot de uma memória antes de uma escrita (para undo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Texto anterior.
    pub text: String,
    /// Objeto/valor anterior.
    pub object: String,
    /// Autoridade anterior.
    pub authority: u8,
    /// Contador de reforço.
    pub reinforced: u64,
    /// Escopo textual.
    pub scope: String,
    /// Proveniência textual.
    pub provenance: String,
}

/// Efeito colateral de uma tool (para rollback).
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Escrita de memória; `prev` é o snapshot pré-write (None = inserção).
    MemoryWrite {
        /// Chave de armazenamento.
        key: String,
        /// Snapshot anterior (None = não existia).
        prev: Option<Snapshot>,
    },
    /// Marca de stage/outbox (watermark LIFO; nunca forget cego).
    Stage {
        /// Chaves staged nesta marca.
        keys: Vec<String>,
        /// Sequência monotônica dentro da transação (watermark estável).
        seq: u64,
        /// Comprimento do outbox imediatamente antes da escrita.
        outbox_len: usize,
    },
    /// Efeito sem compensação automática (exige HITL).
    Irreversible {
        /// Tipo do efeito.
        kind: &'static str,
    },
    /// Escrita em arquivo sandbox; `prev` = conteúdo anterior (`None` = criado).
    FsWrite {
        /// Caminho absoluto canônico dentro da sandbox.
        path: String,
        /// Conteúdo anterior (None = arquivo não existia).
        prev: Option<Vec<u8>>,
    },
    /// Nenhum efeito.
    None,
}

/// Operação de undo (LIFO).
#[derive(Clone, Debug, PartialEq)]
pub enum UndoOp {
    /// Remove a chave inserida pelo passo.
    Insert {
        /// Chave.
        key: String,
    },
    /// Restaura o snapshot anterior.
    Restore {
        /// Chave.
        key: String,
        /// Snapshot.
        prev: Snapshot,
    },
    /// Restaura arquivo sandbox (`prev=None` → apagar).
    FsRestore {
        /// Caminho absoluto.
        path: String,
        /// Conteúdo anterior.
        prev: Option<Vec<u8>>,
    },
    /// Descarta o outbox até a marca da transação.
    Stage {
        /// Transação.
        txn: u64,
        /// Sequência da marca.
        seq: u64,
        /// Chaves staged.
        keys: Vec<String>,
        /// Comprimento do outbox antes da escrita.
        outbox_len: usize,
    },
    /// Efeito sem compensação.
    Irreversible {
        /// Tipo.
        kind: &'static str,
    },
}

/// Resultado de um rewind (nunca silencioso).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UndoReport {
    /// Operações aplicadas com sucesso.
    pub applied: usize,
    /// Operações já aplicadas antes (replay idempotente).
    pub stale: usize,
    /// Falhas: a transação NÃO pode ser considerada desfeita.
    pub failed: Vec<String>,
}

impl UndoReport {
    /// Sem falhas?
    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }

    /// Resumo curto para rationale/telemetria.
    pub fn summary(&self) -> String {
        format!(
            "undo: {} aplicadas, {} stale, {} falhas",
            self.applied,
            self.stale,
            self.failed.len()
        )
    }
}

/// Backend de undo (memória real ou mock).
pub trait UndoBackend: Send + Sync {
    /// Remove a chave inserida (undo de Insert).
    fn insert_undo(&self, key: &str) -> Result<(), String>;
    /// Restaura snapshot (undo de Restore).
    fn restore(&self, key: &str, prev: &Snapshot) -> Result<(), String>;
    /// Descarta outbox até a marca (undo de Stage).
    fn stage_rewind(&self, txn: u64, keys: &[String]) -> Result<(), String>;
    /// Comprimento atual do outbox (watermark pré-escrita).
    fn outbox_len(&self) -> usize {
        0
    }
    /// Rewind de stage com marca explícita. Default delega ao formato antigo.
    fn stage_rewind_to(&self, txn: u64, keys: &[String], outbox_len: usize) -> Result<(), String> {
        let _ = outbox_len;
        self.stage_rewind(txn, keys)
    }
    /// Snapshot atual da chave, **sem** escrever (pré-checagem de autoridade).
    fn peek(&self, key: &str) -> Option<Snapshot> {
        let _ = key;
        None
    }
    /// Registra a marca de stage antes da escrita (watermark por transação) e
    /// devolve a marca efetiva (índice que `stage_rewind_to` deve usar).
    ///
    /// Default: no-op (backends sem outbox).
    fn stage_begin(&self, txn: u64, keys: &[String], outbox_len: usize) -> Result<usize, String> {
        let _ = (txn, keys);
        Ok(outbox_len)
    }
    /// Escrita com before-image e autoridade explícita (para `memory.write`).
    fn write(&self, key: &str, text: &str, authority: u8) -> Result<Option<Snapshot>, String> {
        let _ = (key, text, authority);
        Err("undo backend sem write".into())
    }
    /// Escrita que registra a **fonte** (proveniência) do fato.
    ///
    /// Duas fontes independentes com o mesmo valor corroboram o fato. Default
    /// delega a [`Self::write`] (backends sem proveniência).
    fn write_sourced(
        &self,
        key: &str,
        text: &str,
        authority: u8,
        source: &str,
    ) -> Result<Option<Snapshot>, String> {
        let _ = source;
        self.write(key, text, authority)
    }
}

/// Resultado de uma tool.
#[derive(Clone, Debug)]
pub struct ToolOutcome {
    /// Claims produzidas.
    pub claims: Vec<Claim>,
    /// Certificado físico, quando existe.
    pub certificate: Option<EngineCertificate>,
    /// Confiança.
    pub confidence: f32,
    /// Detalhe de diagnóstico.
    pub detail: String,
    /// Efeitos para rollback.
    pub effects: Vec<Effect>,
    /// Custo medido.
    pub cost_us: u64,
    /// Precisa de humano?
    pub needs_human: bool,
}

impl Default for ToolOutcome {
    fn default() -> Self {
        Self {
            claims: Vec::new(),
            certificate: None,
            confidence: 0.0,
            detail: String::new(),
            effects: Vec::new(),
            cost_us: 0,
            needs_human: false,
        }
    }
}

impl From<ExpertOutput> for ToolOutcome {
    fn from(output: ExpertOutput) -> Self {
        Self {
            claims: output.claims,
            certificate: output.certificate,
            confidence: output.confidence,
            detail: output.detail,
            effects: Vec::new(),
            cost_us: output.cost_us,
            needs_human: false,
        }
    }
}

impl ToolOutcome {
    /// Saída vazia com motivo.
    pub fn empty(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
            ..Self::default()
        }
    }

    /// Primeira claim.
    pub fn first_claim(&self) -> Option<&Claim> {
        self.claims.first()
    }
}

/// Schema declarativo.
#[derive(Clone, Copy, Debug)]
pub struct ToolSchema {
    /// Nome estável.
    pub name: &'static str,
    /// Descrição curta.
    pub description: &'static str,
    /// Campos esperados no `Goal`.
    pub args: &'static [&'static str],
    /// Tipos de objetivo aceitos.
    pub kinds: &'static [GoalKind],
}

/// Contexto de execução (orçamento/efeitos/undo).
#[derive(Clone)]
pub struct ToolContext {
    /// Escopo.
    pub scope: ScopeId,
    /// Orçamento.
    pub budget: Budget,
    /// Tempo gasto.
    pub spent_ms: f32,
    /// Tokens gastos.
    pub spent_tokens: u32,
    /// Trace.
    pub trace_id: u64,
    /// Passos executados (limite global do contexto).
    pub spent_steps: usize,
    /// Efeitos acumulados.
    pub effects: Vec<Effect>,
    /// Claims verificadas dos passos anteriores (blackboard).
    pub claims: Vec<Claim>,
    /// Certificados dos passos anteriores (verificação).
    pub certificates: Vec<EngineCertificate>,
    /// Fatos com certificado válido no workflow (evidência para 200).
    pub verified: Vec<VerifiedFact>,
    /// Backend de undo (memória real ou mock).
    pub undo_backend: Option<Arc<dyn UndoBackend>>,
    /// Log LIFO de undo.
    undo_log: Vec<UndoOp>,
    /// Transação corrente.
    txn: u64,
    /// Sequência monotônica de stages dentro da transação.
    stage_seq: u64,
}

impl ToolContext {
    /// Identificador da transação.
    pub fn txn(&self) -> u64 {
        self.txn
    }
}

impl fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolContext")
            .field("scope", &self.scope)
            .field("spent_ms", &self.spent_ms)
            .field("effects", &self.effects.len())
            .field("claims", &self.claims.len())
            .field("certificates", &self.certificates.len())
            .field("verified", &self.verified.len())
            .field("undo", &self.undo_log.len())
            .field("has_backend", &self.undo_backend.is_some())
            .finish()
    }
}

impl ToolContext {
    /// Contexto novo.
    pub fn new(scope: ScopeId, budget: Budget, trace_id: u64) -> Self {
        Self {
            scope,
            budget,
            spent_ms: 0.0,
            spent_tokens: 0,
            spent_steps: 0,
            trace_id,
            effects: Vec::new(),
            claims: Vec::new(),
            certificates: Vec::new(),
            verified: Vec::new(),
            undo_backend: None,
            undo_log: Vec::new(),
            txn: trace_id,
            stage_seq: 0,
        }
    }

    /// Próxima sequência de stage (watermark estável por passo).
    pub fn next_stage_seq(&mut self) -> u64 {
        self.stage_seq += 1;
        self.stage_seq
    }

    /// Define o backend de undo (memória/mock).
    pub fn with_undo_backend(mut self, backend: Arc<dyn UndoBackend>) -> Self {
        self.undo_backend = Some(backend);
        self
    }

    /// Há orçamento para `cost`?
    pub fn can_spend(&self, cost: Cost) -> bool {
        let ms = self.spent_ms + cost.ms;
        let tokens = self.spent_tokens.saturating_add(cost.tokens);
        self.spent_steps < self.budget.max_steps
            && ms as u64 <= self.budget.max_ms
            && (self.budget.max_tokens == 0 || tokens <= self.budget.max_tokens)
    }

    /// Cobra o custo.
    pub fn charge(&mut self, cost: Cost) {
        self.spent_ms += cost.ms;
        self.spent_tokens = self.spent_tokens.saturating_add(cost.tokens);
    }

    /// Tamanho do log de undo (watermark estável para rewind).
    pub fn undo_len(&self) -> usize {
        self.undo_log.len()
    }

    /// Registra um efeito e a operação de undo correspondente.
    pub fn push_effect(&mut self, effect: Effect) {
        match &effect {
            Effect::MemoryWrite {
                key,
                prev: Some(prev),
            } => {
                self.undo_log.push(UndoOp::Restore {
                    key: key.clone(),
                    prev: prev.clone(),
                });
            }
            Effect::MemoryWrite { key, prev: None } => {
                self.undo_log.push(UndoOp::Insert { key: key.clone() });
            }
            Effect::Stage {
                keys,
                seq,
                outbox_len,
            } => {
                // Materializa a marca no backend (journal) antes do undo op e
                // guarda a marca efetiva devolvida por ele.
                let mark = match &self.undo_backend {
                    Some(backend) => backend
                        .stage_begin(self.txn, keys, *outbox_len)
                        .unwrap_or(*outbox_len),
                    None => *outbox_len,
                };
                self.undo_log.push(UndoOp::Stage {
                    txn: self.txn,
                    seq: *seq,
                    keys: keys.clone(),
                    outbox_len: mark,
                });
            }
            Effect::Irreversible { kind } => {
                self.undo_log.push(UndoOp::Irreversible { kind });
            }
            Effect::FsWrite { path, prev } => {
                self.undo_log.push(UndoOp::FsRestore {
                    path: path.clone(),
                    prev: prev.clone(),
                });
            }
            Effect::None => return,
        }
        self.effects.push(effect);
    }

    /// Rewind LIFO até a marca `watermark`, devolvendo relatório completo.
    ///
    /// - `stale` quando o backend já aplicou a operação (replay idempotente);
    /// - `failed` quando a compensação não pôde ser aplicada: nesse caso o log
    ///   **não** é truncado e a transação não pode ser considerada desfeita.
    pub fn rewind_report(&mut self, watermark: usize) -> UndoReport {
        let mut report = UndoReport::default();
        if self.undo_log.len() <= watermark {
            return report;
        }
        let needs_backend = self.undo_log[watermark..]
            .iter()
            .any(|op| !matches!(op, UndoOp::FsRestore { .. } | UndoOp::Irreversible { .. }));
        if needs_backend && self.undo_backend.is_none() {
            report
                .failed
                .push("undo sem backend: efeito registrado não pode ser compensado".to_string());
            return report;
        }
        for op in self.undo_log[watermark..].iter().rev() {
            let result = match op {
                UndoOp::FsRestore { path, prev } => apply_fs_restore(path, prev.as_deref()),
                UndoOp::Irreversible { kind } => {
                    report
                        .failed
                        .push(format!("efeito irreversível `{kind}`: exige HITL"));
                    break;
                }
                UndoOp::Insert { key } => self
                    .undo_backend
                    .as_ref()
                    .expect("backend")
                    .insert_undo(key),
                UndoOp::Restore { key, prev } => self
                    .undo_backend
                    .as_ref()
                    .expect("backend")
                    .restore(key, prev),
                UndoOp::Stage {
                    txn,
                    keys,
                    outbox_len,
                    ..
                } => self
                    .undo_backend
                    .as_ref()
                    .expect("backend")
                    .stage_rewind_to(*txn, keys, *outbox_len),
            };
            match result {
                Ok(()) => report.applied += 1,
                Err(e) if e.starts_with("stale") => report.stale += 1,
                Err(e) => {
                    report.failed.push(format!("undo: {e}"));
                    break;
                }
            }
        }
        if report.is_ok() {
            self.undo_log.truncate(watermark);
        }
        report
    }

    /// Rewind LIFO até a marca (erro se alguma compensação falhar).
    pub fn rewind(&mut self, watermark: usize) -> Result<(), ToolError> {
        let report = self.rewind_report(watermark);
        if report.is_ok() {
            Ok(())
        } else {
            Err(ToolError::Failed(format!(
                "{}: {}",
                report.summary(),
                report.failed.join("; ")
            )))
        }
    }

    /// Rollback lógico completo (rewind + limpa blackboard).
    ///
    /// Devolve `Err` se alguma compensação falhou; nesse caso o log de undo é
    /// preservado para nova tentativa/HITL.
    pub fn rollback(&mut self) -> Result<(), ToolError> {
        let report = self.rewind_report(0);
        if !report.is_ok() {
            return Err(ToolError::Failed(format!(
                "{}: {}",
                report.summary(),
                report.failed.join("; ")
            )));
        }
        self.effects.clear();
        self.claims.clear();
        self.certificates.clear();
        self.verified.clear();
        Ok(())
    }

    /// Trunca estruturas após um rewind já aplicado.
    pub fn rollback_to(
        &mut self,
        effects: usize,
        claims: usize,
        certificates: usize,
        verified: usize,
    ) {
        self.effects.truncate(effects);
        self.claims.truncate(claims);
        self.certificates.truncate(certificates);
        self.verified.truncate(verified);
    }
}

/// Erro de tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolError {
    /// Argumentos ausentes/inválidos.
    InvalidArgs(String),
    /// Pré-condição não satisfeita.
    Precondition(String),
    /// Orçamento estourado.
    Budget(String),
    /// Falha de execução.
    Failed(String),
    /// Requer humano.
    NeedsHuman(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgs(msg) => write!(f, "args: {msg}"),
            Self::Precondition(msg) => write!(f, "precondição: {msg}"),
            Self::Budget(msg) => write!(f, "orçamento: {msg}"),
            Self::Failed(msg) => write!(f, "falha: {msg}"),
            Self::NeedsHuman(msg) => write!(f, "humano: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Contrato de ferramenta.
pub trait Tool: Send {
    /// Identidade (nome estável).
    fn name(&self) -> &'static str;

    /// Schema.
    fn schema(&self) -> ToolSchema;

    /// Custo estimado.
    fn cost(&self) -> Cost;

    /// Risco.
    fn risk(&self) -> Risk;

    /// Competência para o goal (`0` = não atende).
    fn affordance(&self, goal: &Goal, state: &WorldState) -> f32;

    /// R10/A10 (Sprint A): é candidato a sondagem epístéo? (abdução G-ABD).
    /// Default: `false`. O planner corta candidatos com `true` quando o
    /// orçamento ([`crate::agent::epistemic_budget::remaining`]) esgota.
    fn is_probe(&self) -> bool {
        false
    }

    /// Pré-condições.
    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool;

    /// Executa.
    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError>;

    /// Simulação obrigatória antes de efeito (P1: read-only; P2: efeitos).
    ///
    /// Default: no-op. Tools com efeito devem validar argumentos/sandbox aqui.
    fn dry_run(&self, _goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        Ok(())
    }

    /// Efeitos que a tool **planeja** produzir, declarados antes da `call`.
    ///
    /// O executor usa isto para bloquear `Irreversible` antes do efeito
    /// (ADR-014/SCHEMA_6). Default: nenhum efeito declarado.
    fn planned_effects(&self, _goal: &Goal) -> Vec<Effect> {
        Vec::new()
    }

    /// Tool de escalação (não executa; apenas pede humano). Nunca compete com
    /// tools executáveis no ranking EFE.
    fn escalation(&self) -> bool {
        false
    }
}

/// Converte um `ExpertOutput` em `ToolOutcome` (atalho para as tools reais).
pub fn outcome_from_expert(expert: ExpertId, output: ExpertOutput) -> ToolOutcome {
    let mut outcome = ToolOutcome::from(output);
    outcome.detail = format!("expert={} {}", expert.label(), outcome.detail);
    outcome
}

fn apply_fs_restore(path: &str, prev: Option<&[u8]>) -> Result<(), String> {
    match prev {
        Some(bytes) => std::fs::write(path, bytes).map_err(|e| e.to_string()),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

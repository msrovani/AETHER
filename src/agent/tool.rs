//! Contratos de ferramenta (`Tool`) do agente — P0 do roadmap.
//!
//! Uma tool declara **schema**, **affordance**, **pré-condições**, **custo** e
//! **risco**. O planner só considera o que passa nessas condições; o executor
//! cobra o orçamento e verifica o critério de sucesso. Sem esses campos, uma
//! tool não entra no [`crate::agent::registry::ToolRegistry`].

use std::fmt;

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
                claim.subject.eq_ignore_ascii_case(subject)
                    && claim.relation.eq_ignore_ascii_case(relation)
                    && value.as_ref().is_none_or(|want| {
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
    /// Consulta textual (memória).
    pub query: Option<String>,
    /// Variáveis (SAT/QUBO).
    pub n: Option<usize>,
    /// Cláusulas (SAT).
    pub clauses: Vec<Clause3>,
    /// Matriz QUBO.
    pub j: Vec<f64>,
    /// Campo QUBO.
    pub h: Vec<f64>,
    /// Critério de sucesso.
    pub success: SuccessCriteria,
    /// Orçamento.
    pub budget: Budget,
    /// Teto de risco aceitável sem HITL.
    pub risk_ceiling: Risk,
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
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
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
            success: SuccessCriteria::Certificate,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
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
            success: SuccessCriteria::Coherent,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
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
            success: SuccessCriteria::NonEmpty,
            budget: Budget::default(),
            risk_ceiling: Risk::Low,
        }
    }

    /// Ajusta orçamento/teto.
    pub fn with_budget(mut self, budget: Budget, risk_ceiling: Risk) -> Self {
        self.budget = budget;
        self.risk_ceiling = risk_ceiling;
        self
    }

    /// Cluster de competência (contexto do self-model).
    pub fn cluster(&self) -> u64 {
        let mut h = u64::from(self.kind.code()) | 1;
        for text in [self.subject.as_deref(), self.relation.as_deref(), self.query.as_deref()]
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
}

/// Efeito colateral de uma tool (para rollback).
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Escrita de memória (chave a remover no rollback).
    MemoryWrite {
        /// Chave de armazenamento.
        key: String,
    },
    /// Nenhum efeito.
    None,
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

/// Contexto de execução (orçamento/efeitos).
#[derive(Clone, Debug)]
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
    /// Efeitos acumulados.
    pub effects: Vec<Effect>,
}

impl ToolContext {
    /// Contexto novo.
    pub fn new(scope: ScopeId, budget: Budget, trace_id: u64) -> Self {
        Self {
            scope,
            budget,
            spent_ms: 0.0,
            spent_tokens: 0,
            trace_id,
            effects: Vec::new(),
        }
    }

    /// Há orçamento para `cost`?
    pub fn can_spend(&self, cost: Cost) -> bool {
        let ms = self.spent_ms + cost.ms;
        let tokens = self.spent_tokens.saturating_add(cost.tokens);
        ms as u64 <= self.budget.max_ms
            && (self.budget.max_tokens == 0 || tokens <= self.budget.max_tokens)
    }

    /// Cobra o custo.
    pub fn charge(&mut self, cost: Cost) {
        self.spent_ms += cost.ms;
        self.spent_tokens = self.spent_tokens.saturating_add(cost.tokens);
    }

    /// Registra um efeito.
    pub fn push_effect(&mut self, effect: Effect) {
        if effect != Effect::None {
            self.effects.push(effect);
        }
    }

    /// Rollback lógico (limpa efeitos; integração real vem na P2).
    pub fn rollback(&mut self) {
        self.effects.clear();
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

    /// Pré-condições.
    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool;

    /// Executa.
    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError>;
}

/// Converte um `ExpertOutput` em `ToolOutcome` (atalho para as tools reais).
pub fn outcome_from_expert(expert: ExpertId, output: ExpertOutput) -> ToolOutcome {
    let mut outcome = ToolOutcome::from(output);
    outcome.detail = format!("expert={} {}", expert.label(), outcome.detail);
    outcome
}

//! AETHER Agent — o organismo agêntico que costura os especialistas do motor.
//!
//! # O que este módulo é
//!
//! Uma camada **`T2`** (fora do hot path) que transforma as capacidades já
//! existentes do AETHER em um agente com cinco propriedades explícitas:
//!
//! | Premissa | Como aparece aqui |
//! |---|---|
//! | **Inferência** | FHRR (transplante), `LogicPipeline` (SAT), solver Ising (QUBO), memória de fase |
//! | **Agêntica** | laço perceive → roteia → infere → coere → age → observa, com orçamento de passos |
//! | **MoE** | `MoERouter`: gate por competência + confiabilidade aprendida (EXP3), top-k especialistas |
//! | **Coerência** | `CoherenceEngine`: claims tipados, contradição, certificado e abstenção |
//! | **Autoaprendizado** | EXP3 por feedback verificado, imprint de fase e novos fatos no FHRR |
//!
//! # O que ele **não** é
//!
//! * Não é um LLM: não tokeniza, não usa atenção, não faz backpropagation.
//! * Não “sabe” a resposta por magia: cada claim carrega fonte, autoridade e
//!   confiança; sem suporte, a ação é [`Action::Abstain`].
//! * Não aprende sozinho com a própria saída: o reward vem de
//!   [`Feedback::correct`] (rótulo/verificação externa). Auto-elogio não conta.
//!
//! # Fluxo
//!
//! ```text
//! Task ──features──► Router ──top-k──► Experts ──claims+cert──► Coherence
//!   ▲                                                            │
//!   │                                                            ▼
//!   └──────────── Feedback (correct?) ◄── Action (act/cite/propose/abstain)
//!                         │
//!                         ├─ EXP3 atualiza pesos do gate
//!                         ├─ FHRR aprende fato verificado
//!                         └─ política persiste (JSON)
//! ```

pub mod a8_v1;
pub mod bus;
pub mod cli;
pub mod coherence;
pub mod dec_status;
pub mod dec_upgrade;
pub mod embed;
pub mod gemma_scribe;
pub mod hitl_queue;
pub mod eval;
pub mod executor;
pub mod experts;
pub mod hier_moe;
pub mod hitl;
pub mod language;
pub mod learner;
pub mod linucb;
pub mod nl_out;
pub mod ocr;
pub mod planner;
pub mod predict;
pub mod risk_policy;
pub mod registry;
pub mod router;
pub mod self_model;
pub mod semantic;
pub mod speech;
pub mod telemetry;
pub mod tnl_in;
pub mod tool;
pub mod tool_agent;
pub mod tools;
pub mod undo_sgdb;
pub mod vision;

use std::path::PathBuf;

use crate::core::certificate::{Certificate as EngineCertificate, Provenance};
use crate::core::knowledge_bridge_f1::{Certificate as BridgeCertificate, ScopeId};
use crate::json::Json;

use self::coherence::{CoherenceEngine, CoherenceResult};
use self::experts::{default_experts, Expert};
pub use self::experts::{MemoryCitations, RecallCitation};
use self::router::MoERouter;

pub use dec_status::DecTomadaStatus;
pub use dec_upgrade::{
    abort_rollback, active_model_paths, apply_smoke_result, begin_upgrade, cutover_promote,
    detail_block, engine_label_from_ids, hw_compat_for, load_active_lock, load_catalog,
    mark_download_approved, revert_previous, CatalogReader, DecLamps, DecSlot, DecUpgradePhase,
    DecUpgradeState, LampColor, ACTIVE_LOCK, CATALOG_PATH, STATE_PATH as DEC_UPGRADE_STATE_PATH,
};
pub use a8_v1::{
    a8_v1_flag, criteria as a8_v1_criteria, enter_v1_allowed, enter_v1_default,
    write_criteria_artifact as write_a8_v1_criteria, A8V1Criterion, A8_V1_CRITERIA_PATH,
};
pub use bus::{
    promote_key_255, run_trio_turn, TrioOpts, TrioTurn, LEARN_AUTH_CAP,
};
pub use hitl_queue::{
    append_decision, enqueue as hitl_enqueue, enqueue_promote, load_pending as hitl_load_pending,
    HITL_DECISIONS_PATH, HITL_QUEUE_PATH,
};
pub use embed::{EmbedEra, EmbedPort, EmbedVector};
pub use eval::{default_eval_path, EvalReport, EvalTier};
pub use executor::ToolExecutor;
pub use hier_moe::{lsh_route_refuse, HierMoeStatus, HIER_MOE_STATUS_PATH};
pub use hitl::{apply_hitl, HitlDecision, HitlKind, HitlRequest};
pub use language::{
    adapt_text, GoalSchema, LanguageCaps, LanguageError, LanguageParseTool, LanguagePort,
    ParserPort, Style, Style as LanguageStyle,
};
pub use gemma_scribe::{
    active_scribe_path, ensure_scribe_fields_on_active_lock, scribe_opt_in, scribe_runtime_policy,
    vram_free_from_host_report, GemmaScribePort, ScribeRuntime, DEFAULT_AND_PEAK_MB, ENV_SCRIBE,
};
pub use nl_out::{
    certify_utterance, certify_utterance_template, judge_claim_text, CertifiedUtterance,
};
pub use ocr::{DocRef, OcrPort, OcrResult};
pub use predict::PredictExpert;
pub use speech::{SttPort, SttResult, TtsPort, TtsResult};
pub use tnl_in::{
    gate_fixtures as tnl_in_gate_fixtures, tnl_in_contract_json, write_tnl_in_contract,
    TNL_IN_CONTRACT_PATH, TNL_IN_CONTRACT_VERSION,
};
pub use vision::{VisionObs, VisionPort, VISION_AUTH_CAP};
pub use learner::{EpisodeRecord, Learner};
pub use linucb::{goal_features, linucb_enabled, ContextualTools, LinUcb, LINUCB_DIM};
pub use planner::{PlanStep, Planner};
pub use predict::{
    forecast_series, metacog_should_abstain, parse_series_query, PredictReport, NRMSE_OOD,
    PREDICT_AUTH_CAP, WASHOUT_MIN,
};
pub use registry::ToolRegistry;
pub use risk_policy::{
    agent_kill_switch, class_for_tool, gate_before_call, goal_hitl_token, hitl_medium_required,
    hitl_token_ok, matrix_json, requirement_for, write_matrix_artifact, HitlRequirement, RiskRow,
    RISK_MATRIX,
};
pub use self_model::SelfModel;
pub use tool::{
    Budget, Cost, Effect, Goal, GoalKind, Risk, Snapshot, SuccessCriteria, Tool, ToolContext,
    ToolError, ToolOutcome, ToolSchema, UndoBackend, UndoOp, VerifiedFact, WorldState,
    AUTHORITY_CORROBORATED, AUTHORITY_HITL, AUTHORITY_QUARANTINE, AUTHORITY_VERIFIED,
};
pub use telemetry::{goal_label, EpisodeTrace, StepTrace};
pub use tool_agent::{GoalOutcome, ToolAgent, VerifyTool};
pub use tools::{
    claims_from_research_json, code_generate_mode, code_lm_caps, coder_t3_generator_declared,
    default_coder_t3_path, extract_json_object, grammar_parse_rate, is_research_persist_goal,
    parse_constrained, pass_at_k, rasp_resolve_anchor, research_network_enabled,
    research_persist_authority, resolve_sandbox_path, run_coder_t3, shell_kill_switch, shell_mode,
    CodeApplyProposalTool, CodeAstGrepTool, CodeCheckTool, CodeEvalTool, CodeGenerateTool,
    CodeGenMode, CodeGrammarTool, CodeLmCaps, CodeLmTool, CodeLoopTool, CodePatchTool,
    CodeTestTool, CoderT3Report, FsReadTool, FsWriteTool, GoalProposeTool, HttpGetTool,
    McpCallTool, MemoryWriteTool, PatchOp, PatchProposal, PredictSeriesTool, ResearchExpandTool,
    ResearchQueryTool, ShellMode, ShellTool, CODE_AUTH_CAP, CODER_T3_PILOT, RESEARCH_AUTH_CAP,
    SHELL_AUTH_CAP,
};
pub use undo_sgdb::SgdbUndoBackend;

/// Identidade estável de um especialista (1 byte para logs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExpertId {
    /// Fatos relacionais via FHRR (transplante).
    Transplant,
    /// Memória associativa de fase.
    Phase,
    /// SAT/3-CNF via `LogicPipeline`.
    Logic,
    /// QUBO/Ising via solver termodinâmico.
    Solver,
    /// Memória durável (SGDB).
    Memory,
    /// Séries / mundo preditivo (A8 / P5).
    Predict,
}

impl ExpertId {
    /// Código estável.
    pub const fn code(self) -> u8 {
        match self {
            Self::Transplant => 0,
            Self::Phase => 1,
            Self::Logic => 2,
            Self::Solver => 3,
            Self::Memory => 4,
            Self::Predict => 5,
        }
    }

    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Transplant => "transplant",
            Self::Phase => "phase",
            Self::Logic => "logic",
            Self::Solver => "solver",
            Self::Memory => "memory",
            Self::Predict => "predict",
        }
    }

    /// Custo relativo esperado (roteador penaliza).
    pub const fn cost(self) -> f32 {
        match self {
            Self::Transplant => 0.2,
            Self::Phase => 0.3,
            Self::Logic => 1.0,
            Self::Solver => 1.2,
            Self::Memory => 0.8,
            Self::Predict => 0.6,
        }
    }
}

/// Tarefa estruturada. Texto só existe na borda; o motor vê vetores.
#[derive(Clone, Debug)]
pub enum Task {
    /// Consulta relacional: `subject ⊗ relation → value`.
    Fact {
        /// Entidade.
        subject: String,
        /// Relação.
        relation: String,
    },
    /// Completar padrão de fase a partir de uma pista.
    Recall {
        /// Pista em fases (`D`).
        cue: Vec<f32>,
    },
    /// Satisfazer 3-CNF.
    Satisfy {
        /// Variáveis.
        n: usize,
        /// Cláusulas.
        clauses: Vec<crate::core::logic_pipeline::Clause3>,
    },
    /// Minimizar QUBO `−½ sᵀJs − hᵀs`.
    Optimize {
        /// Dimensão.
        n: usize,
        /// Matriz `n×n` row-major.
        j: Vec<f64>,
        /// Campo `h`.
        h: Vec<f64>,
    },
    /// Consulta textual durável (SGDB).
    Memory {
        /// Consulta.
        query: String,
    },
    /// Previsão de série tipada (T-PRED / A8) — sem bytes crus.
    Predict {
        /// Amostras da série.
        series: Vec<f32>,
        /// Horizonte (1..=5).
        horizon: usize,
    },
}

impl Task {
    /// Tipo grosseiro (para features e logging).
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Fact { .. } => "fact",
            Self::Recall { .. } => "recall",
            Self::Satisfy { .. } => "satisfy",
            Self::Optimize { .. } => "optimize",
            Self::Memory { .. } => "memory",
            Self::Predict { .. } => "predict",
        }
    }
}

/// Features contínuas da tarefa (entrada do gate MoE).
#[derive(Clone, Copy, Debug, Default)]
pub struct TaskFeatures {
    /// `1.0` para o tipo correspondente.
    pub fact: f32,
    /// Tipo recall.
    pub recall: f32,
    /// Tipo SAT.
    pub satisfy: f32,
    /// Tipo QUBO.
    pub optimize: f32,
    /// Tipo memória.
    pub memory: f32,
    /// Tipo previsão (T-PRED).
    pub predict: f32,
    /// Dimensão normalizada (`log2(n+1)/8`).
    pub dims: f32,
    /// Densidade de restrições (`m/(n²)`, saturada).
    pub density: f32,
}

impl TaskFeatures {
    /// Extrai features da tarefa.
    pub fn from_task(task: &Task) -> Self {
        let mut f = Self::default();
        match task {
            Task::Fact { .. } => f.fact = 1.0,
            Task::Recall { cue } => {
                f.recall = 1.0;
                f.dims = ((cue.len().max(1) as f32).log2() / 8.0).min(1.0);
            }
            Task::Satisfy { n, clauses } => {
                f.satisfy = 1.0;
                f.dims = ((*n as f32 + 1.0).log2() / 8.0).min(1.0);
                f.density = (clauses.len() as f32 / (*n as f32 * *n as f32).max(1.0)).min(1.0);
            }
            Task::Optimize { n, .. } => {
                f.optimize = 1.0;
                f.dims = ((*n as f32 + 1.0).log2() / 8.0).min(1.0);
            }
            Task::Memory { .. } => f.memory = 1.0,
            Task::Predict { series, .. } => {
                f.predict = 1.0;
                f.dims = ((series.len().max(1) as f32).log2() / 8.0).min(1.0);
            }
        }
        f
    }
}

/// Afirmação tipada — a unidade de evidência do agente.
#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    /// Sujeito (entidade).
    pub subject: String,
    /// Relação.
    pub relation: String,
    /// Valor/objeto.
    pub value: String,
    /// Confiança do especialista (`0..=1`).
    pub confidence: f32,
    /// Autoridade social (`0..=255`).
    pub authority: u8,
    /// Especialista de origem.
    pub source: ExpertId,
    /// Proveniência.
    pub provenance: Provenance,
}

impl Claim {
    /// Constrói uma claim com autoridade de default do especialista.
    pub fn new(
        subject: impl Into<String>,
        relation: impl Into<String>,
        value: impl Into<String>,
        confidence: f32,
        authority: u8,
        source: ExpertId,
        provenance: Provenance,
    ) -> Self {
        Self {
            subject: subject.into(),
            relation: relation.into(),
            value: value.into(),
            confidence: confidence.clamp(0.0, 1.0),
            authority,
            source,
            provenance,
        }
    }

    /// Chave de grupo (sujeito + relação, normalizados).
    pub fn group(&self) -> (String, String) {
        (
            self.subject.trim().to_lowercase(),
            self.relation.trim().to_lowercase(),
        )
    }

    /// Comparação de valor tolerante a caixa/espaço.
    pub fn same_value(&self, other: &Self) -> bool {
        self.value.trim().eq_ignore_ascii_case(other.value.trim())
    }
}

/// Saída de um especialista.
#[derive(Clone, Debug)]
pub struct ExpertOutput {
    /// Especialista.
    pub expert: ExpertId,
    /// Afirmações.
    pub claims: Vec<Claim>,
    /// Confiança global (`0..=1`).
    pub confidence: f32,
    /// Certificado físico (quando existe: solver/lógica).
    pub certificate: Option<EngineCertificate>,
    /// Detalhe legível (diagnóstico, não prompt).
    pub detail: String,
    /// Custo medido em µs.
    pub cost_us: u64,
}

impl ExpertOutput {
    /// Saída vazia/abstida de um especialista.
    pub fn empty(expert: ExpertId, detail: impl Into<String>) -> Self {
        Self {
            expert,
            claims: Vec::new(),
            confidence: 0.0,
            certificate: None,
            detail: detail.into(),
            cost_us: 0,
        }
    }
}

/// Feedback verificado (externo ao agente).
#[derive(Clone, Debug, Default)]
pub struct Feedback {
    /// O resultado estava correto? (`None` = só feedback intrínseco).
    pub correct: Option<bool>,
    /// Valor esperado (permite aprender um fato novo).
    pub expected: Option<Claim>,
}

/// Ação prescrita pela matriz 2D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Age e cita.
    Act,
    /// Cita, mas não age (autoridade alta + ambíguo/contestado).
    Cite,
    /// Propõe (coerente sem fonte forte).
    Propose,
    /// Abstem-se.
    Abstain,
}

impl Action {
    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Act => "act",
            Self::Cite => "cite",
            Self::Propose => "propose",
            Self::Abstain => "abstain",
        }
    }
}

/// Resultado completo de uma execução do agente.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// Tarefa.
    pub task: Task,
    /// Saídas dos especialistas consultados (ordem de execução).
    pub outputs: Vec<ExpertOutput>,
    /// Coerência final.
    pub coherence: CoherenceResult,
    /// Especialistas executados.
    pub experts_used: Vec<ExpertId>,
    /// Passos do laço.
    pub steps: usize,
    /// Ação final.
    pub action: Action,
    /// Resposta principal (se houver).
    pub answer: Option<Claim>,
}

/// Configuração do agente.
#[derive(Clone, Debug)]
pub struct AgentConfig {
    /// Especialistas por passo (mínimo 1).
    pub top_k: usize,
    /// Passos máximos do laço agêntico.
    pub max_steps: usize,
    /// Confiança mínima para agir sem mais evidência.
    pub min_confidence: f32,
    /// Semente determinística do roteador.
    pub seed: u64,
    /// Dimensão do FHRR do transplante.
    pub dim: usize,
    /// Traços do FHRR (superposição redundante).
    pub traces: usize,
    /// Caminho do JSONL de fatos (`data/facts.jsonl` por default).
    pub facts_path: PathBuf,
    /// Política persistida (pesos EXP3). `None` = só memória.
    pub policy_path: Option<PathBuf>,
    /// Especificação do SGDB para o especialista de memória (`None` = desligado).
    pub sgdb: Option<String>,
    /// Política do self-model das tools (P1). `None` = só memória.
    pub tool_policy_path: Option<PathBuf>,
    /// Escopo das tools.
    pub scope: ScopeId,
    /// Política de mundo (A4 / G-A4).
    pub world: WorldPolicy,
}

/// Política de acesso ao mundo (sandbox / HTTP / MCP). Deny-default.
#[derive(Clone, Debug)]
pub struct WorldPolicy {
    /// Raiz da sandbox de `fs.*` (caminhos resolvidos sob esta raiz).
    pub sandbox_root: PathBuf,
    /// Hosts permitidos em `http.get` (lowercase, sem porta).
    pub http_hosts: Vec<String>,
    /// Se `false`, `http.get` valida allowlist mas **não** abre socket.
    pub http_fetch: bool,
    /// Nomes MCP tipados permitidos em `mcp.call`.
    pub mcp_allow: Vec<String>,
}

impl Default for WorldPolicy {
    fn default() -> Self {
        Self {
            sandbox_root: PathBuf::from("target/aether_sandbox"),
            http_hosts: Vec::new(),
            http_fetch: false,
            mcp_allow: vec![
                "memory.recall".into(),
                "memory.write".into(),
                "sgdb.context".into(),
            ],
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            top_k: 2,
            max_steps: 2,
            min_confidence: 0.75,
            seed: 0xA37E_1A9E,
            dim: 1024,
            traces: 8,
            facts_path: PathBuf::from("data/facts.jsonl"),
            policy_path: None,
            sgdb: None,
            tool_policy_path: None,
            scope: ScopeId::new(1, 0, 1),
            world: WorldPolicy::default(),
        }
    }
}

/// Telemetria do agente.
#[derive(Clone, Copy, Debug, Default)]
pub struct AgentMetrics {
    /// Execuções.
    pub runs: u64,
    /// Passos totais.
    pub steps: u64,
    /// Ações `Act`.
    pub acts: u64,
    /// Abstenções.
    pub abstentions: u64,
    /// Feedback correto.
    pub correct: u64,
    /// Feedback incorreto.
    pub wrong: u64,
}

/// Agente AETHER.
pub struct AetherAgent {
    experts: Vec<Box<dyn Expert>>,
    router: MoERouter,
    coherence: CoherenceEngine,
    cfg: AgentConfig,
    metrics: AgentMetrics,
}

impl std::fmt::Debug for AetherAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AetherAgent")
            .field("experts", &self.experts.len())
            .field("config", &self.cfg)
            .field("metrics", &self.metrics)
            .finish()
    }
}

impl AetherAgent {
    /// Cria o agente com os especialistas padrão.
    pub fn new(cfg: AgentConfig) -> Self {
        let experts = default_experts(&cfg);
        let mut router = MoERouter::new(cfg.seed, cfg.policy_path.clone());
        router.load();
        Self {
            experts,
            router,
            coherence: CoherenceEngine::default(),
            cfg,
            metrics: AgentMetrics::default(),
        }
    }

    /// Configuração.
    pub fn config(&self) -> &AgentConfig {
        &self.cfg
    }

    /// Métricas.
    pub fn metrics(&self) -> AgentMetrics {
        self.metrics
    }

    /// Pesos atuais do gate (ordem dos especialistas).
    pub fn router_weights(&self) -> Vec<f32> {
        self.router.weights()
    }

    /// Especialistas registrados.
    pub fn experts(&self) -> Vec<(ExpertId, f32)> {
        self.experts
            .iter()
            .map(|e| (e.id(), self.router.weight_of(e.id())))
            .collect()
    }

    /// Executa o laço agêntico (sem aprender).
    pub fn run(&mut self, task: &Task) -> Outcome {
        let features = TaskFeatures::from_task(task);
        let mut outputs: Vec<ExpertOutput> = Vec::new();
        let mut used: Vec<ExpertId> = Vec::new();
        let mut steps = 0usize;
        let mut last = self.coherence.merge(&outputs);

        while steps < self.cfg.max_steps.max(1) {
            let probes: Vec<f32> = self
                .experts
                .iter()
                .map(|e| e.probe(task, &features))
                .collect();
            let picks = self.router.select(&probes, self.cfg.top_k, &used);
            if picks.is_empty() {
                break;
            }
            steps += 1;
            for idx in picks {
                let output = self.experts[idx].infer(task);
                used.push(output.expert);
                outputs.push(output);
            }
            last = self.coherence.merge(&outputs);
            // Para cedo quando a evidência já sustenta agir com confiança.
            if last.action == Action::Act && last.support >= self.cfg.min_confidence {
                break;
            }
            // Se a ambiguidade é só falta de um segundo parecer, o laço
            // continua; sem candidatos novos, `select` devolve vazio.
        }

        self.metrics.runs += 1;
        self.metrics.steps += steps as u64;
        if last.action == Action::Act {
            self.metrics.acts += 1;
        }
        if last.action == Action::Abstain {
            self.metrics.abstentions += 1;
        }
        Outcome {
            task: task.clone(),
            outputs,
            action: last.action,
            answer: last.answer.clone(),
            coherence: last,
            experts_used: used,
            steps,
        }
    }

    /// Aprende com feedback verificado.
    ///
    /// Sem `correct`, só o sinal **intrínseco** de coerência conta (e o
    /// reward máximo é 0.5, para não confundir “coerente” com “verdadeiro”).
    pub fn learn(&mut self, outcome: &Outcome, feedback: &Feedback) {
        let reward = match feedback.correct {
            Some(true) => {
                self.metrics.correct += 1;
                1.0
            }
            Some(false) => {
                self.metrics.wrong += 1;
                0.0
            }
            None => {
                if outcome.coherence.action == Action::Abstain {
                    0.0
                } else {
                    0.5 * outcome.coherence.support
                }
            }
        };
        self.router.update(&outcome.experts_used, reward);
        for expert in self.experts.iter_mut() {
            expert.learn(&outcome.task, feedback);
        }
        self.router.persist();
    }

    /// Atalho: consulta um fato e devolve o resultado.
    pub fn ask_fact(&mut self, subject: &str, relation: &str) -> Outcome {
        self.run(&Task::Fact {
            subject: subject.to_string(),
            relation: relation.to_string(),
        })
    }

    /// Atalho: resolve SAT.
    pub fn satisfy(
        &mut self,
        n: usize,
        clauses: Vec<crate::core::logic_pipeline::Clause3>,
    ) -> Outcome {
        self.run(&Task::Satisfy { n, clauses })
    }

    /// Atalho: resolve QUBO.
    pub fn optimize(&mut self, n: usize, j: Vec<f64>, h: Vec<f64>) -> Outcome {
        self.run(&Task::Optimize { n, j, h })
    }

    /// Certificado do `T1` a partir da coerência (para o SGDB/API).
    pub fn bridge_certificate(&self, outcome: &Outcome) -> BridgeCertificate {
        BridgeCertificate {
            energy: 0.0,
            gap: outcome.coherence.support,
            violations: outcome.coherence.contradictions as u32,
        }
    }

    /// Serializa o estado do agente para diagnóstico.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("experts", Json::num(self.experts.len() as f64)),
            ("runs", Json::num(self.metrics.runs as f64)),
            ("steps", Json::num(self.metrics.steps as f64)),
            ("acts", Json::num(self.metrics.acts as f64)),
            ("abstentions", Json::num(self.metrics.abstentions as f64)),
            ("correct", Json::num(self.metrics.correct as f64)),
            ("wrong", Json::num(self.metrics.wrong as f64)),
            (
                "weights",
                Json::Arr(
                    self.experts
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("expert", Json::str(e.id().label())),
                                (
                                    "weight",
                                    Json::num(f64::from(self.router.weight_of(e.id()))),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

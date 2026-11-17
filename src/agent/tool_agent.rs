//! ToolAgent — laço P1: planner → executor → verificação → self-model.
//!
//! É a camada que faz a **autoescolha de ferramenta** sobre o registry. O
//! `AetherAgent` (MoE de especialistas) continua existindo; este agente escolhe
//! *o que usar* para um `Goal`, com orçamento, risco e rollback.

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::knowledge_bridge_f1::ScopeId;

use super::executor::{Execution, ToolExecutor};
use super::planner::{PlanStep, Planner};
use super::registry::ToolRegistry;
use super::self_model::SelfModel;
use super::telemetry::{goal_label, EpisodeTrace, StepTrace};
use super::tool::{
    Budget, Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    UndoBackend, WorldState,
};
use super::tools::default_tools;
use super::{Action, AgentConfig, Claim, ExpertId, Feedback};
use crate::core::certificate::Provenance;

/// Resultado de um goal.
#[derive(Clone, Debug)]
pub struct GoalOutcome {
    /// Goal.
    pub goal: Goal,
    /// Sucesso verificado.
    pub success: bool,
    /// Ação final.
    pub action: Action,
    /// Passos.
    pub steps: Vec<StepTrace>,
    /// Última saída.
    pub outcome: Option<super::tool::ToolOutcome>,
    /// Claims verificadas do workflow (blackboard).
    pub claims: Vec<Claim>,
    /// Motivo.
    pub rationale: String,
    /// Trace.
    pub trace: EpisodeTrace,
}

/// Agente de ferramentas (P1).
pub struct ToolAgent {
    registry: ToolRegistry,
    self_model: SelfModel,
    planner: Planner,
    executor: ToolExecutor,
    scope: ScopeId,
    traces: Vec<EpisodeTrace>,
    undo_backend: Option<Arc<dyn UndoBackend>>,
}

impl std::fmt::Debug for ToolAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolAgent")
            .field("tools", &self.registry.len())
            .field("episodes", &self.self_model.episodes())
            .field("traces", &self.traces.len())
            .finish()
    }
}

impl ToolAgent {
    /// Cria com as tools padrão da configuração.
    pub fn new(cfg: &AgentConfig) -> Self {
        let allowlist = tools_allowlist();
        let mut registry = ToolRegistry::new(allowlist);
        for tool in default_tools(cfg) {
            let _ = registry.register(tool);
        }
        let _ = registry.register(Box::new(VerifyTool));
        let mut self_model = SelfModel::new(cfg.tool_policy_path.clone());
        self_model.load();
        Self {
            registry,
            self_model,
            planner: Planner::default(),
            executor: ToolExecutor,
            scope: cfg.scope,
            traces: Vec::new(),
            undo_backend: None,
        }
    }

    /// Cria com registry/self-model injetados (testes/gates).
    pub fn with_registry(registry: ToolRegistry, self_model: SelfModel, scope: ScopeId) -> Self {
        Self {
            registry,
            self_model,
            planner: Planner::default(),
            executor: ToolExecutor,
            scope,
            traces: Vec::new(),
            undo_backend: None,
        }
    }

    /// Injeta o backend de undo (memória real ou mock) para o workflow.
    pub fn with_undo_backend(mut self, backend: Arc<dyn UndoBackend>) -> Self {
        self.undo_backend = Some(backend);
        self
    }

    /// Anexa o backend de undo durável (journal JSONL) usado por `memory.write`.
    ///
    /// O journal é reaplicado no `new`, então um crash entre a escrita e o
    /// registro do efeito ainda pode ser compensado no próximo run.
    pub fn with_undo_journal(mut self, path: impl Into<PathBuf>) -> Self {
        self.undo_backend = Some(Arc::new(super::undo_sgdb::SgdbUndoBackend::new(path)));
        self
    }

    fn context(&self, budget: Budget, trace_id: u64) -> ToolContext {
        let ctx = ToolContext::new(self.scope, budget, trace_id);
        if let Some(backend) = &self.undo_backend {
            ctx.with_undo_backend(Arc::clone(backend))
        } else {
            ctx
        }
    }

    /// Registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Self-model.
    pub fn self_model(&self) -> &SelfModel {
        &self.self_model
    }

    /// Traces.
    pub fn traces(&self) -> &[EpisodeTrace] {
        &self.traces
    }

    /// Lista de tools (schema/custo/risco) para CLI.
    pub fn tools(&self) -> Vec<ToolSchema> {
        self.registry.schemas()
    }

    /// Custo/risco de uma tool.
    pub fn tool_stats(&self, name: &str) -> Option<(Cost, Risk)> {
        self.registry.stats(name)
    }

    /// Ranking de tools para um goal, sem executar (diagnóstico de autoescolha).
    pub fn plan(&self, goal: &Goal) -> Vec<PlanStep> {
        let state = WorldState {
            scope: self.scope,
            facts_loaded: 0,
            memory_hits: 0,
        };
        self.planner
            .rank(goal, &state, &self.registry, &self.self_model, &[])
    }

    /// Executa o goal com orçamento, risco e verificação.
    ///
    /// Se `goal.pipeline` não estiver vazio, executa os subgoals em sequência,
    /// compartilhando o blackboard (`ToolContext.claims`). Falha em qualquer
    /// passo desfaz os efeitos/claims acumulados (rollback de workflow).
    pub fn solve(&mut self, goal: &Goal) -> GoalOutcome {
        if !goal.pipeline.is_empty() {
            return self.solve_pipeline(goal);
        }
        let started = std::time::Instant::now();
        let state = WorldState {
            scope: self.scope,
            facts_loaded: 0,
            memory_hits: 0,
        };
        let mut ctx = self.context(
            Budget {
                max_steps: goal.budget.max_steps.clamp(1, 16),
                ..goal.budget
            },
            goal.id,
        );
        let (success, action, steps, last, rationale) = self.run_goal(goal, &state, &mut ctx);
        self.finish(
            goal,
            success,
            action,
            steps,
            last,
            rationale,
            ctx.claims.clone(),
            started,
        )
    }

    fn solve_pipeline(&mut self, workflow: &Goal) -> GoalOutcome {
        let started = std::time::Instant::now();
        let state = WorldState {
            scope: self.scope,
            facts_loaded: 0,
            memory_hits: 0,
        };
        let total_steps: usize = workflow
            .pipeline
            .iter()
            .map(|goal| goal.budget.max_steps.max(1))
            .sum::<usize>()
            .clamp(1, 32);
        let mut ctx = self.context(
            Budget {
                max_steps: total_steps,
                ..workflow.budget
            },
            workflow.id,
        );
        let mut steps: Vec<StepTrace> = Vec::new();
        let mut last: Option<Execution> = None;
        let mut success = true;
        let mut action = Action::Act;
        let mut rationale = "workflow verificado".to_string();

        for (index, subgoal) in workflow.pipeline.iter().enumerate() {
            let (ok, sub_action, mut sub_steps, sub_last, why) =
                self.run_goal(subgoal, &state, &mut ctx);
            steps.append(&mut sub_steps);
            if let Some(execution) = sub_last {
                last = Some(execution);
            }
            if sub_action == Action::Cite {
                success = false;
                action = Action::Cite;
                // HITL aborta o workflow: nada do que já foi aplicado fica.
                rationale = match ctx.rollback() {
                    Ok(()) => format!("passo {} pediu HITL", index + 1),
                    Err(e) => format!("passo {} pediu HITL; {e}", index + 1),
                };
                break;
            }
            if !ok {
                success = false;
                action = Action::Abstain;
                rationale = match ctx.rollback() {
                    Ok(()) => format!("passo {} falhou: {why}", index + 1),
                    Err(e) => format!("passo {} falhou: {why}; {e}", index + 1),
                };
                break;
            }
        }
        self.finish(
            workflow,
            success,
            action,
            steps,
            last,
            rationale,
            ctx.claims.clone(),
            started,
        )
    }

    fn run_goal(
        &mut self,
        goal: &Goal,
        state: &WorldState,
        ctx: &mut ToolContext,
    ) -> (bool, Action, Vec<StepTrace>, Option<Execution>, String) {
        let budget = goal.budget.max_steps.clamp(1, 16);
        let claims_before = ctx.claims.len();
        let certificates_before = ctx.certificates.len();
        let mut used: Vec<&'static str> = Vec::new();
        let mut steps: Vec<StepTrace> = Vec::new();
        let mut last: Option<Execution> = None;
        let mut rationale = "sem tool candidata".to_string();
        let mut action = Action::Abstain;
        let mut success = false;

        for _ in 0..budget {
            let Some(step) =
                self.planner
                    .select(goal, state, &self.registry, &self.self_model, &used)
            else {
                break;
            };
            used.push(step.tool);
            if !ctx.can_spend(step.cost) {
                steps.push(StepTrace {
                    tool: step.tool.to_string(),
                    score: step.score,
                    reason: step.reason.clone(),
                    executed: false,
                    met: false,
                    elapsed_us: 0,
                    detail: "custo > orçamento: hold".to_string(),
                });
                rationale = "orçamento insuficiente: hold".to_string();
                continue;
            }
            let execution = self.executor.execute(&mut self.registry, &step, goal, ctx);
            match execution {
                Ok(execution) => {
                    let met = execution.met;
                    steps.push(step_trace(
                        &step,
                        execution.met,
                        execution.elapsed_us,
                        &execution,
                    ));
                    last = Some(execution);
                    if met {
                        success = true;
                        action = Action::Act;
                        rationale = "critério de sucesso verificado".to_string();
                        break;
                    }
                    rationale = format!("{} não satisfez o critério", step.tool);
                }
                Err(e) => {
                    steps.push(StepTrace {
                        tool: step.tool.to_string(),
                        score: step.score,
                        reason: step.reason.clone(),
                        executed: false,
                        met: false,
                        elapsed_us: 0,
                        detail: e.to_string(),
                    });
                    if matches!(e, super::tool::ToolError::NeedsHuman(_)) {
                        action = Action::Cite;
                        rationale = e.to_string();
                        break;
                    }
                    rationale = e.to_string();
                }
            }
        }

        if let Some(execution) = &last {
            if execution.outcome.needs_human {
                // HITL não é sucesso: nada do passo entra no resultado.
                success = false;
                action = Action::Cite;
                rationale = "tool pediu HITL".to_string();
                let effects = ctx.effects.len();
                ctx.rollback_to(effects, claims_before, certificates_before);
            }
        }
        (success, action, steps, last, rationale)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        goal: &Goal,
        success: bool,
        action: Action,
        steps: Vec<StepTrace>,
        last: Option<Execution>,
        rationale: String,
        claims: Vec<Claim>,
        started: std::time::Instant,
    ) -> GoalOutcome {
        let total_us = started.elapsed().as_micros() as u64;
        // Passo que falhou/pediu HITL não pode "vazar" resposta como se fosse
        // sucesso: a evidência fica nos steps, não no answer.
        let answer = if success {
            last.as_ref()
                .and_then(|execution| execution.outcome.first_claim())
                .map(|claim| claim.value.clone())
        } else {
            None
        };
        let trace = EpisodeTrace {
            goal: goal_label(goal),
            steps: steps.clone(),
            success,
            action,
            total_us,
            rationale: rationale.clone(),
            answer: answer.clone(),
        };
        self.traces.push(trace.clone());
        GoalOutcome {
            goal: goal.clone(),
            success,
            action,
            steps,
            outcome: if success {
                last.map(|execution| execution.outcome)
            } else {
                None
            },
            claims,
            rationale,
            trace,
        }
    }

    /// Aprende com feedback verificado; atualiza self-model e persiste.
    pub fn learn(&mut self, outcome: &GoalOutcome, feedback: &Feedback) {
        let reward = feedback.correct.unwrap_or(outcome.success);
        for step in &outcome.steps {
            if step.executed {
                self.self_model.update(&step.tool, &outcome.goal, reward);
            }
        }
        self.self_model.persist();
    }

    /// Caminho da política de tools.
    pub fn policy_path(&self) -> Option<PathBuf> {
        self.self_model.path().map(|path| path.to_path_buf())
    }
}

fn step_trace(step: &PlanStep, met: bool, elapsed_us: u64, execution: &Execution) -> StepTrace {
    StepTrace {
        tool: step.tool.to_string(),
        score: step.score,
        reason: step.reason.clone(),
        executed: true,
        met,
        elapsed_us,
        detail: execution.outcome.detail.clone(),
    }
}

/// Tool de verificação: confere claims/certificados do blackboard.
///
/// É a peça que fecha o pipeline `fact → sat → verify` sem depender de um
/// verificador externo. Não age; só atesta.
#[derive(Debug)]
pub struct VerifyTool;

impl Tool for VerifyTool {
    fn name(&self) -> &'static str {
        "verify"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "verify",
            description: "verifica claims/certificados do blackboard",
            args: &["subject", "relation"],
            kinds: &[GoalKind::Verify],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Verify {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let has_claim = !ctx.claims.is_empty();
        let has_cert = ctx
            .certificates
            .iter()
            .any(|cert| cert.violations == 0 && cert.energy.is_finite());
        if has_claim && has_cert {
            Ok(ToolOutcome {
                claims: vec![Claim::new(
                    "verify",
                    "certificate",
                    "ok",
                    1.0,
                    200,
                    ExpertId::Logic,
                    Provenance::Observed,
                )],
                confidence: 1.0,
                detail: format!(
                    "verificado: {} claim(s), {} certificado(s)",
                    ctx.claims.len(),
                    ctx.certificates.len()
                ),
                ..ToolOutcome::default()
            })
        } else {
            Ok(ToolOutcome::empty(format!(
                "sem evidência: claims={}, certs={}",
                ctx.claims.len(),
                ctx.certificates.len()
            )))
        }
    }
}

fn tools_allowlist() -> Vec<&'static str> {
    vec![
        "fact",
        "sat",
        "qubo",
        "memory",
        "memory.write",
        "verify",
        "ask.human",
        "language.parse",
    ]
}

//! ToolAgent — laço P1: planner → executor → verificação → self-model.
//!
//! É a camada que faz a **autoescolha de ferramenta** sobre o registry. O
//! `AetherAgent` (MoE de especialistas) continua existindo; este agente escolhe
//! *o que usar* para um `Goal`, com orçamento, risco e rollback.

use std::path::PathBuf;

use crate::core::knowledge_bridge_f1::ScopeId;

use super::executor::{Execution, ToolExecutor};
use super::planner::{PlanStep, Planner};
use super::registry::ToolRegistry;
use super::self_model::SelfModel;
use super::telemetry::{goal_label, EpisodeTrace, StepTrace};
use super::tool::{Budget, Cost, Goal, Risk, ToolContext, ToolSchema, WorldState};
use super::tools::default_tools;
use super::{Action, AgentConfig, Feedback};

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
        let mut self_model = SelfModel::new(cfg.tool_policy_path.clone());
        self_model.load();
        Self {
            registry,
            self_model,
            planner: Planner::default(),
            executor: ToolExecutor,
            scope: cfg.scope,
            traces: Vec::new(),
        }
    }

    /// Cria com registry/self-model injetados (testes/gates).
    pub fn with_registry(
        registry: ToolRegistry,
        self_model: SelfModel,
        scope: ScopeId,
    ) -> Self {
        Self {
            registry,
            self_model,
            planner: Planner::default(),
            executor: ToolExecutor,
            scope,
            traces: Vec::new(),
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

    /// Executa o goal com orçamento, risco e verificação.
    pub fn solve(&mut self, goal: &Goal) -> GoalOutcome {
        let started = std::time::Instant::now();
        let state = WorldState {
            scope: self.scope,
            facts_loaded: 0,
            memory_hits: 0,
        };
        let budget = goal.budget.max_steps.max(1).min(16);
        let mut ctx = ToolContext::new(self.scope, Budget {
            max_steps: budget,
            ..goal.budget
        }, goal.id);
        let mut used: Vec<&'static str> = Vec::new();
        let mut steps: Vec<StepTrace> = Vec::new();
        let mut last: Option<Execution> = None;
        let mut rationale = "sem tool candidata".to_string();
        let mut action = Action::Abstain;
        let mut success = false;

        for _ in 0..budget {
            let Some(step) = self.planner.select(
                goal,
                &state,
                &self.registry,
                &self.self_model,
                &used,
            ) else {
                break;
            };
            used.push(step.tool);
            let execution = self
                .executor
                .execute(&mut self.registry, &step, goal, &mut ctx);
            match execution {
                Ok(execution) => {
                    let met = execution.met;
                    steps.push(step_trace(&step, execution.met, execution.elapsed_us, &execution));
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
                action = Action::Cite;
                rationale = "tool pediu HITL".to_string();
            }
        }

        let total_us = started.elapsed().as_micros() as u64;
        let trace = EpisodeTrace {
            goal: goal_label(goal),
            steps: steps.clone(),
            success,
            action,
            total_us,
            rationale: rationale.clone(),
        };
        self.traces.push(trace.clone());
        GoalOutcome {
            goal: goal.clone(),
            success,
            action,
            steps,
            outcome: last.map(|execution| execution.outcome),
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

fn tools_allowlist() -> Vec<&'static str> {
    vec!["fact", "sat", "qubo", "memory", "ask.human"]
}

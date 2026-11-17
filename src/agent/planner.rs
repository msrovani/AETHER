//! Planner single-step: EFE aproximada + UCB + custo/risco.
//!
//! P1 do roadmap: escolhe **uma** tool por passo. A versão multi-passo (P2)
//! reutiliza a mesma função de score dentro de uma busca.

use super::registry::ToolRegistry;
use super::self_model::SelfModel;
use super::tool::{Cost, Goal, Risk, WorldState};

/// Passo de plano.
#[derive(Clone, Debug)]
pub struct PlanStep {
    /// Nome da tool.
    pub tool: &'static str,
    /// Score final.
    pub score: f64,
    /// Custo estimado.
    pub cost: Cost,
    /// Risco.
    pub risk: Risk,
    /// Justificativa curta.
    pub reason: String,
}

/// Planner.
#[derive(Clone, Debug)]
pub struct Planner {
    /// Peso da exploração (UCB).
    pub lambda_ucb: f64,
    /// Peso do custo.
    pub lambda_cost: f64,
    /// Peso do risco.
    pub lambda_risk: f64,
    /// Piso de affordance.
    pub min_affordance: f32,
    /// Piso de score: abaixo disso, agir é pior que abster (EFE pragmática).
    pub min_score: f64,
}

impl Default for Planner {
    fn default() -> Self {
        Self {
            lambda_ucb: 0.35,
            lambda_cost: 0.20,
            lambda_risk: 1.0,
            min_affordance: 0.01,
            min_score: 0.05,
        }
    }
}

impl Planner {
    /// Seleciona o melhor candidato não usado.
    pub fn select(
        &self,
        goal: &Goal,
        state: &WorldState,
        registry: &ToolRegistry,
        self_model: &SelfModel,
        used: &[&str],
    ) -> Option<PlanStep> {
        self.rank(goal, state, registry, self_model, used)
            .into_iter()
            .find(|step| step.score >= self.min_score)
    }

    /// Ranking completo (para CLI/diagnóstico).
    pub fn rank(
        &self,
        goal: &Goal,
        state: &WorldState,
        registry: &ToolRegistry,
        self_model: &SelfModel,
        used: &[&str],
    ) -> Vec<PlanStep> {
        let mut steps: Vec<PlanStep> = registry
            .candidates(goal, state)
            .into_iter()
            .filter(|candidate| !used.contains(&candidate.name))
            .filter(|candidate| candidate.affordance >= self.min_affordance)
            .filter(|candidate| candidate.risk <= goal.risk_ceiling)
            .map(|candidate| {
                let p = self_model.success(candidate.name, goal);
                let u = self_model.ucb(candidate.name, goal);
                let cost = f64::from(candidate.cost.ms) / 1_000.0
                    + f64::from(candidate.cost.tokens) / 10_000.0;
                let risk = candidate.risk.penalty();
                let score = f64::from(candidate.affordance) * (0.5 + 0.5 * p) + self.lambda_ucb * u
                    - self.lambda_cost * cost
                    - self.lambda_risk * risk;
                PlanStep {
                    tool: candidate.name,
                    score,
                    cost: candidate.cost,
                    risk: candidate.risk,
                    reason: format!(
                        "affordance={:.2} p_sucesso={:.2} ucb={:.2} custo={:.2}ms risco={}",
                        candidate.affordance,
                        p,
                        u,
                        candidate.cost.ms,
                        candidate.risk.label()
                    ),
                }
            })
            .collect();
        steps.sort_by(|a, b| b.score.total_cmp(&a.score));
        steps
    }
}

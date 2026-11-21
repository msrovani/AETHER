//! Planner single-step: EFE aproximada + UCB + custo/risco.
//!
//! P1 do roadmap: escolhe **uma** tool por passo. A versão multi-passo (P2)
//! reutiliza a mesma função de score dentro de uma busca.
//!
//! A5/P6: candidatos com pré-condição falsa dados `WorldState.claims` já saem
//! em `ToolRegistry::candidates` (poda barata; sem MCTS).

use super::linucb::ContextualTools;
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
    /// Tool de escalação (não executa; só pede humano).
    pub escalation: bool,
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
        self.select_with(goal, state, registry, self_model, used, None)
    }

    /// Como [`Self::select`], com LinUCB contextual opcional (A6).
    pub fn select_with(
        &self,
        goal: &Goal,
        state: &WorldState,
        registry: &ToolRegistry,
        self_model: &SelfModel,
        used: &[&str],
        linucb: Option<&ContextualTools>,
    ) -> Option<PlanStep> {
        let steps = self.rank_with(goal, state, registry, self_model, used, linucb);
        if let Some(step) = steps
            .iter()
            .find(|step| step.risk <= goal.risk_ceiling && step.score >= self.min_score)
        {
            return Some(step.clone());
        }
        steps.into_iter().find(|step| step.risk > goal.risk_ceiling)
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
        self.rank_with(goal, state, registry, self_model, used, None)
    }

    /// Ranking com LinUCB opt-in (mistura 50/50 com self-model calibrado).
    pub fn rank_with(
        &self,
        goal: &Goal,
        state: &WorldState,
        registry: &ToolRegistry,
        self_model: &SelfModel,
        used: &[&str],
        linucb: Option<&ContextualTools>,
    ) -> Vec<PlanStep> {
        let mut steps: Vec<PlanStep> = registry
            .candidates(goal, state)
            .into_iter()
            .filter(|candidate| !used.contains(&candidate.name))
            .filter(|candidate| candidate.affordance >= self.min_affordance)
            .map(|candidate| {
                let over_ceiling = candidate.risk > goal.risk_ceiling;
                let p_sm = self_model.calibrated_success(candidate.name, goal);
                let u_sm = self_model.ucb(candidate.name, goal);
                let (p, u, lin_tag) = match linucb.and_then(|c| c.score_parts(candidate.name, goal))
                {
                    Some((p_lin, bonus)) => (0.5 * p_sm + 0.5 * p_lin, bonus, true),
                    None => (p_sm, u_sm, false),
                };
                let cost = f64::from(candidate.cost.ms) / 1_000.0
                    + f64::from(candidate.cost.tokens) / 10_000.0;
                let risk = candidate.risk.penalty();
                let score = if candidate.escalation {
                    -1_000.0
                } else if over_ceiling {
                    f64::from(candidate.affordance)
                } else {
                    let base = f64::from(candidate.affordance) * (0.5 + 0.5 * p)
                        + self.lambda_ucb * u
                        - self.lambda_cost * cost
                        - self.lambda_risk * risk;
                    // Routing explícito (affordance≈1, ex.: goal.propose / shell.exec):
                    // High não some sob Medium genérico — piso acima do min_score.
                    if candidate.affordance >= 0.95 {
                        base.max(self.min_score + 1.0)
                    } else {
                        base
                    }
                };
                PlanStep {
                    tool: candidate.name,
                    score,
                    cost: candidate.cost,
                    risk: candidate.risk,
                    escalation: candidate.escalation,
                    reason: if over_ceiling {
                        format!(
                            "HITL: risco {} > teto {}; affordance={:.2}",
                            candidate.risk.label(),
                            goal.risk_ceiling.label(),
                            candidate.affordance
                        )
                    } else if lin_tag {
                        format!(
                            "affordance={:.2} p_mix={:.2} linucb_ucb={:.2} custo={:.2}ms risco={}",
                            candidate.affordance,
                            p,
                            u,
                            candidate.cost.ms,
                            candidate.risk.label()
                        )
                    } else {
                        format!(
                            "affordance={:.2} p_sucesso={:.2} ucb={:.2} custo={:.2}ms risco={}",
                            candidate.affordance,
                            p,
                            u,
                            candidate.cost.ms,
                            candidate.risk.label()
                        )
                    },
                }
            })
            .collect();
        steps.sort_by(|a, b| b.score.total_cmp(&a.score));
        steps
    }
}

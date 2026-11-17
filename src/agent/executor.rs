//! Executor: orçamento, chamada da tool, verificação e rollback.

use super::registry::ToolRegistry;
use super::tool::{Goal, ToolContext, ToolError, ToolOutcome};
use super::planner::PlanStep;

/// Resultado da execução de um passo.
#[derive(Clone, Debug)]
pub struct Execution {
    /// Saída da tool.
    pub outcome: ToolOutcome,
    /// Critério de sucesso atendido?
    pub met: bool,
    /// Tempo medido.
    pub elapsed_us: u64,
}

/// Executor.
#[derive(Clone, Debug, Default)]
pub struct ToolExecutor;

impl ToolExecutor {
    /// Executa o passo, cobrando orçamento e verificando o goal.
    pub fn execute(
        &mut self,
        registry: &mut ToolRegistry,
        step: &PlanStep,
        goal: &Goal,
        ctx: &mut ToolContext,
    ) -> Result<Execution, ToolError> {
        let cost = step.cost;
        if !ctx.can_spend(cost) {
            return Err(ToolError::Budget(format!(
                "gastaria {:.1}ms/{} tokens; orçamento {:.1}ms/{}",
                ctx.spent_ms + cost.ms,
                ctx.spent_tokens.saturating_add(cost.tokens),
                ctx.budget.max_ms,
                ctx.budget.max_tokens
            )));
        }
        let tool = registry
            .get_mut(step.tool)
            .ok_or_else(|| ToolError::InvalidArgs(format!("tool `{}` sumiu", step.tool)))?;
        let started = std::time::Instant::now();
        let outcome = tool.call(goal, ctx)?;
        ctx.charge(cost);
        let elapsed_us = started.elapsed().as_micros() as u64;
        for effect in &outcome.effects {
            ctx.push_effect(effect.clone());
        }
        let met = goal.success.met(&outcome);
        if !met {
            // Efeitos de um passo que não passou no verificador são desfeitos.
            ctx.rollback();
        }
        Ok(Execution {
            outcome,
            met,
            elapsed_us,
        })
    }
}

//! Executor: orçamento, chamada da tool, verificação e rollback.

use super::planner::PlanStep;
use super::registry::ToolRegistry;
use super::tool::{Goal, ToolContext, ToolError, ToolOutcome};

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
        // SCHEMA_6: risco acima do teto ⇒ HITL **antes** da call.
        if tool.risk() > goal.risk_ceiling {
            return Err(ToolError::NeedsHuman(format!(
                "tool `{}` risco {} > teto {}",
                step.tool,
                tool.risk().label(),
                goal.risk_ceiling.label()
            )));
        }
        // Dry-run obrigatório antes de qualquer efeito (P2 reusa isto).
        tool.dry_run(goal, ctx)?;
        let undo_before = ctx.undo_len();
        let effects_before = ctx.effects.len();
        let claims_before = ctx.claims.len();
        let certificates_before = ctx.certificates.len();
        let started = std::time::Instant::now();
        let outcome = match tool.call(goal, ctx) {
            Ok(outcome) => outcome,
            Err(e) => {
                // Efeitos já registrados antes do erro precisam ser desfeitos.
                let _ = ctx.rewind(undo_before);
                return Err(e);
            }
        };
        ctx.charge(cost);
        let elapsed_us = started.elapsed().as_micros() as u64;
        for effect in &outcome.effects {
            // Irreversível sem teto High: não aplica e pede HITL.
            if matches!(effect, super::tool::Effect::Irreversible { .. })
                && goal.risk_ceiling < super::tool::Risk::High
            {
                let _ = ctx.rewind(undo_before);
                return Err(ToolError::NeedsHuman(format!(
                    "efeito irreversível exige risk_ceiling=high (teto={})",
                    goal.risk_ceiling.label()
                )));
            }
            ctx.push_effect(effect.clone());
        }
        let met = goal.success.met(&outcome);
        if met {
            // Blackboard: o próximo passo pode consumir estas claims.
            ctx.claims.extend(outcome.claims.iter().cloned());
            if let Some(cert) = &outcome.certificate {
                ctx.certificates.push(*cert);
            }
        } else {
            // Rewind LIFO (memória) e truncamento do blackboard.
            ctx.rewind(undo_before)?;
            ctx.rollback_to(effects_before, claims_before, certificates_before);
        }
        Ok(Execution {
            outcome,
            met,
            elapsed_us,
        })
    }
}

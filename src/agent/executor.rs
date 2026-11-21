//! Executor: orçamento, chamada da tool, verificação e rollback.
//!
//! A7: `risk_policy::gate_before_call` (kill switch + HITL High/Medium) roda
//! **antes** do dry_run/call.

use super::planner::PlanStep;
use super::registry::ToolRegistry;
use super::risk_policy;
use super::tool::{Cost, Goal, ToolContext, ToolError, ToolOutcome};

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
        // A7: kill switch + matriz HITL (High always / Medium optional).
        risk_policy::gate_before_call(step.tool, tool.risk(), goal, ctx.trace_id)?;
        // SCHEMA_6: risco acima do teto ⇒ HITL **antes** da call.
        if tool.risk() > goal.risk_ceiling {
            return Err(ToolError::NeedsHuman(format!(
                "tool `{}` risco {} > teto {}",
                step.tool,
                tool.risk().label(),
                goal.risk_ceiling.label()
            )));
        }
        // ADR-014/T5: efeito irreversível **declarado** também barra antes da
        // call. O gate pós-call abaixo continua como backstop para tools que
        // declaram menos do que fazem.
        if goal.risk_ceiling < super::tool::Risk::High
            && tool
                .planned_effects(goal)
                .iter()
                .any(|e| matches!(e, super::tool::Effect::Irreversible { .. }))
        {
            return Err(ToolError::NeedsHuman(format!(
                "tool `{}` declara efeito irreversível; teto={} < high",
                step.tool,
                goal.risk_ceiling.label()
            )));
        }
        // Dry-run obrigatório antes de qualquer efeito (P2 reusa isto).
        tool.dry_run(goal, ctx)?;
        ctx.spent_steps += 1;
        let undo_before = ctx.undo_len();
        let effects_before = ctx.effects.len();
        let claims_before = ctx.claims.len();
        let certificates_before = ctx.certificates.len();
        let verified_before = ctx.verified.len();
        let started = std::time::Instant::now();
        let outcome = match tool.call(goal, ctx) {
            Ok(outcome) => outcome,
            Err(e) => {
                // Efeitos já registrados antes do erro precisam ser desfeitos.
                let report = ctx.rewind_report(undo_before);
                if !report.is_ok() {
                    return Err(ToolError::Failed(format!(
                        "{e}; {} — {}",
                        report.summary(),
                        report.failed.join("; ")
                    )));
                }
                return Err(e);
            }
        };
        let elapsed_us = started.elapsed().as_micros() as u64;
        // Cobra o maior entre estimativa e tempo medido: uma tool lenta não
        // pode furar `max_ms` sem ser contabilizada.
        let measured_ms = (elapsed_us as f32 / 1_000.0).max(outcome.cost_us as f32 / 1_000.0);
        ctx.charge(cost);
        if measured_ms > cost.ms {
            ctx.charge(Cost {
                ms: measured_ms - cost.ms,
                tokens: 0,
            });
        }
        for effect in &outcome.effects {
            // Irreversível sem teto High: desfaz o que der e pede HITL.
            if matches!(effect, super::tool::Effect::Irreversible { .. })
                && goal.risk_ceiling < super::tool::Risk::High
            {
                let report = ctx.rewind_report(undo_before);
                if report.is_ok() {
                    ctx.rollback_to(
                        effects_before,
                        claims_before,
                        certificates_before,
                        verified_before,
                    );
                }
                return Err(match report.failed.first() {
                    Some(why) => ToolError::Failed(format!(
                        "{} — efeito irreversível já ocorreu ({why})",
                        report.summary()
                    )),
                    None => ToolError::NeedsHuman(format!(
                        "efeito irreversível exige risk_ceiling=high (teto={})",
                        goal.risk_ceiling.label()
                    )),
                });
            }
            ctx.push_effect(effect.clone());
        }
        let met = goal.success.met(&outcome);
        // Blackboard: claims de uma tool Ok ficam disponíveis ao próximo passo
        // mesmo quando ainda não satisfazem o goal (ex.: raw→final, MSG 0464/0466).
        ctx.claims.extend(outcome.claims.iter().cloned());
        if let Some(cert) = &outcome.certificate {
            ctx.certificates.push(*cert);
        }
        if !met {
            // Rebobina só efeitos de memória (undo); não apaga claims Ok.
            let report = ctx.rewind_report(undo_before);
            if !report.is_ok() {
                // Compensação falhou: remove o que esta tool acabou de publicar.
                ctx.rollback_to(
                    effects_before,
                    claims_before,
                    certificates_before,
                    verified_before,
                );
                return Err(ToolError::Failed(format!(
                    "{}: {}",
                    report.summary(),
                    report.failed.join("; ")
                )));
            }
            let _ = effects_before;
            let _ = verified_before;
        }
        Ok(Execution {
            outcome,
            met,
            elapsed_us,
        })
    }
}

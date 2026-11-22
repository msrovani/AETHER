//! A8 — `predict.series` (read_local) e `goal.propose` (privileged HITL).

use crate::agent::predict::{forecast_series, parse_series_query, PREDICT_AUTH_CAP};
use crate::agent::tool::{
    outcome_from_expert, Cost, Goal, GoalKind, Risk, SuccessCriteria, Tool, ToolContext, ToolError,
    ToolOutcome, ToolSchema, WorldState, AUTHORITY_QUARANTINE,
};
use crate::agent::{Claim, ExpertId};
use crate::core::active_inference::{ActiveInferenceEngine, Zone};
use crate::core::certificate::Provenance;
use crate::json::Json;

/// Tool de previsão de série (reservatório congelado).
#[derive(Debug, Default)]
pub struct PredictSeriesTool;

impl Tool for PredictSeriesTool {
    fn name(&self) -> &'static str {
        "predict.series"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "predict.series",
            description: "forecast 1-passo (+H≤5) via reservoir; OOD/metacog → abstain",
            args: &["series", "horizon"],
            kinds: &[GoalKind::Answer, GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(8.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("predict.series") {
            return 1.0;
        }
        if goal
            .query
            .as_deref()
            .is_some_and(|q| q.contains("\"series\""))
        {
            return 0.95;
        }
        0.0
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
            && goal.query.as_deref().is_some_and(|q| !q.trim().is_empty())
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let q = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("query ausente".into()))?;
        parse_series_query(q).map_err(ToolError::InvalidArgs)?;
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let q = goal.query.as_deref().unwrap();
        let (series, horizon) = parse_series_query(q).map_err(ToolError::InvalidArgs)?;
        let report = forecast_series(&series, horizon);
        let mut outcome = outcome_from_expert(ExpertId::Predict, report.to_expert_output());
        if report.abstain.is_some() {
            outcome.detail = format!(
                "{} scope={:?} trace={} cap={PREDICT_AUTH_CAP}",
                outcome.detail, ctx.scope, ctx.trace_id
            );
        }
        Ok(outcome)
    }
}

/// Nomes das zonas EFE → proposta de próximo goal (nunca executa).
const PROPOSE_ZONES: &[&str] = &["answer.fact", "retrieve.memory", "transform.qubo", "verify"];

/// `goal.propose` — metas endógenas via EFE; High HITL; host decide.
#[derive(Debug, Default)]
pub struct GoalProposeTool;

impl Tool for GoalProposeTool {
    fn name(&self) -> &'static str {
        "goal.propose"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "goal.propose",
            description: "propõe próximo Goal via ActiveInference EFE; nunca executa; HITL always",
            args: &["uncertainty", "hitl_token"],
            kinds: &[GoalKind::Answer, GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::High
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("goal.propose") {
            1.0
        } else if goal
            .query
            .as_deref()
            .is_some_and(|q| q.contains("goal.propose") || q.contains("\"propose\""))
        {
            0.9
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let uncertainty = goal
            .query
            .as_deref()
            .and_then(|q| Json::parse(q.trim()).ok())
            .and_then(|j| {
                j.get("uncertainty")
                    .and_then(|u| u.as_f64())
                    .map(|f| f as f32)
            })
            .unwrap_or(0.35)
            .clamp(0.01, 2.0);

        let zones = vec![
            Zone {
                mean: 0.0,
                var: uncertainty,
                cost: 0.2,
            },
            Zone {
                mean: 0.0,
                var: uncertainty * 0.8,
                cost: 0.4,
            },
            Zone {
                mean: 0.0,
                var: uncertainty * 1.2,
                cost: 0.9,
            },
            Zone {
                mean: 0.0,
                var: uncertainty * 0.5,
                cost: 0.3,
            },
        ];
        let engine = ActiveInferenceEngine::new(zones, 0.5, 0.05)
            .map_err(|e| ToolError::Failed(format!("active_inference: {e}")))?;
        let idx = engine.select();
        let label = PROPOSE_ZONES.get(idx).copied().unwrap_or("answer.fact");
        let g = engine.expected_free_energy(idx);
        let proposed = Json::obj([
            ("kind", Json::str(label)),
            ("zone", Json::num(idx as f64)),
            ("efe", Json::num(f64::from(g))),
            ("uncertainty", Json::num(f64::from(uncertainty))),
            ("execute", Json::Bool(false)),
        ]);
        let auth = AUTHORITY_QUARANTINE.min(PREDICT_AUTH_CAP);
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "propose",
                "goal",
                proposed.to_text(),
                0.6,
                auth,
                ExpertId::Predict,
                Provenance::Inferred,
            )],
            confidence: 0.6,
            detail: format!(
                "goal.propose zone={label} efe={g:.4} execute=false scope={:?} trace={}",
                ctx.scope, ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

impl Goal {
    /// Goal de previsão de série (`predict.series`).
    pub fn predict_series(series: &[f32], horizon: usize) -> Self {
        let arr: Vec<Json> = series.iter().map(|x| Json::num(f64::from(*x))).collect();
        let query = Json::obj([
            ("series", Json::Arr(arr)),
            ("horizon", Json::num(horizon.clamp(1, 5) as f64)),
        ]);
        Self {
            id: 0,
            kind: GoalKind::Answer,
            subject: None,
            relation: Some("predict.series".into()),
            query: Some(query.to_text()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: "series".into(),
                relation: "predict.yhat".into(),
                value: None,
            },
            budget: Default::default(),
            risk_ceiling: Risk::Low,
            pipeline: Vec::new(),
        }
    }

    /// Proposta endógena de goal (High HITL; não executa — só cite).
    pub fn goal_propose(uncertainty: f32) -> Self {
        let query = Json::obj([
            ("propose", Json::Bool(true)),
            ("uncertainty", Json::num(f64::from(uncertainty))),
        ]);
        Self {
            id: 0,
            kind: GoalKind::Answer,
            subject: None,
            relation: Some("goal.propose".into()),
            query: Some(query.to_text()),
            n: None,
            clauses: Vec::new(),
            j: Vec::new(),
            h: Vec::new(),
            path: None,
            url: None,
            mcp_name: None,
            success: SuccessCriteria::Claim {
                subject: "propose".into(),
                relation: "goal".into(),
                value: None,
            },
            budget: Default::default(),
            risk_ceiling: Risk::High,
            pipeline: Vec::new(),
        }
    }
}

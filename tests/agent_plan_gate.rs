//! P2 — workflow multi-passo, blackboard e rollback.
//!
//! G-A2: pipeline de 3 tools com dependência (`fetch → double → verify`)
//! sucede; falha no passo intermediário desfaz o workflow.
//! G-A3: goal OOD sem candidato abstém (não inventa).

#![cfg(feature = "agent")]

use aether_engine::agent::{
    Budget, Claim, Cost, Goal, GoalKind, Risk, SelfModel, SuccessCriteria, Tool, ToolAgent,
    ToolContext, ToolError, ToolOutcome, ToolRegistry, ToolSchema, WorldState,
};
use aether_engine::core::certificate::Provenance;

struct ChainTool {
    name: &'static str,
    kind: GoalKind,
    needs: Option<&'static str>,
    value: &'static str,
    relation: &'static str,
}

impl Tool for ChainTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: "chain mock",
            args: &["subject"],
            kinds: kinds(self.kind),
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == self.kind {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        if let Some(need) = self.needs {
            if !ctx.claims.iter().any(|claim| claim.value == need) {
                return Ok(ToolOutcome::empty(format!("pré-requisito {need} ausente")));
            }
        }
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                self.relation,
                self.value,
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            confidence: 1.0,
            ..ToolOutcome::default()
        })
    }
}

fn kinds(kind: GoalKind) -> &'static [GoalKind] {
    const ANSWER: [GoalKind; 1] = [GoalKind::Answer];
    const TRANSFORM: [GoalKind; 1] = [GoalKind::Transform];
    const VERIFY: [GoalKind; 1] = [GoalKind::Verify];
    const RETRIEVE: [GoalKind; 1] = [GoalKind::Retrieve];
    const ACT: [GoalKind; 1] = [GoalKind::Act];
    match kind {
        GoalKind::Answer => &ANSWER,
        GoalKind::Transform => &TRANSFORM,
        GoalKind::Verify => &VERIFY,
        GoalKind::Retrieve => &RETRIEVE,
        GoalKind::Act => &ACT,
    }
}

fn registry(tools: Vec<ChainTool>) -> ToolRegistry {
    let names: Vec<&'static str> = tools.iter().map(|tool| tool.name).collect();
    let mut registry = ToolRegistry::new(names);
    for tool in tools {
        registry.register(Box::new(tool)).expect("registro");
    }
    registry
}

fn goal(kind: GoalKind, relation: &str, success: SuccessCriteria) -> Goal {
    let mut goal = Goal::fact("x", relation);
    goal.kind = kind;
    goal.success = success;
    goal.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

fn pipeline(fetch_value: &'static str) -> Goal {
    let fetch = goal(
        GoalKind::Answer,
        "value",
        SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "value".into(),
            value: Some(fetch_value.into()),
        },
    );
    let double = goal(
        GoalKind::Transform,
        "double",
        SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "double".into(),
            value: Some("84".into()),
        },
    );
    let verify = goal(GoalKind::Verify, "verified", SuccessCriteria::Any);
    Goal::pipeline(vec![fetch, double, verify]).with_budget(
        Budget {
            max_steps: 6,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

#[test]
fn gate_a2_workflow_encadeia_e_verifica() {
    let registry = registry(vec![
        ChainTool {
            name: "fetch",
            kind: GoalKind::Answer,
            needs: None,
            value: "42",
            relation: "value",
        },
        ChainTool {
            name: "double",
            kind: GoalKind::Transform,
            needs: Some("42"),
            value: "84",
            relation: "double",
        },
        ChainTool {
            name: "verify",
            kind: GoalKind::Verify,
            needs: Some("84"),
            value: "ok",
            relation: "verified",
        },
    ]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&pipeline("42"));
    assert!(outcome.success, "{:?}", outcome.rationale);
    assert_eq!(outcome.steps.len(), 3, "{:?}", outcome.steps);
    let tools: Vec<&str> = outcome
        .steps
        .iter()
        .map(|step| step.tool.as_str())
        .collect();
    assert_eq!(tools, ["fetch", "double", "verify"]);
    assert_eq!(outcome.claims.len(), 3);
    assert_eq!(outcome.trace.answer.as_deref(), Some("ok"));
}

#[test]
fn gate_a2_falha_no_meio_desfaz_workflow() {
    let registry = registry(vec![
        ChainTool {
            name: "fetch",
            kind: GoalKind::Answer,
            needs: None,
            value: "41",
            relation: "value",
        },
        ChainTool {
            name: "double",
            kind: GoalKind::Transform,
            needs: Some("42"),
            value: "84",
            relation: "double",
        },
    ]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&pipeline("41"));
    assert!(!outcome.success);
    assert_eq!(outcome.action, aether_engine::agent::Action::Abstain);
    assert_eq!(outcome.steps.len(), 2);
    assert!(outcome.rationale.contains("passo 2"));
    assert!(
        outcome.claims.is_empty(),
        "rollback não limpou o blackboard"
    );
}

#[test]
fn gate_a3_ood_abstem_sem_candidato() {
    let registry = registry(vec![ChainTool {
        name: "fetch",
        kind: GoalKind::Answer,
        needs: None,
        value: "42",
        relation: "value",
    }]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    for _ in 0..20 {
        let outcome = agent.solve(&Goal::memory("fora do domínio"));
        assert!(!outcome.success);
        assert_eq!(outcome.action, aether_engine::agent::Action::Abstain);
        assert!(outcome.steps.is_empty());
    }
}

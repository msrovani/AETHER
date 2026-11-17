//! P0/P1 — contratos de tool, registry, planner, executor e G-A1.
//!
//! G-A1: em 50 episódios, a primeira ferramenta escolhida é a correta em
//! ≥ 90% dos casos, com exploração UCB e feedback verificado.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    AgentConfig, Budget, Claim, Cost, Goal, GoalKind, Risk, SelfModel, Tool, ToolAgent,
    ToolContext, ToolError, ToolOutcome, ToolRegistry, ToolSchema, WorldState,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::logic_pipeline::Clause3;

struct MockTool {
    name: &'static str,
    success: bool,
}

impl Tool for MockTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: "mock",
            args: &["subject", "relation"],
            kinds: &[GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Answer {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        if self.success {
            Ok(ToolOutcome {
                claims: vec![Claim::new(
                    "Brasil",
                    "capital",
                    "Brasília",
                    1.0,
                    200,
                    aether_engine::agent::ExpertId::Transplant,
                    Provenance::Transplanted,
                )],
                confidence: 1.0,
                ..ToolOutcome::default()
            })
        } else {
            Ok(ToolOutcome::empty("mock falhou"))
        }
    }
}

fn mock_registry(order: [(&'static str, bool); 2]) -> ToolRegistry {
    let mut registry = ToolRegistry::new(vec!["good", "bad"]);
    for (name, success) in order {
        registry
            .register(Box::new(MockTool { name, success }))
            .expect("registro");
    }
    registry
}

#[test]
fn gate_a1_planner_aprende_ferramenta_certa() {
    let goal = Goal::fact("Brasil", "capital");
    let state = WorldState::default();
    let planner = aether_engine::agent::Planner::default();
    for seed in 0..5u64 {
        // Alterna a ordem para não vencer por acaso do registry.
        let registry = if seed % 2 == 0 {
            mock_registry([("good", true), ("bad", false)])
        } else {
            mock_registry([("bad", false), ("good", true)])
        };
        let mut model = SelfModel::new(None);
        let mut correct = 0usize;
        for _ in 0..50 {
            let step = planner
                .select(&goal, &state, &registry, &model, &[])
                .expect("candidato");
            let ok = step.tool == "good";
            if ok {
                correct += 1;
            }
            model.update(step.tool, &goal, ok);
        }
        assert!(
            correct >= 45,
            "seed {seed}: {correct}/50 escolhas certas (<90%)"
        );
        assert!(model.success("good", &goal) > model.success("bad", &goal));
    }
}

#[test]
fn p0_registry_rejeita_allowlist_e_duplicata() {
    let mut registry = ToolRegistry::new(vec!["good"]);
    assert!(registry
        .register(Box::new(MockTool {
            name: "bad",
            success: true
        }))
        .is_err());
    assert!(registry
        .register(Box::new(MockTool {
            name: "good",
            success: true
        }))
        .is_ok());
    assert!(registry
        .register(Box::new(MockTool {
            name: "good",
            success: true
        }))
        .is_err());
}

#[test]
fn p0_executor_respeita_orcamento() {
    let registry = mock_registry([("good", true), ("bad", false)]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let goal = Goal::fact("Brasil", "capital").with_budget(
        Budget {
            max_steps: 1,
            max_ms: 0,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let outcome = agent.solve(&goal);
    assert!(!outcome.success);
    assert!(!outcome.steps.is_empty());
    assert!(outcome.steps[0].detail.contains("orçamento"));
}

#[test]
fn p1_tool_agent_responde_fato_e_sat_e_qubo() {
    let mut agent = ToolAgent::new(&AgentConfig {
        facts_path: "data/agent_facts.jsonl".into(),
        ..AgentConfig::default()
    });

    let fact = agent.solve(&Goal::fact("AETHER", "proibe"));
    assert!(fact.success, "{:?}", fact.rationale);
    assert_eq!(fact.outcome.unwrap().first_claim().unwrap().value, "backpropagation");

    let sat = agent.solve(&Goal::satisfy(
        3,
        vec![Clause3::new2(0, 1, 1, 1), Clause3::new2(0, -1, 2, 1)],
    ));
    assert!(sat.success, "{:?}", sat.rationale);

    let n = 4;
    let qubo = agent.solve(&Goal::optimize(n, vec![0.0; n * n], vec![1.0; n]));
    assert!(qubo.success, "{:?}", qubo.rationale);
    assert_eq!(qubo.outcome.unwrap().first_claim().unwrap().value, "++++");

    // Tools aparecem no registry.
    assert!(agent.registry().contains("fact"));
    assert!(agent.registry().contains("sat"));
    assert!(agent.registry().contains("qubo"));
}

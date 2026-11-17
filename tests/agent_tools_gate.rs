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
fn gate_a3_efe_abstem_quando_custo_domina() {
    struct ExpensiveTool;
    impl Tool for ExpensiveTool {
        fn name(&self) -> &'static str {
            "expensive"
        }
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: "expensive",
                description: "caro",
                args: &["subject", "relation"],
                kinds: &[GoalKind::Answer],
            }
        }
        fn cost(&self) -> Cost {
            Cost::local(10_000.0)
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
                ..ToolOutcome::default()
            })
        }
    }

    let mut registry = ToolRegistry::new(vec!["expensive"]);
    registry.register(Box::new(ExpensiveTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let goal = Goal::fact("Brasil", "capital");
    let ranked = agent.plan(&goal);
    assert!(!ranked.is_empty());
    assert!(
        ranked[0].score < 0.05,
        "score deveria ficar abaixo do piso: {}",
        ranked[0].score
    );
    let outcome = agent.solve(&goal);
    assert!(!outcome.success);
    assert_eq!(outcome.action, aether_engine::agent::Action::Abstain);
    assert!(outcome.steps.is_empty());
}

#[test]
fn gate_a1_planner_aprende_ferramenta_certa() {
    let goal = Goal::fact("Brasil", "capital");
    let state = WorldState::default();
    let planner = aether_engine::agent::Planner::default();
    // BUFFY (MSG 0267): n >= 200 e limite inferior do IC95 >= 85%, por seed.
    let n = 200usize;
    for seed in 0..5u64 {
        let registry = if seed % 2 == 0 {
            mock_registry([("good", true), ("bad", false)])
        } else {
            mock_registry([("bad", false), ("good", true)])
        };
        let mut model = SelfModel::new(None);
        let mut correct = 0usize;
        let mut wrong = 0usize;
        for _ in 0..n {
            let step = planner
                .select(&goal, &state, &registry, &model, &[])
                .expect("candidato");
            let ok = step.tool == "good";
            if ok {
                correct += 1;
            } else {
                wrong += 1;
            }
            model.update(step.tool, &goal, ok);
        }
        assert_eq!(correct + wrong, n, "matriz de confusão incompleta");
        let p = correct as f64 / n as f64;
        let z = 1.96f64;
        let denom = 1.0 + z * z / n as f64;
        let center = (p + z * z / (2.0 * n as f64)) / denom;
        let margin = z
            * ((p * (1.0 - p) / n as f64) + (z * z / (4.0 * (n as f64) * (n as f64)))).sqrt()
            / denom;
        let lower = center - margin;
        assert!(
            lower >= 0.85,
            "seed {seed}: {correct}/{n} (p={p:.3}, IC95 inferior={lower:.3} < 0.85)"
        );
        assert!(
            correct * 10 >= n * 9,
            "seed {seed}: piso por seed < 90% ({correct}/{n})"
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
    assert_eq!(
        fact.outcome.unwrap().first_claim().unwrap().value,
        "backpropagation"
    );

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

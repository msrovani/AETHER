//! A6 / P4 — LinUCB no ToolAgent + learner verificado (G-A5 produto).
//!
//! Complementa `agent_learn_gate` (bandit isolado) e `agent_self_model_gate`
//! (ECE/temperatura): aqui o caminho opt-in entra no planner e só treina com
//! `Feedback.correct`.

#![cfg(feature = "agent")]

use std::path::PathBuf;
use std::sync::Mutex;

use aether_engine::agent::{
    goal_features, linucb_enabled, Budget, Claim, ContextualTools, EpisodeRecord, ExpertId,
    Feedback, Goal, GoalKind, Learner, Risk, SelfModel, SuccessCriteria, Tool, ToolAgent,
    ToolContext, ToolError, ToolOutcome, ToolRegistry, ToolSchema, WorldState, LINUCB_DIM,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::knowledge_bridge_f1::ScopeId;

static LINUCB_ENV: Mutex<()> = Mutex::new(());

fn registry_two_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new(vec!["good.tool", "bad.tool"]);
    struct Good;
    impl Tool for Good {
        fn name(&self) -> &'static str {
            "good.tool"
        }
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: "good.tool",
                description: "boa",
                args: &["query"],
                kinds: &[GoalKind::Answer],
            }
        }
        fn cost(&self) -> aether_engine::agent::Cost {
            aether_engine::agent::Cost::local(1.0)
        }
        fn risk(&self) -> Risk {
            Risk::Low
        }
        fn affordance(&self, goal: &Goal, _: &WorldState) -> f32 {
            if goal.kind == GoalKind::Answer {
                0.6
            } else {
                0.0
            }
        }
        fn preconditions(&self, g: &Goal, s: &WorldState) -> bool {
            self.affordance(g, s) > 0.0
        }
        fn call(&mut self, _: &Goal, _: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
            Ok(ToolOutcome {
                claims: vec![Claim::new(
                    "x",
                    "y",
                    "ok",
                    1.0,
                    96,
                    ExpertId::Memory,
                    Provenance::Inferred,
                )],
                confidence: 1.0,
                detail: "good".into(),
                ..ToolOutcome::default()
            })
        }
    }
    struct Bad;
    impl Tool for Bad {
        fn name(&self) -> &'static str {
            "bad.tool"
        }
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: "bad.tool",
                description: "ruim",
                args: &["query"],
                kinds: &[GoalKind::Answer],
            }
        }
        fn cost(&self) -> aether_engine::agent::Cost {
            aether_engine::agent::Cost::local(1.0)
        }
        fn risk(&self) -> Risk {
            Risk::Low
        }
        fn affordance(&self, goal: &Goal, _: &WorldState) -> f32 {
            if goal.kind == GoalKind::Answer {
                0.6
            } else {
                0.0
            }
        }
        fn preconditions(&self, g: &Goal, s: &WorldState) -> bool {
            self.affordance(g, s) > 0.0
        }
        fn call(&mut self, _: &Goal, _: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
            Ok(ToolOutcome {
                claims: vec![Claim::new(
                    "x",
                    "y",
                    "bad",
                    0.2,
                    60,
                    ExpertId::Memory,
                    Provenance::Inferred,
                )],
                confidence: 0.2,
                detail: "bad".into(),
                ..ToolOutcome::default()
            })
        }
    }
    registry.register(Box::new(Good)).unwrap();
    registry.register(Box::new(Bad)).unwrap();
    registry
}

fn answer_goal() -> Goal {
    let mut g = Goal::fact("x", "y");
    g.kind = GoalKind::Answer;
    g.budget = Budget {
        max_steps: 2,
        max_ms: 5_000,
        max_tokens: 0,
    };
    g.risk_ceiling = Risk::Low;
    g.success = SuccessCriteria::NonEmpty;
    g
}

#[test]
fn goal_features_dim_estavel() {
    let f = goal_features(&answer_goal());
    assert_eq!(f.len(), LINUCB_DIM);
    assert!(f.iter().all(|v| v.is_finite()));
}

#[test]
fn linucb_off_por_default() {
    let _lock = LINUCB_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_LINUCB");
    assert!(!linucb_enabled());
    let agent = ToolAgent::with_registry(
        registry_two_tools(),
        SelfModel::new(None),
        ScopeId::new(1, 0, 1),
    );
    assert!(!agent.linucb_active());
}

#[test]
fn linucb_aprende_so_com_feedback_verificado() {
    let _lock = LINUCB_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_LINUCB");

    let path = PathBuf::from("target/aether_a6_learner.jsonl");
    let _ = std::fs::remove_file(&path);
    let mut learner = Learner::with_path(&path);
    let goal = answer_goal();

    // Journal: good recompensado, bad punido (só verified).
    for _ in 0..40 {
        assert!(learner.record(EpisodeRecord::verified("good.tool", &goal, true)));
        assert!(learner.record(EpisodeRecord::verified("bad.tool", &goal, false)));
    }
    assert_eq!(learner.verified_count(), 80);

    let agent = ToolAgent::with_registry(
        registry_two_tools(),
        SelfModel::new(None),
        ScopeId::new(1, 0, 1),
    )
    .with_learner(learner)
    .with_linucb(true);
    assert!(agent.linucb_active());

    let plan = agent.plan(&goal);
    assert_eq!(
        plan.first().map(|p| p.tool),
        Some("good.tool"),
        "após replay LinUCB deve preferir good: {plan:?}"
    );
    assert!(
        plan.first().unwrap().reason.contains("linucb")
            || plan.first().unwrap().reason.contains("p_mix"),
        "{}",
        plan.first().unwrap().reason
    );

    // learn() sem correct não grava.
    let mut agent_live = ToolAgent::with_registry(
        registry_two_tools(),
        SelfModel::new(None),
        ScopeId::new(1, 0, 1),
    )
    .with_learner(Learner::memory())
    .with_linucb(true);
    let out = agent_live.solve(&goal);
    agent_live.learn(
        &out,
        &Feedback {
            correct: None,
            expected: None,
        },
    );
    assert_eq!(agent_live.learner().verified_count(), 0);

    // learn() com correct grava.
    agent_live.learn(
        &out,
        &Feedback {
            correct: Some(out.steps.first().is_some_and(|s| s.tool == "good.tool")),
            expected: None,
        },
    );
    assert_eq!(agent_live.learner().verified_count(), 1);

    // Replay de arquivo em agent novo.
    let learner2 = Learner::with_path(&path);
    assert_eq!(learner2.verified_count(), 80);
    let agent2 = ToolAgent::with_registry(
        registry_two_tools(),
        SelfModel::new(None),
        ScopeId::new(1, 0, 1),
    )
    .with_learner(learner2)
    .with_linucb(true);
    assert_eq!(
        agent2.plan(&goal).first().map(|p| p.tool),
        Some("good.tool")
    );
}

#[test]
fn episode_nao_verificado_nao_entra_no_learner() {
    let mut learner = Learner::memory();
    let mut ep = EpisodeRecord::verified("t", &answer_goal(), true);
    ep.verified = false;
    assert!(!learner.record(ep));
    assert_eq!(learner.verified_count(), 0);

    let mut ctx = ContextualTools::new(vec!["t"], 0.4);
    assert_eq!(learner.replay(&mut ctx), 0);
}

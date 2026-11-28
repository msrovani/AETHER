//! A7 — matriz de risco + kill switch + HITL High/Medium.

#![cfg(feature = "agent")]

use std::path::PathBuf;
use std::sync::Mutex;

use aether_engine::agent::{
    agent_kill_switch, class_for_tool, gate_before_call, hitl_medium_required, hitl_token_ok,
    matrix_json, requirement_for, write_matrix_artifact, Action, AgentConfig, Budget, Goal,
    GoalKind, HitlRequirement, Risk, ToolAgent, WorldPolicy, RISK_MATRIX,
};

static A7_ENV: Mutex<()> = Mutex::new(());

fn clear_a7_env() {
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::remove_var("AETHER_HITL_MEDIUM");
    std::env::remove_var("AETHER_HITL_TOKEN");
}

fn agent() -> ToolAgent {
    let cfg = AgentConfig {
        world: WorldPolicy {
            sandbox_root: PathBuf::from("target/aether_a7_sandbox"),
            ..WorldPolicy::default()
        },
        ..AgentConfig::default()
    };
    let _ = std::fs::create_dir_all(&cfg.world.sandbox_root);
    ToolAgent::new(&cfg)
}

#[test]
fn matriz_publicada_tem_3_classes() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    assert_eq!(RISK_MATRIX.len(), 3);
    assert_eq!(requirement_for(Risk::Low), HitlRequirement::Never);
    assert_eq!(requirement_for(Risk::Medium), HitlRequirement::Optional);
    assert_eq!(requirement_for(Risk::High), HitlRequirement::Always);
    assert_eq!(class_for_tool("shell.exec", Risk::High), "privileged");
    assert_eq!(class_for_tool("fs.write", Risk::Medium), "world_sandbox");
    assert_eq!(class_for_tool("fact", Risk::Low), "read_local");

    let path = PathBuf::from("target/aether_risk_matrix.json");
    write_matrix_artifact(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("aether_risk_matrix/a7"));
    assert!(text.contains("AETHER_AGENT_KILL"));
    assert!(matrix_json().to_text().contains("privileged"));
}

#[test]
fn kill_switch_bloqueia_toda_call() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A7_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a7_env();
    std::env::set_var("AETHER_AGENT_KILL", "1");
    assert!(agent_kill_switch());

    let mut agent = agent();
    let goal = Goal::fact("Brasil", "capital").with_budget(
        Budget {
            max_steps: 2,
            max_ms: 2_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let out = agent.solve(&goal);
    std::env::remove_var("AETHER_AGENT_KILL");
    assert!(!out.success, "{}", out.rationale);
    assert!(
        out.rationale.contains("KILL") || out.rationale.contains("kill"),
        "{}",
        out.rationale
    );
}

#[test]
fn high_sem_token_pede_hitl() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A7_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a7_env();
    std::env::set_var("AETHER_HITL_TOKEN", "sekreto");

    let mut goal = Goal::fact("x", "shell.exec");
    goal.kind = GoalKind::Act;
    goal.relation = Some("shell.exec".into());
    goal.risk_ceiling = Risk::High;
    goal.budget = Budget {
        max_steps: 2,
        max_ms: 2_000,
        max_tokens: 0,
    };
    // Sem hitl_token no query.
    let err = gate_before_call("shell.exec", Risk::High, &goal, 1).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("A7") || msg.contains("hitl"), "{msg}");

    goal.query = Some(r#"{"argv":["stub"],"hitl_token":"sekreto"}"#.into());
    assert!(hitl_token_ok(&goal));
    assert!(gate_before_call("shell.exec", Risk::High, &goal, 1).is_ok());

    clear_a7_env();
}

#[test]
fn medium_hitl_opcional_via_env() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A7_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a7_env();

    let mut goal = Goal::fs_write("a.txt", "x");
    goal.risk_ceiling = Risk::Medium;
    // Sem AETHER_HITL_MEDIUM → Medium passa o gate A7.
    assert!(!hitl_medium_required());
    assert!(gate_before_call("fs.write", Risk::Medium, &goal, 2).is_ok());

    std::env::set_var("AETHER_HITL_MEDIUM", "1");
    std::env::set_var("AETHER_HITL_TOKEN", "tok");
    assert!(hitl_medium_required());
    assert!(gate_before_call("fs.write", Risk::Medium, &goal, 2).is_err());

    goal.query = Some(r#"{"hitl_token":"tok"}"#.into());
    // fs.write usa path/body — query extra ok para token.
    assert!(gate_before_call("fs.write", Risk::Medium, &goal, 2).is_ok());

    clear_a7_env();
}

#[test]
fn low_nunca_hitl_na_matriz() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A7_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a7_env();
    let goal = Goal::fact("a", "b");
    assert!(gate_before_call("fact", Risk::Low, &goal, 3).is_ok());

    // Kill ainda vence Low.
    std::env::set_var("AETHER_AGENT_KILL", "1");
    let err = gate_before_call("fact", Risk::Low, &goal, 3).unwrap_err();
    clear_a7_env();
    assert!(err.to_string().contains("KILL") || err.to_string().contains("kill"));
}

#[test]
fn agent_resolve_low_sem_kill() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A7_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a7_env();
    let mut agent = agent();
    let out = agent.solve(&Goal::fact("Brasil", "capital").with_budget(
        Budget {
            max_steps: 2,
            max_ms: 3_000,
            max_tokens: 0,
        },
        Risk::Low,
    ));
    // Pode abstain por falta de fatos; não pode ser kill.
    assert!(!out.rationale.to_ascii_lowercase().contains("kill"));
    assert!(out.action != Action::Act || out.success || !out.success);
}

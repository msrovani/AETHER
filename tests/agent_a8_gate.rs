//! A8 / P5 — fio de produto: predict.series + metacog + goal.propose HITL.
//!
//! Gates falsificáveis (números no artefato `target/aether_a8_gate.json`):
//! 1. série MG → claim `predict.yhat` e NRMSE < 0.15;
//! 2. OOD (3 pontos / lixo) → 0 claims inventadas; rationale ood/abstain;
//! 3. metacog: surpresa alta → abstain;
//! 4. `goal.propose` sem token → HITL; com token → proposta sem Act automático;
//! 5. `enter_v1_default = false`.

#![cfg(feature = "agent")]

use std::path::PathBuf;
use std::sync::Mutex;

use aether_engine::agent::{
    class_for_tool, forecast_series, gate_before_call, metacog_should_abstain, AgentConfig, Budget,
    Goal, Risk, ToolAgent, WorldPolicy, NRMSE_OOD,
};
use aether_engine::core::reservoir::mackey_glass;
use aether_engine::json::Json;

static A8_ENV: Mutex<()> = Mutex::new(());

fn clear_a8_env() {
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::remove_var("AETHER_HITL_MEDIUM");
    std::env::remove_var("AETHER_HITL_TOKEN");
}

fn agent() -> ToolAgent {
    let cfg = AgentConfig {
        world: WorldPolicy {
            sandbox_root: PathBuf::from("target/aether_a8_sandbox"),
            ..WorldPolicy::default()
        },
        ..AgentConfig::default()
    };
    let _ = std::fs::create_dir_all(&cfg.world.sandbox_root);
    ToolAgent::new(&cfg)
}

fn write_artifact(
    mg_nrmse: f32,
    ood_empty: bool,
    metacog_high: bool,
    metacog_low: bool,
    propose_hitl: bool,
    propose_ok: bool,
) {
    let doc = Json::obj([
        ("schema", Json::str("aether_a8_gate/a8")),
        ("measured_at", Json::str("2026-11-21")),
        ("enter_v1_default", Json::Bool(false)),
        ("predict_series", Json::str("read_local")),
        ("goal_propose", Json::str("privileged")),
        (
            "mg",
            Json::obj([
                ("nrmse", Json::num(f64::from(mg_nrmse))),
                ("nrmse_ood", Json::num(f64::from(NRMSE_OOD))),
                ("pass", Json::Bool(mg_nrmse < NRMSE_OOD)),
            ]),
        ),
        (
            "ood",
            Json::obj([
                ("empty_claims", Json::Bool(ood_empty)),
                ("pass", Json::Bool(ood_empty)),
            ]),
        ),
        (
            "metacog",
            Json::obj([
                ("high_abstain", Json::Bool(metacog_high)),
                ("low_ok", Json::Bool(metacog_low)),
                ("pass", Json::Bool(metacog_high && metacog_low)),
            ]),
        ),
        (
            "goal_propose_hitl",
            Json::obj([
                ("without_token_blocked", Json::Bool(propose_hitl)),
                ("with_token_proposes", Json::Bool(propose_ok)),
                ("pass", Json::Bool(propose_hitl && propose_ok)),
            ]),
        ),
        (
            "matrix",
            Json::obj([
                (
                    "predict_series_class",
                    Json::str(class_for_tool("predict.series", Risk::Low)),
                ),
                (
                    "goal_propose_class",
                    Json::str(class_for_tool("goal.propose", Risk::High)),
                ),
            ]),
        ),
    ]);
    let path = PathBuf::from("target/aether_a8_gate.json");
    let _ = std::fs::create_dir_all("target");
    std::fs::write(&path, doc.to_text()).expect("write a8 artifact");
}

#[test]
fn predict_mg_emite_yhat() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let series = mackey_glass(500, 17, 0.1);
    let mut agent = agent();
    let goal = Goal::predict_series(&series, 1).with_budget(
        Budget {
            max_steps: 3,
            max_ms: 30_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let out = agent.solve(&goal);
    println!(
        "[A8] MG success={} action={} rationale={}",
        out.success,
        out.action.label(),
        out.rationale
    );
    assert!(
        out.claims
            .iter()
            .any(|c| c.relation == "predict.yhat" && !c.value.is_empty()),
        "sem predict.yhat: {:?}",
        out.claims
    );
    let nrmse = out
        .claims
        .iter()
        .find(|c| c.relation == "predict.nrmse")
        .and_then(|c| c.value.parse::<f32>().ok())
        .unwrap_or(f32::INFINITY);
    assert!(nrmse < NRMSE_OOD, "nrmse={nrmse}");
}

#[test]
fn ood_serie_curta_nao_inventa() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let mut agent = agent();
    let goal = Goal::predict_series(&[1.0, 2.0, 3.0], 1).with_budget(
        Budget {
            max_steps: 2,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let out = agent.solve(&goal);
    let invented = out.claims.iter().any(|c| c.relation == "predict.yhat");
    assert!(!invented, "false-act yhat: {:?}", out.claims);
    let why = out.rationale.to_ascii_lowercase();
    assert!(
        why.contains("ood") || why.contains("abstain") || !out.success,
        "{}",
        out.rationale
    );
}

#[test]
fn metacog_surpresa() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    assert!(metacog_should_abstain(0.12));
    assert!(!metacog_should_abstain(0.001));
    let noise: Vec<f32> = (0..80).map(|i| ((i * 17) % 97) as f32 / 97.0).collect();
    let rep = forecast_series(&noise, 1);
    // Ruído pode ser ood ou abstain — nunca yhat inventado sem report.
    if rep.abstain.is_some() {
        assert!(rep.yhat.is_empty());
    }
}

#[test]
fn goal_propose_exige_hitl() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let _lock = A8_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a8_env();
    std::env::set_var("AETHER_HITL_TOKEN", "a8tok");

    let goal = Goal::goal_propose(0.4);
    let err = gate_before_call("goal.propose", Risk::High, &goal, 9).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("A7") || msg.contains("hitl") || msg.contains("HITL"),
        "{msg}"
    );

    let mut with_tok = goal.clone();
    with_tok.query = Some(r#"{"propose":true,"uncertainty":0.4,"hitl_token":"a8tok"}"#.into());
    assert!(gate_before_call("goal.propose", Risk::High, &with_tok, 9).is_ok());

    let mut agent = agent();
    with_tok.budget = Budget {
        max_steps: 3,
        max_ms: 5_000,
        max_tokens: 0,
    };
    let out = agent.solve(&with_tok);
    clear_a8_env();

    let proposal = out
        .claims
        .iter()
        .find(|c| c.relation == "goal")
        .map(|c| c.value.as_str())
        .unwrap_or("");
    assert!(
        proposal.contains("\"execute\":false"),
        "sem proposta execute=false: {} / {:?}",
        out.rationale,
        out.claims
    );
    // Host decide — claim nunca pede execute=true; pipeline do goal continua vazio.
    assert!(!proposal.contains("\"execute\":true"));
    assert!(with_tok.pipeline.is_empty());
}

#[test]
fn artefato_a8_gate() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let series = mackey_glass(400, 17, 0.1);
    let mg = forecast_series(&series, 1);
    let mg_nrmse = mg.nrmse;
    assert!(mg.abstain.is_none(), "{:?}", mg.abstain);
    assert!(mg_nrmse < NRMSE_OOD);

    let ood = forecast_series(&[0.0, 1.0, -1.0], 1);
    let ood_empty = ood.yhat.is_empty() && ood.abstain.is_some();

    let metacog_high = metacog_should_abstain(0.12);
    let metacog_low = !metacog_should_abstain(0.001);

    let _lock = A8_ENV.lock().unwrap_or_else(|e| e.into_inner());
    clear_a8_env();
    std::env::set_var("AETHER_HITL_TOKEN", "a8tok");
    let bare = Goal::goal_propose(0.3);
    let propose_hitl = gate_before_call("goal.propose", Risk::High, &bare, 1).is_err();
    let mut tok = bare.clone();
    tok.query = Some(r#"{"propose":true,"hitl_token":"a8tok"}"#.into());
    let propose_ok = gate_before_call("goal.propose", Risk::High, &tok, 1).is_ok();
    clear_a8_env();

    write_artifact(
        mg_nrmse,
        ood_empty,
        metacog_high,
        metacog_low,
        propose_hitl,
        propose_ok,
    );
    let text = std::fs::read_to_string("target/aether_a8_gate.json").unwrap();
    assert!(text.contains("aether_a8_gate/a8"));
    assert!(text.contains("\"enter_v1_default\":false"));
    assert_eq!(class_for_tool("predict.series", Risk::Low), "read_local");
    assert_eq!(class_for_tool("goal.propose", Risk::High), "privileged");
}

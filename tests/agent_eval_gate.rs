//! D0 / D-G0 — artefato de avaliação em tiers (ADR-011).
//!
//! Gate: `EvalReport` marca T0 ≠ T3, grava JSON e o CLI `eval` reproduz.

#![cfg(feature = "agent")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use aether_engine::agent::{EvalReport, EvalTier};

#[test]
fn d_g0_tiers_e_artefato_json() {
    let report = EvalReport::collect(42);
    assert!(report.self_suite_not_external);
    assert_eq!(report.tiers[0].tier, EvalTier::T0SelfSuite);
    assert!(report.tiers[0].ready);
    assert_eq!(report.tiers[3].tier, EvalTier::T3External);
    assert!(!report.tiers[3].ready);
    assert!(!EvalTier::T0SelfSuite.comparable_to_llm());
    assert!(EvalTier::T1Coprocessor.comparable_to_llm());

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("agent_eval_d0");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tmpdir");
    let path = dir.join("aether_agent_eval.json");
    report.write_to(&path).expect("write");
    let raw = fs::read_to_string(&path).expect("read");
    assert!(
        raw.contains("\"schema\":\"aether_agent_eval/d0\"") || raw.contains("aether_agent_eval/d0")
    );
    assert!(raw.contains("self_suite_not_external_benchmark"));
    assert!(raw.contains("T0_self_suite"));
    assert!(raw.contains("T3_external"));
    assert!(raw.contains("D-G0"));
}

#[test]
fn d_g0_cli_eval_reproduzivel() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("agent_eval_cli");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tmpdir");
    let path = dir.join("eval.json");

    let bin = env!("CARGO_BIN_EXE_aether-agent");
    let out = Command::new(bin)
        .args([
            "eval",
            "--seed",
            "7",
            "--json",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("spawn aether-agent eval");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("aether_agent_eval/d0"));
    let raw = fs::read_to_string(&path).expect("artefato");
    assert!(raw.contains("\"seed\":7") || raw.contains("\"seed\": 7") || raw.contains("7"));
    assert!(raw.contains("self_suite_not_external_benchmark"));
}

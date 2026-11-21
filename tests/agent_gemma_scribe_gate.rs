//! Gate: Gemma sentinela ∧ escriba — path compartilhado, P2, sem decide no scribe.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    active_scribe_path, certify_utterance, ensure_scribe_fields_on_active_lock, load_catalog,
    scribe_runtime_policy, Action, Claim, ExpertId, GemmaScribePort, ParserPort, Style,
    DEFAULT_AND_PEAK_MB, ScribeRuntime, CATALOG_PATH,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::json::Json;

fn claim() -> Claim {
    Claim::new(
        "brasil",
        "capital",
        "brasília",
        0.9,
        128,
        ExpertId::Transplant,
        Provenance::Transplanted,
    )
}

#[test]
fn catalog_sentinel_tem_role_scribe() {
    let cat = load_catalog().expect("catalog");
    let g = cat
        .iter()
        .find(|r| r.id == "gemma-3-1b-prose")
        .expect("gemma");
    // roles no JSON — re-lê arquivo
    let text = std::fs::read_to_string(CATALOG_PATH).unwrap();
    assert!(text.contains("\"roles\""));
    assert!(text.contains("scribe"));
    assert_eq!(g.slot.label(), "sentinel");
}

#[test]
fn scribe_anchored_passa_p2() {
    std::env::set_var("AETHER_SCRIBE", "gemma");
    let mut scribe = GemmaScribePort::from_env(Some(8192)).expect("scribe");
    let utt = certify_utterance(&mut scribe, &[claim()], Action::Cite, Style::Factual).unwrap();
    assert!(utt.may_speak_fact());
    assert!(utt.text.to_lowercase().contains("brasil"));
    std::env::remove_var("AETHER_SCRIBE");
}

#[test]
fn template_ainda_funciona() {
    let mut p = ParserPort::new();
    let utt = certify_utterance(&mut p, &[claim()], Action::Cite, Style::Factual).unwrap();
    assert!(utt.may_speak_fact());
}

#[test]
fn policy_user_final() {
    assert_eq!(
        scribe_runtime_policy(Some(100), DEFAULT_AND_PEAK_MB, true),
        ScribeRuntime::TemplateOnly
    );
    assert_eq!(
        scribe_runtime_policy(Some(8000), DEFAULT_AND_PEAK_MB, true),
        ScribeRuntime::Resident
    );
}

#[test]
fn lock_scribe_path_amarrado() {
    let _ = std::fs::create_dir_all(".vega/profiles/active");
    let lock = Json::obj([
        ("schema", Json::str("engine.lock/1")),
        ("bank", Json::str("active")),
        (
            "sentinel",
            Json::obj([("id", Json::str("gemma-3-1b-prose"))]),
        ),
        ("primary", Json::obj([("id", Json::str("vega-u-0.8b"))])),
    ]);
    std::fs::write(
        ".vega/profiles/active/engine.lock.json",
        lock.to_string(),
    )
    .unwrap();
    ensure_scribe_fields_on_active_lock().unwrap();
    let path = active_scribe_path().expect("scribe_path");
    assert_eq!(path, "gemma-3-1b-prose");
    let text = std::fs::read_to_string(".vega/profiles/active/engine.lock.json").unwrap();
    assert!(text.contains("scribe_enabled"));
    assert!(text.contains("gemma_local"));
}

#[test]
fn artefato_gate() {
    let art = Json::obj([
        ("schema", Json::str("gemma_scribe_gate/1")),
        ("one_artifact_two_plugs", Json::Bool(true)),
        ("vegaml_decide_in_scribe", Json::Bool(false)),
        ("default_backend", Json::str("template")),
        ("opt_in", Json::str("AETHER_SCRIBE=gemma")),
        ("doc", Json::str("docs/GEMMA_SENTINEL_SCRIBE.md")),
    ]);
    std::fs::create_dir_all("target").ok();
    std::fs::write("target/aether_gemma_scribe_gate.json", art.to_string()).unwrap();
    let t = std::fs::read_to_string("target/aether_gemma_scribe_gate.json").unwrap();
    assert!(t.contains("\"vegaml_decide_in_scribe\":false"));
}

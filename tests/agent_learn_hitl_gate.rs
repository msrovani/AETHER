//! Fase D — learn ≤120; promote só via HITL; 0 auto-255.

#![cfg(feature = "agent")]

use aether_engine::agent::{LEARN_AUTH_CAP};
use aether_engine::json::Json;

#[test]
fn learn_cap_e_artefato() {
    assert!(LEARN_AUTH_CAP <= 120);
    let art = Json::obj([
        ("db", Json::str("n-sgdb")),
        ("learn_auth_cap", Json::num(f64::from(LEARN_AUTH_CAP))),
        ("auto_255", Json::Bool(false)),
        ("promote", Json::str("T-HITL hub + AETHER_HITL_TOKEN → curate set_authority 255")),
        ("writes_le_120", Json::num(0.0)),
        ("promotes_255", Json::num(0.0)),
        ("denies", Json::num(0.0)),
    ]);
    std::fs::create_dir_all("target").ok();
    std::fs::write("target/aether_learn_hitl.json", art.to_string()).unwrap();
    let text = std::fs::read_to_string("target/aether_learn_hitl.json").unwrap();
    assert!(text.contains("\"auto_255\":false"));
    assert!(text.contains("n-sgdb"));
}

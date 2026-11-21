//! Fase 7 — critérios A8 enter_v1; default false até sanção HITL.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    a8_v1_criteria, enter_v1_allowed, enter_v1_default, write_a8_v1_criteria, A8_V1_CRITERIA_PATH,
};

#[test]
fn enter_v1_ainda_false() {
    write_a8_v1_criteria(A8_V1_CRITERIA_PATH).unwrap();
    let text = std::fs::read_to_string(A8_V1_CRITERIA_PATH).unwrap();
    assert!(text.contains("\"enter_v1_default\":false"));
    assert!(!enter_v1_default());
    assert!(!enter_v1_allowed());
    let hitl = a8_v1_criteria()
        .into_iter()
        .find(|c| c.id == "hitl_sanction_255")
        .unwrap();
    assert!(!hitl.met);
}

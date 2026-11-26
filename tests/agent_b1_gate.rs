//! Gate **B1** — sanção A8 `enter_v1`.
//!
//! * Sem sanção + flag → `enter_v1_allowed == false`, `enter_v1_default == false`;
//! * Com `AETHER_HITL_SANCTION_255=1` + `AETHER_A8_V1=1` + checklist → **ambos true**;
//! * `hitl_sanction_255` é **humano**; nunca auto. Contrato TOMADAS_FUTURO §B1.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    enter_v1_allowed, enter_v1_default, hitl_sanction_255, write_a8_v1_criteria, A8_V1_CRITERIA_PATH,
};

#[test]
fn enter_v1_only_with_hitl_sanction_and_flag() {
    // Limpar sanção + flag: default false (nunca auto).
    std::env::remove_var("AETHER_HITL_SANCTION_255");
    std::env::remove_var("AETHER_A8_V1");
    assert!(!hitl_sanction_255());
    assert!(!enter_v1_allowed());
    assert!(!enter_v1_default());

    // Sanção HITL explícita + flag humano → enter_v1.
    std::env::set_var("AETHER_HITL_SANCTION_255", "1");
    std::env::set_var("AETHER_A8_V1", "1");
    assert!(hitl_sanction_255());
    assert!(enter_v1_allowed(), "com sanção + flag, enter_v1_allowed deve ser true");
    assert!(enter_v1_default(), "B1: enter_v1_default=true só com sanção + flag");

    // artefato de critérios reflete o estado
    write_a8_v1_criteria(A8_V1_CRITERIA_PATH).unwrap();
    let text = std::fs::read_to_string(A8_V1_CRITERIA_PATH).unwrap();
    assert!(text.contains("\"hitl_sanction_255\""));
    assert!(text.contains("\"enter_v1_allowed\":true"));

    // remover sanção reverte para false (nunca persiste auto)
    std::env::remove_var("AETHER_HITL_SANCTION_255");
    std::env::remove_var("AETHER_A8_V1");
    assert!(!enter_v1_allowed());
    assert!(!enter_v1_default());
}

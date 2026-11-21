//! Fase 0 — gate contrato T-NL-IN: fixtures + OOV + artefato.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    tnl_in_contract_json, tnl_in_gate_fixtures, write_tnl_in_contract, ParserPort,
    TNL_IN_CONTRACT_PATH,
};

#[test]
fn gate_tnl_in_contrato_e_fixtures() {
    write_tnl_in_contract(TNL_IN_CONTRACT_PATH).expect("write contract");
    let text = std::fs::read_to_string(TNL_IN_CONTRACT_PATH).unwrap();
    assert!(text.contains("T-NL-IN"));
    assert!(text.contains("LanguagePort::parse"));
    assert!(tnl_in_contract_json().to_string().contains("abstain_ceiling"));

    let mut port = ParserPort::new();
    tnl_in_gate_fixtures(&mut port).expect("fixtures");
}

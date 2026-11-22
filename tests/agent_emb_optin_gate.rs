//! Fase 4 — T-EMB opt-in; 0 mistura de eras; lexical intacto com off.

#![cfg(feature = "agent")]

use aether_engine::agent::{EmbedEra, EmbedPort};

#[test]
fn emb_era_bge_m3_encaixada_sem_mistura() {
    // Encaixe HITL 2026-11-22: era default BGE-M3 1024d; legado 384 opcional.
    let mut on = EmbedPort {
        enabled: true,
        era: Some(EmbedEra::bge_m3_1024()),
    };
    assert!(on.accept(EmbedEra::bge_m3_1024(), vec![0.0; 1024]).is_ok());
    assert!(on.accept(EmbedEra::bge_m3_1024(), vec![0.0; 384]).is_err());
    assert!(on.accept(EmbedEra::minilm_384(), vec![0.0; 384]).is_err());
    on.era = Some(EmbedEra::minilm_384());
    assert!(on.accept(EmbedEra::bge_m3_1024(), vec![0.0; 1024]).is_err());
    // F4 segue congelada no status (encaixar era ≠ passar gate)
    let j = EmbedPort::off().status_json().to_string();
    assert!(j.contains("frozen"));
}

#[test]
fn emb_off_recusa_e_era_mismatch() {
    let off = EmbedPort::off();
    assert!(!off.enabled);
    assert!(off.encode_refuse("x").is_err());

    let mut on = EmbedPort {
        enabled: true,
        era: Some(EmbedEra::minilm_384()),
    };
    assert!(on.accept(EmbedEra::minilm_384(), vec![0.0; 384]).is_ok());
    assert!(on.accept(EmbedEra::minilm_384(), vec![0.0; 128]).is_err());
    on.era = Some(EmbedEra {
        model_id: "other".into(),
        dim: 384,
    });
    assert!(on.accept(EmbedEra::minilm_384(), vec![0.0; 384]).is_err());
    let j = EmbedPort::off().status_json().to_string();
    assert!(j.contains("frozen"));
}

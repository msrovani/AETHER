//! Fase 4 — T-EMB opt-in; 0 mistura de eras; lexical intacto com off.

#![cfg(feature = "agent")]

use aether_engine::agent::{EmbedEra, EmbedPort};

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
    assert!(on
        .accept(EmbedEra::minilm_384(), vec![0.0; 384])
        .is_err());
    let j = EmbedPort::off().status_json().to_string();
    assert!(j.contains("frozen"));
}

//! Fase 2 — T-NL-OUT P2: CertifiedUtterance; 0 frase sem evidência.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    certify_utterance, judge_claim_text, Action, Claim, ExpertId, LanguageStyle, ParserPort,
};
use aether_engine::core::certificate::Provenance;

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
fn gate_p2_certifica_e_rejeita_sem_evidencia() {
    let mut p = ParserPort::new();
    let utt = certify_utterance(&mut p, &[claim()], Action::Cite, LanguageStyle::Factual).unwrap();
    assert!(utt.may_speak_fact());
    assert!(!utt.sources.is_empty());

    assert!(certify_utterance(&mut p, &[], Action::Cite, LanguageStyle::Compact).is_err());
    let (ok, _, _, _) = judge_claim_text(&[claim()], "texto sem slots");
    assert!(!ok);
}

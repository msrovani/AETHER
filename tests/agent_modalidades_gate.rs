//! Fases 3–5 — OCR / STT / VIS / TTS stubs → T-NL-IN; TTS só CertifiedUtterance.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    certify_utterance, Action, Claim, DocRef, ExpertId, LanguageStyle, OcrPort, ParserPort,
    SttPort, TtsPort, VisionPort,
};
use aether_engine::core::certificate::Provenance;

#[test]
fn ocr_stt_vis_funil_tnl_in() {
    let mut p = ParserPort::new();
    let g = OcrPort::off()
        .to_goal(
            &DocRef {
                id: "fixture/hello".into(),
                mime: "text/plain".into(),
            },
            &mut p,
        )
        .unwrap();
    assert!(g.subject.as_deref().unwrap().eq_ignore_ascii_case("brasil"));

    let g = SttPort::off().to_goal("hello.wav", &mut p).unwrap();
    assert!(g
        .relation
        .as_deref()
        .unwrap()
        .eq_ignore_ascii_case("capital"));

    let g = VisionPort::off()
        .to_goal("fixture/brasil.png", &mut p)
        .unwrap();
    assert!(g.subject.as_deref().unwrap().eq_ignore_ascii_case("brasil"));
}

#[test]
fn tts_so_certified_abstain_beep() {
    let mut p = ParserPort::new();
    let claim = Claim::new(
        "brasil",
        "capital",
        "brasília",
        0.9,
        128,
        ExpertId::Transplant,
        Provenance::Transplanted,
    );
    let utt = certify_utterance(&mut p, &[claim], Action::Cite, LanguageStyle::Factual).unwrap();
    let speak = TtsPort::off().speak(&utt);
    assert_eq!(speak.kind, "speak");

    let beep = TtsPort::off().speak(&aether_engine::agent::CertifiedUtterance {
        text: "x".into(),
        action: Action::Abstain,
        auth: 0,
        sources: vec![],
        judge_conf: 0.0,
    });
    assert_eq!(beep.kind, "beep");
}

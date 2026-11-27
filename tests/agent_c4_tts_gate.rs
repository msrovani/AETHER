//! Gate **C4** — T-TTS backend real via sidecar `AETHER_TTS_CMD`.
//! Só `CertifiedUtterance` fala; `abstain`/sem fonte → beep "não sei"; PCM não cruza o T0;
//! `Err` honesto sem backend; `feature speech`. Contrato: docs/TOMADAS_FUTURO §C4.
//! NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::nl_out::CertifiedUtterance;
use aether_engine::agent::speech::TtsPort;
use aether_engine::agent::Action;

fn utt(action: Action, text: &str, sources: Vec<String>) -> CertifiedUtterance {
    CertifiedUtterance { text: text.into(), action, auth: 120, sources, judge_conf: 0.95 }
}

#[test]
fn c4_tts_so_certified_abstain_beep_nao_sei() {
    let port = TtsPort { enabled: true };
    std::env::remove_var("AETHER_TTS_CMD");

    // abstain → beep "não sei", nunca fala fato (não exige backend)
    let abst = utt(Action::Abstain, "???", vec![]);
    let r = port.speak_sidecar(&abst).unwrap();
    assert_eq!(r.kind, "beep");
    assert_eq!(r.spoken, "não sei");
    // sem fonte → não pode falar fato → beep
    let nofonte = utt(Action::Cite, "algo", vec![]);
    assert_eq!(port.speak_sidecar(&nofonte).unwrap().kind, "beep");

    // CertifiedUtterance com fonte + não-abstain + sem backend → Err honesto
    let ok = utt(Action::Cite, "capital do Brasil é Brasília", vec!["nsgdb".into()]);
    assert!(port.speak_sidecar(&ok).is_err(), "certificado mas sem AETHER_TTS_CMD → Err honesto");
    // comando inexistente → Err, não panic
    std::env::set_var("AETHER_TTS_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.speak_sidecar(&ok).is_err());
    std::env::remove_var("AETHER_TTS_CMD");

    // backend stub real (Windows)
    #[cfg(windows)]
    {
        std::env::set_var("AETHER_TTS_CMD", "cmd /c echo & rem");
        let r = port.speak_sidecar(&ok).expect("stub backend");
        assert_eq!(r.kind, "speak");
        assert_eq!(r.spoken, ok.text, "fala só texto certificado");
        std::env::remove_var("AETHER_TTS_CMD");
    }
}

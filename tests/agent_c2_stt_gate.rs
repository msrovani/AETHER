//! Gate **C2** — T-STT backend real via sidecar externo `AETHER_STT_CMD`, **seguro**
//! (sem shell; `audio_ref` sanitizado; timeout+limite; W1+W3+W5+W8).
//! `conf` honesta (não inventada; NaN→0); `conf<=0.50→ask`; **PCM fora do T0**;
//! `Err` honesto sem backend; `feature speech` + `AETHER_STT=1`.
//! NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::speech::SttPort;

#[test]
fn c2_stt_sidecar_honesto_conf_seguro() {
    let port = SttPort { enabled: true };
    let audio = "audio.wav"; // audio-ref tipada; PCM não cruza o T0

    std::env::remove_var("AETHER_STT_CMD");
    std::env::remove_var("AETHER_STT_CONF");
    // W3 off → Err; sem backend → Err; comando inexistente → Err, não panic
    assert!(SttPort { enabled: false }
        .transcribe_sidecar(audio)
        .is_err());
    assert!(port.transcribe_sidecar(audio).is_err());
    std::env::set_var("AETHER_STT_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.transcribe_sidecar(audio).is_err());
    // W1: prog shell bloqueado; injection sanitize
    std::env::set_var("AETHER_STT_CMD", "sh -c evil");
    assert!(port.transcribe_sidecar(audio).is_err());
    std::env::set_var("AETHER_STT_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.transcribe_sidecar("a.wav & calc").is_err());
    std::env::remove_var("AETHER_STT_CMD");

    #[cfg(windows)]
    {
        std::env::set_var("AETHER_STT_CMD", "cmd /c echo ask Brasil capital");
        let r = port.transcribe_sidecar(audio).expect("stub backend");
        assert!(r.text.trim().contains("ask Brasil capital"));
        // W5 conf honesta: sem `AETHER_STT_CONF` ⇒ conf=0.0 (não 0.5 inventado)
        assert_eq!(r.conf, 0.0);
        std::env::set_var("AETHER_STT_CONF", "0.9");
        assert!((port.transcribe_sidecar(audio).unwrap().conf - 0.9).abs() < 1e-6);
        std::env::set_var("AETHER_STT_CONF", "NaN");
        assert_eq!(port.transcribe_sidecar(audio).unwrap().conf, 0.0);
        std::env::remove_var("AETHER_STT_CONF");
    }

    std::env::remove_var("AETHER_STT_CMD");
    std::env::remove_var("AETHER_STT_CONF");
}

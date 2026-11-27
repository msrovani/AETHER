//! Gate **C2** — T-STT backend real via sidecar externo `AETHER_STT_CMD`.
//! `conf` não inventada; `conf<0.50→ask`; **PCM fora do T0**; `Err` honesto sem backend;
//! `feature speech` + `AETHER_STT=1`. Contrato: docs/TOMADAS_FUTURO §C2.
//! NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::speech::SttPort;

#[test]
fn c2_stt_sidecar_honesto_e_conf() {
    let port = SttPort { enabled: true };
    let audio = "audio.wav"; // audio-ref tipada; PCM não cruza o T0

    // sem backend → Err honesto; comando inexistente → Err, não panic
    std::env::remove_var("AETHER_STT_CMD");
    std::env::remove_var("AETHER_STT_CONF");
    assert!(port.transcribe_sidecar(audio).is_err());
    std::env::set_var("AETHER_STT_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.transcribe_sidecar(audio).is_err());
    std::env::remove_var("AETHER_STT_CMD");

    // backend stub real (Windows): cmd /c echo imprime texto; doc/audio-ref anexado
    #[cfg(windows)]
    {
        std::env::set_var("AETHER_STT_CMD", "cmd /c echo ask Brasil capital & rem");
        let r = port.transcribe_sidecar(audio).expect("stub backend");
        assert_eq!(r.text, "ask Brasil capital");
        assert!(r.conf >= 0.5, "conf default 0.5; = {}", r.conf);
        // conf override; texto vazio → 0.0
        std::env::set_var("AETHER_STT_CONF", "0.9");
        assert!((port.transcribe_sidecar(audio).unwrap().conf - 0.9).abs() < 1e-6);
        std::env::set_var("AETHER_STT_CMD", "cmd /c echo. & rem");
        assert_eq!(port.transcribe_sidecar(audio).unwrap().conf, 0.0);
    }

    std::env::remove_var("AETHER_STT_CMD");
    std::env::remove_var("AETHER_STT_CONF");
}

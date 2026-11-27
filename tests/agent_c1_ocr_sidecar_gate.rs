//! Gate **C1** (T-OCR backend real via sidecar externo) — sem dependência nova;
//! `AETHER_OCR_CMD` recebe `doc.id`, imprime texto; **honesto `Err` sem backend**;
//! `conf` não inventada alta; PII/conf<0.50 → ask/deny; pixels fora do T0.
//! Contrato: docs/TOMADAS_FUTURO.md §C1; docs/CORE_TOMADAS.md T-OCR.
//!
//! NOTA: um único teste — muta `env`, e testes em threads compartilham o processo.

#![cfg(feature = "agent")]

use aether_engine::agent::ocr::{DocRef, OcrPort};

#[test]
fn c1_sidecar_honesto_e_extrai_texto() {
    let port = OcrPort { enabled: true };
    let doc = DocRef { id: "".into(), mime: "image/png".into() };

    // --- sem backend → Err honesto; nunca finge OCR; comando inexistente → Err, não panic
    std::env::remove_var("AETHER_OCR_CMD");
    std::env::remove_var("AETHER_OCR_CONF");
    assert!(port.extract_sidecar(&doc).is_err(), "sem AETHER_OCR_CMD deve ser Err honesto");
    std::env::set_var("AETHER_OCR_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.extract_sidecar(&doc).is_err());
    std::env::remove_var("AETHER_OCR_CMD");

    // --- backend real (stub): cmd /c echo (Windows) imprime texto UTF-8
    #[cfg(windows)]
    {
        std::env::set_var("AETHER_OCR_CMD", "cmd /c echo ask Brasil capital & rem");
        let r = port.extract_sidecar(&doc).expect("sidecar stub backend");
        assert_eq!(r.text, "ask Brasil capital", "texto UTF-8 do stdout");
        assert!(r.conf >= 0.5, "conf default 0.5 (não inventa alta); = {}", r.conf);
        assert!(!r.pii);

        // conf override humano
        std::env::set_var("AETHER_OCR_CONF", "0.9");
        let r = port.extract_sidecar(&doc).unwrap();
        assert!((r.conf - 0.9).abs() < 1e-6);

        // texto vazio → conf=0.0
        std::env::set_var("AETHER_OCR_CMD", "cmd /c echo. & rem");
        let r = port.extract_sidecar(&doc).unwrap();
        assert_eq!(r.conf, 0.0, "texto vazio => conf 0.0");
    }

    std::env::remove_var("AETHER_OCR_CMD");
    std::env::remove_var("AETHER_OCR_CONF");
}

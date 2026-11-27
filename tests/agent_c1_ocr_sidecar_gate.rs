//! Gate **C1** (T-OCR backend real via sidecar externo) — **seguro**: sem shell;
//! `AETHER_OCR_CMD` = binário concreto; `doc.id` **sanitizado**; timeout + `stdout`;
//! `Err` honesto sem backend; **conf honesta (não inventada; NaN→0)**; **PII** no
//! texto extraído; `feature ocr` + `AETHER_OCR=1`. W1+W3+W5+W8.
//! Contrato: docs/TOMADAS_FUTURO §C1. NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::ocr::{DocRef, OcrPort};

#[test]
fn c1_sidecar_honesto_seguro_conf_pii() {
    let port = OcrPort { enabled: true };
    let doc = DocRef {
        id: "".into(),
        mime: "image/png".into(),
    };

    // --- W3: tomada off → Err honesto; **nunca spawn**
    assert!(OcrPort { enabled: false }.extract_sidecar(&doc).is_err());

    // --- sem backend → Err honesto; comando inexistente → Err, não panic
    std::env::remove_var("AETHER_OCR_CMD");
    std::env::remove_var("AETHER_OCR_CONF");
    assert!(port.extract_sidecar(&doc).is_err());
    std::env::set_var("AETHER_OCR_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.extract_sidecar(&doc).is_err());
    std::env::remove_var("AETHER_OCR_CMD");

    // --- W1: prog shell bloqueado; injeção no `doc.id` sanitizada
    std::env::set_var("AETHER_OCR_CMD", "sh -c evil");
    assert!(port.extract_sidecar(&doc).is_err(), "prog shell bloqueado");
    std::env::remove_var("AETHER_OCR_CMD");
    let inj = DocRef {
        id: "x & calc".into(),
        mime: "image/png".into(),
    };
    std::env::set_var("AETHER_OCR_CMD", "definitely-not-a-real-binary-xyz");
    assert!(
        port.extract_sidecar(&inj).is_err(),
        "injection → sanitize/Err, 0 segundo comando"
    );
    std::env::remove_var("AETHER_OCR_CMD");
    // `sanitize_ref` unitário
    assert!(aether_engine::agent::backend::sanitize_ref("x & calc").is_err());
    assert!(aether_engine::agent::backend::sanitize_ref("x | y").is_err());
    assert!(aether_engine::agent::backend::sanitize_ref("a;b").is_err());
    assert!(aether_engine::agent::backend::sanitize_ref("normal-id_1.2").is_ok());

    // --- backend stub real (Windows `cmd /c echo`, **sem appendar dado não-confiável**; `cmd` não é recomendado)
    #[cfg(windows)]
    {
        std::env::set_var("AETHER_OCR_CMD", "cmd /c echo ask Brasil capital");
        let r = port.extract_sidecar(&doc).expect("stub backend");
        assert!(
            r.text.trim().contains("ask Brasil capital"),
            "texto UTF-8 do stdout; got {:?}",
            r.text
        );
        // W5 conf honesta: sem `AETHER_OCR_CONF` declarado ⇒ `conf=0.0`, **não** 0.5 inventado
        assert_eq!(r.conf, 0.0, "conf não inventada; abstain");
        assert!(!r.pii);
        // conf só com `AETHER_OCR_CONF` declarado + finito; NaN rejeitado→0
        std::env::set_var("AETHER_OCR_CONF", "0.9");
        assert!((port.extract_sidecar(&doc).unwrap().conf - 0.9).abs() < 1e-6);
        std::env::set_var("AETHER_OCR_CONF", "NaN");
        assert_eq!(
            port.extract_sidecar(&doc).unwrap().conf,
            0.0,
            "NaN conf → 0.0"
        );
        std::env::remove_var("AETHER_OCR_CONF");
        // texto vazio → conf 0.0
        std::env::set_var("AETHER_OCR_CMD", "cmd /c echo.");
        assert_eq!(port.extract_sidecar(&doc).unwrap().conf, 0.0);
        std::env::remove_var("AETHER_OCR_CMD");
    }

    // --- W5 PII detect_pii no texto extraído (via stub) — best effort conservador
    #[cfg(windows)]
    {
        // `cmd /c echo` imprime o que está em cmd_line (user-set trusted); PII detectável no texto
        std::env::set_var("AETHER_OCR_CMD", "cmd /c echo meu email user@example.com");
        let r = port.extract_sidecar(&doc).unwrap();
        assert!(r.pii, "PII no texto extraído → deny");
        std::env::remove_var("AETHER_OCR_CMD");
    }
}

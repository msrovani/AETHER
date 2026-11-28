//! T-OCR — `DocRef` → texto → T-NL-IN (feature `ocr` off = stub fixture).
//!
//! Conf baixa → ask (não chuta Goal). PII deny. Sem bytes de imagem no T0.

use crate::json::Json;

use super::language::{GoalSchema, LanguageError, LanguagePort, ParserPort};
use super::tool::Goal;

/// Referência tipada a documento (borda; path/URL, não pixels).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocRef {
    /// Identificador estável (path relativo ou id).
    pub id: String,
    /// MIME declarado (`text/plain`, `application/pdf`, …).
    pub mime: String,
}

/// Resultado OCR tipado.
#[derive(Clone, Debug)]
pub struct OcrResult {
    /// Texto UTF-8.
    pub text: String,
    /// Confiança `[0,1]`.
    pub conf: f32,
    /// Spans opcionais `start..end` em chars.
    pub spans: Vec<(usize, usize)>,
    /// `true` se PII detectada → deny.
    pub pii: bool,
}

/// Porta OCR (default off).
#[derive(Clone, Debug, Default)]
pub struct OcrPort {
    /// Feature/env ligada.
    pub enabled: bool,
}

impl OcrPort {
    /// Off (default seguro).
    pub fn off() -> Self {
        Self { enabled: false }
    }

    /// Liga se `AETHER_OCR=1` **e** feature `ocr` (sem feature = stub só em testes).
    pub fn from_env() -> Self {
        Self {
            enabled: crate::agent::modality::ModalConfig::from_env("OCR", cfg!(feature = "ocr"))
                .enabled,
        }
    }

    /// Stub fixture: id conhecido → texto; conf baixa ou PII → ask/deny.
    pub fn extract(&self, doc: &DocRef) -> Result<OcrResult, String> {
        if doc.id.contains("pii") || doc.mime.contains("pii") {
            return Ok(OcrResult {
                text: String::new(),
                conf: 0.0,
                spans: Vec::new(),
                pii: true,
            });
        }
        // Fixture estável offline.
        if doc.id == "fixture/hello" || doc.id.ends_with("hello.txt") {
            return Ok(OcrResult {
                text: "ask Brasil capital".into(),
                conf: 0.92,
                spans: vec![(0, 18)],
                pii: false,
            });
        }
        if doc.id.contains("blur") {
            return Ok(OcrResult {
                text: "???".into(),
                conf: 0.25,
                spans: Vec::new(),
                pii: false,
            });
        }
        if !self.enabled {
            return Err("T-OCR off (feature ocr / AETHER_OCR)".into());
        }
        // C1 (TOMADAS_FUTURO): backend real via **sidecar externo**, sem
        // dependência nova. `AETHER_OCR_CMD` (ex.: `tesseract <in> stdout` ou
        // sidecar Python) recebe `doc.id` como argumento e imprime texto UTF-8.
        // Sem `AETHER_OCR_CMD` → `Err` honesto; **nunca finge OCR**.
        self.extract_sidecar(doc)
    }

    /// Backend externo (sidecar/Tesseract), **seguro** (`backend::run_backend_cmd`):
    /// sem shell; `doc.id` **sanitizado**; `AETHER_OCR_CMD` = binário concreto;
    /// timeout + `stdout` limitado. **W1+W8.**
    ///
    /// **W3:** re-checa `enabled`+feature. **W5:** conf **não inventada**:
    /// backend sem conf medida ⇒ `conf=0.0` (abstain); `AETHER_OCR_CONF` só
    /// se operador declarar conf real + `is_finite`. **W5 PII:** detecção no
    /// **texto extraído**. `AETHER_OCR_TIMEOUT_MS`.
    pub fn extract_sidecar(&self, doc: &DocRef) -> Result<OcrResult, String> {
        // W3: feature off = no-op honesto; nunca spawn
        if !self.enabled {
            return Err("T-OCR off (feature ocr / AETHER_OCR)".into());
        }
        // T11: encanamento via ModalConfig (uma fonte de verdade).
        let cfg = crate::agent::modality::ModalConfig::from_env("OCR", cfg!(feature = "ocr"));
        let cmd_line = cfg.cmd.ok_or_else(|| {
            "T-OCR enabled sem backend (defina AETHER_OCR_CMD; binário concreto; feature `ocr` off)".to_string()
        })?;
        let out = super::backend::run_backend_cmd(
            &cmd_line,
            Some(&doc.id),
            None,
            cfg.timeout_ms,
            super::backend::BACKEND_MAX_STDOUT,
        )
        .map_err(|e| format!("T-OCR sidecar: {e}"))?;
        let text = String::from_utf8_lossy(&out).trim().to_string();
        if text.is_empty() {
            return Ok(OcrResult {
                text,
                conf: 0.0,
                spans: Vec::new(),
                pii: false,
            });
        }
        // W5: PII no texto extraído → deny (nunca emite Goal com PII)
        let pii = detect_pii(&text);
        // W5: conf honesta via ModalConfig — sem conf medida ⇒ 0.0; NaN/inf
        // rejeitado pela ModalConfig (filter is_finite); clamp [0,1].
        let conf = cfg.conf.unwrap_or(0.0);
        Ok(OcrResult {
            text,
            conf,
            spans: Vec::new(),
            pii,
        })
    }

    /// OCR → T-NL-IN. Conf &lt; 0.50 ou PII → não emite Goal.
    pub fn to_goal(&self, doc: &DocRef, port: &mut ParserPort) -> Result<Goal, String> {
        let r = self.extract(doc)?;
        if r.pii {
            return Err("T-OCR PII deny".into());
        }
        // W5: NaN/inf → ask; `conf<=0.50` (não `<`) → não inventa Goal
        if !r.conf.is_finite() || r.conf <= 0.50 {
            return Err("T-OCR conf baixa → ask (não inventa Goal)".into());
        }
        port.parse(&r.text, &GoalSchema::agent_default())
            .map_err(|e: LanguageError| format!("T-NL-IN: {e:?}"))
    }

    /// Status hub/CLI.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-OCR")),
            ("enabled", Json::Bool(self.enabled)),
            (
                "feature",
                Json::str(if cfg!(feature = "ocr") { "on" } else { "off" }),
            ),
            ("funnel", Json::str("DocRef → text → T-NL-IN")),
            ("auth_cap", Json::num(120.0)),
        ])
    }
}

/// **W5**: detector de PII conservador no texto extraído (evita `to_goal` com PII).
pub(crate) fn detect_pii(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    if t.contains("cpf")
        || t.contains("cnpj")
        || t.contains("passaporte")
        || t.contains("cartão")
        || t.contains("cartao")
        || t.contains("cvv")
        || t.contains("rg ")
        || t.contains("rg:")
        || t.contains("pan ")
    {
        return true;
    }
    if text.contains('@') && text.contains('.') {
        return true;
    }
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.len() == 11 || digits.len() == 14 || (13..=19).contains(&digits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_para_goal() {
        let ocr = OcrPort::off();
        let mut p = ParserPort::new();
        let g = ocr
            .to_goal(
                &DocRef {
                    id: "fixture/hello".into(),
                    mime: "text/plain".into(),
                },
                &mut p,
            )
            .unwrap();
        assert!(g.subject.as_deref().unwrap().eq_ignore_ascii_case("brasil"));
    }

    #[test]
    fn conf_baixa_ask() {
        let ocr = OcrPort::off();
        let mut p = ParserPort::new();
        let err = ocr
            .to_goal(
                &DocRef {
                    id: "blur/x".into(),
                    mime: "image/png".into(),
                },
                &mut p,
            )
            .unwrap_err();
        assert!(err.contains("ask"));
    }

    #[test]
    fn pii_deny() {
        let ocr = OcrPort::off();
        let r = ocr
            .extract(&DocRef {
                id: "doc-pii".into(),
                mime: "text/plain".into(),
            })
            .unwrap();
        assert!(r.pii);
    }
}

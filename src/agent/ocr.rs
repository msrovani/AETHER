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
        let env_on = std::env::var("AETHER_OCR")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            enabled: env_on && cfg!(feature = "ocr"),
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
        Err("T-OCR enabled sem backend — use fixture id".into())
    }

    /// OCR → T-NL-IN. Conf &lt; 0.50 ou PII → não emite Goal.
    pub fn to_goal(&self, doc: &DocRef, port: &mut ParserPort) -> Result<Goal, String> {
        let r = self.extract(doc)?;
        if r.pii {
            return Err("T-OCR PII deny".into());
        }
        if r.conf < 0.50 {
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
            ("feature", Json::str(if cfg!(feature = "ocr") { "on" } else { "off" })),
            ("funnel", Json::str("DocRef → text → T-NL-IN")),
            ("auth_cap", Json::num(120.0)),
        ])
    }
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

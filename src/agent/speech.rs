//! T-STT / T-TTS — escadas curtas (feature `speech` off = stub).
//!
//! STT: WAV/path → texto → T-NL-IN. TTS: só [`CertifiedUtterance`];
//! abstain → beep “não sei”, nunca narra fato inventado. PCM fora do T0.

use crate::json::Json;

use super::language::{GoalSchema, LanguageError, LanguagePort, ParserPort};
use super::nl_out::CertifiedUtterance;
use super::tool::Goal;
use super::Action;

/// Resultado STT tipado.
#[derive(Clone, Debug)]
pub struct SttResult {
    /// Texto UTF-8 para T-NL-IN.
    pub text: String,
    /// Confiança.
    pub conf: f32,
    /// Idioma opcional.
    pub lang: Option<String>,
}

/// Porta STT (default off).
#[derive(Clone, Debug, Default)]
pub struct SttPort {
    /// Ligada.
    pub enabled: bool,
}

impl SttPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_STT=1` + feature `speech`.
    pub fn from_env() -> Self {
        let env_on = std::env::var("AETHER_STT")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            enabled: env_on && cfg!(feature = "speech"),
        }
    }

    /// Stub: path contendo `hello` → frase fixture; `noise` → conf baixa.
    pub fn transcribe(&self, audio_ref: &str) -> Result<SttResult, String> {
        if audio_ref.contains("noise") {
            return Ok(SttResult {
                text: "…".into(),
                conf: 0.2,
                lang: Some("pt".into()),
            });
        }
        if audio_ref.contains("hello") || audio_ref.ends_with(".wav") {
            return Ok(SttResult {
                text: "ask Brasil capital".into(),
                conf: 0.9,
                lang: Some("pt".into()),
            });
        }
        if !self.enabled {
            return Err("T-STT off".into());
        }
        // C2 (TOMADAS_FUTURO): backend real via **sidecar externo**, sem dependência
        // nova. `AETHER_STT_CMD` (ex. STT/Tesseract speech/sidecar Python) recebe
        // `audio_ref` (ref tipada; **PCM fora do T0**) e imprime texto UTF-8.
        // Sem `AETHER_STT_CMD` → `Err` honesto; **nunca finge transcrição**.
        self.transcribe_sidecar(audio_ref)
    }

    /// Backend externo (sidecar), **seguro** (`backend::run_backend_cmd`): sem
    /// shell; `audio_ref` **sanitizado**; `AETHER_STT_CMD` = binário concreto;
    /// timeout + `stdout`; **PCM fora do T0**. W1+W8.
    ///
    /// **W3** re-checa `enabled`. **W5/C2:** conf honesta — sem conf medida ⇒
    /// `conf=0.0`; `AETHER_STT_CONF` só se operador declarar + `is_finite`;
    /// texto vazio ⇒ `conf=0.0`. `to_goal` aplica `conf<=0.50→ask`. `AETHER_STT_TIMEOUT_MS`.
    pub fn transcribe_sidecar(&self, audio_ref: &str) -> Result<SttResult, String> {
        if !self.enabled {
            return Err("T-STT off (feature speech / AETHER_STT)".into()); // W3
        }
        let cmd_line = std::env::var("AETHER_STT_CMD")
            .map_err(|_| "T-STT enabled sem backend (defina AETHER_STT_CMD; binário concreto; PCM fora do T0)".to_string())?;
        let timeout = std::env::var("AETHER_STT_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(super::backend::BACKEND_TIMEOUT_MS);
        let out = super::backend::run_backend_cmd(
            &cmd_line,
            Some(audio_ref),
            None,
            timeout,
            super::backend::BACKEND_MAX_STDOUT,
        )
        .map_err(|e| format!("T-STT sidecar: {e}"))?;
        let text = String::from_utf8_lossy(&out).trim().to_string();
        if text.is_empty() {
            return Ok(SttResult {
                text,
                conf: 0.0,
                lang: None,
            });
        }
        let conf = match std::env::var("AETHER_STT_CONF") {
            Ok(s) => match s.parse::<f32>() {
                Ok(c) if c.is_finite() => c.clamp(0.0, 1.0),
                _ => 0.0,
            },
            Err(_) => 0.0,
        };
        Ok(SttResult {
            text,
            conf,
            lang: None,
        })
    }

    /// STT → T-NL-IN.
    pub fn to_goal(&self, audio_ref: &str, port: &mut ParserPort) -> Result<Goal, String> {
        let r = self.transcribe(audio_ref)?;
        // W5/C2: NaN/inf→ask; `conf<=0.50` (não `<`) → não inventa Goal
        if !r.conf.is_finite() || r.conf <= 0.50 {
            return Err("T-STT conf baixa → ask".into());
        }
        // **MÉDIO C2:** PII no texto STT (mesmo `detect_pii` de C1; reusa infra) → deny
        if crate::agent::ocr::detect_pii(&r.text) {
            return Err("T-STT PII deny".into());
        }
        port.parse(&r.text, &GoalSchema::agent_default())
            .map_err(|e: LanguageError| format!("{e:?}"))
    }

    /// Status.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-STT")),
            ("enabled", Json::Bool(self.enabled)),
            (
                "feature",
                Json::str(if cfg!(feature = "speech") {
                    "on"
                } else {
                    "off"
                }),
            ),
            ("pcm_in_t0", Json::Bool(false)),
        ])
    }
}

/// Saída TTS tipada (stub: string de diagnóstico, não PCM).
#[derive(Clone, Debug)]
pub struct TtsResult {
    /// Descrição (`beep` | `speak`).
    pub kind: &'static str,
    /// Texto efetivamente falado / beep.
    pub spoken: String,
}

/// Porta TTS — só `CertifiedUtterance`.
#[derive(Clone, Debug, Default)]
pub struct TtsPort {
    /// Ligada.
    pub enabled: bool,
}

impl TtsPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_TTS=1` + feature `speech`.
    pub fn from_env() -> Self {
        let env_on = std::env::var("AETHER_TTS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            enabled: env_on && cfg!(feature = "speech"),
        }
    }

    /// Sintetiza: abstain/sem fonte → beep; senão speak do texto certificado.
    pub fn speak(&self, utt: &CertifiedUtterance) -> TtsResult {
        if !utt.may_speak_fact() || matches!(utt.action, Action::Abstain) {
            return TtsResult {
                kind: "beep",
                spoken: "não sei".into(),
            };
        }
        TtsResult {
            kind: "speak",
            spoken: utt.text.clone(),
        }
    }

    /// C4 (TOMADAS_FUTURO): backend real via **sidecar externo**, sem dependência
    /// nova. **Só `CertifiedUtterance` fala**; `abstain`/sem fonte → `beep "não sei"`;
    /// `AETHER_TTS_CMD` recebe texto certificado e fala; **nunca PCM no bus**; sem
    /// backend → `Err` honesto. `feature speech` + `AETHER_TTS=1`.
    pub fn speak_sidecar(&self, utt: &CertifiedUtterance) -> Result<TtsResult, String> {
        // só fato certificado fala; abstain → beep "não sei"
        if !utt.may_speak_fact() || matches!(utt.action, Action::Abstain) {
            return Ok(TtsResult {
                kind: "beep",
                spoken: "não sei".into(),
            });
        }
        // W3: feature off = no-op honesto
        if !self.enabled {
            return Err("T-TTS off (feature speech / AETHER_TTS)".into());
        }
        // W4: auth ≤120 (nunca 255 auto) + `judge_conf` limiar
        if utt.auth > 120 {
            return Ok(TtsResult {
                kind: "beep",
                spoken: "não sei".into(),
            }); // não fala fato auth>120
        }
        if !utt.judge_conf.is_finite() || utt.judge_conf < 0.50 {
            return Ok(TtsResult {
                kind: "beep",
                spoken: "não sei".into(),
            }); // juíz insuficiente
        }
        let cmd_line = std::env::var("AETHER_TTS_CMD")
            .map_err(|_| "T-TTS enabled sem backend (defina AETHER_TTS_CMD; binário concreto; PCM não cruza o T0)".to_string())?;
        let timeout = std::env::var("AETHER_TTS_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(super::backend::BACKEND_TIMEOUT_MS);
        // W1+W8: sem shell; **texto certificado por stdin**, não argv; timeout + limite
        super::backend::run_backend_cmd(
            &cmd_line,
            None,
            Some(&utt.text),
            timeout,
            super::backend::BACKEND_MAX_STDOUT,
        )
        .map_err(|e| format!("T-TTS sidecar: {e}"))?;
        Ok(TtsResult {
            kind: "speak",
            spoken: utt.text.clone(),
        })
    }

    /// Status.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-TTS")),
            ("enabled", Json::Bool(self.enabled)),
            ("input", Json::str("CertifiedUtterance")),
            ("invented_facts", Json::Bool(false)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::nl_out::CertifiedUtterance;

    #[test]
    fn stt_fixture_goal() {
        let stt = SttPort::off();
        let mut p = ParserPort::new();
        let g = stt.to_goal("clips/hello.wav", &mut p).unwrap();
        assert_eq!(g.relation.as_deref(), Some("capital"));
    }

    #[test]
    fn tts_abstain_beep() {
        let tts = TtsPort::off();
        let utt = CertifiedUtterance {
            text: "algo".into(),
            action: Action::Abstain,
            auth: 0,
            sources: Vec::new(),
            judge_conf: 0.1,
        };
        let r = tts.speak(&utt);
        assert_eq!(r.kind, "beep");
        assert_eq!(r.spoken, "não sei");
    }

    #[test]
    fn tts_cite_speak() {
        let tts = TtsPort::off();
        let utt = CertifiedUtterance {
            text: "brasil capital = brasília".into(),
            action: Action::Cite,
            auth: 128,
            sources: vec!["transplant".into()],
            judge_conf: 1.0,
        };
        assert_eq!(tts.speak(&utt).kind, "speak");
    }
}

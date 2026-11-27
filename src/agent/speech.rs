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

    /// Backend externo (sidecar). `conf` não é medida pelo backend: default `0.5`
    /// (limiar `to_goal`); override `AETHER_STT_CONF`; texto vazio ⇒ `conf=0.0`.
    /// `to_goal` ainda aplica `conf<0.50→ask`.
    pub fn transcribe_sidecar(&self, audio_ref: &str) -> Result<SttResult, String> {
        let cmd_line = std::env::var("AETHER_STT_CMD")
            .map_err(|_| "T-STT enabled sem backend (defina AETHER_STT_CMD; sidecar; PCM fora do T0)".to_string())?;
        let mut parts = cmd_line.split_whitespace();
        let prog = parts.next().ok_or_else(|| "AETHER_STT_CMD vazio".to_string())?;
        let mut args: Vec<String> = parts.map(|s| s.to_string()).collect();
        args.push(audio_ref.to_string());
        let output = std::process::Command::new(prog)
            .args(&args)
            .output()
            .map_err(|e| format!("T-STT sidecar spawn: {e}"))?;
        if !output.status.success() {
            return Err(format!("T-STT sidecar status {}", output.status));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Ok(SttResult { text, conf: 0.0, lang: None });
        }
        let conf = std::env::var("AETHER_STT_CONF")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .map(|c| c.clamp(0.0, 1.0))
            .unwrap_or(0.5);
        Ok(SttResult { text, conf, lang: None })
    }

    /// STT → T-NL-IN.
    pub fn to_goal(&self, audio_ref: &str, port: &mut ParserPort) -> Result<Goal, String> {
        let r = self.transcribe(audio_ref)?;
        if r.conf < 0.50 {
            return Err("T-STT conf baixa → ask".into());
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
            return Ok(TtsResult { kind: "beep", spoken: "não sei".into() });
        }
        let cmd_line = std::env::var("AETHER_TTS_CMD")
            .map_err(|_| "T-TTS enabled sem backend (defina AETHER_TTS_CMD; sidecar; PCM não cruza o T0)".to_string())?;
        let mut parts = cmd_line.split_whitespace();
        let prog = parts.next().ok_or_else(|| "AETHER_TTS_CMD vazio".to_string())?;
        let mut args: Vec<String> = parts.map(|s| s.to_string()).collect();
        args.push(utt.text.clone());
        let output = std::process::Command::new(prog)
            .args(&args)
            .output()
            .map_err(|e| format!("T-TTS sidecar spawn: {e}"))?;
        if !output.status.success() {
            return Err(format!("T-TTS sidecar status {}", output.status));
        }
        Ok(TtsResult { kind: "speak", spoken: utt.text.clone() })
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

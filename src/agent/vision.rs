//! T-VIS — labels tipados → Goal / T-NL-IN (feature `vision` off = stub).
//!
//! V6 embed só com T-EMB. Pixels fora do T0. Auth ≤120. Não libera `act` sozinha.

use crate::json::Json;

use super::embed::EmbedPort;
use super::language::{GoalSchema, LanguageError, LanguagePort, ParserPort};
use super::tool::{Goal, AUTHORITY_QUARANTINE};

/// Cap de autoridade de visão (quarentena).
pub const VISION_AUTH_CAP: u8 = AUTHORITY_QUARANTINE;

/// Observação tipada (sem pixels).
#[derive(Clone, Debug)]
pub struct VisionObs {
    /// Labels `(nome, conf)`.
    pub labels: Vec<(String, f32)>,
    /// Entidades para recall / Goal.
    pub entities: Vec<String>,
    /// Embed opcional (só se T-EMB + era ok).
    pub embed: Option<Vec<f32>>,
    /// Cap de auth.
    pub auth_cap: u8,
}

/// Porta de visão (default off).
#[derive(Clone, Debug, Default)]
pub struct VisionPort {
    /// Ligada.
    pub enabled: bool,
}

impl VisionPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_VIS=1` + feature `vision`.
    pub fn from_env() -> Self {
        let env_on = std::env::var("AETHER_VIS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            enabled: env_on && cfg!(feature = "vision"),
        }
    }

    /// Stub: `ImageRef` com `brasil` → labels; `dark` → conf baixa.
    pub fn observe(&self, image_ref: &str, emb: Option<&EmbedPort>) -> Result<VisionObs, String> {
        if image_ref.contains("dark") {
            return Ok(VisionObs {
                labels: vec![("unknown".into(), 0.2)],
                entities: Vec::new(),
                embed: None,
                auth_cap: VISION_AUTH_CAP,
            });
        }
        if image_ref.contains("brasil") || image_ref.contains("fixture") {
            let embed = emb.and_then(|p| {
                if !p.enabled {
                    return None;
                }
                // Não gera vetor sem sidecar — só declara ausência.
                let _ = p.encode_refuse("vision");
                None
            });
            return Ok(VisionObs {
                labels: vec![("brasil".into(), 0.88), ("mapa".into(), 0.7)],
                entities: vec!["brasil".into()],
                embed,
                auth_cap: VISION_AUTH_CAP,
            });
        }
        if !self.enabled {
            return Err("T-VIS off".into());
        }
        Err("T-VIS sem backend".into())
    }

    /// Labels → texto → T-NL-IN (nunca `act` direto).
    pub fn to_goal(&self, image_ref: &str, port: &mut ParserPort) -> Result<Goal, String> {
        let obs = self.observe(image_ref, None)?;
        let best = obs
            .labels
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((label, conf)) = best else {
            return Err("T-VIS sem labels".into());
        };
        if *conf < 0.50 {
            return Err("T-VIS conf baixa → ask".into());
        }
        // Stub: entidade conhecida → ask capital.
        let text = if label.eq_ignore_ascii_case("brasil") {
            "ask Brasil capital".to_string()
        } else {
            format!("ask {label} capital")
        };
        port.parse(&text, &GoalSchema::agent_default())
            .map_err(|e: LanguageError| format!("{e:?}"))
    }

    /// Status.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-VIS")),
            ("enabled", Json::Bool(self.enabled)),
            (
                "feature",
                Json::str(if cfg!(feature = "vision") {
                    "on"
                } else {
                    "off"
                }),
            ),
            ("pixels_in_t0", Json::Bool(false)),
            ("auth_cap", Json::num(f64::from(VISION_AUTH_CAP))),
            ("act_alone", Json::Bool(false)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_goal() {
        let v = VisionPort::off();
        let mut p = ParserPort::new();
        let g = v.to_goal("img/brasil.png", &mut p).unwrap();
        assert!(g.subject.as_deref().unwrap().eq_ignore_ascii_case("brasil"));
    }

    #[test]
    fn dark_ask() {
        let v = VisionPort::off();
        let mut p = ParserPort::new();
        assert!(v.to_goal("dark/x", &mut p).unwrap_err().contains("ask"));
    }
}

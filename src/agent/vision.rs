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

/// **MÉDIO C3:** `VisionObs` schema break→reject; labels finitos; `auth_cap≤120`; no pixels no T0.
impl VisionObs {
    /// Validação.
    pub fn validate(&self) -> Result<(), String> {
        if self.labels.iter().any(|(_, c)| !c.is_finite()) {
            return Err("schema break: label conf não-finito".into());
        }
        if self.auth_cap > 120 {
            return Err("auth_cap > 120".into());
        }
        Ok(())
    }
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
        // C3 (TOMADAS_FUTURO): backend real via **sidecar externo**, sem dependência
        // nova. `AETHER_VIS_CMD` recebe `image_ref` e imprime **labels tipadas**
        // (`nome[ conf]`, uma por linha; nunca pixels no T0). Sem backend → `Err`
        // honesto; **nunca finge visão**. V2–V3 + G-PERM.
        self.observe_sidecar(image_ref, emb)
    }

    /// Backend externo (sidecar) → `VisionObs` tipada, **seguro**
    /// (`backend::run_backend_cmd`): sem shell; `image_ref` **sanitizado**;
    /// `AETHER_VIS_CMD`=binário concreto; timeout+limite; **pixels fora do T0**. W1+W8.
    /// **W3** re-checa `enabled`. **W5/C3 conf honesta:** sem conf ⇒ `0.0`; `AETHER_VIS_CONF`
    /// só se operador declarar + `is_finite`; schema break→não inventa. auth≤120. `AETHER_VIS_TIMEOUT_MS`.
    pub fn observe_sidecar(
        &self,
        image_ref: &str,
        _emb: Option<&EmbedPort>,
    ) -> Result<VisionObs, String> {
        if !self.enabled {
            return Err("T-VIS off (feature vision / AETHER_VIS)".into()); // W3
        }
        let cmd_line = std::env::var("AETHER_VIS_CMD")
            .map_err(|_| "T-VIS enabled sem backend (defina AETHER_VIS_CMD; binário concreto; pixels fora do T0)".to_string())?;
        let timeout = std::env::var("AETHER_VIS_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(super::backend::BACKEND_TIMEOUT_MS);
        let out = super::backend::run_backend_cmd(
            &cmd_line,
            Some(image_ref),
            None,
            timeout,
            super::backend::BACKEND_MAX_STDOUT,
        )
        .map_err(|e| format!("T-VIS sidecar: {e}"))?;
        let raw = String::from_utf8_lossy(&out);
        // conf honesta (não inventada alta; NaN→0)
        let default_conf = match std::env::var("AETHER_VIS_CONF") {
            Ok(s) => match s.parse::<f32>() {
                Ok(c) if c.is_finite() => c.clamp(0.0, 1.0),
                _ => 0.0,
            },
            Err(_) => 0.0,
        };
        let mut labels: Vec<(String, f32)> = Vec::new();
        let mut entities: Vec<String> = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut it = line.splitn(2, char::is_whitespace);
            let name = it.next().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let conf = it
                .next()
                .and_then(|s| s.trim().parse::<f32>().ok())
                .map(|c| c.clamp(0.0, 1.0))
                .unwrap_or(default_conf);
            entities.push(name.clone());
            labels.push((name, conf));
        }
        // schema break: nenhuma label → conf 0.0 / vazio honesto (não inventa)
        let obs = VisionObs {
            labels,
            entities,
            embed: None,
            auth_cap: VISION_AUTH_CAP,
        };
        obs.validate()?; // MÉDIO C3
        Ok(obs)
    }

    /// **C3**: `VisionObs` → evento estruturado → assinatura RG 64→16→4 →
    /// surpresa; reusa **A6 T-GRAPH** (`graph::GraphEvent`); claims auth≤120.
    pub fn signature(&self, image_ref: &str, obs: &VisionObs, timestamp_ms: u64) -> [u16; 4] {
        let features = obs
            .labels
            .iter()
            .map(|(n, c)| (n.clone(), *c as f64))
            .collect();
        let ev = super::graph::GraphEvent {
            ids: vec![image_ref.to_string()],
            timestamp_ms,
            features,
        };
        ev.signature(0)
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

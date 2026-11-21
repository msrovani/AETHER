//! T-EMB — tomada de embedding (era declarada). Feature off por default.
//!
//! O core **não** embute modelo. Host passa vetor + `model_id`/`dim`, ou usa
//! sidecar (`tools/embed_server.py`). Misturar dims/eras = deny.
//! F4 hybrid permanece congelada até HITL sancionar novo gate (≥0.974).

use crate::json::Json;

/// Identidade de era de embedding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbedEra {
    /// Modelo (ex.: `paraphrase-multilingual-MiniLM-L12-v2`).
    pub model_id: String,
    /// Dimensão.
    pub dim: usize,
}

impl EmbedEra {
    /// MiniLM 384-dim (contrato F4 / nsgdb-embed) — **não** ativa sozinho.
    pub fn minilm_384() -> Self {
        Self {
            model_id: "paraphrase-multilingual-MiniLM-L12-v2".into(),
            dim: 384,
        }
    }
}

/// Resultado tipado da tomada (nunca tratado como UTF-8 no core).
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedVector {
    /// Era.
    pub era: EmbedEra,
    /// Valores.
    pub values: Vec<f32>,
}

impl EmbedVector {
    /// Valida dim == era.dim.
    pub fn validate(&self) -> Result<(), String> {
        if self.values.len() != self.era.dim {
            return Err(format!(
                "dim mismatch: got {} want {} (era={})",
                self.values.len(),
                self.era.dim,
                self.era.model_id
            ));
        }
        Ok(())
    }

    /// Recusa misturar duas eras.
    pub fn same_era(&self, other: &Self) -> bool {
        self.era.model_id == other.era.model_id && self.era.dim == other.era.dim
    }
}

/// Porta host: default = off (lexical only).
#[derive(Clone, Debug, Default)]
pub struct EmbedPort {
    /// Se `false`, qualquer encode devolve None (ADR-0008 lexical).
    pub enabled: bool,
    /// Era ativa quando enabled.
    pub era: Option<EmbedEra>,
}

impl EmbedPort {
    /// Porta desligada (default seguro).
    pub fn off() -> Self {
        Self::default()
    }

    /// Recusa encode se off ou era ausente.
    pub fn encode_refuse(&self, _text: &str) -> Result<EmbedVector, String> {
        if !self.enabled {
            return Err("T-EMB off: use lexical recall (ADR-0008)".into());
        }
        Err("T-EMB enabled sem backend host — passe embedding= ou sidecar".into())
    }

    /// Aceita vetor do host se bater com a era.
    pub fn accept(&self, era: EmbedEra, values: Vec<f32>) -> Result<EmbedVector, String> {
        if !self.enabled {
            return Err("T-EMB off".into());
        }
        if let Some(active) = &self.era {
            if active.model_id != era.model_id || active.dim != era.dim {
                return Err(format!(
                    "era mismatch: active={}/{} got={}/{}",
                    active.model_id, active.dim, era.model_id, era.dim
                ));
            }
        }
        let v = EmbedVector { era, values };
        v.validate()?;
        Ok(v)
    }

    /// Status JSON para hub/CLI.
    pub fn status_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-EMB")),
            ("enabled", Json::Bool(self.enabled)),
            (
                "era",
                self.era
                    .as_ref()
                    .map(|e| {
                        Json::obj([
                            ("model_id", Json::str(&e.model_id)),
                            ("dim", Json::num(e.dim as f64)),
                        ])
                    })
                    .unwrap_or(Json::Null),
            ),
            ("f4_gate", Json::str("frozen_reprovado_ate_hitl")),
            ("default_recall", Json::str("lexical")),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_recusa_encode() {
        let p = EmbedPort::off();
        assert!(p.encode_refuse("oi").is_err());
    }

    #[test]
    fn misturar_dim_nega() {
        let mut p = EmbedPort {
            enabled: true,
            era: Some(EmbedEra::minilm_384()),
        };
        let bad = p.accept(EmbedEra::minilm_384(), vec![0.0; 128]);
        assert!(bad.is_err());
        let ok = p.accept(EmbedEra::minilm_384(), vec![0.0; 384]);
        assert!(ok.is_ok());
        p.era = Some(EmbedEra {
            model_id: "other".into(),
            dim: 384,
        });
        assert!(p
            .accept(EmbedEra::minilm_384(), vec![0.0; 384])
            .is_err());
    }
}

//! T-EMB — tomada de embedding (era declarada). Feature off por default.
//!
//! O core **não** embute modelo. Host passa vetor + `model_id`/`dim`, ou usa
//! sidecar (`tools/embed_server.py`). Misturar dims/eras = deny.
//! Era encaixada: `BAAI/bge-m3` 1024d (HITL 2026-11-22 — V3-A venceu com
//! 0.9366); `AETHER_EMBED_ERA=minilm` declara a era legada 384.
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

    /// BGE-M3 1024-dim — era **encaixada** (HITL 2026-11-22): vencedor do
    /// V3-A no set v2 congelado (embed hit@5 0.9366; MiniLM 0.6761).
    /// É a era default do sidecar `tools/embed_server.py` (cap 512 tokens).
    pub fn bge_m3_1024() -> Self {
        Self {
            model_id: "BAAI/bge-m3".into(),
            dim: 1024,
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

/// **B3-prep (F0 / TOMADAS_FUTURO §B3)** — dual-bank de embeddings (`T-EMB`
/// era nova), reusa o padrão **dual-bank do T-DEC**.
///
/// * `active` = era vigente (F4 **congelada**: `BAAI/bge-m3` 1024d, off até sanção);
/// * `candidate` = era nova proposta; **candidate nunca roteia**;
/// * **cutover só HITL** (token); **0 mistura de eras** (`dim`/`model_id` checados).
#[derive(Clone, Debug)]
pub struct EmbedBank {
    /// Era ativa no bus.
    pub active: EmbedEra,
    /// Era nova proposta (não roteia).
    pub candidate: Option<EmbedEra>,
}

impl EmbedBank {
    /// F4 congelada hoje (`BAAI/bge-m3` 1024d); sem candidate.
    pub fn frozen_f4() -> Self {
        Self { active: EmbedEra::bge_m3_1024(), candidate: None }
    }

    /// Propor era nova como `candidate`; **não** ativa; rejeita igual a `active`.
    pub fn propose_candidate(mut self, era: EmbedEra) -> Result<Self, String> {
        if era.model_id == self.active.model_id && era.dim == self.active.dim {
            return Err("candidate igual a active".into());
        }
        self.candidate = Some(era);
        Ok(self)
    }

    /// **Cutover só HITL** (token explícito); candidate→active; nunca auto; **nunca mistura**.
    pub fn cutover(mut self, hitl_token: &str) -> Result<Self, String> {
        if hitl_token.trim().is_empty() {
            return Err("cutover B3 requer token HITL".into());
        }
        match self.candidate.take() {
            Some(c) => {
                self.active = c;
                Ok(self)
            }
            None => Err("cutover sem candidate".into()),
        }
    }

    /// Era que roteia no bus = **sempre `active`**; `candidate` nunca roteia.
    pub fn route_era(&self) -> &EmbedEra {
        &self.active
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

    /// Opt-in: `AETHER_EMBED=1` declara a era ativa — default **BGE-M3 1024d**
    /// (encaixado por HITL 2026-11-22). `AETHER_EMBED_ERA=minilm` mantém a era
    /// legada 384 (vetores já gravados no nsgdb). F4 permanece congelada —
    /// hybrid só após `health(view=era)` no host.
    pub fn from_env() -> Self {
        let on = match std::env::var("AETHER_EMBED") {
            Ok(v) => matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            Err(_) => false,
        };
        if on {
            let era = match std::env::var("AETHER_EMBED_ERA") {
                Ok(v) if v.trim().eq_ignore_ascii_case("minilm") => EmbedEra::minilm_384(),
                _ => EmbedEra::bge_m3_1024(),
            };
            Self {
                enabled: true,
                era: Some(era),
            }
        } else {
            Self::off()
        }
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
    fn era_bge_m3_1024_valida_e_nao_mistura() {
        let mut p = EmbedPort {
            enabled: true,
            era: Some(EmbedEra::bge_m3_1024()),
        };
        // era encaixada aceita 1024 e recusa dim alheia
        assert!(p.accept(EmbedEra::bge_m3_1024(), vec![0.0; 1024]).is_ok());
        assert!(p.accept(EmbedEra::bge_m3_1024(), vec![0.0; 384]).is_err());
        // 0 mistura de eras: era legada MiniLM recusada na porta BGE
        assert!(p.accept(EmbedEra::minilm_384(), vec![0.0; 384]).is_err());
        // e o inverso
        p.era = Some(EmbedEra::minilm_384());
        assert!(p.accept(EmbedEra::bge_m3_1024(), vec![0.0; 1024]).is_err());
        // porta off continua negando encode (default lexical intacto)
        assert!(EmbedPort::off().encode_refuse("x").is_err());
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
        assert!(p.accept(EmbedEra::minilm_384(), vec![0.0; 384]).is_err());
    }
}

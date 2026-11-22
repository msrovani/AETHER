//! Fase 8 — Hierarchical MoE + LSH (posição na fila; feature `hier_moe` off).
//!
//! **Não** é pré-requisito de T-NL-IN / hub HITL / STT/VIS/OCR / A8-v1.
//! Ordem: T-EMB + B1 (10k) → decidir D/LSH → M7 router-of-routers → gate.

use crate::json::Json;

/// Path do artefato de posição.
pub const HIER_MOE_STATUS_PATH: &str = "target/aether_hier_moe_status.json";

/// Status da trilha de pesquisa→produto.
#[derive(Clone, Debug)]
pub struct HierMoeStatus {
    /// Feature compilada.
    pub feature_on: bool,
    /// Env opt-in.
    pub env_on: bool,
    /// Dep T-EMB ok?
    pub emb_ready: bool,
    /// Escala B1 (10k) medida?
    pub b1_ready: bool,
    /// No caminho default?
    pub default_path: bool,
}

impl HierMoeStatus {
    /// Lê flags atuais (default: off / não default).
    pub fn from_env(emb_enabled: bool) -> Self {
        let env_on = matches!(
            std::env::var("AETHER_HIER_MOE")
                .ok()
                .as_deref()
                .map(|s| s.trim().to_ascii_lowercase())
                .as_deref(),
            Some("1" | "true" | "yes" | "on")
        );
        Self {
            feature_on: cfg!(feature = "hier_moe"),
            env_on,
            emb_ready: emb_enabled,
            b1_ready: false, // escala ainda não gateada neste release
            default_path: false,
        }
    }

    /// Pode ativar router hierárquico? (todos deps).
    pub fn may_enable(&self) -> bool {
        self.feature_on && self.env_on && self.emb_ready && self.b1_ready
    }

    /// JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str("T-MOE-HIER")),
            ("feature", Json::Bool(self.feature_on)),
            ("env_AETHER_HIER_MOE", Json::Bool(self.env_on)),
            ("emb_ready", Json::Bool(self.emb_ready)),
            ("b1_ready", Json::Bool(self.b1_ready)),
            ("default_path", Json::Bool(self.default_path)),
            ("may_enable", Json::Bool(self.may_enable())),
            (
                "deps",
                Json::str("Fase4 T-EMB + B1 10k + M7 LSH; pós funil T-NL-IN estável"),
            ),
            ("blocks_tnl_in", Json::Bool(false)),
        ])
    }
}

/// Stub LSH: recusa até B1/T-EMB (não no hot path).
pub fn lsh_route_refuse(_key: &str) -> Result<usize, String> {
    Err("LSH off até T-EMB + B1 (Fase 8)".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_nao_bloqueia_tnl_in() {
        let s = HierMoeStatus::from_env(false);
        assert!(!s.may_enable());
        assert!(!s.default_path);
        let j = s.to_json().to_string();
        assert!(j.contains("\"blocks_tnl_in\":false"));
    }
}

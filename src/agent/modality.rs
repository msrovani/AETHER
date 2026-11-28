//! Infraestrutura compartilhada das portas de modalidade (T11).
//!
//! Antes: cada porta (`ocr`, `speech`, `vision`, `sensor`, `haptic`, `ar`,
//! `robot`) copiava o mesmo encanamento — env-gate, `*_CMD`, `*_TIMEOUT_MS`,
//! `*_CONF` — e um bug de contrato (ex. W5 conf honesta) precisava ser
//! corrigido 5×. Agora: **uma** fonte de verdade:
//!
//! * [`ModalConfig::from_env`] — `AETHER_<P>` (1/true), `AETHER_<P>_CMD`,
//!   `AETHER_<P>_TIMEOUT_MS`, `AETHER_<P>_CONF` (só se operador declarar +
//!   `is_finite`; NaN/inf ⇒ 0.0 honesto).
//! * [`ModalError`] — erro tipado com `thiserror` (causa preservada).
//!
//! Contrato **unchanged**: feature off ⇒ no-op honesto; `enabled` = env **e**
//! `cfg!(feature)`; conf não medida ⇒ 0.0; o dado do chamador nunca vai em
//! argv (ref sanitizada / stdin) — isso continua em `backend.rs`.

use thiserror::Error;

/// Erro tipado das portas de modalidade (T11).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModalError {
    /// Feature/cfg/env off → no-op honesto (nunca spawn).
    #[error("{port} off (feature {feature} / AETHER_{env})")]
    Off {
        port: &'static str,
        feature: &'static str,
        env: &'static str,
    },
    /// `enabled` sem backend concreto configurado.
    #[error("{port} enabled sem backend (defina AETHER_{env}_CMD; binário concreto)")]
    NoBackend {
        port: &'static str,
        env: &'static str,
    },
    /// Backend quebrou o schema / saída vazia / conf não finita.
    #[error("{0}")]
    Schema(String),
    /// Falha do backend externo (spawn/timeout/exit/estouro) — causa real.
    #[error("{port} backend: {cause}")]
    Backend { port: &'static str, cause: String },
}

/// Config compartilhada de uma porta: gate (env **e** feature) + backend
/// concreto + timeout + conf declarada pelo operador.
#[derive(Debug, Clone, PartialEq)]
pub struct ModalConfig {
    /// `AETHER_<P>=1|true` **e** feature on.
    pub enabled: bool,
    /// `AETHER_<P>_CMD` (binário concreto, sem shell).
    pub cmd: Option<String>,
    /// `AETHER_<P>_TIMEOUT_MS` (default `BACKEND_TIMEOUT_MS`).
    pub timeout_ms: u64,
    /// `AETHER_<P>_CONF` — None = não medida ⇒ chamador usa 0.0.
    pub conf: Option<f32>,
}

impl ModalConfig {
    /// Lê `AETHER_<P>`, `AETHER_<P>_CMD`, `AETHER_<P>_TIMEOUT_MS` e
    /// `AETHER_<P>_CONF`. `feature_on` = `cfg!(feature = "...")` do chamador.
    pub fn from_env(prefix: &str, feature_on: bool) -> Self {
        let env_on = std::env::var(format!("AETHER_{prefix}"))
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let cmd = std::env::var(format!("AETHER_{prefix}_CMD")).ok();
        let timeout_ms = std::env::var(format!("AETHER_{prefix}_TIMEOUT_MS"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(crate::agent::backend::BACKEND_TIMEOUT_MS);
        let conf = std::env::var(format!("AETHER_{prefix}_CONF"))
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|c| c.is_finite())
            .map(|c| c.clamp(0.0, 1.0));
        Self {
            enabled: env_on && feature_on,
            cmd,
            timeout_ms,
            conf,
        }
    }

    /// Erro de no-op honesto para esta porta (feature/env off).
    pub fn off_err(
        &self,
        port: &'static str,
        feature: &'static str,
        env: &'static str,
    ) -> ModalError {
        ModalError::Off { port, feature, env }
    }
}

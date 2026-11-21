//! T-DEC — status da tomada DecisionPort/Vega para hub e CLI.
//!
//! Feature off → stub honesto (`engine_unavailable` / degraded). Não panica.

use crate::json::Json;

/// Snapshot de telemetria da tomada T-DEC (sem prosa/pixels no T0).
#[derive(Clone, Debug)]
pub struct DecTomadaStatus {
    /// Tomada inventariada.
    pub tomada: &'static str,
    /// Feature/opt-in ativo?
    pub enabled: bool,
    /// Degradado (stub ou engine down)?
    pub degraded: bool,
    /// Motivo estável.
    pub reason: String,
    /// Backend (`stub` | `vega` | `and_gate`).
    pub backend: &'static str,
}

impl DecTomadaStatus {
    /// Default seguro: off → stub.
    pub fn stub_off() -> Self {
        Self {
            tomada: "T-DEC",
            enabled: false,
            degraded: true,
            reason: "engine_unavailable".into(),
            backend: "stub",
        }
    }

    /// Opt-in Vega anunciado (host confirma wiring real).
    pub fn vega_opt_in(degraded: bool, reason: impl Into<String>) -> Self {
        Self {
            tomada: "T-DEC",
            enabled: true,
            degraded,
            reason: reason.into(),
            backend: "vega",
        }
    }

    /// Lê env `AETHER_VEGA=1` / `VEGA_MODEL` para anunciar estado (sem importar PyO3).
    pub fn from_env() -> Self {
        let opt_in = std::env::var("AETHER_VEGA")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            || std::env::var("VEGA_MODEL").is_ok();
        if !opt_in {
            return Self::stub_off();
        }
        Self::vega_opt_in(false, "opt_in_declared")
    }

    /// JSON para hub / Detalhe.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("tomada", Json::str(self.tomada)),
            ("enabled", Json::Bool(self.enabled)),
            ("degraded", Json::Bool(self.degraded)),
            ("reason", Json::str(&self.reason)),
            ("backend", Json::str(self.backend)),
            ("false_act_confident", Json::num(0.0)),
        ])
    }

    /// Linha curta para coluna Detalhe do hub.
    pub fn detail_line(&self) -> String {
        format!(
            "{} backend={} enabled={} degraded={} reason={}",
            self.tomada, self.backend, self.enabled, self.degraded, self.reason
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_off_e_degraded() {
        let s = DecTomadaStatus::stub_off();
        assert!(s.degraded);
        assert!(!s.enabled);
        assert_eq!(s.reason, "engine_unavailable");
        assert!(s.detail_line().contains("T-DEC"));
    }
}

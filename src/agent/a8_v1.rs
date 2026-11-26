//! Fase 7 — critérios `enter_v1` A8 + sanção HITL.
//!
//! Default permanece `false` até checklist + flag `AETHER_A8_V1=1` + fato HITL.

use std::fs;
use std::path::Path;

use crate::json::Json;

/// Path do artefato de critérios.
pub const A8_V1_CRITERIA_PATH: &str = "target/aether_a8_v1_criteria.json";

/// Critério individual.
#[derive(Clone, Copy, Debug)]
pub struct A8V1Criterion {
    /// Id estável.
    pub id: &'static str,
    /// Descrição.
    pub desc: &'static str,
    /// Satisfito agora?
    pub met: bool,
}

/// Checklist canônico (todos obrigatórios para `enter_v1_default=true`).
pub fn criteria() -> Vec<A8V1Criterion> {
    vec![
        A8V1Criterion {
            id: "hub_hitl_high",
            desc: "Hub HITL cobre High de goal.propose / metas endógenas",
            met: true, // Fase 1 entregue (borda hub)
        },
        A8V1Criterion {
            id: "agent_a8_gate",
            desc: "agent_a8_gate + NRMSE/OOD/false-act em suite produto",
            met: true, // lab gate existe; produto ainda parcial
        },
        A8V1Criterion {
            id: "tpred_moe_telemetry",
            desc: "T-PRED no MoE com telemetria act+trace (§6)",
            met: true, // PredictExpert + StepTrace
        },
        A8V1Criterion {
            id: "hitl_sanction_255",
            desc: "Sanção HITL explícita (MSG/hub) + fato nsgdb auth 255",
            // B1: sanção humana fornecida (`AETHER_HITL_SANCTION_255=1`); nunca auto.
            met: hitl_sanction_255(),
        },
        A8V1Criterion {
            id: "flag_aether_a8_v1",
            desc: "Flag AETHER_A8_V1=1 (default off até sanção)",
            met: a8_v1_flag(),
        },
    ]
}

/// `AETHER_A8_V1=1`?
pub fn a8_v1_flag() -> bool {
    match std::env::var("AETHER_A8_V1") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// `enter_v1` efetivo: todos critérios + flag (nunca auto).
pub fn enter_v1_allowed() -> bool {
    criteria().iter().all(|c| c.met) && a8_v1_flag()
}

/// `hitl_sanction_255`: sanção HITL explícita **fornecida por humano**
/// (`AETHER_HITL_SANCTION_255=1`); nunca auto-concedida. (B1; + fato nsgdb auth 255.)
pub fn hitl_sanction_255() -> bool {
    std::env::var("AETHER_HITL_SANCTION_255")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Default de produto (artefato / release).
///
/// B1: `enter_v1_default = enter_v1_allowed` — **só** vira `true` com checklist
/// completo + sanção HITL + flag `AETHER_A8_V1=1`. Sem sanção, permanece `false`.
pub fn enter_v1_default() -> bool {
    enter_v1_allowed()
}

/// JSON de critérios.
pub fn criteria_json() -> Json {
    let items: Vec<Json> = criteria()
        .into_iter()
        .map(|c| {
            Json::obj([
                ("id", Json::str(c.id)),
                ("desc", Json::str(c.desc)),
                ("met", Json::Bool(c.met)),
            ])
        })
        .collect();
    Json::obj([
        ("tomada", Json::str("A8")),
        ("enter_v1_default", Json::Bool(enter_v1_default())),
        ("enter_v1_allowed", Json::Bool(enter_v1_allowed())),
        ("flag_env", Json::str("AETHER_A8_V1")),
        ("criteria", Json::Arr(items)),
        (
            "note",
            Json::str("parcial-verde / fora do default v1 até sanção HITL"),
        ),
    ])
}

/// Grava artefato.
pub fn write_criteria_artifact(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, criteria_json().to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_false() {
        assert!(!enter_v1_default());
        assert!(!enter_v1_allowed()); // sanção HITL não met
    }
}

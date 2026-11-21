//! A7 — matriz de risco publicada + kill switch + HITL calibrado.
//!
//! - `Low` → nunca HITL (só teto do goal).
//! - `Medium` → HITL **opcional** (`AETHER_HITL_MEDIUM=1`).
//! - `High` → HITL **sempre** (token casando `AETHER_HITL_TOKEN`).
//! - `AETHER_AGENT_KILL=1` → corta **toda** call (vence teto/token).

use crate::json::Json;

use super::hitl::{HitlKind, HitlRequest};
use super::tool::{Goal, Risk, ToolError, AUTHORITY_QUARANTINE};

/// Exigência HITL da matriz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitlRequirement {
    /// Sem pedido humano (Low).
    Never,
    /// Opcional: só se `AETHER_HITL_MEDIUM` ligado.
    Optional,
    /// Sempre (High).
    Always,
}

impl HitlRequirement {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Optional => "optional",
            Self::Always => "always",
        }
    }
}

/// Linha da matriz publicada (A7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskRow {
    /// Classe estável.
    pub class: &'static str,
    /// Risco canônico.
    pub risk: Risk,
    /// Política HITL.
    pub hitl: HitlRequirement,
    /// Exemplos de tools.
    pub examples: &'static [&'static str],
}

/// Matriz canônica (não inventar classes fora daqui).
pub const RISK_MATRIX: &[RiskRow] = &[
    RiskRow {
        class: "read_local",
        risk: Risk::Low,
        hitl: HitlRequirement::Never,
        examples: &[
            "fact",
            "sat",
            "qubo",
            "memory.recall",
            "language.parse",
            "code.check",
            "code.ast_grep",
            "code.test",
            "verify",
            "predict.series",
        ],
    },
    RiskRow {
        class: "world_sandbox",
        risk: Risk::Medium,
        hitl: HitlRequirement::Optional,
        examples: &[
            "fs.read",
            "fs.write",
            "http.get",
            "mcp.call",
            "memory.write",
            "code.patch",
            "code.generate",
            "code.apply_proposal",
            "code.loop",
            "code.grammar",
            "code.lm",
            "code.eval",
            "research.query",
        ],
    },
    RiskRow {
        class: "privileged",
        risk: Risk::High,
        hitl: HitlRequirement::Always,
        examples: &["shell.exec", "research.expand", "ask.human", "goal.propose"],
    },
];

/// Kill switch global do agente.
pub fn agent_kill_switch() -> bool {
    env_flag("AETHER_AGENT_KILL")
}

/// Medium exige HITL quando ligado.
pub fn hitl_medium_required() -> bool {
    env_flag("AETHER_HITL_MEDIUM")
}

fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "kill"
        ),
        Err(_) => false,
    }
}

/// Exigência efetiva para o risco da tool.
pub fn requirement_for(risk: Risk) -> HitlRequirement {
    match risk {
        Risk::Low => HitlRequirement::Never,
        Risk::Medium => HitlRequirement::Optional,
        Risk::High => HitlRequirement::Always,
    }
}

/// Classe da matriz que casa o nome da tool (fallback pelo risco).
pub fn class_for_tool(name: &str, risk: Risk) -> &'static str {
    for row in RISK_MATRIX {
        if row.examples.iter().any(|e| *e == name) {
            return row.class;
        }
    }
    match risk {
        Risk::Low => "read_local",
        Risk::Medium => "world_sandbox",
        Risk::High => "privileged",
    }
}

fn expected_hitl_token() -> Option<String> {
    std::env::var("AETHER_HITL_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Extrai `hitl_token` do `goal.query` (JSON) se presente.
pub fn goal_hitl_token(goal: &Goal) -> Option<String> {
    let raw = goal.query.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(v) = Json::parse(raw) {
        if let Some(t) = v.get("hitl_token").and_then(|t| t.as_str()) {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Token do goal casa com `AETHER_HITL_TOKEN` do host.
pub fn hitl_token_ok(goal: &Goal) -> bool {
    let Some(expected) = expected_hitl_token() else {
        return false;
    };
    let Some(presented) = goal_hitl_token(goal) else {
        return false;
    };
    token_eq(presented.trim(), &expected)
}

/// Gate A7 **antes** da call (kill → High always → Medium optional).
pub fn gate_before_call(
    tool_name: &str,
    tool_risk: Risk,
    goal: &Goal,
    trace_id: u64,
) -> Result<(), ToolError> {
    if agent_kill_switch() {
        return Err(ToolError::Failed(
            "AETHER_AGENT_KILL: agente parado (kill switch A7)".into(),
        ));
    }
    let class = class_for_tool(tool_name, tool_risk);
    let req = requirement_for(tool_risk);
    let needs = match req {
        HitlRequirement::Never => false,
        HitlRequirement::Optional => hitl_medium_required(),
        HitlRequirement::Always => true,
    };
    if !needs {
        return Ok(());
    }
    if hitl_token_ok(goal) {
        return Ok(());
    }
    let reason = match req {
        HitlRequirement::Always => format!(
            "A7: risco high (`{tool_name}` class={class}) exige hitl_token + AETHER_HITL_TOKEN"
        ),
        HitlRequirement::Optional => format!(
            "A7: AETHER_HITL_MEDIUM=1 — `{tool_name}` (class={class}) exige hitl_token"
        ),
        HitlRequirement::Never => unreachable!(),
    };
    let hitl = HitlRequest {
        request_id: trace_id,
        kind: HitlKind::Ask,
        reason: reason.clone(),
        from_authority: AUTHORITY_QUARANTINE,
        to_authority: AUTHORITY_QUARANTINE,
        subject: Some(tool_name.into()),
        timeout_ms: 0,
    };
    Err(ToolError::NeedsHuman(format!(
        "{reason} request={}",
        hitl.to_json()
    )))
}

/// JSON da matriz (artefato / audit).
pub fn matrix_json() -> Json {
    let rows: Vec<Json> = RISK_MATRIX
        .iter()
        .map(|row| {
            Json::obj([
                ("class", Json::str(row.class)),
                ("risk", Json::str(row.risk.label())),
                ("hitl", Json::str(row.hitl.label())),
                (
                    "examples",
                    Json::Arr(row.examples.iter().map(|e| Json::str(*e)).collect()),
                ),
            ])
        })
        .collect();
    Json::obj([
        ("schema", Json::str("aether_risk_matrix/a7")),
        ("kill_env", Json::str("AETHER_AGENT_KILL")),
        ("medium_hitl_env", Json::str("AETHER_HITL_MEDIUM")),
        ("token_env", Json::str("AETHER_HITL_TOKEN")),
        ("rows", Json::Arr(rows)),
    ])
}

/// Grava artefato da matriz (gate D-G style).
pub fn write_matrix_artifact(path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, matrix_json().to_text())
}

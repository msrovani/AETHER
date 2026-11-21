//! K6 — small code LM local (offline); budget de RAM no host.
//!
//! Opt-in: `AETHER_CODE_GENERATE=small_lm` → `tools/code_lm_local.py`.
//! Sem rede por default. Auth ≤120. Núcleo não carrega pesos.

use super::grammar::parse_constrained;
use super::{sandbox_root, CODE_AUTH_CAP, PatchProposal};
use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;
use crate::json::Json;
use std::path::PathBuf;
use std::process::Command;

/// Caps declaradas do small code LM (borda).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeLmCaps {
    /// Offline (sem rede).
    pub offline: bool,
    /// Teto de RAM declarado (MiB).
    pub max_ram_mb: u32,
}

impl Default for CodeLmCaps {
    fn default() -> Self {
        Self {
            offline: true,
            max_ram_mb: 2048,
        }
    }
}

/// Lê caps de env (`AETHER_CODE_LM_RAM_MB`, default 2048).
pub fn code_lm_caps() -> CodeLmCaps {
    let max_ram_mb = std::env::var("AETHER_CODE_LM_RAM_MB")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(2048);
    CodeLmCaps {
        offline: true,
        max_ram_mb,
    }
}

fn code_lm_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/code_lm_local.py")
}

/// Script local presente?
pub(crate) fn code_lm_script_exists() -> bool {
    code_lm_script().is_file()
}

/// Invoca o small LM local (offline) e devolve proposal + auth + backend + ram.
pub fn invoke_small_lm(brief: &str) -> Result<(PatchProposal, u8, String, u32), ToolError> {
    let caps = code_lm_caps();
    let script = code_lm_script();
    if !script.is_file() {
        return Err(ToolError::Failed(format!(
            "code_lm_local ausente: {}",
            script.display()
        )));
    }
    let out = Command::new("python")
        .arg(&script)
        .arg(brief)
        .arg(format!("--ram-budget={}", caps.max_ram_mb))
        .output()
        .map_err(|e| ToolError::Failed(format!("spawn code_lm_local: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(ToolError::Failed(format!(
            "code_lm_local exit={:?}: {err}",
            out.status.code()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v = Json::parse(text.trim()).map_err(|e| ToolError::Failed(format!("json lm: {e}")))?;
    if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
        let err = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("erro");
        return Err(ToolError::Failed(format!("code_lm_local: {err}")));
    }
    if v.get("offline").and_then(|o| o.as_bool()) == Some(false) {
        return Err(ToolError::Precondition(
            "small_lm recusou: backend pediu rede (K6 offline)".into(),
        ));
    }
    let ram = v
        .get("ram_mb")
        .and_then(|r| r.as_u64().or_else(|| r.as_f64().map(|f| f as u64)))
        .unwrap_or(64) as u32;
    if ram > caps.max_ram_mb {
        return Err(ToolError::Precondition(format!(
            "small_lm RAM {ram}MiB > budget {}MiB",
            caps.max_ram_mb
        )));
    }
    let prop = if let Some(inner) = v.get("proposal") {
        parse_constrained(&inner.to_text(), None)?
    } else {
        parse_constrained(text.trim(), None)?
    };
    let auth = v
        .get("authority")
        .and_then(|a| a.as_u64().or_else(|| a.as_f64().map(|f| f as u64)))
        .unwrap_or(60) as u8;
    let backend = v
        .get("backend")
        .and_then(|b| b.as_str())
        .unwrap_or("small_lm_local")
        .to_string();
    Ok((prop, auth.min(CODE_AUTH_CAP), backend, ram))
}

/// pass@k sobre briefs: sucesso se ≥1 de `k` amostras parseia via grammar.
///
/// Offline: varia `seed` no envelope JSON (`"_seed"`) para o script local.
pub fn pass_at_k(briefs: &[&str], k: usize) -> f64 {
    if briefs.is_empty() || k == 0 {
        return 0.0;
    }
    let mut ok = 0usize;
    for brief in briefs {
        let mut hit = false;
        for seed in 0..k {
            let seeded = inject_seed(brief, seed);
            if invoke_small_lm(&seeded).is_ok() {
                hit = true;
                break;
            }
        }
        if hit {
            ok += 1;
        }
    }
    ok as f64 / briefs.len() as f64
}

fn inject_seed(brief: &str, seed: usize) -> String {
    if let Ok(mut v) = Json::parse(brief.trim()) {
        if let Json::Obj(ref mut pairs) = v {
            pairs.retain(|(k, _)| k != "_seed");
            pairs.push(("_seed".into(), Json::num(seed as f64)));
            return v.to_text();
        }
    }
    // Prosa: anexa marcador ignorado pelo extract se não for JSON — wrap.
    format!("{{\"brief\":{brief:?},\"_seed\":{seed},\"path\":\"x.txt\",\"op\":\"delete\",\"anchor\":\"Z\"}}")
}

/// `code.lm` — small code LM local (K6); propõe, não aplica.
#[derive(Debug)]
pub struct CodeLmTool {
    root: PathBuf,
}

impl CodeLmTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeLmTool {
    fn name(&self) -> &'static str {
        "code.lm"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.lm",
            description: "K6 small code LM local offline; proposal auth≤120; sem apply",
            args: &["query"],
            kinds: &[GoalKind::Transform, GoalKind::Retrieve],
        }
    }

    fn cost(&self) -> Cost {
        Cost {
            ms: 300.0,
            tokens: 1024,
        }
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.lm") && goal.query.is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let q = goal
            .query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArgs("brief ausente".into()))?;
        if !code_lm_script().is_file() {
            return Err(ToolError::Failed("code_lm_local.py ausente".into()));
        }
        let _ = q;
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let brief = goal.query.as_deref().unwrap().trim();
        let caps = code_lm_caps();
        let (prop, auth, backend, ram) = invoke_small_lm(brief)?;
        // Validação leve na sandbox se o arquivo existir.
        if let Ok(path) =
            crate::agent::tools::fs::resolve_sandbox_path(&self.root, &prop.path)
        {
            if path.is_file() {
                let src = std::fs::read_to_string(&path)
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                let _ = prop.apply(&src)?;
            }
        }
        let auth = auth.min(CODE_AUTH_CAP);
        Ok(ToolOutcome {
            claims: vec![
                Claim::new(
                    prop.path.clone(),
                    "code.proposal",
                    prop.to_json(),
                    0.7,
                    auth,
                    ExpertId::Memory,
                    Provenance::Inferred,
                ),
                Claim::new(
                    prop.path.clone(),
                    "code.lm",
                    format!("backend={backend} ram_mb={ram} offline={}", caps.offline),
                    0.7,
                    auth,
                    ExpertId::Memory,
                    Provenance::Inferred,
                ),
            ],
            confidence: 0.7,
            detail: format!(
                "code.lm ok backend={backend} ram_mb={ram}/{} offline={} \
                 applied=false auth_cap={CODE_AUTH_CAP} scope={:?} trace={}",
                caps.max_ram_mb,
                caps.offline,
                ctx.scope,
                ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

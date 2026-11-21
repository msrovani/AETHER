//! K3 — `code.generate`: propõe `PatchProposal`; nunca aplica (sem auto-merge).
//!
//! Opt-in `AETHER_CODE_GENERATE`:
//! - off (default) → não inventa (`codegen_off`)
//! - `stub` → proposta tipada a partir do JSON de brief
//! - `1`/`live` → `tools/code_generate.py` (borda LLM/code LM)
//! - `small_lm` → `tools/code_lm_local.py` (K6 offline)

use super::{sandbox_root, CODE_AUTH_CAP, PatchProposal};
use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use crate::agent::tools::fs::resolve_sandbox_path;
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;
use crate::json::Json;
use std::path::PathBuf;
use std::process::Command;

/// Modo do gerador de código (borda).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeGenMode {
    /// Desligado — não inventa.
    Off,
    /// Stub determinístico (testes / CI).
    Stub,
    /// Script live (`code_generate.py`).
    Live,
    /// Small code LM local offline (K6).
    SmallLm,
}

/// Lê `AETHER_CODE_GENERATE`.
pub fn code_generate_mode() -> CodeGenMode {
    match std::env::var("AETHER_CODE_GENERATE") {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" | "live" => CodeGenMode::Live,
            "stub" => CodeGenMode::Stub,
            "small_lm" | "small-lm" | "local" => CodeGenMode::SmallLm,
            _ => CodeGenMode::Off,
        },
        Err(_) => CodeGenMode::Off,
    }
}

fn code_generate_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/code_generate.py")
}

/// Parse brief → `PatchProposal` (K5 grammar; stub / envelope / prosa com JSON).
#[allow(dead_code)] // API simétrica; callers usam `_in` com sandbox.
pub(crate) fn proposal_from_brief(raw: &str) -> Result<PatchProposal, ToolError> {
    proposal_from_brief_in(raw, None)
}

/// Como [`proposal_from_brief`], com sandbox para RASP (`anchor_hint`).
pub(crate) fn proposal_from_brief_in(
    raw: &str,
    root: Option<&std::path::Path>,
) -> Result<PatchProposal, ToolError> {
    if let Ok(p) = super::grammar::parse_constrained(raw, root) {
        return Ok(p);
    }
    // Fallback: envelope/campos no topo (sem fence).
    let v = Json::parse(raw.trim()).map_err(|e| ToolError::InvalidArgs(format!("brief json: {e}")))?;
    if let Some(inner) = v.get("proposal") {
        let prop = PatchProposal::from_json(&inner.to_text())?;
        super::grammar::grammar_validate(&prop)?;
        return Ok(prop);
    }
    let path = v
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("brief: path ausente".into()))?;
    let op = v
        .get("op")
        .and_then(|o| o.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("brief: op ausente".into()))?;
    let anchor = v
        .get("anchor")
        .and_then(|a| a.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("brief: anchor ausente".into()))?;
    let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let prop = PatchProposal::from_json(
        &Json::obj([
            ("path", Json::str(path)),
            ("op", Json::str(op)),
            ("anchor", Json::str(anchor)),
            ("content", Json::str(content)),
        ])
        .to_text(),
    )?;
    super::grammar::grammar_validate(&prop)?;
    Ok(prop)
}

pub(crate) fn proposal_from_live(raw: &str) -> Result<(PatchProposal, u8, String), ToolError> {
    let script = code_generate_script();
    if !script.is_file() {
        return Err(ToolError::Failed(format!(
            "code_generate ausente: {}",
            script.display()
        )));
    }
    let out = Command::new("python")
        .arg(&script)
        .arg(raw)
        .output()
        .map_err(|e| ToolError::Failed(format!("spawn code_generate: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(ToolError::Failed(format!(
            "code_generate exit={:?}: {err}",
            out.status.code()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v = Json::parse(text.trim()).map_err(|e| ToolError::Failed(format!("json live: {e}")))?;
    if v.get("ok").and_then(|o| o.as_bool()) == Some(false) {
        let err = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("erro");
        return Err(ToolError::Failed(format!("code_generate: {err}")));
    }
    let prop = if let Some(inner) = v.get("proposal") {
        PatchProposal::from_json(&inner.to_text())?
    } else {
        PatchProposal::from_json(text.trim())?
    };
    let auth = v
        .get("authority")
        .and_then(|a| a.as_u64().or_else(|| a.as_f64().map(|f| f as u64)))
        .unwrap_or(60) as u8;
    let backend = v
        .get("backend")
        .and_then(|b| b.as_str())
        .unwrap_or("code_generate.py")
        .to_string();
    Ok((prop, auth.min(CODE_AUTH_CAP), backend))
}

pub(crate) fn proposal_claim(prop: &PatchProposal, backend: &str, auth: u8) -> Vec<Claim> {
    let auth = auth.min(CODE_AUTH_CAP);
    vec![
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
            "code.generate",
            format!("backend={backend} op={}", prop.op.label()),
            0.7,
            auth,
            ExpertId::Memory,
            Provenance::Inferred,
        ),
    ]
}

/// `code.generate` — propõe patch; **não** escreve arquivo (K3).
#[derive(Debug)]
pub struct CodeGenerateTool {
    root: PathBuf,
}

impl CodeGenerateTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeGenerateTool {
    fn name(&self) -> &'static str {
        "code.generate"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.generate",
            description: "propõe PatchProposal (LLM/stub); auth≤120; sem auto-merge",
            args: &["query"],
            kinds: &[GoalKind::Transform, GoalKind::Retrieve],
        }
    }

    fn cost(&self) -> Cost {
        // Cara de propósito (EFE prefere check/patch tipado).
        Cost {
            ms: 200.0,
            tokens: 512,
        }
    }

    fn risk(&self) -> Risk {
        // Proposal-only (sem FsWrite): Medium. High fica para apply/shell (K4/K8).
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.generate") && goal.query.is_some() {
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
            .ok_or_else(|| ToolError::InvalidArgs("brief/query ausente".into()))?;
        match code_generate_mode() {
            CodeGenMode::Off => Ok(()),
            CodeGenMode::Stub => {
                let _ = proposal_from_brief_in(q, Some(&self.root))?;
                Ok(())
            }
            CodeGenMode::Live => {
                if !code_generate_script().is_file() {
                    return Err(ToolError::Failed("code_generate.py ausente".into()));
                }
                Ok(())
            }
            CodeGenMode::SmallLm => {
                if !super::small_lm::code_lm_script_exists() {
                    return Err(ToolError::Failed("code_lm_local.py ausente".into()));
                }
                Ok(())
            }
        }
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let brief = goal.query.as_deref().unwrap().trim();
        let mode = code_generate_mode();
        if mode == CodeGenMode::Off {
            return Ok(ToolOutcome {
                claims: Vec::new(),
                confidence: 0.0,
                detail: format!(
                    "code.generate codegen_off scope={:?} trace={} — AETHER_CODE_GENERATE=stub|small_lm|1",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }

        let (prop, auth, backend) = propose_from_mode_in(brief, mode, Some(&self.root))?;
        validate_anchor_if_present(&self.root, &prop)?;
        let claims = proposal_claim(&prop, &backend, auth);
        // Cap defensivo.
        let claims: Vec<Claim> = claims
            .into_iter()
            .map(|mut c| {
                c.authority = c.authority.min(CODE_AUTH_CAP);
                c
            })
            .collect();
        // Nunca registra Effect::FsWrite — sem auto-merge.
        Ok(ToolOutcome {
            claims,
            confidence: 0.7,
            detail: format!(
                "code.generate proposal backend={backend} auth_cap={CODE_AUTH_CAP} \
                 applied=false scope={:?} trace={}",
                ctx.scope, ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

/// Resolve proposta conforme modo (não trata `Off`).
#[allow(dead_code)]
pub(crate) fn propose_from_mode(
    brief: &str,
    mode: CodeGenMode,
) -> Result<(PatchProposal, u8, String), ToolError> {
    propose_from_mode_in(brief, mode, None)
}

pub(crate) fn propose_from_mode_in(
    brief: &str,
    mode: CodeGenMode,
    root: Option<&std::path::Path>,
) -> Result<(PatchProposal, u8, String), ToolError> {
    match mode {
        CodeGenMode::Stub => {
            let p = proposal_from_brief_in(brief, root)?;
            Ok((p, CODE_AUTH_CAP.min(60), "stub".to_string()))
        }
        CodeGenMode::Live => proposal_from_live(brief),
        CodeGenMode::SmallLm => {
            let (p, auth, backend, _ram) = super::small_lm::invoke_small_lm(brief)?;
            Ok((p, auth, backend))
        }
        CodeGenMode::Off => Err(ToolError::Precondition("codegen_off".into())),
    }
}

pub(crate) fn validate_anchor_if_present(
    root: &std::path::Path,
    prop: &PatchProposal,
) -> Result<(), ToolError> {
    if let Ok(path) = resolve_sandbox_path(root, &prop.path) {
        if path.is_file() {
            let src =
                std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
            let _ = prop.apply(&src)?;
        }
    }
    Ok(())
}

/// Lê `test_expect` opcional do brief JSON.
pub(crate) fn test_expect_from_brief(raw: &str) -> String {
    Json::parse(raw.trim())
        .ok()
        .and_then(|v| {
            v.get("test_expect")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "# AETHER_TEST_OK".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_brief_vira_proposal() {
        let p = proposal_from_brief(
            r#"{"path":"a.rs","op":"replace","anchor":"OLD","content":"NEW"}"#,
        )
        .unwrap();
        assert_eq!(p.path, "a.rs");
        assert_eq!(p.to_json().contains("NEW"), true);
    }
}

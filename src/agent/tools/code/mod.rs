//! T-CODE K1–K8 — check / … / lm / eval T3; shell HITL em `tools/shell.rs`.
//!
//! K1: só lê e valida sob `sandbox_root`.
//! K2: ver [`patch`] (`PatchProposal` + `code.patch`).
//! K3: ver [`generate`] (`code.generate` — propõe, não aplica).
//! K4: ver [`loop_tool`] (`code.loop` keep|rollback + `code.apply_proposal`).
//! K5: ver [`grammar`] (grammar/RASP → proposal parseável).
//! K6: ver [`small_lm`] (small code LM local offline).
//! K7: ver [`eval_t3`] (`code.eval` + artefato D-G2).
//! K8: `shell.exec` HITL + kill switch (módulo `tools/shell`).

mod eval_t3;
mod generate;
mod grammar;
mod loop_tool;
mod patch;
mod small_lm;

pub use eval_t3::{
    coder_t3_generator_declared, default_coder_t3_path, run_coder_t3, CodeEvalTool, CoderT3Report,
    CODER_T3_PILOT,
};
pub use generate::{code_generate_mode, CodeGenMode, CodeGenerateTool};
pub use grammar::{
    extract_json_object, grammar_parse_rate, parse_constrained, rasp_resolve_anchor,
    CodeGrammarTool,
};
pub use loop_tool::{CodeApplyProposalTool, CodeLoopTool};
pub use patch::{CodePatchTool, PatchOp, PatchProposal};
pub use small_lm::{code_lm_caps, pass_at_k, CodeLmCaps, CodeLmTool};

use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
    AUTHORITY_QUARANTINE,
};
use crate::agent::tools::fs::resolve_sandbox_path;
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;
use std::path::PathBuf;

/// Cap de autoridade para claims de coder (quarentena).
pub const CODE_AUTH_CAP: u8 = 120;

pub(crate) fn claim(
    subj: impl Into<String>,
    rel: &str,
    val: impl Into<String>,
    conf: f32,
) -> Claim {
    Claim::new(
        subj,
        rel,
        val,
        conf,
        AUTHORITY_QUARANTINE.min(CODE_AUTH_CAP),
        ExpertId::Memory,
        Provenance::Inferred,
    )
}

pub(crate) fn sandbox_root(policy: &WorldPolicy) -> PathBuf {
    policy.sandbox_root.clone()
}

/// `code.check` — lê arquivo na sandbox e reporta metadados (K1).
#[derive(Debug)]
pub struct CodeCheckTool {
    root: PathBuf,
}

impl CodeCheckTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeCheckTool {
    fn name(&self) -> &'static str {
        "code.check"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.check",
            description: "valida arquivo na sandbox (existe, utf-8, linhas); sem gerar",
            args: &["path"],
            kinds: &[GoalKind::Verify, GoalKind::Retrieve],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(3.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.check") && goal.path.is_some() {
            1.0
        } else if goal.kind == GoalKind::Verify && goal.path.is_some() {
            0.5
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let rel = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let _ = resolve_sandbox_path(&self.root, rel)?;
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let rel = goal.path.as_deref().unwrap();
        let path = resolve_sandbox_path(&self.root, rel)?;
        let bytes = std::fs::read(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| ToolError::Failed("arquivo não é utf-8".into()))?;
        let lines = text.lines().count();
        let needle = goal.query.as_deref().unwrap_or("");
        let has_needle = needle.is_empty() || text.contains(needle);
        if !has_needle {
            return Ok(ToolOutcome {
                claims: vec![claim(rel, "code.check", "fail:needle", 0.9)],
                confidence: 0.2,
                detail: format!(
                    "code.check fail needle scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }
        Ok(ToolOutcome {
            claims: vec![claim(
                rel,
                "code.check",
                format!("ok lines={lines} bytes={}", text.len()),
                1.0,
            )],
            confidence: 1.0,
            detail: format!("code.check ok scope={:?} trace={}", ctx.scope, ctx.trace_id),
            ..ToolOutcome::default()
        })
    }
}

/// `code.ast_grep` — busca literal na sandbox (K1; sem shell).
#[derive(Debug)]
pub struct CodeAstGrepTool {
    root: PathBuf,
}

impl CodeAstGrepTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeAstGrepTool {
    fn name(&self) -> &'static str {
        "code.ast_grep"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.ast_grep",
            description: "busca padrão literal em arquivo sandbox (ast-grep light)",
            args: &["path", "query"],
            kinds: &[GoalKind::Retrieve, GoalKind::Verify],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(4.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.ast_grep")
            && goal.path.is_some()
            && goal.query.is_some()
        {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let rel = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let pat = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("query/padrão ausente".into()))?;
        if pat.is_empty() {
            return Err(ToolError::InvalidArgs("padrão vazio".into()));
        }
        let path = resolve_sandbox_path(&self.root, rel)?;
        let text = std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let mut hits = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.contains(pat) {
                hits.push(format!("{}:{}", i + 1, line.trim()));
            }
            if hits.len() >= 20 {
                break;
            }
        }
        let n = hits.len();
        let preview = hits.join(" | ");
        Ok(ToolOutcome {
            claims: vec![claim(
                rel,
                "code.ast_grep",
                if n == 0 {
                    "0 hits".into()
                } else {
                    format!("{n} hits: {preview}")
                },
                if n > 0 { 1.0 } else { 0.4 },
            )],
            confidence: if n > 0 { 1.0 } else { 0.3 },
            detail: format!(
                "code.ast_grep hits={n} scope={:?} trace={}",
                ctx.scope, ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

/// `code.test` — harness mínimo na sandbox (K1), sem shell.
#[derive(Debug)]
pub struct CodeTestTool {
    root: PathBuf,
}

impl CodeTestTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeTestTool {
    fn name(&self) -> &'static str {
        "code.test"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.test",
            description: "teste sandbox: exige # AETHER_TEST_OK ou query substring",
            args: &["path", "query"],
            kinds: &[GoalKind::Verify, GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(5.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.test") && goal.path.is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let rel = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let path = resolve_sandbox_path(&self.root, rel)?;
        let text = std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let expect = goal
            .query
            .as_deref()
            .filter(|q| !q.is_empty())
            .unwrap_or("# AETHER_TEST_OK");
        let pass = text.contains(expect);
        Ok(ToolOutcome {
            claims: vec![claim(
                rel,
                "code.test",
                if pass { "pass" } else { "fail" },
                1.0,
            )],
            confidence: if pass { 1.0 } else { 0.1 },
            detail: format!(
                "code.test {} expect={expect:?} scope={:?} trace={}",
                if pass { "pass" } else { "fail" },
                ctx.scope,
                ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

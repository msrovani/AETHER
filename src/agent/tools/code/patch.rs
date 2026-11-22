//! K2 — `PatchProposal` + `code.patch` (replace/insert/delete com âncora única).

use super::{claim, sandbox_root, CODE_AUTH_CAP};
use crate::agent::tool::{
    Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    WorldState,
};
use crate::agent::tools::fs::resolve_sandbox_path;
use crate::agent::WorldPolicy;
use crate::json::Json;
use std::path::PathBuf;

/// Operação tipada de patch (K2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchOp {
    /// Substitui a âncora por `content`.
    Replace,
    /// Insere `content` imediatamente após a âncora.
    Insert,
    /// Remove a âncora.
    Delete,
}

impl PatchOp {
    fn parse(s: &str) -> Result<Self, ToolError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "insert" => Ok(Self::Insert),
            "delete" => Ok(Self::Delete),
            other => Err(ToolError::InvalidArgs(format!("op inválida: {other}"))),
        }
    }

    /// Rótulo estável.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Insert => "insert",
            Self::Delete => "delete",
        }
    }
}

/// Proposta de patch tipada (K2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchProposal {
    /// Path relativo à sandbox.
    pub path: String,
    /// Operação.
    pub op: PatchOp,
    /// Âncora que deve aparecer **exatamente uma** vez.
    pub anchor: String,
    /// Conteúdo (replace/insert); vazio em delete.
    pub content: String,
}

impl PatchProposal {
    /// Parse de JSON (campos: path, op, anchor, content?).
    pub fn from_json(text: &str) -> Result<Self, ToolError> {
        let v =
            Json::parse(text.trim()).map_err(|e| ToolError::InvalidArgs(format!("json: {e}")))?;
        let path = v
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?
            .to_string();
        let op = PatchOp::parse(
            v.get("op")
                .and_then(|o| o.as_str())
                .ok_or_else(|| ToolError::InvalidArgs("op ausente".into()))?,
        )?;
        let anchor = v
            .get("anchor")
            .and_then(|a| a.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("anchor ausente".into()))?
            .to_string();
        if anchor.is_empty() {
            return Err(ToolError::InvalidArgs("anchor vazio".into()));
        }
        let content = v
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if matches!(op, PatchOp::Replace | PatchOp::Insert) && content.is_empty() {
            return Err(ToolError::InvalidArgs(
                "content obrigatório para replace/insert".into(),
            ));
        }
        Ok(Self {
            path,
            op,
            anchor,
            content,
        })
    }

    /// Serializa para JSON compacto (claim `code.proposal`).
    pub fn to_json(&self) -> String {
        Json::obj([
            ("path", Json::str(&self.path)),
            ("op", Json::str(self.op.label())),
            ("anchor", Json::str(&self.anchor)),
            ("content", Json::str(&self.content)),
        ])
        .to_text()
    }

    /// Aplica sobre `src`; Err se âncora ≠ 1 ocorrência.
    pub fn apply(&self, src: &str) -> Result<String, ToolError> {
        let n = src.matches(&self.anchor).count();
        if n == 0 {
            return Err(ToolError::Precondition(
                "anchor não encontrada — abort".into(),
            ));
        }
        if n > 1 {
            return Err(ToolError::Precondition(format!(
                "anchor ambígua ({n}×) — abort"
            )));
        }
        Ok(match self.op {
            PatchOp::Replace => src.replacen(&self.anchor, &self.content, 1),
            PatchOp::Insert => {
                src.replacen(&self.anchor, &format!("{}{}", self.anchor, self.content), 1)
            }
            PatchOp::Delete => src.replacen(&self.anchor, "", 1),
        })
    }
}

/// `code.patch` — aplica `PatchProposal` JSON em `query` (K2).
#[derive(Debug)]
pub struct CodePatchTool {
    root: PathBuf,
}

impl CodePatchTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodePatchTool {
    fn name(&self) -> &'static str {
        "code.patch"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.patch",
            description: "aplica PatchProposal JSON (replace|insert|delete + âncora única)",
            args: &["query"],
            kinds: &[GoalKind::Act, GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(8.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.patch") && goal.query.is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let raw = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("PatchProposal JSON ausente".into()))?;
        let prop = PatchProposal::from_json(raw)?;
        let path = resolve_sandbox_path(&self.root, &prop.path)?;
        let src = std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let _ = prop.apply(&src)?;
        Ok(())
    }

    fn planned_effects(&self, goal: &Goal) -> Vec<Effect> {
        let path = goal
            .query
            .as_deref()
            .and_then(|q| PatchProposal::from_json(q).ok())
            .map(|p| p.path)
            .unwrap_or_default();
        vec![Effect::FsWrite { path, prev: None }]
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let raw = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("PatchProposal JSON ausente".into()))?;
        let prop = PatchProposal::from_json(raw)?;
        let path = resolve_sandbox_path(&self.root, &prop.path)?;
        let prev_bytes = std::fs::read(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let src = String::from_utf8(prev_bytes.clone())
            .map_err(|_| ToolError::Failed("arquivo não utf-8".into()))?;
        let next = prop.apply(&src)?;
        std::fs::write(&path, next.as_bytes()).map_err(|e| ToolError::Failed(e.to_string()))?;
        let abs = path.display().to_string();
        ctx.push_effect(Effect::FsWrite {
            path: abs,
            prev: Some(prev_bytes),
        });
        Ok(ToolOutcome {
            claims: vec![claim(
                prop.path,
                "code.patch",
                format!("applied op={}", prop.op.label()),
                1.0,
            )],
            confidence: 1.0,
            detail: format!(
                "code.patch ok op={} auth_cap={CODE_AUTH_CAP} scope={:?} trace={}",
                prop.op.label(),
                ctx.scope,
                ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

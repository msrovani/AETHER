//! T-FS — leitura/escrita sob `sandbox_root` (A4 / G-A4).
//!
//! Paths são resolvidos de forma canônica; traversal (`..`) e escape da raiz
//! viram `ToolError::Precondition`. Symlinks que saem da raiz também negam.

use std::path::{Component, Path, PathBuf};

use crate::agent::tool::{
    Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    WorldState, AUTHORITY_QUARANTINE,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;

/// Resolve `rel` sob `root` sem permitir escape. Retorna caminho canônico.
pub fn resolve_sandbox_path(root: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    if rel.trim().is_empty() {
        return Err(ToolError::InvalidArgs("path vazio".into()));
    }
    let raw = Path::new(rel);
    if raw.is_absolute() {
        return Err(ToolError::Precondition(
            "path absoluto fora da sandbox".into(),
        ));
    }
    for c in raw.components() {
        match c {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ToolError::Precondition(
                    "path com traversal ou prefixo inválido".into(),
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| {
        let _ = std::fs::create_dir_all(root);
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
    });
    let joined = root.join(raw);
    if let Ok(canon) = std::fs::canonicalize(&joined) {
        if !canon.starts_with(&root) {
            return Err(ToolError::Precondition(
                "path escapa da sandbox (symlink)".into(),
            ));
        }
        return Ok(canon);
    }
    // Arquivo ainda não existe: validar o pai canônico.
    if let Some(parent) = joined.parent() {
        if parent.exists() {
            let parent_canon = std::fs::canonicalize(parent)
                .map_err(|e| ToolError::Precondition(format!("sandbox pai inválido: {e}")))?;
            if !parent_canon.starts_with(&root) {
                return Err(ToolError::Precondition(
                    "path escapa da sandbox (pai)".into(),
                ));
            }
            return Ok(parent_canon.join(joined.file_name().unwrap_or_default()));
        }
    }
    if !joined.starts_with(&root) {
        return Err(ToolError::Precondition("path escapa da sandbox".into()));
    }
    Ok(joined)
}

/// `fs.read`
#[derive(Debug)]
pub struct FsReadTool {
    root: PathBuf,
}

impl FsReadTool {
    /// Nova tool com a política de mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: policy.sandbox_root.clone(),
        }
    }
}

impl Tool for FsReadTool {
    fn name(&self) -> &'static str {
        "fs.read"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "fs.read",
            description: "lê arquivo relativo à sandbox_root",
            args: &["path"],
            kinds: &[GoalKind::Retrieve, GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(2.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.path.is_some()
            && (goal.relation.as_deref() == Some("fs.read")
                || goal.kind == GoalKind::Retrieve && goal.url.is_none() && goal.mcp_name.is_none())
        {
            if goal.relation.as_deref() == Some("fs.read") {
                1.0
            } else {
                0.4
            }
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0 && goal.path.is_some()
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let path = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let _ = resolve_sandbox_path(&self.root, path)?;
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let rel = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let path = resolve_sandbox_path(&self.root, rel)?;
        let bytes = std::fs::read(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let preview: String = text.chars().take(2_048).collect();
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                path.display().to_string(),
                "fs.read",
                preview,
                1.0,
                AUTHORITY_QUARANTINE,
                ExpertId::Memory,
                Provenance::Inferred,
            )],
            confidence: 1.0,
            detail: format!(
                "fs.read ok scope={:?} trace={} bytes={}",
                ctx.scope,
                ctx.trace_id,
                bytes.len()
            ),
            ..ToolOutcome::default()
        })
    }
}

/// `fs.write`
#[derive(Debug)]
pub struct FsWriteTool {
    root: PathBuf,
}

impl FsWriteTool {
    /// Nova tool com a política de mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: policy.sandbox_root.clone(),
        }
    }
}

impl Tool for FsWriteTool {
    fn name(&self) -> &'static str {
        "fs.write"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "fs.write",
            description: "escreve arquivo relativo à sandbox_root (corpo em query)",
            args: &["path", "query"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(3.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.path.is_some() && goal.relation.as_deref() == Some("fs.write") {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let path = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let _ = resolve_sandbox_path(&self.root, path)?;
        Ok(())
    }

    fn planned_effects(&self, goal: &Goal) -> Vec<Effect> {
        let path = goal.path.clone().unwrap_or_default();
        vec![Effect::FsWrite { path, prev: None }]
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let rel = goal
            .path
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
        let body = goal.query.as_deref().unwrap_or("");
        let path = resolve_sandbox_path(&self.root, rel)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Failed(e.to_string()))?;
        }
        let prev = match std::fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(ToolError::Failed(e.to_string())),
        };
        std::fs::write(&path, body.as_bytes()).map_err(|e| ToolError::Failed(e.to_string()))?;
        let abs = path.display().to_string();
        ctx.push_effect(Effect::FsWrite {
            path: abs.clone(),
            prev,
        });
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                abs,
                "fs.write",
                format!("{} bytes", body.len()),
                1.0,
                AUTHORITY_QUARANTINE,
                ExpertId::Memory,
                Provenance::Inferred,
            )],
            confidence: 1.0,
            detail: format!(
                "fs.write ok scope={:?} trace={} auth_cap={}",
                ctx.scope, ctx.trace_id, AUTHORITY_QUARANTINE
            ),
            ..ToolOutcome::default()
        })
    }
}

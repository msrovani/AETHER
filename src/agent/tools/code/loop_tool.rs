//! K4 — `code.loop` (generate→apply→test→keep|rollback) + `code.apply_proposal`.

use super::generate::{
    code_generate_mode, proposal_claim, proposal_from_brief_in, propose_from_mode_in,
    test_expect_from_brief, validate_anchor_if_present, CodeGenMode,
};
use super::{claim, sandbox_root, PatchProposal, CODE_AUTH_CAP};
use crate::agent::tool::{
    Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    WorldState,
};
use crate::agent::tools::fs::resolve_sandbox_path;
use crate::agent::{Claim, WorldPolicy};
use std::path::PathBuf;

/// Aplica proposal no filesystem e registra undo.
pub(crate) fn apply_proposal(
    root: &std::path::Path,
    prop: &PatchProposal,
    ctx: &mut ToolContext,
) -> Result<(), ToolError> {
    let path = resolve_sandbox_path(root, &prop.path)?;
    let prev_bytes = std::fs::read(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
    let src = String::from_utf8(prev_bytes.clone())
        .map_err(|_| ToolError::Failed("arquivo não utf-8".into()))?;
    let next = prop.apply(&src)?;
    std::fs::write(&path, next.as_bytes()).map_err(|e| ToolError::Failed(e.to_string()))?;
    ctx.push_effect(Effect::FsWrite {
        path: path.display().to_string(),
        prev: Some(prev_bytes),
    });
    Ok(())
}

fn run_sandbox_test(root: &std::path::Path, rel: &str, expect: &str) -> Result<bool, ToolError> {
    let path = resolve_sandbox_path(root, rel)?;
    let text = std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
    Ok(text.contains(expect))
}

fn latest_proposal(ctx: &ToolContext) -> Result<PatchProposal, ToolError> {
    ctx.claims
        .iter()
        .rev()
        .find(|c| c.relation == "code.proposal")
        .ok_or_else(|| ToolError::Precondition("sem code.proposal no blackboard".into()))
        .and_then(|c| PatchProposal::from_json(&c.value))
}

/// `code.apply_proposal` — aplica `code.proposal` do blackboard (ou query).
#[derive(Debug)]
pub struct CodeApplyProposalTool {
    root: PathBuf,
}

impl CodeApplyProposalTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeApplyProposalTool {
    fn name(&self) -> &'static str {
        "code.apply_proposal"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.apply_proposal",
            description: "aplica code.proposal do blackboard (K4 bridge); auth≤120",
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
        if goal.relation.as_deref() == Some("code.apply_proposal") {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let prop = if let Some(q) = goal
            .query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if q.starts_with('{') {
                PatchProposal::from_json(q)?
            } else {
                latest_proposal(ctx)?
            }
        } else {
            latest_proposal(ctx)?
        };
        apply_proposal(&self.root, &prop, ctx)?;
        Ok(ToolOutcome {
            claims: vec![claim(
                prop.path,
                "code.apply_proposal",
                format!("applied op={}", prop.op.label()),
                1.0,
            )],
            confidence: 1.0,
            detail: format!(
                "code.apply_proposal ok scope={:?} trace={}",
                ctx.scope, ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

/// `code.loop` — generate → apply → test → keep | rollback (K4).
#[derive(Debug)]
pub struct CodeLoopTool {
    root: PathBuf,
}

impl CodeLoopTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeLoopTool {
    fn name(&self) -> &'static str {
        "code.loop"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.loop",
            description: "K4: generate→apply→test→keep|rollback; auth≤120; keep≠255",
            args: &["query"],
            kinds: &[GoalKind::Act, GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost {
            ms: 250.0,
            tokens: 512,
        }
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.loop") && goal.query.is_some() {
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
        match code_generate_mode() {
            CodeGenMode::Off => Ok(()),
            CodeGenMode::Stub => {
                let _ = proposal_from_brief_in(q, Some(&self.root))?;
                Ok(())
            }
            CodeGenMode::Live => {
                let _ = propose_from_mode_in(q, CodeGenMode::Live, Some(&self.root))?;
                Ok(())
            }
            CodeGenMode::SmallLm => {
                let _ = propose_from_mode_in(q, CodeGenMode::SmallLm, Some(&self.root))?;
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
                    "code.loop codegen_off scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }

        let expect = test_expect_from_brief(brief);
        let (prop, auth, backend) = propose_from_mode_in(brief, mode, Some(&self.root))?;
        validate_anchor_if_present(&self.root, &prop)?;

        let mut claims = proposal_claim(&prop, &backend, auth);
        let mark = ctx.undo_len();
        apply_proposal(&self.root, &prop, ctx)?;
        claims.push(claim(
            prop.path.clone(),
            "code.patch",
            format!("applied op={}", prop.op.label()),
            1.0,
        ));

        // check leve (arquivo ainda utf-8 / existe)
        let path = resolve_sandbox_path(&self.root, &prop.path)?;
        let _ = std::fs::read_to_string(&path).map_err(|e| ToolError::Failed(e.to_string()))?;
        claims.push(claim(prop.path.clone(), "code.check", "ok:post-patch", 1.0));

        let pass = run_sandbox_test(&self.root, &prop.path, &expect)?;
        if pass {
            claims.push(claim(prop.path.clone(), "code.test", "pass", 1.0));
            // keep: NÃO promove autoridade — quarentena permanece.
            claims.push(claim(prop.path.clone(), "code.loop", "keep", 1.0));
            let claims: Vec<Claim> = claims
                .into_iter()
                .map(|mut c| {
                    c.authority = c.authority.min(CODE_AUTH_CAP);
                    c
                })
                .collect();
            Ok(ToolOutcome {
                claims,
                confidence: 0.85,
                detail: format!(
                    "code.loop keep backend={backend} auth_cap={CODE_AUTH_CAP} \
                     applied=true scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            })
        } else {
            let report = ctx.rewind_report(mark);
            if !report.is_ok() {
                return Err(ToolError::Failed(format!(
                    "rollback falhou: {}",
                    report.failed.join("; ")
                )));
            }
            claims.push(claim(prop.path.clone(), "code.test", "fail", 1.0));
            claims.push(claim(prop.path.clone(), "code.loop", "rollback", 1.0));
            let claims: Vec<Claim> = claims
                .into_iter()
                .map(|mut c| {
                    c.authority = c.authority.min(CODE_AUTH_CAP);
                    c
                })
                .collect();
            Ok(ToolOutcome {
                claims,
                confidence: 0.2,
                detail: format!(
                    "code.loop rollback backend={backend} expect={expect:?} \
                     scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            })
        }
    }
}

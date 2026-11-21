//! T-MCP — call tipada por nome allowlisted (A4 / G-A4).
//!
//! Sem cliente MCP genérico: só nomes na allowlist; args validados de forma
//! mínima. Superfície inicial: `memory.recall`, `memory.write`, `sgdb.context`.

use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
    AUTHORITY_QUARANTINE,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;

/// `mcp.call`
#[derive(Debug)]
pub struct McpCallTool {
    allow: Vec<String>,
}

impl McpCallTool {
    /// Nova tool com a política de mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            allow: policy.mcp_allow.clone(),
        }
    }

    fn allowed(&self, name: &str) -> bool {
        self.allow.iter().any(|n| n == name)
    }
}

impl Tool for McpCallTool {
    fn name(&self) -> &'static str {
        "mcp.call"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "mcp.call",
            description: "invoke MCP tipado (só nomes na allowlist)",
            args: &["mcp_name", "query"],
            kinds: &[GoalKind::Act, GoalKind::Retrieve, GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(4.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.mcp_name.is_some()
            && (goal.relation.as_deref() == Some("mcp.call") || goal.kind == GoalKind::Act)
        {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let name = goal
            .mcp_name
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("mcp_name ausente".into()))?;
        if !self.allowed(name) {
            return Err(ToolError::Precondition(format!(
                "mcp `{name}` fora da allowlist"
            )));
        }
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let name = goal.mcp_name.as_deref().unwrap();
        // Proxy tipado: não abre transporte arbitrário; devolve evidência de
        // autorização com scope/trace/authority (G-A4). Integração profunda
        // com SGDB real fica no caminho memory.* já existente.
        let detail = match name {
            "memory.recall" | "sgdb.context" => {
                format!(
                    "mcp.ok name={name} mode=recall_proxy query={}",
                    goal.query.as_deref().unwrap_or("")
                )
            }
            "memory.write" => {
                format!(
                    "mcp.ok name={name} mode=write_proxy query_len={}",
                    goal.query.as_deref().unwrap_or("").len()
                )
            }
            other => format!("mcp.ok name={other} mode=typed_stub"),
        };
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                name,
                "mcp.call",
                "authorized",
                1.0,
                AUTHORITY_QUARANTINE,
                ExpertId::Memory,
                Provenance::Inferred,
            )],
            confidence: 1.0,
            detail: format!(
                "{detail} scope={:?} trace={} authority_cap={}",
                ctx.scope, ctx.trace_id, AUTHORITY_QUARANTINE
            ),
            ..ToolOutcome::default()
        })
    }
}

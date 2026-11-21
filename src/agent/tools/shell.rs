//! Shell — **sempre off** no default (A4 / G-A4 / ADR-006).
//!
//! A tool existe no inventário para que tentativa de uso seja **deny** auditável
//! (`NeedsHuman`), nunca execução silenciosa.

use crate::agent::tool::{
    Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    WorldState,
};

/// `shell.exec` — deny default.
#[derive(Debug, Default)]
pub struct ShellTool;

impl Tool for ShellTool {
    fn name(&self) -> &'static str {
        "shell.exec"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "shell.exec",
            description: "DESLIGADO: shell exige HITL explícito (nunca default)",
            args: &["query"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(0.0)
    }

    fn risk(&self) -> Risk {
        Risk::High
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("shell.exec") || goal.kind == GoalKind::Act {
            // Affordance baixa: planner só escolhe se nada mais serve — e ainda
            // assim a call nega.
            0.05
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn planned_effects(&self, _goal: &Goal) -> Vec<Effect> {
        vec![Effect::Irreversible {
            kind: "shell.exec",
        }]
    }

    fn dry_run(&self, _goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        Err(ToolError::NeedsHuman(
            "shell.exec desligado: exige HITL (G-A4)".into(),
        ))
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Err(ToolError::NeedsHuman(
            "shell.exec desligado: 0 shell sem HITL (G-A4)".into(),
        ))
    }

    fn escalation(&self) -> bool {
        true
    }
}

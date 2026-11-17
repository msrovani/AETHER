//! Tools reais do P1: fatos (FHRR), SAT, QUBO, memória durável e HITL.
//!
//! Cada tool encapsula um especialista e declara schema/custo/risco. Não há
//! efeito externo no P1: memória é leitura; `ask.human` apenas sinaliza.

use super::experts::{make_logic, make_memory, make_solver, make_transplant, Expert};
use super::tool::{
    outcome_from_expert, Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError,
    ToolOutcome, ToolSchema, WorldState, AUTHORITY_CORROBORATED, AUTHORITY_HITL,
    AUTHORITY_QUARANTINE, AUTHORITY_VERIFIED,
};
use super::{Claim, ExpertId, Task};
use crate::agent::AgentConfig;
use crate::core::certificate::Provenance;

/// Tool de fato relacional (FHRR/transplante).
pub struct FactTool {
    expert: Box<dyn Expert>,
}

impl std::fmt::Debug for FactTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FactTool").finish()
    }
}

impl FactTool {
    /// Cria a tool a partir da configuração.
    pub fn new(cfg: &AgentConfig) -> Option<Self> {
        make_transplant(cfg.dim, cfg.traces, &cfg.facts_path).map(|expert| Self { expert })
    }
}

impl Tool for FactTool {
    fn name(&self) -> &'static str {
        "fact"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "fact",
            description: "consulta relacional subject+relation no FHRR do transplante",
            args: &["subject", "relation"],
            kinds: &[GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.5)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Answer
            && goal.subject.is_some()
            && goal.relation.is_some()
            && goal.clauses.is_empty()
        {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let subject = goal
            .subject
            .clone()
            .ok_or_else(|| ToolError::InvalidArgs("subject ausente".into()))?;
        let relation = goal
            .relation
            .clone()
            .ok_or_else(|| ToolError::InvalidArgs("relation ausente".into()))?;
        let output = self.expert.infer(&Task::Fact { subject, relation });
        Ok(outcome_from_expert(ExpertId::Transplant, output))
    }
}

/// Tool de SAT.
pub struct SatTool {
    expert: Box<dyn Expert>,
}

impl std::fmt::Debug for SatTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SatTool").finish()
    }
}

impl SatTool {
    /// Cria a tool.
    pub fn new() -> Self {
        Self {
            expert: make_logic(96, 512),
        }
    }
}

impl Default for SatTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SatTool {
    fn name(&self) -> &'static str {
        "sat"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "sat",
            description: "resolve 3-SAT e devolve assignment + certificado",
            args: &["n", "clauses"],
            kinds: &[GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(20.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Answer && goal.n.is_some() && !goal.clauses.is_empty() {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let n = goal
            .n
            .ok_or_else(|| ToolError::InvalidArgs("n ausente".into()))?;
        let output = self.expert.infer(&Task::Satisfy {
            n,
            clauses: goal.clauses.clone(),
        });
        Ok(outcome_from_expert(ExpertId::Logic, output))
    }
}

/// Tool de QUBO.
pub struct QuboTool {
    expert: Box<dyn Expert>,
}

impl std::fmt::Debug for QuboTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuboTool").finish()
    }
}

impl QuboTool {
    /// Cria a tool.
    pub fn new() -> Self {
        Self {
            expert: make_solver(256, 400),
        }
    }
}

impl Default for QuboTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for QuboTool {
    fn name(&self) -> &'static str {
        "qubo"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "qubo",
            description: "minimiza QUBO e devolve estado + certificado",
            args: &["n", "j", "h"],
            kinds: &[GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(12.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Transform
            && goal.n.is_some()
            && goal.j.len() == goal.n.unwrap_or(0) * goal.n.unwrap_or(0)
            && goal.h.len() == goal.n.unwrap_or(0)
        {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let n = goal
            .n
            .ok_or_else(|| ToolError::InvalidArgs("n ausente".into()))?;
        let output = self.expert.infer(&Task::Optimize {
            n,
            j: goal.j.clone(),
            h: goal.h.clone(),
        });
        Ok(outcome_from_expert(ExpertId::Solver, output))
    }
}

/// Tool de memória durável (leitura).
pub struct MemoryTool {
    expert: Box<dyn Expert>,
}

impl std::fmt::Debug for MemoryTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryTool").finish()
    }
}

impl MemoryTool {
    /// Cria a tool se houver SGDB configurado.
    pub fn new(spec: &str) -> Option<Self> {
        make_memory(spec).map(|expert| Self { expert })
    }
}

impl Tool for MemoryTool {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "memory",
            description: "consulta fatos duráveis no neural-sgdb (leitura)",
            args: &["query"],
            kinds: &[GoalKind::Retrieve],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(50.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Retrieve && goal.query.as_ref().is_some_and(|q| !q.is_empty()) {
            0.9
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let query = goal
            .query
            .clone()
            .ok_or_else(|| ToolError::InvalidArgs("query ausente".into()))?;
        let output = self.expert.infer(&Task::Memory { query });
        Ok(outcome_from_expert(ExpertId::Memory, output))
    }
}

/// Escrita de memória com before-image (MSG 0316 pendente → fechado).
///
/// Requer `ToolContext.undo_backend` com `write`. Emite `Stage` + `MemoryWrite`.
#[derive(Debug, Default)]
pub struct MemoryWriteTool;

impl Tool for MemoryWriteTool {
    fn name(&self) -> &'static str {
        "memory.write"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "memory.write",
            description: "escreve fato na memória com Effect.prev para undo",
            args: &["subject", "relation", "query"],
            kinds: &[GoalKind::Act, GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(40.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        let has_key = goal.subject.is_some() && goal.relation.is_some();
        let has_val = goal.query.as_ref().is_some_and(|q| !q.is_empty());
        if has_key && has_val && matches!(goal.kind, GoalKind::Act | GoalKind::Answer) {
            0.95
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0 && goal.risk_ceiling >= Risk::Medium
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let subject = goal
            .subject
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("subject ausente".into()))?;
        let relation = goal
            .relation
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("relation ausente".into()))?;
        let value = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("query/valor ausente".into()))?;
        let key = format!("{subject}|{relation}");
        let seq = ctx.next_stage_seq();
        let backend = ctx
            .undo_backend
            .as_ref()
            .ok_or_else(|| ToolError::Failed("memory.write exige undo_backend".into()))?;
        // Autoridade: não sobrescreve fato verificado/HITL sem evidência nova.
        let verified = ctx.verified.iter().any(|fact| {
            fact.subject == subject && fact.relation == relation && fact.value == value
        });
        let current = backend.peek(&key);
        if let Some(snapshot) = &current {
            if snapshot.authority == AUTHORITY_HITL || (snapshot.authority >= 200 && !verified) {
                return Err(ToolError::NeedsHuman(format!(
                    "`{key}` tem autoridade {} (>=200): escrita exige HITL ou certificado",
                    snapshot.authority
                )));
            }
        }
        // Autoridade: nunca promove sozinho. Fato novo entra em quarentena
        // (proveniência `Inferred`); promoção exige HITL (>=200) ou
        // **corroboração**: outra fonte independente já registrou o mesmo valor.
        const SOURCE: &str = "inferred";
        let corroborated = current.as_ref().is_some_and(|snapshot| {
            snapshot.text == value
                && !snapshot.provenance.is_empty()
                && snapshot.provenance != SOURCE
                && snapshot.authority < AUTHORITY_CORROBORATED
        });
        let authority = if verified {
            AUTHORITY_VERIFIED
        } else if corroborated {
            AUTHORITY_CORROBORATED
        } else {
            current
                .as_ref()
                .map(|snapshot| snapshot.authority)
                .unwrap_or(AUTHORITY_QUARANTINE)
        };
        // Watermark ANTES da escrita: o rewind volta o outbox a este ponto.
        let outbox_len = backend.outbox_len();
        let prev = backend
            .write_sourced(&key, value, authority, SOURCE)
            .map_err(ToolError::Failed)?;
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                subject,
                relation,
                value,
                1.0,
                authority,
                ExpertId::Memory,
                Provenance::Inferred,
            )],
            effects: vec![
                Effect::Stage {
                    keys: vec![key.clone()],
                    seq,
                    outbox_len,
                },
                Effect::MemoryWrite { key, prev },
            ],
            confidence: 1.0,
            detail: "memory.write ok".into(),
            ..ToolOutcome::default()
        })
    }
}

/// Tool de HITL: não executa, apenas sinaliza.
#[derive(Debug)]
pub struct AskHumanTool;

impl Tool for AskHumanTool {
    fn name(&self) -> &'static str {
        "ask.human"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "ask.human",
            description: "escalona para humano quando risco/incerteza excede o teto",
            args: &["reason"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(0.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Act {
            0.5
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn escalation(&self) -> bool {
        true
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome {
            needs_human: true,
            detail: "escalado para HITL".to_string(),
            ..ToolOutcome::default()
        })
    }
}

/// Registry padrão do agente.
pub fn default_tools(cfg: &AgentConfig) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    if let Some(tool) = FactTool::new(cfg) {
        tools.push(Box::new(tool));
    }
    tools.push(Box::new(SatTool::new()));
    tools.push(Box::new(QuboTool::new()));
    if let Some(spec) = &cfg.sgdb {
        if let Some(tool) = MemoryTool::new(spec) {
            tools.push(Box::new(tool));
        }
    }
    tools.push(Box::new(AskHumanTool));
    tools.push(Box::new(MemoryWriteTool));
    // C0 LanguagePort (MSG 0292 ACK_Q1): tool em language.rs, registro aqui.
    tools.push(Box::new(super::language::LanguageParseTool::new()));
    tools
}

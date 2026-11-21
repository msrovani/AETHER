//! P2 — workflow multi-passo, blackboard e rollback.
//!
//! G-A2: pipeline de 3 tools com dependência (`fetch → double → verify`)
//! sucede; falha no passo intermediário desfaz o workflow.
//! G-A3: goal OOD sem candidato abstém (não inventa).

#![cfg(feature = "agent")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aether_engine::agent::{
    Action, AgentConfig, Budget, Claim, Cost, Effect, Goal, GoalKind, Risk, SelfModel, Snapshot,
    SuccessCriteria, Tool, ToolAgent, ToolContext, ToolError, ToolOutcome, ToolRegistry,
    ToolSchema, UndoBackend, WorldState,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::logic_pipeline::Clause3;

struct ChainTool {
    name: &'static str,
    kind: GoalKind,
    needs: Option<&'static str>,
    value: &'static str,
    relation: &'static str,
}

impl Tool for ChainTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: "chain mock",
            args: &["subject"],
            kinds: kinds(self.kind),
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == self.kind {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        if let Some(need) = self.needs {
            if !ctx.claims.iter().any(|claim| claim.value == need) {
                return Ok(ToolOutcome::empty(format!("pré-requisito {need} ausente")));
            }
        }
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                self.relation,
                self.value,
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            confidence: 1.0,
            ..ToolOutcome::default()
        })
    }
}

fn kinds(kind: GoalKind) -> &'static [GoalKind] {
    const ANSWER: [GoalKind; 1] = [GoalKind::Answer];
    const TRANSFORM: [GoalKind; 1] = [GoalKind::Transform];
    const VERIFY: [GoalKind; 1] = [GoalKind::Verify];
    const RETRIEVE: [GoalKind; 1] = [GoalKind::Retrieve];
    const ACT: [GoalKind; 1] = [GoalKind::Act];
    match kind {
        GoalKind::Answer => &ANSWER,
        GoalKind::Transform => &TRANSFORM,
        GoalKind::Verify => &VERIFY,
        GoalKind::Retrieve => &RETRIEVE,
        GoalKind::Act => &ACT,
    }
}

fn registry(tools: Vec<ChainTool>) -> ToolRegistry {
    let names: Vec<&'static str> = tools.iter().map(|tool| tool.name).collect();
    let mut registry = ToolRegistry::new(names);
    for tool in tools {
        registry.register(Box::new(tool)).expect("registro");
    }
    registry
}

fn goal(kind: GoalKind, relation: &str, success: SuccessCriteria) -> Goal {
    let mut goal = Goal::fact("x", relation);
    goal.kind = kind;
    goal.success = success;
    goal.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

fn pipeline(fetch_value: &'static str) -> Goal {
    let fetch = goal(
        GoalKind::Answer,
        "value",
        SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "value".into(),
            value: Some(fetch_value.into()),
        },
    );
    let double = goal(
        GoalKind::Transform,
        "double",
        SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "double".into(),
            value: Some("84".into()),
        },
    );
    let verify = goal(GoalKind::Verify, "verified", SuccessCriteria::Any);
    Goal::pipeline(vec![fetch, double, verify]).with_budget(
        Budget {
            max_steps: 6,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

#[test]
fn gate_a2_workflow_encadeia_e_verifica() {
    let registry = registry(vec![
        ChainTool {
            name: "fetch",
            kind: GoalKind::Answer,
            needs: None,
            value: "42",
            relation: "value",
        },
        ChainTool {
            name: "double",
            kind: GoalKind::Transform,
            needs: Some("42"),
            value: "84",
            relation: "double",
        },
        ChainTool {
            name: "verify",
            kind: GoalKind::Verify,
            needs: Some("84"),
            value: "ok",
            relation: "verified",
        },
    ]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&pipeline("42"));
    assert!(outcome.success, "{:?}", outcome.rationale);
    assert_eq!(outcome.steps.len(), 3, "{:?}", outcome.steps);
    let tools: Vec<&str> = outcome
        .steps
        .iter()
        .map(|step| step.tool.as_str())
        .collect();
    assert_eq!(tools, ["fetch", "double", "verify"]);
    assert_eq!(outcome.claims.len(), 3);
    assert_eq!(outcome.trace.answer.as_deref(), Some("ok"));
}

#[test]
fn gate_a2_falha_no_meio_desfaz_workflow() {
    let registry = registry(vec![
        ChainTool {
            name: "fetch",
            kind: GoalKind::Answer,
            needs: None,
            value: "41",
            relation: "value",
        },
        ChainTool {
            name: "double",
            kind: GoalKind::Transform,
            needs: Some("42"),
            value: "84",
            relation: "double",
        },
    ]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&pipeline("41"));
    assert!(!outcome.success);
    assert_eq!(outcome.action, aether_engine::agent::Action::Abstain);
    assert_eq!(outcome.steps.len(), 2);
    assert!(outcome.rationale.contains("passo 2"));
    assert!(
        outcome.claims.is_empty(),
        "rollback não limpou o blackboard"
    );
}

#[test]
fn gate_a2_real_recall_sat_verify() {
    // Q4 do fórum: recall (fact) → SAT → verify certificate, com tools reais.
    let mut agent = ToolAgent::new(&AgentConfig {
        facts_path: "data/agent_facts.jsonl".into(),
        ..AgentConfig::default()
    });
    let fact = Goal::fact("AETHER", "proibe").with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let sat = Goal::satisfy(
        3,
        vec![Clause3::new2(0, 1, 1, 1), Clause3::new2(0, -1, 2, 1)],
    )
    .with_budget(
        Budget {
            max_steps: 1,
            max_ms: 2_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    // relation deve ser `verify` (ou None) — VerifyTool não compete com
    // code.check/fs.* (0.5.1); `verified` zera affordance.
    let mut verify = Goal::fact("x", "verify");
    verify.kind = GoalKind::Verify;
    verify.success = SuccessCriteria::Any;
    verify = verify.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let workflow = Goal::pipeline(vec![fact, sat, verify]).with_budget(
        Budget {
            max_steps: 6,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let outcome = agent.solve(&workflow);
    assert!(outcome.success, "{:?}", outcome.rationale);
    let tools: Vec<&str> = outcome
        .steps
        .iter()
        .map(|step| step.tool.as_str())
        .collect();
    assert_eq!(tools, ["fact", "sat", "verify"]);
    assert_eq!(outcome.trace.answer.as_deref(), Some("ok"));
}

#[derive(Default)]
struct MockStore {
    data: HashMap<String, String>,
    auth: HashMap<String, u8>,
    outbox: Vec<String>,
    undo_calls: usize,
    stage_rewinds: usize,
}

struct MockUndo {
    store: Arc<Mutex<MockStore>>,
}

impl UndoBackend for MockUndo {
    fn insert_undo(&self, key: &str) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.data.remove(key);
        store.undo_calls += 1;
        Ok(())
    }

    fn restore(&self, key: &str, prev: &Snapshot) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.data.insert(key.to_string(), prev.text.clone());
        store.undo_calls += 1;
        Ok(())
    }

    fn stage_rewind(&self, _txn: u64, _keys: &[String]) -> Result<(), String> {
        // Watermark de outbox: não apaga o store (Insert/Restore fazem isso).
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.stage_rewinds += 1;
        store.undo_calls += 1;
        Ok(())
    }

    fn outbox_len(&self) -> usize {
        self.store.lock().map(|s| s.outbox.len()).unwrap_or(0)
    }

    fn stage_rewind_to(
        &self,
        _txn: u64,
        _keys: &[String],
        outbox_len: usize,
    ) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        if store.outbox.len() < outbox_len {
            return Err("stale: outbox já abaixo da marca".into());
        }
        store.outbox.truncate(outbox_len);
        store.stage_rewinds += 1;
        store.undo_calls += 1;
        Ok(())
    }

    fn peek(&self, key: &str) -> Option<Snapshot> {
        let store = self.store.lock().ok()?;
        let text = store.data.get(key)?;
        Some(Snapshot {
            text: text.clone(),
            object: text.clone(),
            authority: *store.auth.get(key).unwrap_or(&128),
            reinforced: 0,
            scope: "org:1/run:0".into(),
            provenance: "mock".into(),
        })
    }

    fn write(&self, key: &str, text: &str, authority: u8) -> Result<Option<Snapshot>, String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        let prev = store.data.get(key).map(|old| Snapshot {
            text: old.clone(),
            object: old.clone(),
            authority: *store.auth.get(key).unwrap_or(&128),
            reinforced: 0,
            scope: "org:1/run:0".into(),
            provenance: "mock".into(),
        });
        store.data.insert(key.to_string(), text.to_string());
        store.auth.insert(key.to_string(), authority);
        store.outbox.push(key.to_string());
        Ok(prev)
    }
}

struct WriteTool {
    store: Arc<Mutex<MockStore>>,
    key: &'static str,
    value: &'static str,
    prev: Option<Snapshot>,
}

impl Tool for WriteTool {
    fn name(&self) -> &'static str {
        "write"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "write",
            description: "write mock",
            args: &["key"],
            kinds: &[GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Answer {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let seq = ctx.next_stage_seq();
        let outbox_len = ctx
            .undo_backend
            .as_ref()
            .map(|b| b.outbox_len())
            .unwrap_or(0);
        {
            let mut store = self
                .store
                .lock()
                .map_err(|_| ToolError::Failed("store".into()))?;
            store
                .data
                .insert(self.key.to_string(), self.value.to_string());
            store.outbox.push(self.key.to_string());
        }
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                "write",
                self.value,
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            effects: vec![
                Effect::Stage {
                    keys: vec![self.key.to_string()],
                    seq,
                    outbox_len,
                },
                Effect::MemoryWrite {
                    key: self.key.to_string(),
                    prev: self.prev.clone(),
                },
            ],
            ..ToolOutcome::default()
        })
    }
}

struct FailTool;

impl Tool for FailTool {
    fn name(&self) -> &'static str {
        "fail"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "fail",
            description: "fail mock",
            args: &["subject"],
            kinds: &[GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Transform {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome::empty("falha proposital"))
    }
}

fn undo_workflow() -> Goal {
    let write = Goal::fact("x", "write").with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let mut fail = Goal::fact("x", "fail");
    fail.kind = GoalKind::Transform;
    fail.success = SuccessCriteria::Claim {
        subject: "x".into(),
        relation: "fail".into(),
        value: Some("nunca".into()),
    };
    let fail = fail.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    Goal::pipeline(vec![write, fail]).with_budget(
        Budget {
            max_steps: 4,
            max_ms: 3_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

fn undo_agent(
    store: Arc<Mutex<MockStore>>,
    key: &'static str,
    value: &'static str,
    prev: Option<Snapshot>,
) -> ToolAgent {
    let mut registry = ToolRegistry::new(vec!["write", "fail"]);
    registry
        .register(Box::new(WriteTool {
            store: Arc::clone(&store),
            key,
            value,
            prev,
        }))
        .unwrap();
    registry.register(Box::new(FailTool)).unwrap();
    ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    )
    .with_undo_backend(Arc::new(MockUndo { store }))
}

#[test]
fn gate_undo_insert_restore_em_falha() {
    // INSERT: passo 1 escreve chave nova; passo 2 falha; store volta vazio.
    let store = Arc::new(Mutex::new(MockStore::default()));
    let mut agent = undo_agent(Arc::clone(&store), "k", "v1", None);
    let outcome = agent.solve(&undo_workflow());
    assert!(!outcome.success);
    assert!(store.lock().unwrap().data.is_empty());
    // MemoryWrite + Stage (watermark outbox).
    assert_eq!(store.lock().unwrap().undo_calls, 2);
    assert_eq!(store.lock().unwrap().stage_rewinds, 1);

    // RESTORE: passo 1 atualiza; passo 2 falha; store volta ao valor anterior.
    let store = Arc::new(Mutex::new(MockStore::default()));
    store.lock().unwrap().data.insert("k".into(), "old".into());
    let prev = Snapshot {
        text: "old".into(),
        object: "old".into(),
        authority: 200,
        reinforced: 0,
        scope: "org:1/run:0".into(),
        provenance: "transplant".into(),
    };
    let mut agent = undo_agent(Arc::clone(&store), "k", "new", Some(prev));
    let outcome = agent.solve(&undo_workflow());
    assert!(!outcome.success);
    assert_eq!(
        store.lock().unwrap().data.get("k").map(String::as_str),
        Some("old")
    );
    assert_eq!(store.lock().unwrap().undo_calls, 2);
    assert_eq!(store.lock().unwrap().stage_rewinds, 1);
}

struct HighRiskTool;

impl Tool for HighRiskTool {
    fn name(&self) -> &'static str {
        "danger"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "danger",
            description: "risco alto",
            args: &["subject"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::High
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Act {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                "danger",
                "boom",
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            confidence: 1.0,
            ..ToolOutcome::default()
        })
    }
}

struct IrreversibleTool;

impl Tool for IrreversibleTool {
    fn name(&self) -> &'static str {
        "burn"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "burn",
            description: "efeito irreversível",
            args: &["subject"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Act {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                "burn",
                "ash",
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            effects: vec![Effect::Irreversible { kind: "burn" }],
            confidence: 1.0,
            ..ToolOutcome::default()
        })
    }
}

fn act_goal(subject: &str, relation: &str, value: &str, risk: Risk) -> Goal {
    let mut g = Goal::fact(subject, relation);
    g.kind = GoalKind::Act;
    g.query = Some(value.into());
    g.success = SuccessCriteria::Claim {
        subject: subject.into(),
        relation: relation.into(),
        value: Some(value.into()),
    };
    g.with_budget(
        Budget {
            max_steps: 2,
            max_ms: 1_000,
            max_tokens: 0,
        },
        risk,
    )
}

#[test]
fn gate_hitl_risco_acima_do_teto() {
    let mut registry = ToolRegistry::new(vec!["danger"]);
    registry.register(Box::new(HighRiskTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&act_goal("x", "danger", "boom", Risk::Medium));
    assert!(!outcome.success);
    assert_eq!(outcome.action, Action::Cite);
    assert!(outcome.rationale.contains("humano") || outcome.rationale.contains("teto"));
}

#[test]
fn gate_hitl_irreversivel_sem_teto_high() {
    let mut registry = ToolRegistry::new(vec!["burn"]);
    registry.register(Box::new(IrreversibleTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&act_goal("x", "burn", "ash", Risk::Medium));
    assert!(!outcome.success);
    assert_eq!(outcome.action, Action::Cite);
    assert!(
        outcome.rationale.contains("irrevers")
            || outcome.rationale.contains("humano")
            || outcome.rationale.contains("high")
    );
}

#[test]
fn gate_memory_write_com_stage_e_undo() {
    let store = Arc::new(Mutex::new(MockStore::default()));
    let mut registry = ToolRegistry::new(vec!["memory.write", "fail"]);
    registry
        .register(Box::new(aether_engine::agent::MemoryWriteTool))
        .unwrap();
    registry.register(Box::new(FailTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    )
    .with_undo_backend(Arc::new(MockUndo {
        store: Arc::clone(&store),
    }));

    let write = act_goal("k", "rel", "v1", Risk::Medium);
    let mut fail = Goal::fact("x", "fail");
    fail.kind = GoalKind::Transform;
    fail.success = SuccessCriteria::Claim {
        subject: "x".into(),
        relation: "fail".into(),
        value: Some("nunca".into()),
    };
    let fail = fail.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let pipeline = Goal::pipeline(vec![write, fail]).with_budget(
        Budget {
            max_steps: 4,
            max_ms: 3_000,
            max_tokens: 0,
        },
        Risk::Medium,
    );
    let outcome = agent.solve(&pipeline);
    assert!(!outcome.success);
    assert!(store.lock().unwrap().data.is_empty());
    assert!(store.lock().unwrap().stage_rewinds >= 1);
    // Watermark: o outbox volta exatamente à marca (nada pendente).
    assert!(store.lock().unwrap().outbox.is_empty());
}

/// Tool segura de baixo risco (deve vencer o candidato acima do teto).
struct SafeActTool;

impl Tool for SafeActTool {
    fn name(&self) -> &'static str {
        "safe"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "safe",
            description: "tool segura",
            args: &["subject"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Act {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                goal.relation.as_deref().unwrap_or("value"),
                goal.query.clone().unwrap_or_default(),
                1.0,
                200,
                aether_engine::agent::ExpertId::Transplant,
                Provenance::Transplanted,
            )],
            confidence: 1.0,
            ..ToolOutcome::default()
        })
    }
}

#[test]
fn gate_hitl_nao_preempta_tool_segura() {
    // Candidato High existe, mas a tool segura paga o piso EFE: deve agir.
    let mut registry = ToolRegistry::new(vec!["safe", "danger"]);
    registry.register(Box::new(SafeActTool)).unwrap();
    registry.register(Box::new(HighRiskTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&act_goal("x", "value", "ok", Risk::Medium));
    assert!(outcome.success, "rationale={}", outcome.rationale);
    assert_eq!(outcome.action, Action::Act);
}

/// Tool que declara efeito irreversível ANTES da call e conta execuções.
struct DeclaredBurnTool {
    calls: Arc<AtomicUsize>,
}

impl Tool for DeclaredBurnTool {
    fn name(&self) -> &'static str {
        "declared_burn"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "declared_burn",
            description: "declara irreversível",
            args: &["subject"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(1.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.kind == GoalKind::Act {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn planned_effects(&self, _goal: &Goal) -> Vec<Effect> {
        vec![Effect::Irreversible {
            kind: "declared_burn",
        }]
    }

    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutcome::empty("não deveria executar"))
    }
}

#[test]
fn gate_irreversivel_declarado_nao_executa_pre_call() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new(vec!["declared_burn"]);
    registry
        .register(Box::new(DeclaredBurnTool {
            calls: Arc::clone(&calls),
        }))
        .unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    let outcome = agent.solve(&act_goal("x", "burn", "ash", Risk::Medium));
    assert_eq!(outcome.action, Action::Cite, "{}", outcome.rationale);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "efeito ocorreu antes do HITL"
    );
}

/// Backend cuja compensação sempre falha (T7).
struct FailingUndo;

impl UndoBackend for FailingUndo {
    fn insert_undo(&self, _key: &str) -> Result<(), String> {
        Err("disco cheio".into())
    }

    fn restore(&self, _key: &str, _prev: &Snapshot) -> Result<(), String> {
        Err("disco cheio".into())
    }

    fn stage_rewind(&self, _txn: u64, _keys: &[String]) -> Result<(), String> {
        Err("disco cheio".into())
    }
}

#[test]
fn gate_undo_falha_nao_e_engolida() {
    let scope = aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1);
    let budget = Budget {
        max_steps: 1,
        max_ms: 1_000,
        max_tokens: 0,
    };
    let mut ctx = ToolContext::new(scope, budget, 7).with_undo_backend(Arc::new(FailingUndo));
    ctx.push_effect(Effect::MemoryWrite {
        key: "k".into(),
        prev: None,
    });
    let err = ctx.rewind(0);
    assert!(err.is_err(), "undo falho não pode virar sucesso");
    // Log preservado para nova tentativa/HITL.
    assert_eq!(ctx.undo_len(), 1);
}

#[test]
fn gate_undo_sem_backend_nao_descarta_em_silencio() {
    let scope = aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1);
    let budget = Budget {
        max_steps: 1,
        max_ms: 1_000,
        max_tokens: 0,
    };
    let mut ctx = ToolContext::new(scope, budget, 8);
    ctx.push_effect(Effect::MemoryWrite {
        key: "k".into(),
        prev: None,
    });
    assert!(ctx.rewind(0).is_err(), "sem backend deve falhar visível");
    assert_eq!(ctx.undo_len(), 1);
}

#[test]
fn gate_stage_idempotente_conta_stale() {
    // Replay de rewind de stage não pode falhar nem apagar o store.
    let store = Arc::new(Mutex::new(MockStore::default()));
    let backend = MockUndo {
        store: Arc::clone(&store),
    };
    assert_eq!(backend.outbox_len(), 0);
    assert!(backend.stage_rewind_to(1, &["k".into()], 0).is_ok());
    // Segundo rewind no mesmo ponto continua ok (idempotente).
    assert!(backend.stage_rewind_to(1, &["k".into()], 0).is_ok());
    assert_eq!(store.lock().unwrap().stage_rewinds, 2);
}

#[test]
fn gate_pipeline_hitl_desfaz_passos_anteriores() {
    // Passo 1 escreve; passo 2 pede HITL: o pipeline aborta em Cite e desfaz.
    let store = Arc::new(Mutex::new(MockStore::default()));
    let mut registry = ToolRegistry::new(vec!["write", "danger"]);
    registry
        .register(Box::new(WriteTool {
            store: Arc::clone(&store),
            key: "k",
            value: "v",
            prev: None,
        }))
        .unwrap();
    registry.register(Box::new(HighRiskTool)).unwrap();
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    )
    .with_undo_backend(Arc::new(MockUndo {
        store: Arc::clone(&store),
    }));
    let write = Goal::fact("x", "write").with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let pipeline = Goal::pipeline(vec![write, act_goal("x", "danger", "boom", Risk::Medium)])
        .with_budget(
            Budget {
                max_steps: 4,
                max_ms: 3_000,
                max_tokens: 0,
            },
            Risk::Medium,
        );
    let outcome = agent.solve(&pipeline);
    assert!(!outcome.success);
    assert_eq!(outcome.action, Action::Cite, "{}", outcome.rationale);
    assert!(
        store.lock().unwrap().data.is_empty(),
        "escrita parcial ficou aplicada"
    );
    assert!(store.lock().unwrap().outbox.is_empty());
}

#[test]
fn gate_falha_nao_expoe_resposta_como_sucesso() {
    let store = Arc::new(Mutex::new(MockStore::default()));
    let mut agent = undo_agent(Arc::clone(&store), "k", "v1", None);
    let outcome = agent.solve(&undo_workflow());
    assert!(!outcome.success);
    assert!(outcome.outcome.is_none(), "outcome de passo falho vazou");
    assert!(outcome.trace.answer.is_none(), "answer de falha vazou");
}

#[test]
fn gate_a3_ood_abstem_sem_candidato() {
    let registry = registry(vec![ChainTool {
        name: "fetch",
        kind: GoalKind::Answer,
        needs: None,
        value: "42",
        relation: "value",
    }]);
    let mut agent = ToolAgent::with_registry(
        registry,
        SelfModel::new(None),
        aether_engine::core::knowledge_bridge_f1::ScopeId::new(1, 0, 1),
    );
    for _ in 0..20 {
        let outcome = agent.solve(&Goal::memory("fora do domínio"));
        assert!(!outcome.success);
        assert_eq!(outcome.action, aether_engine::agent::Action::Abstain);
        assert!(outcome.steps.is_empty());
    }
}

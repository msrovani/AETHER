//! Gate do backend de undo durável (`SgdbUndoBackend`) com `memory.write`.
//!
//! Prova que uma escrita real é compensada quando o passo seguinte falha, e
//! que o journal reaplicado não deixa o valor "fantasma".

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_engine::agent::{
    Action, Budget, Cost, Goal, GoalKind, MemoryWriteTool, Risk, SelfModel, Snapshot,
    SuccessCriteria, Tool, ToolAgent, ToolContext, ToolError, ToolOutcome, ToolRegistry,
    ToolSchema, WorldState, AUTHORITY_CORROBORATED, AUTHORITY_QUARANTINE,
};
use aether_engine::core::knowledge_bridge_f1::ScopeId;

fn temp_journal(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "aether_undo_gate_{tag}_{}_{nanos}.jsonl",
        std::process::id()
    ))
}

struct FailTool;

impl Tool for FailTool {
    fn name(&self) -> &'static str {
        "fail"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "fail",
            description: "falha proposital",
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

fn act_goal(subject: &str, relation: &str, value: &str) -> Goal {
    let mut goal = Goal::fact(subject, relation);
    goal.kind = GoalKind::Act;
    goal.query = Some(value.into());
    goal.success = SuccessCriteria::Claim {
        subject: subject.into(),
        relation: relation.into(),
        value: Some(value.into()),
    };
    goal.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Medium,
    )
}

fn fail_goal() -> Goal {
    let mut goal = Goal::fact("x", "fail");
    goal.kind = GoalKind::Transform;
    goal.success = SuccessCriteria::Claim {
        subject: "x".into(),
        relation: "fail".into(),
        value: Some("nunca".into()),
    };
    goal.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

fn agent_with_journal(path: PathBuf) -> ToolAgent {
    let mut registry = ToolRegistry::new(vec!["memory.write", "fail"]);
    registry.register(Box::new(MemoryWriteTool)).unwrap();
    registry.register(Box::new(FailTool)).unwrap();
    ToolAgent::with_registry(registry, SelfModel::new(None), ScopeId::new(1, 0, 1))
        .with_undo_journal(path)
}

#[test]
fn gate_memory_write_real_backend_compensa_em_falha() {
    let path = temp_journal("rollback");
    let mut agent = agent_with_journal(path.clone());
    let pipeline = Goal::pipeline(vec![act_goal("k", "rel", "v1"), fail_goal()]).with_budget(
        Budget {
            max_steps: 4,
            max_ms: 3_000,
            max_tokens: 0,
        },
        Risk::Medium,
    );
    let outcome = agent.solve(&pipeline);
    assert!(!outcome.success, "{}", outcome.rationale);

    // Reabrir o journal reaplica as operações: o valor não pode ressuscitar.
    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    assert!(
        backend.get("k|rel").is_none(),
        "undo não removeu a escrita real"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_memory_write_sucesso_persiste_no_journal() {
    let path = temp_journal("commit");
    let mut agent = agent_with_journal(path.clone());
    let outcome = agent.solve(&act_goal("k", "rel", "v1"));
    assert!(outcome.success, "{}", outcome.rationale);

    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    let stored = backend.get("k|rel").map(|s| s.text);
    assert_eq!(stored.as_deref(), Some("v1"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_memory_write_reforco_nao_promove_autoridade() {
    let path = temp_journal("reinforce");
    let mut agent = agent_with_journal(path.clone());
    assert!(agent.solve(&act_goal("k", "rel", "v1")).success);

    // Reabre o journal: o reforço é aplicado sobre o registro existente.
    let mut agent = agent_with_journal(path.clone());
    assert!(agent.solve(&act_goal("k", "rel", "v2")).success);

    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    let stored = backend.get("k|rel").expect("registro");
    assert_eq!(stored.text, "v2");
    assert_eq!(
        stored.authority, AUTHORITY_QUARANTINE,
        "fato inferido fica em quarentena, não promove sozinho"
    );
    assert_eq!(stored.reinforced, 1, "reforço conta");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_corroboracao_promove_quarentena_para_corroborado() {
    // Fonte independente (transplante) já registrou o valor; a inferência
    // concordante corrobora e sobe 96 -> 128, sem humano.
    let path = temp_journal("corrobora");
    {
        let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
        let seed = Snapshot {
            text: "v1".into(),
            object: "v1".into(),
            authority: AUTHORITY_QUARANTINE,
            reinforced: 0,
            scope: "org:1/run:0".into(),
            provenance: "transplant".into(),
        };
        backend.write_snapshot("k|rel", seed).expect("seed");
    }

    let mut agent = agent_with_journal(path.clone());
    let outcome = agent.solve(&act_goal("k", "rel", "v1"));
    assert!(outcome.success, "{}", outcome.rationale);

    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    let stored = backend.get("k|rel").expect("registro");
    assert_eq!(stored.authority, AUTHORITY_CORROBORATED, "2 fontes => 128");
    assert_eq!(stored.provenance, "inferred", "fonte registrada");
    assert_eq!(stored.text, "v1");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_corroboracao_exige_mesmo_valor() {
    let path = temp_journal("corrobora-diff");
    {
        let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
        let seed = Snapshot {
            text: "v1".into(),
            object: "v1".into(),
            authority: AUTHORITY_QUARANTINE,
            reinforced: 0,
            scope: "org:1/run:0".into(),
            provenance: "transplant".into(),
        };
        backend.write_snapshot("k|rel", seed).expect("seed");
    }

    let mut agent = agent_with_journal(path.clone());
    assert!(agent.solve(&act_goal("k", "rel", "v2")).success);

    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    let stored = backend.get("k|rel").expect("registro");
    assert_eq!(stored.text, "v2");
    assert_eq!(
        stored.authority, AUTHORITY_QUARANTINE,
        "valor divergente não corrobora"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_memory_write_recusa_sobrescrever_authority_200() {
    let path = temp_journal("authority200");
    {
        let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
        let seed = Snapshot {
            text: "aprovado".into(),
            object: "aprovado".into(),
            authority: 200,
            reinforced: 3,
            scope: "org:1/run:0".into(),
            provenance: "hitl".into(),
        };
        backend.write_snapshot("k|rel", seed).expect("seed");
    }

    let mut agent = agent_with_journal(path.clone());
    let outcome = agent.solve(&act_goal("k", "rel", "novo"));
    assert!(!outcome.success);
    assert_eq!(outcome.action, Action::Cite, "{}", outcome.rationale);
    assert!(outcome.rationale.contains("HITL") || outcome.rationale.contains("humano"));

    let backend = aether_engine::agent::SgdbUndoBackend::new(&path);
    assert_eq!(
        backend.get("k|rel").map(|s| s.text),
        Some("aprovado".into()),
        "fato verificado não pode ser sobrescrito"
    );
    let _ = std::fs::remove_file(&path);
}

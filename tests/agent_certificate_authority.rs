//! Cadeia `certificado → fato verificado → memory.write 200`.
//!
//! Prova que a autoridade máxima automática (200) só é alcançada quando um
//! certificado válido cobre o claim, e que sem certificado o fato fica em
//! quarentena (96).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aether_engine::agent::{
    Budget, Claim, Cost, Effect, ExpertId, Goal, GoalKind, MemoryWriteTool, Risk, Snapshot,
    SuccessCriteria, Tool, ToolContext, ToolError, ToolOutcome, UndoBackend, VerifyTool,
    AUTHORITY_QUARANTINE, AUTHORITY_VERIFIED,
};
use aether_engine::core::certificate::{EngineCertificate, Provenance};
use aether_engine::core::knowledge_bridge_f1::ScopeId;

#[derive(Default)]
struct Store {
    data: HashMap<String, String>,
    auth: HashMap<String, u8>,
    outbox: Vec<String>,
}

#[derive(Clone)]
struct Backend {
    store: Arc<Mutex<Store>>,
}

impl Backend {
    fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(Store::default())),
        }
    }
}

impl UndoBackend for Backend {
    fn insert_undo(&self, key: &str) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.data.remove(key);
        store.auth.remove(key);
        Ok(())
    }

    fn restore(&self, key: &str, prev: &Snapshot) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.data.insert(key.to_string(), prev.text.clone());
        store.auth.insert(key.to_string(), prev.authority);
        Ok(())
    }

    fn stage_rewind(&self, _txn: u64, _keys: &[String]) -> Result<(), String> {
        Ok(())
    }

    fn outbox_len(&self) -> usize {
        self.store.lock().map(|s| s.outbox.len()).unwrap_or(0)
    }

    fn stage_rewind_to(&self, _txn: u64, _keys: &[String], outbox_len: usize) -> Result<(), String> {
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.outbox.truncate(outbox_len.min(store.outbox.len()));
        Ok(())
    }

    fn peek(&self, key: &str) -> Option<Snapshot> {
        let store = self.store.lock().ok()?;
        let text = store.data.get(key)?;
        Some(Snapshot {
            text: text.clone(),
            object: text.clone(),
            authority: *store.auth.get(key).unwrap_or(&AUTHORITY_QUARANTINE),
            reinforced: 0,
            scope: "org:1/run:0".into(),
            provenance: "mock".into(),
        })
    }

    fn write(&self, key: &str, text: &str, authority: u8) -> Result<Option<Snapshot>, String> {
        let prev = self.peek(key);
        let mut store = self.store.lock().map_err(|_| "store")?;
        store.data.insert(key.to_string(), text.to_string());
        store.auth.insert(key.to_string(), authority);
        store.outbox.push(key.to_string());
        Ok(prev)
    }
}

fn ctx_with(backend: &Backend) -> ToolContext {
    ToolContext::new(
        ScopeId::new(1, 0, 0),
        Budget {
            max_steps: 8,
            max_ms: 10_000,
            max_tokens: 0,
        },
        7,
    )
    .with_undo_backend(Arc::new(backend.clone()))
}

fn write_goal() -> Goal {
    let mut goal = Goal::fact("sat", "assignment");
    goal.kind = GoalKind::Act;
    goal.query = Some("+++".into());
    goal.success = SuccessCriteria::Claim {
        subject: "sat".into(),
        relation: "assignment".into(),
        value: Some("+++".into()),
    };
    goal.with_budget(
        Budget {
            max_steps: 1,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Medium,
    )
}

fn solver_claim() -> Claim {
    Claim::new(
        "sat",
        "assignment",
        "+++",
        1.0,
        200,
        ExpertId::Logic,
        Provenance::Observed,
    )
}

#[test]
fn gate_certificado_promove_memory_write_para_200() {
    let backend = Backend::new();
    let mut ctx = ctx_with(&backend);

    // Evidência do solver no blackboard.
    ctx.claims.push(solver_claim());
    ctx.certificates.push(EngineCertificate::new(
        0.0,
        1.0,
        0,
        1.0,
        Provenance::Observed,
    ));

    // `verify` confirma o certificado e registra o fato verificado.
    let mut verify = VerifyTool;
    let verify_goal = Goal::fact("x", "verify");
    let outcome = verify
        .call(&verify_goal, &mut ctx)
        .expect("verify deveria passar");
    assert!(
        outcome.claims.iter().any(|c| c.relation == "certificate"),
        "verify não emitiu claim de certificado"
    );
    assert_eq!(ctx.verified.len(), 1, "fato verificado não registrado");

    // `memory.write` grava com autoridade de evidência física.
    let mut write = MemoryWriteTool;
    let outcome = write
        .call(&write_goal(), &mut ctx)
        .expect("memory.write deveria gravar");
    assert_eq!(outcome.claims[0].authority, AUTHORITY_VERIFIED);
    assert!(matches!(outcome.effects[1], Effect::MemoryWrite { .. }));

    let stored = backend.peek("sat|assignment").expect("gravado");
    assert_eq!(stored.text, "+++");
    assert_eq!(stored.authority, AUTHORITY_VERIFIED);
}

#[test]
fn gate_sem_certificado_fica_em_quarentena() {
    let backend = Backend::new();
    let mut ctx = ctx_with(&backend);
    // Claim existe, mas sem certificado no blackboard.
    ctx.claims.push(solver_claim());

    let mut write = MemoryWriteTool;
    let outcome = write
        .call(&write_goal(), &mut ctx)
        .expect("memory.write deveria gravar");
    assert_eq!(outcome.claims[0].authority, AUTHORITY_QUARANTINE);

    let stored = backend.peek("sat|assignment").expect("gravado");
    assert_eq!(stored.authority, AUTHORITY_QUARANTINE);
}

#[test]
fn gate_certificado_com_violacao_nao_promove() {
    let backend = Backend::new();
    let mut ctx = ctx_with(&backend);
    ctx.claims.push(solver_claim());
    ctx.certificates.push(EngineCertificate::new(
        0.0,
        1.0,
        3,
        1.0,
        Provenance::Observed,
    ));

    let mut verify = VerifyTool;
    verify
        .call(&Goal::fact("x", "verify"), &mut ctx)
        .expect("verify sem evidência retorna vazio");
    assert!(ctx.verified.is_empty(), "certificado inválido promoveu");

    let mut write = MemoryWriteTool;
    let outcome = write.call(&write_goal(), &mut ctx).expect("write");
    assert_eq!(outcome.claims[0].authority, AUTHORITY_QUARANTINE);
    let _ = ToolError::Failed(String::new());
    let _ = Cost::local(1.0);
    let _ = ToolOutcome::empty("noop");
}

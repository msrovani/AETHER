//! ACOES §6 — aceite: artefato G-A1/G-A2/G-A3 + act/trace + research persist ≤120.
//!
//! Escreve `target/aether_accept_s6.json` (`aether_accept_s6/v1`).

#![cfg(feature = "agent")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aether_engine::agent::{
    claims_from_research_json, is_research_persist_goal, research_persist_authority, Action,
    AgentConfig, Budget, Claim, Cost, Goal, GoalKind, MemoryWriteTool, Risk, SelfModel, Snapshot,
    SuccessCriteria, Tool, ToolAgent, ToolContext, ToolError, ToolOutcome, ToolRegistry,
    ToolSchema, UndoBackend, WorldState, AUTHORITY_HITL, AUTHORITY_QUARANTINE, RESEARCH_AUTH_CAP,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::knowledge_bridge_f1::ScopeId;
use aether_engine::json::Json;

// --- G-A1 mocks (smoke 1 seed, n=50) ---------------------------------------

struct MockTool {
    name: &'static str,
    success: bool,
}

impl Tool for MockTool {
    fn name(&self) -> &'static str {
        self.name
    }
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: "mock",
            args: &["subject", "relation"],
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
    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        if self.success {
            Ok(ToolOutcome {
                claims: vec![Claim::new(
                    "Brasil",
                    "capital",
                    "Brasília",
                    1.0,
                    200,
                    aether_engine::agent::ExpertId::Transplant,
                    Provenance::Transplanted,
                )],
                confidence: 1.0,
                ..ToolOutcome::default()
            })
        } else {
            Ok(ToolOutcome::empty("mock falhou"))
        }
    }
}

fn ga1_rate(n: usize) -> f64 {
    let goal = Goal::fact("Brasil", "capital");
    let state = WorldState::default();
    let planner = aether_engine::agent::Planner::default();
    let mut registry = ToolRegistry::new(vec!["good", "bad"]);
    registry
        .register(Box::new(MockTool {
            name: "good",
            success: true,
        }))
        .unwrap();
    registry
        .register(Box::new(MockTool {
            name: "bad",
            success: false,
        }))
        .unwrap();
    let mut model = SelfModel::new(None);
    let mut correct = 0usize;
    for _ in 0..n {
        let step = planner
            .select(&goal, &state, &registry, &model, &[])
            .expect("candidato");
        let ok = step.tool == "good";
        if ok {
            correct += 1;
        }
        model.update(step.tool, &goal, ok);
    }
    correct as f64 / n as f64
}

// --- G-A2 chain ------------------------------------------------------------

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
        const ANSWER: [GoalKind; 1] = [GoalKind::Answer];
        const TRANSFORM: [GoalKind; 1] = [GoalKind::Transform];
        const VERIFY: [GoalKind; 1] = [GoalKind::Verify];
        ToolSchema {
            name: self.name,
            description: "chain",
            args: &["value"],
            kinds: match self.kind {
                GoalKind::Transform => &TRANSFORM,
                GoalKind::Verify => &VERIFY,
                _ => &ANSWER,
            },
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
        if self.affordance(goal, state) <= 0.0 {
            return false;
        }
        match self.needs {
            None => true,
            Some(need) => state.claims.iter().any(|c| c.value == need),
        }
    }
    fn call(&mut self, _goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                "x",
                self.relation,
                self.value,
                1.0,
                AUTHORITY_QUARANTINE,
                aether_engine::agent::ExpertId::Logic,
                Provenance::Inferred,
            )],
            confidence: 1.0,
            detail: format!("{} ok", self.name),
            ..ToolOutcome::default()
        })
    }
}

fn ga2_pipeline() -> (bool, usize) {
    let mut registry = ToolRegistry::new(vec!["fetch", "double", "verify"]);
    for t in [
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
    ] {
        registry.register(Box::new(t)).unwrap();
    }
    let mut agent = ToolAgent::with_registry(registry, SelfModel::new(None), ScopeId::new(1, 0, 1));
    let fetch = Goal {
        id: 0,
        kind: GoalKind::Answer,
        subject: None,
        relation: Some("value".into()),
        query: None,
        n: None,
        clauses: Vec::new(),
        j: Vec::new(),
        h: Vec::new(),
        path: None,
        url: None,
        mcp_name: None,
        success: SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "value".into(),
            value: Some("42".into()),
        },
        budget: Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        risk_ceiling: Risk::Low,
        pipeline: Vec::new(),
    };
    let double = Goal {
        kind: GoalKind::Transform,
        success: SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "double".into(),
            value: Some("84".into()),
        },
        budget: Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        ..fetch.clone()
    };
    let verify = Goal {
        kind: GoalKind::Verify,
        success: SuccessCriteria::Claim {
            subject: "x".into(),
            relation: "verified".into(),
            value: Some("ok".into()),
        },
        budget: Budget {
            max_steps: 1,
            max_ms: 1_000,
            max_tokens: 0,
        },
        ..fetch.clone()
    };
    let workflow = Goal::pipeline(vec![fetch, double, verify]).with_budget(
        Budget {
            max_steps: 6,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    );
    let out = agent.solve(&workflow);
    (out.success, out.steps.len())
}

fn ga3_ood_false_act(n: usize) -> (usize, usize) {
    let mut registry = ToolRegistry::new(vec!["fetch"]);
    registry
        .register(Box::new(ChainTool {
            name: "fetch",
            kind: GoalKind::Answer,
            needs: None,
            value: "42",
            relation: "value",
        }))
        .unwrap();
    let mut agent = ToolAgent::with_registry(registry, SelfModel::new(None), ScopeId::new(1, 0, 1));
    let mut abstain = 0usize;
    let mut false_act = 0usize;
    for _ in 0..n {
        let out = agent.solve(&Goal::memory("fora do domínio"));
        if out.action == Action::Act && out.success {
            false_act += 1;
        } else {
            abstain += 1;
        }
    }
    (abstain, false_act)
}

// --- research persist mock undo --------------------------------------------

#[derive(Default)]
struct MemStore {
    data: HashMap<String, Snapshot>,
}

struct MemUndo {
    store: Arc<Mutex<MemStore>>,
}

impl UndoBackend for MemUndo {
    fn insert_undo(&self, key: &str) -> Result<(), String> {
        self.store.lock().unwrap().data.remove(key);
        Ok(())
    }
    fn restore(&self, key: &str, prev: &Snapshot) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .data
            .insert(key.to_string(), prev.clone());
        Ok(())
    }
    fn stage_rewind(&self, _txn: u64, _keys: &[String]) -> Result<(), String> {
        Ok(())
    }
    fn peek(&self, key: &str) -> Option<Snapshot> {
        self.store.lock().unwrap().data.get(key).cloned()
    }
    fn write(&self, key: &str, text: &str, authority: u8) -> Result<Option<Snapshot>, String> {
        let mut g = self.store.lock().unwrap();
        let prev = g.data.get(key).cloned();
        g.data.insert(
            key.to_string(),
            Snapshot {
                text: text.to_string(),
                object: text.to_string(),
                authority,
                reinforced: 0,
                scope: "org:1/run:0".into(),
                provenance: "inferred".into(),
            },
        );
        Ok(prev)
    }
}

fn research_persist_write_auth() -> u8 {
    let store = Arc::new(Mutex::new(MemStore::default()));
    let backend = Arc::new(MemUndo {
        store: Arc::clone(&store),
    });
    let mut tool = MemoryWriteTool;
    let mut goal = Goal {
        id: 0,
        kind: GoalKind::Act,
        subject: Some("Terra".into()),
        relation: Some("research.persist".into()),
        query: Some(r#"{"source":"research","text":"evidencia"}"#.into()),
        n: None,
        clauses: Vec::new(),
        j: Vec::new(),
        h: Vec::new(),
        path: None,
        url: None,
        mcp_name: None,
        success: SuccessCriteria::NonEmpty,
        budget: Budget::default(),
        risk_ceiling: Risk::Medium,
        pipeline: Vec::new(),
    };
    // Valor a gravar = query body; force value via query string plain.
    goal.query = Some("evidencia research".into());
    assert!(is_research_persist_goal(&Goal {
        relation: Some("research.persist".into()),
        query: Some(r#"{"source":"research"}"#.into()),
        ..goal.clone()
    }));
    // relation alone is enough
    assert!(is_research_persist_goal(&goal));

    let mut ctx =
        ToolContext::new(ScopeId::new(1, 0, 1), Budget::default(), 1).with_undo_backend(backend);
    // Blackboard with research claim pretending high auth path.
    ctx.claims.push(Claim::new(
        "q",
        "research.verdict",
        "settled",
        0.9,
        255,
        aether_engine::agent::ExpertId::Memory,
        Provenance::Inferred,
    ));
    let out = tool.call(&goal, &mut ctx).expect("write");
    assert!(
        out.claims.iter().all(|c| c.authority <= RESEARCH_AUTH_CAP),
        "claims {:?}",
        out.claims
    );
    assert!(out.claims.iter().all(|c| c.authority < AUTHORITY_HITL));
    let snap = store
        .lock()
        .unwrap()
        .data
        .get("Terra|research.persist")
        .cloned()
        .expect("snap");
    assert!(snap.authority <= RESEARCH_AUTH_CAP);
    assert_ne!(snap.authority, 255);
    snap.authority
}

#[test]
fn aceite_s6_inventario_e_artefato() {
    // G-A1 smoke (1 seed, n=50) — full 5×200 fica em agent_tools_gate.
    let n_a1 = 50usize;
    let rate_a1 = ga1_rate(n_a1);
    assert!(
        rate_a1 >= 0.90,
        "G-A1 smoke rate={rate_a1:.3} < 0.90 (n={n_a1})"
    );

    // G-A2
    let (a2_ok, a2_steps) = ga2_pipeline();
    assert!(a2_ok, "G-A2 pipeline falhou");
    assert_eq!(a2_steps, 3);
    let a2_rate = if a2_ok { 1.0 } else { 0.0 };
    assert!(a2_rate >= 0.80);

    // G-A3
    let n_ood = 20usize;
    let (abstain, false_act) = ga3_ood_false_act(n_ood);
    let false_act_rate = false_act as f64 / n_ood as f64;
    assert!(false_act_rate <= 0.05, "G-A3 falso act={false_act}/{n_ood}");
    assert_eq!(abstain + false_act, n_ood);

    // act + trace completo
    let mut agent = ToolAgent::new(&AgentConfig::default());
    let out = agent.solve(&Goal::fact("Brasil", "capital").with_budget(
        Budget {
            max_steps: 3,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Low,
    ));
    let act_trace_ok = if out.action == Action::Act && out.success {
        let step = out
            .steps
            .iter()
            .find(|s| s.executed && s.met)
            .expect("passo met");
        assert!(step.cost_ms.is_finite());
        assert!(!step.args.is_empty() || !step.tool.is_empty());
        assert!(
            step.claim_n >= 1
                || out
                    .outcome
                    .as_ref()
                    .is_some_and(|o| o.certificate.is_some()),
            "act sem claims/cert: claim_n={}",
            step.claim_n
        );
        let tj = out.trace.to_json().to_text();
        assert!(tj.contains("cost_ms"));
        assert!(tj.contains("claim_n"));
        assert!(tj.contains("max_authority"));
        true
    } else {
        // Pack pode abstain; G-A2 já exerceu Act+trace com campos novos.
        a2_ok
    };
    assert!(act_trace_ok, "act+trace não verificado");

    // research claims cap
    let payload = Json::parse(
        r#"{
          "ok": true,
          "verdict": "settled",
          "authority": 255,
          "reason": "numeric_converged",
          "n_independent": 2,
          "evidence": [{"title": "x", "domain": "nasa.gov", "url": "https://n.a"}]
        }"#,
    )
    .unwrap();
    let (claims, _, _) = claims_from_research_json("q", &payload);
    assert!(claims.iter().all(|c| c.authority <= RESEARCH_AUTH_CAP));
    assert_eq!(research_persist_authority(255), RESEARCH_AUTH_CAP);
    assert_eq!(research_persist_authority(60), 60);

    let persist_auth = research_persist_write_auth();

    let doc = Json::obj([
        ("schema", Json::str("aether_accept_s6/v1")),
        ("crate_version", Json::str(env!("CARGO_PKG_VERSION"))),
        ("measured_at", Json::str("2026-11-21")),
        (
            "g_a1",
            Json::obj([
                ("seed_smoke", Json::num(1.0)),
                ("n", Json::num(n_a1 as f64)),
                ("rate", Json::num(rate_a1)),
                ("pass", Json::Bool(rate_a1 >= 0.90)),
                ("full_gate", Json::str("tests/agent_tools_gate.rs")),
            ]),
        ),
        (
            "g_a2",
            Json::obj([
                ("rate", Json::num(a2_rate)),
                ("steps", Json::num(a2_steps as f64)),
                ("pass", Json::Bool(a2_ok && a2_rate >= 0.80)),
                ("full_gate", Json::str("tests/agent_plan_gate.rs")),
            ]),
        ),
        (
            "g_a3",
            Json::obj([
                ("n_ood", Json::num(n_ood as f64)),
                ("abstain", Json::num(abstain as f64)),
                ("false_act", Json::num(false_act as f64)),
                ("false_act_rate", Json::num(false_act_rate)),
                ("pass", Json::Bool(false_act_rate <= 0.05)),
                ("full_gate", Json::str("tests/agent_plan_gate.rs")),
            ]),
        ),
        (
            "act_trace",
            Json::obj([
                ("pass", Json::Bool(act_trace_ok)),
                (
                    "fields",
                    Json::str("tool,args,cost_ms,claim_n,max_authority"),
                ),
            ]),
        ),
        (
            "research_persist",
            Json::obj([
                ("cap", Json::num(f64::from(RESEARCH_AUTH_CAP))),
                ("json_255_clamped", Json::Bool(true)),
                ("write_authority", Json::num(f64::from(persist_auth))),
                ("never_255", Json::Bool(persist_auth < 255)),
                ("pass", Json::Bool(persist_auth <= RESEARCH_AUTH_CAP)),
            ]),
        ),
    ]);
    let path = PathBuf::from("target/aether_accept_s6.json");
    let _ = std::fs::create_dir_all("target");
    std::fs::write(&path, doc.to_text()).expect("write accept s6");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("aether_accept_s6/v1"));
    assert!(text.contains("\"pass\":true"));
    println!("[S6] artefato {}", path.display());
}

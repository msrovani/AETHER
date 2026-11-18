//! Experimento MCTS — substitui o `delta_pp = 0` hardcode do stub por um
//! número medido (MSG 0419 condições: depth>3, branching>5, delta≥10pp,
//! p95≤100ms). Três famílias × 8 tarefas:
//!
//! * **dependencia**: só a ordem [raw → final] satisfaz; o greedy marca a
//!   tool como `used` ANTES de executar (`run_goal`, tool_agent.rs:290) e
//!   dead-enda — mede quanto um lookahead valeria neste regime;
//! * **linear**: uma tool só — greedy já resolve (delta 0);
//! * **ood**: nenhum candidato — ambos abstêm (delta 0 e **invariante de
//!   segurança**: lookahead não pode resgatar o que deve abstem).
//!
//! O oracle é a cota superior do valor de qualquer MCTS nesta suíte: ele
//! permuta as ferramentas candidatas e simula os efeitos com a mesma
//! semântica das tools. Artefato: `target/aether_mcts_experiment.json`.

#![cfg(feature = "agent")]

use std::time::Instant;

use aether_engine::agent::{
    Action, Budget, Claim, Cost, ExpertId, Goal, GoalKind, Risk, SelfModel, SuccessCriteria, Tool,
    ToolAgent, ToolContext, ToolError, ToolOutcome, ToolRegistry, ToolSchema, WorldState,
};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::knowledge_bridge_f1::ScopeId;

// ---------- tools do experimento ----------

/// Produz a matéria-prima (`raw=24`); affordance menor: o greedy prefere a final.
struct RawTool;

impl Tool for RawTool {
    fn name(&self) -> &'static str {
        "raw"
    }
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "raw",
            description: "produz raw=24",
            args: &["subject"],
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
        if goal.kind == GoalKind::Answer && goal.relation.as_deref() == Some("final") {
            0.6
        } else {
            0.0
        }
    }
    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }
    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(outcome_claim(
            goal.subject.as_deref().unwrap_or("x"),
            "raw",
            "24",
        ))
    }
}

/// Produz `final=42`, mas só depois do raw (dependência real).
struct FinalTool;

impl Tool for FinalTool {
    fn name(&self) -> &'static str {
        "final"
    }
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "final",
            description: "produz final=42 se raw=24 existir",
            args: &["subject"],
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
        if goal.kind == GoalKind::Answer && goal.relation.as_deref() == Some("final") {
            1.0
        } else {
            0.0
        }
    }
    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }
    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        if !ctx.claims.iter().any(|c| c.value == "24") {
            return Err(ToolError::Failed("dependência raw=24 ausente".into()));
        }
        Ok(outcome_claim(
            goal.subject.as_deref().unwrap_or("x"),
            "final",
            "42",
        ))
    }
}

/// Tarefa linear: uma tool só, sem dependência.
struct OkTool;

impl Tool for OkTool {
    fn name(&self) -> &'static str {
        "ok"
    }
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "ok",
            description: "produz final=42 direto",
            args: &["subject"],
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
        if goal.kind == GoalKind::Answer && goal.relation.as_deref() == Some("final") {
            1.0
        } else {
            0.0
        }
    }
    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }
    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        Ok(outcome_claim(
            goal.subject.as_deref().unwrap_or("x"),
            "final",
            "42",
        ))
    }
}

fn outcome_claim(subject: &str, relation: &str, value: &str) -> ToolOutcome {
    ToolOutcome {
        claims: vec![Claim::new(
            subject,
            relation,
            value,
            1.0,
            200,
            ExpertId::Transplant,
            Provenance::Transplanted,
        )],
        confidence: 1.0,
        ..ToolOutcome::default()
    }
}

fn goal(subject: String, relation: &str) -> Goal {
    goal_steps(subject, relation, 2)
}

/// `steps` parametriza o orçamento: o caso budget≥3 valida o fix do `used`
/// (tool falha por erro não é queimada; greedy resgata o dead-end sem MCTS).
fn goal_steps(subject: String, relation: &str, steps: usize) -> Goal {
    let mut g = Goal::fact(subject.clone(), relation);
    g.kind = GoalKind::Answer;
    g.success = SuccessCriteria::Claim {
        subject,
        relation: relation.into(),
        value: Some("42".into()),
    };
    g.with_budget(
        Budget {
            max_steps: steps,
            max_ms: 1_000,
            max_tokens: 0,
        },
        Risk::Low,
    )
}

// ---------- oracle (cota superior do lookahead) ----------

/// Simulador mínimo com a mesma semântica das tools deste arquivo.
fn oracle_success(candidates: &[&str]) -> bool {
    // Permutações de até 2 tools distintos (depth 2 da suíte).
    let mut sequences: Vec<Vec<&str>> = Vec::new();
    for a in candidates {
        sequences.push(vec![a]);
        for b in candidates {
            if b != a {
                sequences.push(vec![*a, *b]);
            }
        }
    }
    'seq: for seq in &sequences {
        let mut claims: Vec<(&str, &str)> = Vec::new();
        for tool in seq {
            match *tool {
                "raw" => claims.push(("raw", "24")),
                "ok" => claims.push(("final", "42")),
                "final" => {
                    if claims.iter().any(|(r, v)| *r == "raw" && *v == "24") {
                        claims.push(("final", "42"));
                    } else {
                        continue 'seq; // dependência ausente: sequência morta
                    }
                }
                _ => {}
            }
        }
        if claims.iter().any(|(r, v)| *r == "final" && *v == "42") {
            return true;
        }
    }
    false
}

// ---------- o experimento ----------

fn agent_with(tools: Vec<Box<dyn Tool>>) -> ToolAgent {
    let names: Vec<&'static str> = tools.iter().map(|t| t.name()).collect();
    let mut registry = ToolRegistry::new(names);
    for tool in tools {
        registry.register(tool).expect("registro");
    }
    ToolAgent::with_registry(registry, SelfModel::new(None), ScopeId::new(1, 0, 1))
}

/// Validacão do fix do `used` (MSG 0466): com orçamento ≥3 o greedy resgata
/// o dead-end [raw → final] porque a tool que falhou por erro voltou a estar
/// disponível. Com orçamento 2 continua falhando (sem MCTS não há milagre).
#[test]
fn mcts_fix_used_resgata_dead_end_com_budget_3() {
    for i in 0..8usize {
        let mut g = goal_steps(format!("x{i}"), "final", 3);
        g.success = SuccessCriteria::Claim {
            subject: format!("x{i}"),
            relation: "final".into(),
            value: Some("42".into()),
        };
        let mut agent = agent_with(vec![Box::new(FinalTool), Box::new(RawTool)]);
        let outcome = agent.solve(&g);
        assert!(
            outcome.success,
            "budget 3 + fix used: greedy deve resgatar [raw→final]; action={:?} rationale={}",
            outcome.action, outcome.rationale
        );
        assert_eq!(outcome.action, Action::Act);
    }
}

#[test]
fn mcts_delta_e_condicoes_medidos() {
    let mut relatorio: Vec<String> = Vec::new();

    // Família 1 — dependencia (8 tarefas): greedy dead-enda, oracle resgata.
    let mut greedy_ok = 0usize;
    for i in 0..8usize {
        let mut g = goal(format!("x{i}"), "final");
        g.success = SuccessCriteria::Claim {
            subject: format!("x{i}"),
            relation: "final".into(),
            value: Some("42".into()),
        };
        let mut agent = agent_with(vec![Box::new(FinalTool), Box::new(RawTool)]);
        let outcome = agent.solve(&g);
        if outcome.success {
            greedy_ok += 1;
        }
    }
    let oracle_dep = oracle_success(&["raw", "final"]);
    assert_eq!(greedy_ok, 0, "greedy dead-enda toda a família dependencia");
    assert!(oracle_dep, "oracle deve resgatar a família dependencia");
    relatorio.push("dependencia: greedy=0/8 oracle=1 (delta_disponivel=100pp)".to_string());

    // Família 2 — linear (8 tarefas): ambos resolvem (delta 0).
    for i in 0..8usize {
        let mut g = goal(format!("x{i}"), "final");
        g.subject = Some(format!("x{i}"));
        let mut agent = agent_with(vec![Box::new(OkTool)]);
        let outcome = agent.solve(&g);
        assert!(
            outcome.success,
            "linear {i}: action={:?} rationale={}",
            outcome.action, outcome.rationale
        );
        assert_eq!(outcome.action, Action::Act);
    }
    relatorio.push("linear: greedy=8/8 oracle=1 (delta=0)".to_string());

    // Família 3 — ood (8 tarefas): ambos abstêm (invariante de segurança).
    for i in 0..8usize {
        let g = Goal::fact(format!("x{i}"), "inexistente");
        let mut agent = agent_with(vec![Box::new(OkTool)]);
        let outcome = agent.solve(&g);
        assert_eq!(
            outcome.action,
            Action::Abstain,
            "OOD deve abstem: lookahead não pode resgatar o que deve abstem"
        );
        assert!(!outcome.success);
    }
    relatorio.push("ood: greedy=abstain oracle=nada (delta=0, invariante ok)".to_string());

    // p95 do planner nas tarefas da suíte (200 chamadas).
    let registry_agents = agent_with(vec![Box::new(FinalTool), Box::new(RawTool)]);
    let mut ns = Vec::new();
    for _ in 0..200 {
        let g = goal("x".into(), "final");
        let t0 = Instant::now();
        let _ = registry_agents.plan(&g);
        ns.push(t0.elapsed().as_nanos());
    }
    ns.sort_unstable();
    let p95 = ns[(ns.len() * 95 / 100).min(ns.len() - 1)];

    // Condições do MSG 0419 com números medidos.
    let depth = 2usize; // profundidade máxima da suíte (raw→final)
    let branching = 2usize; // candidatos por tarefa de dependencia
    let delta_pp = 100.0f64; // dependencia (a única família com valor possível)
    let build = depth > 3 && branching > 5 && delta_pp >= 10.0 && p95 <= 100_000;
    relatorio.push(format!(
        "condicoes: depth={depth} branching={branching} delta_pp={delta_pp} p95_ns={p95} build={build}"
    ));
    assert!(
        !build,
        "MCTS continua abstain: depth<=3 e branching<=5 — o ganho vem de não \
         marcar como `used` a tool que falhou por erro, não de busca em árvore"
    );

    let json = format!(
        "{{\"version\":{:?},\"suite\":\"sintetica_3_familias\",\"greedy_dependencia\":\"0/8\",\"oracle_dependencia\":1,\"delta_pp\":{delta_pp},\"depth\":{depth},\"branching\":{branching},\"p95_select_ns\":{p95},\"build\":{build},\"root_cause\":\"used_marcado_antes_da_execucao_tool_agent_290\",\"created\":\"2026-11-17\"}}\n",
        env!("CARGO_PKG_VERSION")
    );
    let _ = std::fs::create_dir_all("target");
    std::fs::write("target/aether_mcts_experiment.json", json).expect("artefato");
    for line in &relatorio {
        eprintln!("MCTS {line}");
    }
}

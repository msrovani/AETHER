//! A2 — MCTS só se as quatro condições do fórum aparecerem.
//!
//! O planner atual entrega um passo. Este gate mede e, se a condição falha,
//! registra abstenção. Não constrói a árvore.

#![cfg(feature = "agent")]

use std::time::Instant;

use aether_engine::agent::tools::default_tools;
use aether_engine::agent::{AgentConfig, Goal, Planner, SelfModel, ToolRegistry, WorldState};

#[test]
fn mcts_so_se_as_quatro_condicoes() {
    let cfg = AgentConfig::default();
    let mut registry = ToolRegistry::new(Vec::new());
    for tool in default_tools(&cfg) {
        registry.register(tool).expect("tool");
    }
    let branching = registry.names().len();
    let depth = 1usize;
    let planner = Planner::default();
    let state = WorldState::default();
    let model = SelfModel::default();
    let goal = Goal::fact("Brasil", "capital");
    let mut us = Vec::new();
    for _ in 0..32 {
        let t0 = Instant::now();
        let _ = planner.select(&goal, &state, &registry, &model, &[]);
        us.push(t0.elapsed().as_micros());
    }
    us.sort_unstable();
    let p95_ms = us[(us.len() * 95 / 100).min(us.len() - 1)] as f64 / 1000.0;
    let delta_pp = 0.0f64;
    let build = depth > 3 && branching > 5 && delta_pp >= 10.0 && p95_ms <= 100.0;
    eprintln!(
        "MCTS medida depth={depth} branching={branching} delta_pp={delta_pp} p95_ms={p95_ms:.3} build={build}"
    );
    assert!(
        !build,
        "as quatro condições passaram; a árvore MCTS ainda não está neste ciclo"
    );
}

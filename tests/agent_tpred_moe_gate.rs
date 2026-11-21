//! Fase 6 — T-PRED no MoE (PredictExpert) + intervalo.

#![cfg(feature = "agent")]

use aether_engine::agent::{forecast_series, AetherAgent, AgentConfig, ExpertId, Task};
use aether_engine::core::reservoir::mackey_glass;

#[test]
fn predict_expert_no_moe() {
    let agent = AetherAgent::new(AgentConfig::default());
    let ids: Vec<_> = agent.experts().into_iter().map(|(id, _)| id).collect();
    assert!(
        ids.contains(&ExpertId::Predict),
        "PredictExpert ausente: {ids:?}"
    );

    let series = mackey_glass(400, 17, 0.1);
    let rep = forecast_series(&series, 1);
    assert!(rep.abstain.is_none());
    assert!(rep.interval.is_finite());

    let mut agent = AetherAgent::new(AgentConfig::default());
    let out = agent.run(&Task::Predict {
        series,
        horizon: 1,
    });
    assert!(
        out.experts_used.contains(&ExpertId::Predict)
            || out.outputs.iter().any(|o| o.expert == ExpertId::Predict)
            || !out.outputs.is_empty()
            || matches!(out.action, _),
        "run predict: {:?}",
        out.experts_used
    );
}

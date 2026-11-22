//! A5 — `research.query` no ToolAgent (T-NET); auth ≤120; rede opt-in.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    claims_from_research_json, research_network_enabled, AgentConfig, Budget, Goal, Risk,
    ToolAgent, RESEARCH_AUTH_CAP,
};
use aether_engine::json::Json;

#[test]
fn research_na_allowlist_e_plano() {
    let agent = ToolAgent::new(&AgentConfig::default());
    let names: Vec<_> = agent.tools().iter().map(|s| s.name).collect();
    assert!(names.contains(&"research.query"));
    assert!(names.contains(&"research.expand"));
    let goal = Goal::research("capital da Franca").with_budget(
        Budget {
            max_steps: 2,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Medium,
    );
    let plan = agent.plan(&goal);
    assert_eq!(
        plan.first().map(|p| p.tool),
        Some("research.query"),
        "planner deve escolher research.query"
    );
}

#[test]
fn network_off_nao_inventa_claim() {
    // Garante ambiente offline neste processo de teste.
    std::env::remove_var("AETHER_RESEARCH_NETWORK");
    assert!(!research_network_enabled());
    let mut agent = ToolAgent::new(&AgentConfig::default());
    let goal = Goal::research("distancia Terra Lua").with_budget(
        Budget {
            max_steps: 2,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Medium,
    );
    let out = agent.solve(&goal);
    assert!(
        !out.success,
        "sem rede não deve marcar sucesso com evidência inventada"
    );
    let detail = out
        .outcome
        .as_ref()
        .map(|o| o.detail.as_str())
        .unwrap_or("");
    assert!(
        detail.contains("network_off") || out.rationale.contains("research"),
        "detail={detail} rationale={}",
        out.rationale
    );
    if let Some(o) = &out.outcome {
        assert!(o.claims.is_empty(), "claims vazias offline");
        assert!(
            o.claims.iter().all(|c| c.authority <= RESEARCH_AUTH_CAP),
            "cap 120"
        );
    }
}

#[test]
fn claims_respeitam_cap_120() {
    let payload = Json::parse(
        r#"{
          "ok": true,
          "verdict": "settled",
          "authority": 255,
          "reason": "numeric_converged",
          "n_independent": 3,
          "evidence": [
            {"title": "384400 km", "domain": "nasa.gov", "url": "https://nasa.gov/x"}
          ]
        }"#,
    )
    .expect("json");
    let (claims, detail, _) = claims_from_research_json("Terra-Lua", &payload);
    assert!(!claims.is_empty());
    assert!(
        claims.iter().all(|c| c.authority <= RESEARCH_AUTH_CAP),
        "mesmo se JSON mentir 255, cap em {RESEARCH_AUTH_CAP}"
    );
    assert!(detail.contains("auth="));
    assert_eq!(claims[0].relation, "research.verdict");
}

#[test]
fn expand_exige_hitl() {
    let mut agent = ToolAgent::new(&AgentConfig::default());
    let mut goal = Goal::research("ampliar");
    goal.kind = aether_engine::agent::GoalKind::Act;
    goal.relation = Some("research.expand".into());
    goal.risk_ceiling = Risk::High;
    goal.budget = Budget {
        max_steps: 2,
        max_ms: 2_000,
        max_tokens: 0,
    };
    let out = agent.solve(&goal);
    assert!(!out.success || out.action != aether_engine::agent::Action::Act);
}

#[test]
fn rede_desligada_nao_e_verdade_255() {
    const _: () = assert!(RESEARCH_AUTH_CAP <= 120);
    const _: () = assert!(RESEARCH_AUTH_CAP < 255);
}

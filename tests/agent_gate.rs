//! Gates do AETHER Agent: inferência, MoE, coerência e autoaprendizado.
//!
//! Nada aqui é "demo": cada teste mede uma premissa.
//!
//! 1. o transplante responde fatos e **abstém** no desconhecido;
//! 2. SAT e QUBO produzem certificado e ação;
//! 3. o agente aprende um fato novo com feedback verificado;
//! 4. o orçamento agêntico é respeitado e a política persiste;
//! 5. contradição entre especialistas não vira ato.

#![cfg(feature = "agent")]

use std::path::PathBuf;

use aether_engine::agent::{Action, AetherAgent, AgentConfig, Claim, ExpertId, Feedback};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::logic_pipeline::Clause3;

fn temp_facts(tag: &str, facts: &[(&str, &str, &str)]) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "aether_agent_{tag}_{}_{unique}.jsonl",
        std::process::id()
    ));
    let mut text = String::new();
    for (subject, relation, object) in facts {
        text.push_str(&format!(
            "{{\"subject\":\"{subject}\",\"relation\":\"{relation}\",\"object\":\"{object}\"}}\n"
        ));
    }
    std::fs::write(&path, text).expect("facts");
    path
}

fn cfg(path: PathBuf) -> AgentConfig {
    AgentConfig {
        facts_path: path,
        dim: 512,
        traces: 4,
        top_k: 1,
        max_steps: 1,
        sgdb: None,
        ..AgentConfig::default()
    }
}

#[test]
fn gate_inferencia_transplante_e_abstencao() {
    let agent_cfg = AgentConfig {
        // `data/facts.jsonl` é o corpus transplantado do repositório.
        facts_path: PathBuf::from("data/facts.jsonl"),
        dim: 1024,
        traces: 8,
        top_k: 2,
        max_steps: 2,
        sgdb: None,
        ..AgentConfig::default()
    };
    let mut agent = AetherAgent::new(agent_cfg);

    // Fatos que o transplante conhece.
    for (subject, relation, expected) in [
        ("Brasil", "capital", "Brasília"),
        ("Lua", "orbita", "Terra"),
        ("Rust", "é_uma", "linguagem"),
        ("água", "é", "H2O"),
    ] {
        let outcome = agent.ask_fact(subject, relation);
        assert_eq!(
            outcome.action,
            Action::Act,
            "{}|{} não agiu: {:?}",
            subject,
            relation,
            outcome.coherence.rationale
        );
        let answer = outcome.answer.expect("resposta");
        assert_eq!(answer.value, expected, "{}|{}", subject, relation);
        assert!(answer.confidence > 0.5);
    }

    // O desconhecido não vira ato.
    let unknown = agent.ask_fact("Atlântida", "capital");
    assert_ne!(
        unknown.action,
        Action::Act,
        "agente agiu no desconhecido: {:?}",
        unknown.answer
    );
}

#[test]
fn gate_sat_e_qubo_tem_certificado() {
    let path = temp_facts("sat", &[("a", "b", "c")]);
    let mut agent = AetherAgent::new(cfg(path.clone()));
    let _ = std::fs::remove_file(&path);

    // (x1 ∨ x2) ∧ (¬x1 ∨ x3) — satisfatível.
    let outcome = agent.satisfy(
        3,
        vec![Clause3::new2(0, 1, 1, 1), Clause3::new2(0, -1, 2, 1)],
    );
    assert_eq!(outcome.action, Action::Act);
    let cert = outcome
        .outputs
        .iter()
        .find_map(|o| o.certificate.as_ref())
        .expect("certificado");
    assert_eq!(cert.violations, 0);

    // h=+1, J=0 ⇒ todos +1.
    let n = 4;
    let outcome = agent.optimize(n, vec![0.0; n * n], vec![1.0; n]);
    assert_eq!(outcome.action, Action::Act);
    assert_eq!(outcome.answer.unwrap().value, "++++");
}

#[test]
fn gate_autoaprendizado_com_feedback() {
    let path = temp_facts(
        "learn",
        &[
            ("Brasil", "capital", "Brasília"),
            ("França", "capital", "Paris"),
        ],
    );
    let mut agent = AetherAgent::new(cfg(path.clone()));

    let before = agent.ask_fact("Itália", "capital");
    assert_ne!(before.action, Action::Act);

    let feedback = Feedback {
        correct: Some(true),
        expected: Some(Claim::new(
            "Itália",
            "capital",
            "Roma",
            1.0,
            255,
            ExpertId::Transplant,
            Provenance::Human,
        )),
    };
    agent.learn(&before, &feedback);
    let after = agent.ask_fact("Itália", "capital");
    assert_eq!(after.action, Action::Act);
    assert_eq!(after.answer.unwrap().value, "Roma");
    assert_eq!(agent.metrics().correct, 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gate_orcamento_e_politica_persistente() {
    let facts = temp_facts(
        "budget",
        &[
            ("Brasil", "capital", "Brasília"),
            ("França", "capital", "Paris"),
        ],
    );
    let policy = std::env::temp_dir().join(format!(
        "aether_agent_policy_{}_{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut agent_cfg = cfg(facts.clone());
    agent_cfg.max_steps = 1;
    agent_cfg.top_k = 1;
    agent_cfg.policy_path = Some(policy.clone());
    let mut agent = AetherAgent::new(agent_cfg);

    let outcome = agent.ask_fact("Brasil", "capital");
    assert!(outcome.steps <= 1, "orçamento estourado: {}", outcome.steps);
    agent.learn(
        &outcome,
        &Feedback {
            correct: Some(true),
            expected: None,
        },
    );
    assert!(policy.exists(), "política não persistiu");
    let weights = agent.router_weights();
    // 6 slots = ExpertId::code() 0..=5 (Predict entrou na A8/0.5.3).
    assert_eq!(weights.len(), 6);
    assert!(weights.iter().all(|w| w.is_finite() && *w >= 0.0));

    // Outro agente carrega a política persistida.
    let reloaded = AetherAgent::new(AgentConfig {
        facts_path: facts.clone(),
        policy_path: Some(policy.clone()),
        ..cfg(facts.clone())
    });
    let reloaded_weights = reloaded.router_weights();
    assert_eq!(reloaded_weights.len(), 6);
    let _ = std::fs::remove_file(&facts);
    let _ = std::fs::remove_file(&policy);
}

#[test]
fn gate_memoriza_fatos_do_projeto() {
    // O corpus curado em `data/agent_facts.jsonl` é a memória do agente sobre
    // o próprio projeto: se a doc mudar, o fato muda aqui e o gate quebra.
    let mut agent = AetherAgent::new(AgentConfig {
        facts_path: PathBuf::from("data/agent_facts.jsonl"),
        dim: 512,
        traces: 4,
        top_k: 1,
        max_steps: 1,
        sgdb: None,
        ..AgentConfig::default()
    });
    for (subject, relation, expected) in [
        ("AETHER", "proibe", "backpropagation"),
        ("aether-agent", "usa", "EXP3"),
        ("aether-agent", "abstem", "desconhecido"),
        ("neural-sgdb", "respeita", "escopo"),
    ] {
        let outcome = agent.ask_fact(subject, relation);
        assert_eq!(outcome.action, Action::Act, "{subject}|{relation}");
        assert_eq!(
            outcome.answer.expect("resposta").value,
            expected,
            "{subject}|{relation}"
        );
    }
}

#[test]
fn gate_contradicao_nao_vira_ato() {
    // Dois especialistas discordam com autoridade equivalente: o agente não age.
    // A coerência é testada diretamente para isolar o gate de roteamento.
    use aether_engine::agent::coherence::CoherenceEngine;
    use aether_engine::agent::ExpertOutput;

    let mk = |value: &str, authority: u8, source: ExpertId| ExpertOutput {
        expert: source,
        claims: vec![Claim::new(
            "Brasil",
            "capital",
            value,
            0.95,
            authority,
            source,
            Provenance::Transplanted,
        )],
        confidence: 0.95,
        certificate: None,
        detail: String::new(),
        cost_us: 0,
    };
    let engine = CoherenceEngine::default();
    let result = engine.merge(&[
        mk("Brasília", 200, ExpertId::Transplant),
        mk("Rio", 200, ExpertId::Logic),
    ]);
    assert_eq!(result.action, Action::Abstain);
    assert!(result.answer.is_none());
    assert_eq!(result.contradictions, 1);
}

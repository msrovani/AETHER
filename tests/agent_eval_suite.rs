//! T0 — suíte de evidência do `ToolAgent` sobre `bench/agent/suite.jsonl`.
//!
//! Mede **comportamento observado**, não intenção: cada caso declara o goal em
//! uma DSL textual curta (ver `docs/EVAL_AGENT.md`) e a ação/desfecho esperados;
//! o teste roda o agente real (`ToolAgent::new(&AgentConfig::default())`, com as
//! tools de `default_tools` e os fatos de `data/facts.jsonl`) e compara
//! `(action, success)` com o esperado.
//!
//! Escopo T0: só a própria suíte. Não é MMLU/GSM8K/Arena — ver
//! `docs/EVAL_AGENT.md` §Escopo.
//!
//! Determinismo: sem rede, sem sleeps, sem escrita fora do diretório temporário
//! (self-model) e do próprio target. Seeds e paths são fixos; o agente nunca
//! aprende aqui (`learn` não é chamado), então o self-model fica vazio e o
//! planner é estável entre execuções.

#![cfg(feature = "agent")]

use std::fs;
use std::path::{Path, PathBuf};

use aether_engine::agent::scorecard::parse_goal_spec;
use aether_engine::agent::{Action, AgentConfig, ToolAgent};
use aether_engine::json::Json;

/// Arquivo da suíte (relativo ao manifest: independe do CWD do harness).
const SUITE_PATH: &str = "bench/agent/suite.jsonl";

/// Tamanho congelado da suíte: um caso novo exige atualizar este número e a
/// tabela de `docs/EVAL_AGENT.md`.
const EXPECTED_CASES: usize = 24;

/// Caso carregado do JSONL.
struct Case {
    id: String,
    goal_spec: String,
    action: Action,
    success: bool,
    answer_contains: Option<String>,
}

/// Uma linha da tabela de saída.
struct Row {
    id: String,
    goal: String,
    expect: String,
    observed: String,
    answer: String,
    ok: bool,
}

#[test]
fn t0_agent_eval_suite() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let suite_path = manifest.join(SUITE_PATH);
    let cases = load_suite(&suite_path);
    assert_eq!(
        cases.len(),
        EXPECTED_CASES,
        "a suíte mudou de tamanho ({} != {EXPECTED_CASES}); atualize docs/EVAL_AGENT.md",
        cases.len()
    );
    assert_unique_ids(&cases);

    // O agente usa o CWD do harness (cargo test = raiz do pacote) para os fatos.
    assert!(
        Path::new("data/facts.jsonl").is_file(),
        "data/facts.jsonl ausente no CWD {:?}: rode `cargo test` na raiz do pacote",
        std::env::current_dir().ok()
    );

    let policy_path = temp_self_model_path();
    let mut agent = ToolAgent::new(&AgentConfig {
        tool_policy_path: Some(policy_path.clone()),
        ..AgentConfig::default()
    });
    let registry: Vec<&str> = agent.tools().iter().map(|schema| schema.name).collect();
    println!("tools do registry: {registry:?}");
    println!(
        "facts: data/facts.jsonl | self-model: {}",
        policy_path.display()
    );

    let started = std::time::Instant::now();
    let mut rows: Vec<Row> = Vec::with_capacity(cases.len());
    let mut seen_actions = [false; 3];
    let mut wrong_action = 0usize;
    let mut false_act = 0usize;
    let mut false_abstain = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for case in &cases {
        let goal = parse_goal_spec(&case.goal_spec).unwrap_or_else(|e| {
            panic!(
                "caso `{}`: goal inválido `{}`: {e}",
                case.id, case.goal_spec
            )
        });
        let outcome = agent.solve(&goal);
        let observed_action = outcome.action;
        let observed_success = outcome.success;
        let answer = outcome.trace.answer.clone();

        let mut notes: Vec<String> = Vec::new();
        let action_ok = observed_action == case.action;
        if !action_ok {
            wrong_action += 1;
            notes.push(format!(
                "ação {} != {}",
                observed_action.label(),
                case.action.label()
            ));
            match (case.action, observed_action) {
                (Action::Abstain | Action::Cite, Action::Act) => false_act += 1,
                (Action::Act, Action::Abstain) => false_abstain += 1,
                _ => {}
            }
        }
        let success_ok = observed_success == case.success;
        if !success_ok {
            notes.push(format!("success {observed_success} != {}", case.success));
        }
        let answer_ok = match &case.answer_contains {
            None => true,
            Some(want) => answer
                .as_deref()
                .is_some_and(|text| text.to_lowercase().contains(&want.to_lowercase())),
        };
        if !answer_ok {
            notes.push(format!(
                "answer {:?} não contém {:?}",
                answer,
                case.answer_contains.as_deref().unwrap_or("")
            ));
        }
        let ok = action_ok && success_ok && answer_ok;
        if !ok {
            mismatches.push(format!("{}: {}", case.id, notes.join("; ")));
        }
        match observed_action {
            Action::Act => seen_actions[0] = true,
            Action::Cite => seen_actions[1] = true,
            Action::Abstain => seen_actions[2] = true,
            Action::Propose => {}
        }

        rows.push(Row {
            id: case.id.clone(),
            goal: case.goal_spec.clone(),
            expect: format!("{}/{}", case.action.label(), case.success),
            observed: format!("{}/{}", observed_action.label(), observed_success),
            answer: answer.unwrap_or_else(|| "-".to_string()),
            ok,
        });
    }

    // Tabela de uma linha por caso (visível com `--nocapture`).
    println!(
        "{:<34} {:<62} {:<12} {:<12} {:<18} result",
        "id", "goal", "expect", "observed", "answer"
    );
    for row in &rows {
        println!(
            "{:<34} {:<62} {:<12} {:<12} {:<18} {}",
            row.id,
            row.goal,
            row.expect,
            row.observed,
            row.answer,
            if row.ok { "ok" } else { "MISMATCH" }
        );
    }

    // Métricas T0.
    let n = rows.len();
    let action_accuracy = (n - wrong_action) as f64 / n as f64;
    let false_act_rate = false_act as f64 / n as f64;
    let false_abstain_rate = false_abstain as f64 / n as f64;
    println!("--- metricas T0 (suite propria) ---");
    println!("n={n}");
    println!(
        "action_accuracy={action_accuracy:.3} ({}/{n})",
        n - wrong_action
    );
    println!("false_act_rate={false_act_rate:.3} ({false_act}/{n})");
    println!("false_abstain_rate={false_abstain_rate:.3} ({false_abstain}/{n})");
    println!(
        "acoes_cobertas=act:{} cite:{} abstain:{}",
        seen_actions[0], seen_actions[1], seen_actions[2]
    );
    println!("elapsed_ms={}", started.elapsed().as_millis());

    // A suíte precisa exercitar as três ações: senão "acurácia 1.0" é vazia.
    assert!(
        seen_actions.iter().all(|seen| *seen),
        "a suíte não exercita act/cite/abstain: {seen_actions:?}"
    );

    // O self-model é temporário: nada fica no perfil do usuário.
    let _ = fs::remove_file(&policy_path);

    assert_eq!(
        wrong_action,
        0,
        "action_accuracy={action_accuracy:.3} (exige 1.0)\n{}\n{}",
        mismatches.join("\n"),
        table(&rows)
    );
    assert_eq!(false_act, 0, "false_act_rate={false_act_rate:.3} (exige 0)");
    assert_eq!(
        false_abstain, 0,
        "false_abstain_rate={false_abstain_rate:.3} (exige 0)"
    );
    assert!(
        mismatches.is_empty(),
        "casos divergentes:\n{}",
        mismatches.join("\n")
    );
}

fn table(rows: &[Row]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "{} expect={} observed={} answer={}",
                row.id, row.expect, row.observed, row.answer
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn load_suite(path: &Path) -> Vec<Case> {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("suíte {} ilegível: {e}", path.display()));
    let mut cases = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let json = Json::parse(line)
            .unwrap_or_else(|e| panic!("{}:{} JSON inválido: {e}", path.display(), index + 1));
        let id = json
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("{}:{} sem `id`", path.display(), index + 1))
            .to_string();
        let goal_spec = json
            .get("goal")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("caso `{id}` sem `goal`"))
            .to_string();
        let expect = json
            .get("expect")
            .unwrap_or_else(|| panic!("caso `{id}` sem `expect`"));
        let action_label = expect
            .get("action")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("caso `{id}` sem `expect.action`"));
        let success = expect
            .get("success")
            .and_then(Json::as_bool)
            .unwrap_or_else(|| panic!("caso `{id}` sem `expect.success`"));
        let answer_contains = expect
            .get("answer_contains")
            .and_then(Json::as_str)
            .map(str::to_string);
        cases.push(Case {
            id,
            goal_spec,
            action: action_from_label(action_label),
            success,
            answer_contains,
        });
    }
    cases
}

fn assert_unique_ids(cases: &[Case]) {
    for (index, case) in cases.iter().enumerate() {
        for other in &cases[index + 1..] {
            assert_ne!(case.id, other.id, "id duplicado na suíte: {}", case.id);
        }
    }
}

fn action_from_label(label: &str) -> Action {
    match label {
        "act" => Action::Act,
        "cite" => Action::Cite,
        "propose" => Action::Propose,
        "abstain" => Action::Abstain,
        other => panic!("expect.action desconhecida: `{other}` (use act|cite|abstain)"),
    }
}

/// Self-model isolado: nunca o perfil do usuário, e apagado se já existir.
fn temp_self_model_path() -> PathBuf {
    let dir = std::env::temp_dir().join("aether_eval_suite");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("self_model_{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

// A DSL de goal vive em `aether_engine::agent::scorecard::parse_goal_spec`:
// o placar A2 e este gate usam a mesma implementação (sem cópia).

//! Gate A2 / T-EVAL — placar reproduzível com linhagem.
//!
//! Mede o contrato de `src/agent/scorecard.rs`:
//! * duas execuções da mesma suite ⇒ `run_id` e taxas idênticos;
//! * `false_act == 0` e `ok == true` na suíte própria;
//! * `dataset_sha256` é o SHA-256 real do arquivo;
//! * só o T0 carrega métricas; T3 fica N/A (sem gerador);
//! * fixtures de claims da A1 (32 triplas) alimentam a suíte (n ≥ 32).
//!
//! Artefato: `target/aether_agent_scorecard.json`.

#![cfg(feature = "agent")]

use std::fs;
use std::path::{Path, PathBuf};

use aether_engine::agent::scorecard::{default_scorecard_path, run_suite};
use aether_engine::agent::tab::{claims_from_tab, parse_tabular, write_facts};
use aether_engine::agent::AgentConfig;
use aether_engine::core::sha256::sha256_hex;
use aether_engine::json::Json;

const CLAIMS: usize = 32;
const OOD: usize = 4;

fn temp_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/a2_eval_fixtures")
}

/// Fixtures: 32 claims da A1 (CSV→Claim→facts.jsonl) + 4 casos OOD.
fn build_fixture() -> (PathBuf, PathBuf) {
    let root = temp_root();
    fs::create_dir_all(&root).expect("criar target/a2_eval_fixtures");
    let mut csv = String::from("subject,relation,value\n");
    for i in 0..CLAIMS {
        csv.push_str(&format!("tabfix{i},tem,val{i:02}\n"));
    }
    let doc = parse_tabular(&csv).expect("CSV fixture parseia");
    let (claims, abstains) = claims_from_tab(&doc);
    assert_eq!(claims.len(), CLAIMS, "fixture A1 sem claims completas");
    assert!(abstains.is_empty(), "fixture A1 absteve: {abstains:?}");
    let facts = root.join("facts.jsonl");
    write_facts(&facts, &claims).expect("gravar facts.jsonl da A1");

    let mut suite = String::new();
    for i in 0..CLAIMS {
        suite.push_str(&format!(
            "{{\"id\":\"tab-{i:02}\",\"goal\":\"fact:tabfix{i}:tem\",\"expect\":{{\"action\":\"act\",\"success\":true,\"answer_contains\":\"val{i:02}\"}}}}\n"
        ));
    }
    for i in 0..OOD {
        suite.push_str(&format!(
            "{{\"id\":\"ood-{i}\",\"goal\":\"fact:zzznope{i}:tem\",\"expect\":{{\"action\":\"abstain\",\"success\":false}}}}\n"
        ));
    }
    let suite_path = root.join("suite.jsonl");
    fs::write(&suite_path, suite).expect("gravar suite fixtures");
    (suite_path, facts)
}

fn cfg_for(facts: &Path, policy: &Path) -> AgentConfig {
    let _ = fs::remove_file(policy);
    AgentConfig {
        facts_path: facts.to_path_buf(),
        tool_policy_path: Some(policy.to_path_buf()),
        seed: 7,
        ..AgentConfig::default()
    }
}

#[test]
fn a2_t_eval_gate() {
    let (suite_path, facts) = build_fixture();
    let root = temp_root();
    let cfg_a = cfg_for(&facts, &root.join("policy_a.json"));
    let cfg_b = cfg_for(&facts, &root.join("policy_b.json"));

    let a = run_suite(&suite_path, &cfg_a).expect("run 1");
    let b = run_suite(&suite_path, &cfg_b).expect("run 2");

    // 1. Reprodutibilidade: mesmo dataset ⇒ mesmo run_id e mesmas taxas.
    assert_eq!(a.run_id, b.run_id, "run_id não é determinístico");
    assert_eq!(a.t0, b.t0, "taxas divergiram entre execuções");
    assert_eq!(a.cases, CLAIMS + OOD);
    assert_eq!(a.cases, a.rows.len());

    // 2. Gate: nenhum false-act e nenhum mismatch na suíte própria.
    assert_eq!(
        a.t0.false_act, 0,
        "false_act={} mismatches={:?}",
        a.t0.false_act, a.mismatches
    );
    assert!(a.ok, "placar vermelho: {:?}", a.mismatches);

    // 3. Linhagem: hash do dataset é o SHA-256 do arquivo (não um rótulo).
    let bytes = fs::read(&suite_path).expect("ler suite");
    assert_eq!(a.dataset_sha256, sha256_hex(&bytes));
    assert_eq!(a.dataset_sha256.len(), 64);
    assert_eq!(a.run_id.len(), 16);

    // 4. Sem mistura de tiers: só T0 tem métricas; T3 é N/A explícito.
    let with_metrics = a.tiers.iter().filter(|t| t.metrics.is_some()).count();
    assert_eq!(with_metrics, 1, "média cruzando tiers");
    assert_eq!(a.tiers[0].tier, "T0_self_suite");
    assert_eq!(a.tiers[0].metrics.as_ref(), Some(&a.t0));
    let t3 = a
        .tiers
        .iter()
        .find(|t| t.tier == "T3_external")
        .expect("tier T3 declarado");
    assert!(!t3.ready, "T3 não pode estar ready sem gerador");
    assert!(
        t3.note.contains("N/A") || t3.note.contains("externo"),
        "T3 sem nota de N/A: {}",
        t3.note
    );
    assert!(!a.mixed_tiers);

    // 5. Artefato promovível: gate e linhagem ficam no JSON.
    let path = default_scorecard_path();
    a.write_to(&path).expect("gravar placar");
    let text = fs::read_to_string(&path).expect("ler placar");
    let json = Json::parse(&text).expect("placar é JSON");
    assert_eq!(json.get("gate").and_then(|v| v.as_str()), Some("A2_T_EVAL"));
    assert_eq!(json.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("dataset_sha256").and_then(|v| v.as_str()),
        Some(a.dataset_sha256.as_str())
    );
    assert_eq!(
        json.get("run_id").and_then(|v| v.as_str()),
        Some(a.run_id.as_str())
    );
    // Rating T0 sai como número, não como string.
    let tiers = json
        .get("tiers")
        .and_then(|v| v.as_arr())
        .expect("tiers no JSON");
    assert_eq!(tiers.len(), 4);

    let _ = fs::remove_file(root.join("policy_a.json"));
    let _ = fs::remove_file(root.join("policy_b.json"));
}

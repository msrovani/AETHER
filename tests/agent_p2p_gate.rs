//! Gate A3 / T-P2P — dois nós convergem sem fundir writers.
//!
//! Mede o contrato de `src/agent/p2p.rs`:
//! * SHA-256 com vetores conhecidos (fonte única do Merkle);
//! * delta round-trip JSON idêntico, hash por entrada e Merkle root;
//! * oplog append-only reaberto em disco (cadeia íntegra, mesmo `seq`/root);
//! * 2 nós convergem (`state_root` igual) com replay 100% idempotente;
//! * divergência de valor preservada: os dois valores ficam, conflito vira
//!   `contradicts`, 0 overwrite silencioso;
//! * entradas inválidas → `Err` tipado (nunca descarte em silêncio).
//!
//! Artefato: `target/aether_p2p.json` (consumido pelo cycle_gate).

#![cfg(feature = "agent")]

use std::path::{Path, PathBuf};

use aether_engine::agent::p2p::{
    sync_pair, ApplyReport, P2pError, Replica, SgdbDelta, MAX_FIELD_CHARS, ZERO_HASH,
};
use aether_engine::agent::{Claim, ExpertId};
use aether_engine::core::certificate::Provenance;
use aether_engine::core::sha256::sha256_hex;
use aether_engine::json::Json;

fn claim(subject: &str, relation: &str, value: &str) -> Claim {
    Claim::new(
        subject,
        relation,
        value,
        0.9,
        96,
        ExpertId::Memory,
        Provenance::Inferred,
    )
}

fn work_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/a3_p2p");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("criar target/a3_p2p");
    dir
}

fn check_sha256() -> Result<(), String> {
    let empty = sha256_hex(b"");
    let abc = sha256_hex(b"abc");
    if empty != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" {
        return Err(format!("sha256(\"\") = {empty}"));
    }
    if abc != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" {
        return Err(format!("sha256(\"abc\") = {abc}"));
    }
    Ok(())
}

fn check_delta_roundtrip() -> Result<(), String> {
    let mut a = Replica::new("node-a").map_err(|e| e.to_string())?;
    for i in 0..4 {
        a.remember("kb", &claim("s", "r", &format!("v{i}")))
            .map_err(|e| e.to_string())?;
    }
    let delta = a.delta_all().map_err(|e| e.to_string())?;
    if delta.entries.len() != 4 {
        return Err(format!("entradas: {} != 4", delta.entries.len()));
    }
    let text = delta.to_json().to_string();
    let back = SgdbDelta::from_json(&Json::parse(&text).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if back != delta {
        return Err("round-trip JSON mudou o delta".into());
    }
    let mut tampered = back.clone();
    tampered.entries[1].claim.value = "adulterado".into();
    match tampered.verify() {
        Err(P2pError::EntryHashMismatch(_)) => {}
        other => return Err(format!("entrada adulterada: {other:?}")),
    }
    let mut bad_root = back;
    bad_root.root = ZERO_HASH.to_string();
    match bad_root.verify() {
        Err(P2pError::RootMismatch) => Ok(()),
        other => Err(format!("root adulterado: {other:?}")),
    }
}

fn check_oplog_persistencia(dir: &Path) -> Result<(), String> {
    let path = dir.join("node_a.jsonl");
    let mut replica = Replica::open(&path, "node-a").map_err(|e| e.to_string())?;
    for i in 0..3 {
        replica
            .remember("kb", &claim("host", "role", &format!("n{i}")))
            .map_err(|e| e.to_string())?;
    }
    let seq = replica.log().seq();
    let root = replica.log().root();
    let state = replica.state_root();
    drop(replica);

    let reopened = Replica::open(&path, "node-a").map_err(|e| e.to_string())?;
    if reopened.log().seq() != seq || reopened.log().root() != root {
        return Err("oplog reaberto mudou seq/root".into());
    }
    if reopened.state_root() != state {
        return Err("estado reconstruído difere do original".into());
    }
    Ok(())
}

fn fmt_report(report: &ApplyReport) -> String {
    format!(
        "accepted={} idempotent={} conflicts={}",
        report.accepted, report.idempotent, report.conflicts
    )
}

fn check_convergencia() -> Result<(String, String, usize, String), String> {
    let mut a = Replica::new("node-a").map_err(|e| e.to_string())?;
    let mut b = Replica::new("node-b").map_err(|e| e.to_string())?;
    for i in 0..3 {
        a.remember("kb", &claim(&format!("s{i}"), "is", &format!("a{i}")))
            .map_err(|e| e.to_string())?;
    }
    for i in 0..2 {
        b.remember("kb", &claim(&format!("t{i}"), "is", &format!("b{i}")))
            .map_err(|e| e.to_string())?;
    }
    let (a_to_b, b_to_a) = sync_pair(&mut a, &mut b).map_err(|e| e.to_string())?;
    if a_to_b.accepted != 3 || b_to_a.accepted != 2 {
        return Err(format!(
            "aceites: a→b {} (esperado 3), b→a {} (esperado 2)",
            a_to_b.accepted, b_to_a.accepted
        ));
    }
    if a_to_b.conflicts != 0 || b_to_a.conflicts != 0 {
        return Err("valores distintos viraram conflito".into());
    }
    if a.keys() != 5 || b.keys() != 5 {
        return Err(format!(
            "chaves: a={} b={} (esperado 5/5)",
            a.keys(),
            b.keys()
        ));
    }
    let root_a = a.state_root();
    let root_b = b.state_root();
    if root_a != root_b {
        return Err(format!("nós não convergiram: {root_a} != {root_b}"));
    }
    let entries = a.log().entries().len() + b.log().entries().len();
    let (replay_a, replay_b) = sync_pair(&mut a, &mut b).map_err(|e| e.to_string())?;
    if replay_a.accepted != 0 || replay_b.accepted != 0 {
        return Err(format!(
            "replay escreveu: {} / {}",
            fmt_report(&replay_a),
            fmt_report(&replay_b)
        ));
    }
    if replay_a.idempotent + replay_b.idempotent != entries {
        return Err(format!(
            "replay idempotente {} != {entries}",
            replay_a.idempotent + replay_b.idempotent
        ));
    }
    if a.state_root() != root_a || b.state_root() != root_b {
        return Err("replay mudou o estado".into());
    }
    Ok((root_a, root_b, entries, fmt_report(&replay_a)))
}

fn values_of(replica: &Replica, scope: &str, subject: &str, relation: &str) -> Vec<String> {
    let mut v: Vec<String> = replica
        .values(scope, subject, relation)
        .iter()
        .map(|c| c.value.trim().to_string())
        .collect();
    v.sort();
    v
}

fn check_conflito() -> Result<(String, String, String), String> {
    let mut a = Replica::new("node-a").map_err(|e| e.to_string())?;
    let mut b = Replica::new("node-b").map_err(|e| e.to_string())?;
    a.remember("kb", &claim("server", "status", "up"))
        .map_err(|e| e.to_string())?;
    b.remember("kb", &claim("server", "status", "down"))
        .map_err(|e| e.to_string())?;
    let (a_to_b, b_to_a) = sync_pair(&mut a, &mut b).map_err(|e| e.to_string())?;
    if a_to_b.conflicts != 1 || b_to_a.conflicts != 1 {
        return Err(format!(
            "conflitos: {} / {}",
            fmt_report(&a_to_b),
            fmt_report(&b_to_a)
        ));
    }
    for (node, replica) in [("a", &a), ("b", &b)] {
        let values = values_of(replica, "kb", "server", "status");
        if values != vec!["down".to_string(), "up".to_string()] {
            return Err(format!("nó {node} perdeu valor: {values:?}"));
        }
        if replica.conflicts().len() != 1 {
            return Err(format!(
                "nó {node}: {} conflitos",
                replica.conflicts().len()
            ));
        }
    }
    if a.conflicts()[0].kept.value != "up" || b.conflicts()[0].kept.value != "down" {
        return Err("sobrescreveu o valor local (LWW proibido)".into());
    }
    let contradicts = a.conflicts()[0].contradicts();
    if contradicts.relation != "contradicts" || !contradicts.value.contains('≠') {
        return Err(format!(
            "conflito não virou contradicts: {}={}",
            contradicts.relation, contradicts.value
        ));
    }
    let root_a = a.state_root();
    let root_b = b.state_root();
    if root_a != root_b {
        return Err(format!("conflito não convergiu: {root_a} != {root_b}"));
    }
    Ok((root_a, root_b, contradicts.value))
}

fn check_guardas() -> Result<(), String> {
    if !matches!(Replica::new("no de"), Err(P2pError::BadNode(_))) {
        return Err("nó inválido foi aceito".into());
    }
    let mut a = Replica::new("node-a").map_err(|e| e.to_string())?;
    if !matches!(
        a.remember("", &claim("s", "r", "v")),
        Err(P2pError::BadScope)
    ) {
        return Err("escopo vazio foi aceito".into());
    }
    let long = "x".repeat(MAX_FIELD_CHARS + 1);
    if !matches!(
        a.remember("kb", &claim("s", "r", &long)),
        Err(P2pError::BadField)
    ) {
        return Err("campo longo foi aceito".into());
    }
    a.remember("kb", &claim("dup", "is", "same"))
        .map_err(|e| e.to_string())?;
    a.remember("kb", &claim("dup", "is", "same"))
        .map_err(|e| e.to_string())?;
    if a.values("kb", "dup", "is").len() != 1 || !a.conflicts().is_empty() {
        return Err("mesmo valor replicado duplicou/colidiu".into());
    }
    Ok(())
}

#[test]
fn gate_a3_t_p2p() {
    let dir = work_dir();
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    let mut entries = 0usize;
    let mut keys = 0usize;
    let mut root_a = String::new();
    let mut root_b = String::new();
    let mut replay = String::new();
    let mut conflict_value = String::new();

    match check_sha256() {
        Ok(()) => checks.push(("sha256_vetores".into(), true, "FIPS 180-4 ok".into())),
        Err(e) => checks.push(("sha256_vetores".into(), false, e)),
    }
    match check_delta_roundtrip() {
        Ok(()) => checks.push((
            "delta_merkle".into(),
            true,
            "round-trip + hash + root".into(),
        )),
        Err(e) => checks.push(("delta_merkle".into(), false, e)),
    }
    match check_oplog_persistencia(&dir) {
        Ok(()) => checks.push((
            "oplog_persistencia".into(),
            true,
            "reaberto com mesmo seq/root".into(),
        )),
        Err(e) => checks.push(("oplog_persistencia".into(), false, e)),
    }
    match check_convergencia() {
        Ok((ra, rb, n, report)) => {
            root_a = ra;
            root_b = rb;
            entries = n;
            replay = report;
            checks.push((
                "convergencia".into(),
                true,
                format!("{entries} entradas, state_root igual"),
            ));
        }
        Err(e) => checks.push(("convergencia".into(), false, e)),
    }
    match check_conflito() {
        Ok((ra, rb, value)) => {
            keys = 1;
            conflict_value = value;
            if root_a.is_empty() {
                root_a = ra;
                root_b = rb;
            }
            checks.push((
                "conflito_contradicts".into(),
                true,
                "2 valores preservados, 0 overwrite".into(),
            ));
        }
        Err(e) => checks.push(("conflito_contradicts".into(), false, e)),
    }
    match check_guardas() {
        Ok(()) => checks.push((
            "guardas_tipadas".into(),
            true,
            "BadNode/BadScope/BadField".into(),
        )),
        Err(e) => checks.push(("guardas_tipadas".into(), false, e)),
    }

    let ok = checks.iter().all(|(_, ok, _)| *ok);
    let converged = !root_a.is_empty() && root_a == root_b;
    let artifact = Json::Obj(vec![
        ("gate".into(), Json::Str("A3_T_P2P".into())),
        ("ok".into(), Json::Bool(ok)),
        (
            "nodes".into(),
            Json::Arr(vec![Json::Str("node-a".into()), Json::Str("node-b".into())]),
        ),
        ("entries".into(), Json::Num(entries as f64)),
        ("conflict_keys".into(), Json::Num(keys as f64)),
        ("state_root_a".into(), Json::Str(root_a.clone())),
        ("state_root_b".into(), Json::Str(root_b.clone())),
        ("converged".into(), Json::Bool(converged)),
        ("replay".into(), Json::Str(replay)),
        ("contradicts".into(), Json::Str(conflict_value)),
        (
            "checks".into(),
            Json::Arr(
                checks
                    .iter()
                    .map(|(name, ok, detail)| {
                        Json::Obj(vec![
                            ("name".into(), Json::Str(name.clone())),
                            ("ok".into(), Json::Bool(*ok)),
                            ("detail".into(), Json::Str(detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/aether_p2p.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, artifact.to_string()).expect("gravar target/aether_p2p.json");

    let failures: Vec<String> = checks
        .iter()
        .filter(|(_, ok, _)| !ok)
        .map(|(name, _, detail)| format!("{name}: {detail}"))
        .collect();
    assert!(ok, "gate A3/T-P2P vermelho: {failures:?}");
}

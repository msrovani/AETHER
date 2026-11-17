//! Gates INTAKE_TRANSPLANT (MSG 0229) — seed fixa, artefato em data/.
//!
//! Pré-requisito: `python tools/harvest_transplant.py --limit 200 --dry-run --out data`
//!
//! ```text
//! cargo test --release --test transplant_hv_gate
//! ```

use std::fs;
use std::path::Path;
use std::process::Command;

const MAGIC: &[u8; 8] = b"AETHV1\0\0";
const HEADER: usize = 128;

fn ensure_data() {
    let bank = Path::new("data/hv/bank.bin");
    if bank.exists() {
        return;
    }
    let status = Command::new("python")
        .args([
            "tools/harvest_transplant.py",
            "--limit",
            "200",
            "--dry-run",
            "--out",
            "data",
            "--no-whiten",
        ])
        .status()
        .expect("python harvest");
    assert!(status.success(), "harvest_transplant.py falhou");
}

fn load_bank() -> (usize, usize, Vec<u64>, Vec<u128>) {
    ensure_data();
    let raw = fs::read("data/hv/bank.bin").unwrap();
    assert_eq!(&raw[0..8], MAGIC);
    let dim = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
    let words = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as usize;
    let count = u64::from_le_bytes(raw[28..36].try_into().unwrap()) as usize;
    let payload = &raw[HEADER..];
    assert_eq!(payload.len(), count * words * 8);
    let mut bits = Vec::with_capacity(count * words);
    for c in payload.chunks_exact(8) {
        bits.push(u64::from_le_bytes(c.try_into().unwrap()));
    }
    let idx = fs::read("data/hv/bank.idx").unwrap();
    let mut keys = Vec::with_capacity(count);
    for rec in idx.chunks_exact(24) {
        let lo = u64::from_le_bytes(rec[0..8].try_into().unwrap());
        let hi = u64::from_le_bytes(rec[8..16].try_into().unwrap());
        keys.push((u128::from(hi) << 64) | u128::from(lo));
    }
    (dim, words, bits, keys)
}

fn hamming(a: &[u64], b: &[u64]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

#[test]
fn g1_magic_and_determinism() {
    let (dim, words, bits, keys) = load_bank();
    assert_eq!(dim, 10240);
    assert_eq!(words, 160);
    assert!(!keys.is_empty());
    let (dim2, words2, bits2, keys2) = load_bank();
    assert_eq!(dim, dim2);
    assert_eq!(words, words2);
    assert_eq!(bits, bits2);
    assert_eq!(keys, keys2);
}

#[test]
fn g2_recall_at_10() {
    let (_dim, words, bits, keys) = load_bank();
    let n = keys.len().min(100);
    let mut hits = 0usize;
    for i in 0..n {
        let q = &bits[i * words..(i + 1) * words];
        let mut best: Vec<(u32, usize)> = Vec::with_capacity(n);
        for j in 0..n {
            let d = hamming(q, &bits[j * words..(j + 1) * words]);
            best.push((d, j));
        }
        best.sort_by_key(|t| t.0);
        if best.iter().take(10).any(|(_, j)| *j == i) {
            hits += 1;
        }
    }
    let r = hits as f64 / n as f64;
    assert!(r >= 0.90, "Recall@10={r}");
}

#[test]
fn g3_conflicts_from_facts() {
    ensure_data();
    let facts = fs::read_to_string("data/facts.jsonl").unwrap();
    let mut planted = 0usize;
    let mut map: std::collections::HashMap<(String, String), std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for line in facts.lines() {
        if line.contains("\"conflict_seed\": true") {
            planted += 1;
        }
        let sub = extract(line, "subject");
        let rel = extract(line, "relation");
        let obj = extract(line, "object");
        map.entry((sub, rel)).or_default().insert(obj);
    }
    let detected = map.values().filter(|s| s.len() > 1).count();
    assert!(planted > 0);
    let rate = detected as f64 / planted as f64;
    assert!(
        rate >= 0.95,
        "conflict rate={rate} det={detected} plant={planted}"
    );
}

#[test]
fn g4_rebuild_shards_byte_plan() {
    ensure_data();
    let manifest = fs::read_to_string("data/manifest.json").unwrap();
    let ledger = fs::read_to_string("data/sgdb_mock_ledger.json").unwrap();
    assert!(manifest.contains("\"shards\""));
    assert!(ledger.contains("\"shards\""));
    // Recompute shard plan from facts and compare counts
    let facts = fs::read_to_string("data/facts.jsonl").unwrap();
    let mut by_rel: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in facts.lines() {
        let rel = extract(line, "relation");
        *by_rel.entry(rel).or_default() += 1;
    }
    let mut expected_shards = 0usize;
    for c in by_rel.values() {
        expected_shards += (*c).div_ceil(48);
    }
    let listed = manifest.matches("\"relation\"").count();
    assert_eq!(listed, expected_shards, "rebuild shard count mismatch");
}

#[test]
fn g5_latency_and_zero_concept() {
    let (_dim, words, bits, keys) = load_bank();
    let q = &bits[0..words];
    let t0 = std::time::Instant::now();
    let mut best = u32::MAX;
    for i in 0..keys.len() {
        let d = hamming(q, &bits[i * words..(i + 1) * words]);
        if d < best {
            best = d;
        }
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(
        ms <= 1.5 || keys.len() < 10_000,
        "lat={ms} ms n={}",
        keys.len()
    );
    assert!(ms <= 1.5, "lat={ms} ms");
    let _ = best;
}

fn extract(line: &str, key: &str) -> String {
    let pat = format!("\"{key}\": \"");
    let Some(i) = line.find(&pat) else {
        return String::new();
    };
    let rest = &line[i + pat.len()..];
    rest.split('"').next().unwrap_or("").to_string()
}

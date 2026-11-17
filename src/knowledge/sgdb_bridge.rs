//! Sink/Source de proveniência para o lote de transplante (mock local + contrato).

use std::fs;
use std::path::Path;

use crate::core::certificate::{Evidence, EvidenceSource};
use crate::core::hv::shard_plan_from_relations;

/// Registro durável (o que iria ao neural-sgdb L4).
#[derive(Clone, Debug)]
pub struct AtomRecord {
    pub key: u128,
    pub label: String,
    pub authority: u8,
    pub provenance: &'static str,
    pub wall_ms: u64,
}

/// Lote com audit checkpoint.
#[derive(Clone, Debug, Default)]
pub struct TransplantBatch {
    pub atoms: Vec<AtomRecord>,
    pub facts_jsonl: String,
    pub shard_plan: Vec<(String, usize, usize)>,
    pub manifest_sha: String,
}

impl TransplantBatch {
    /// Carrega mock ledger gerado pelo ETL (`data/sgdb_mock_ledger.json` + facts).
    pub fn load_mock(data_dir: &Path) -> Result<Self, String> {
        let facts = fs::read_to_string(data_dir.join("facts.jsonl")).map_err(|e| e.to_string())?;
        let mut rel_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for line in facts.lines() {
            if let Some(rel) = extract(line, "relation") {
                *rel_counts.entry(rel).or_default() += 1;
            }
        }
        let mut pairs: Vec<(String, usize)> = rel_counts.into_iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let shard_plan = shard_plan_from_relations(&pairs);
        let manifest = fs::read_to_string(data_dir.join("manifest.json")).unwrap_or_default();
        let sha = manifest
            .lines()
            .find(|l| l.contains("sha256_bank"))
            .unwrap_or("")
            .to_string();
        let atoms = load_atoms_from_ledger(data_dir, &facts);
        Ok(Self {
            atoms,
            facts_jsonl: facts,
            shard_plan,
            manifest_sha: sha,
        })
    }

    /// Rebuild determinístico do plano de shards — deve casar com o manifest.
    pub fn rebuild_shards_identical(&self, expected_shard_count: usize) -> bool {
        self.shard_plan.len() == expected_shard_count
    }

    /// Evidência típica de um hit HV (authority ≤ 240).
    pub fn evidence_hv(id: u64, score: f32, authority: u8) -> Evidence {
        Evidence {
            source: EvidenceSource::Hypervector,
            id,
            score,
            authority: authority.min(240),
        }
    }
}

fn extract(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\": \"");
    let i = line.find(&pat)? + pat.len();
    let rest = &line[i..];
    Some(rest.split('"').next()?.to_string())
}

fn extract_u64(line: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{key}\": ");
    let i = line.find(&pat)? + pat.len();
    let rest = &line[i..];
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

fn parse_key_hex(s: &str) -> Option<u128> {
    let s = s.trim().trim_start_matches("0x");
    u128::from_str_radix(s, 16).ok()
}

/// Popula `AtomRecord` a partir do ledger mock (+ authority/wall dos facts).
fn load_atoms_from_ledger(data_dir: &Path, facts: &str) -> Vec<AtomRecord> {
    let ledger_path = data_dir.join("sgdb_mock_ledger.json");
    let Ok(raw) = fs::read_to_string(&ledger_path) else {
        return Vec::new();
    };
    // Índice (subject|relation|object) → (authority, wall_ms)
    let mut fact_meta: std::collections::HashMap<String, (u8, u64)> =
        std::collections::HashMap::new();
    for line in facts.lines() {
        let sub = extract(line, "subject").unwrap_or_default();
        let rel = extract(line, "relation").unwrap_or_default();
        let obj = extract(line, "object").unwrap_or_default();
        let auth = extract_u64(line, "authority").unwrap_or(100) as u8;
        let wall = extract_u64(line, "wall_ms").unwrap_or(0);
        fact_meta.insert(format!("{sub}|{rel}|{obj}"), (auth.min(240), wall));
    }
    let mut atoms = Vec::new();
    // Parse mínimo sem serde: blocos "key"/"label"
    let mut i = 0usize;
    let bytes = raw.as_str();
    while let Some(kpos) = bytes[i..].find("\"key\"") {
        let abs = i + kpos;
        let Some(key_s) = extract(&bytes[abs..], "key") else {
            i = abs + 5;
            continue;
        };
        let Some(label) = extract(&bytes[abs..], "label") else {
            i = abs + 5;
            continue;
        };
        let Some(key) = parse_key_hex(&key_s) else {
            i = abs + 5;
            continue;
        };
        let (authority, wall_ms) = fact_meta.get(&label).copied().unwrap_or((100, 0));
        atoms.push(AtomRecord {
            key,
            label,
            authority,
            provenance: "Transplanted",
            wall_ms,
        });
        i = abs + 5;
    }
    atoms
}

/// Loader de arranque: valida manifest; se faltar bank, retorna None (sem panic).
pub fn try_load_hv_bank(data_dir: &Path) -> Result<Option<crate::core::hv::HvBank>, String> {
    let bank = data_dir.join("hv/bank.bin");
    let idx = data_dir.join("hv/bank.idx");
    let manifest = data_dir.join("manifest.json");
    if !manifest.exists() {
        return Ok(None);
    }
    if !bank.exists() || !idx.exists() {
        return Ok(None);
    }
    Ok(Some(crate::core::hv::HvBank::load(&bank, &idx)?))
}

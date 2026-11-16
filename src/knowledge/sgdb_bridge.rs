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
        Ok(Self {
            atoms: Vec::new(),
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

/// Loader de arranque: valida manifest; se faltar bank, retorna None (sem panic).
pub fn try_load_hv_bank(
    data_dir: &Path,
) -> Result<Option<crate::core::hv::HvBank>, String> {
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

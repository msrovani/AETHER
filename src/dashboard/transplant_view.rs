//! Visão de doação / ETL / transplante HV para o hub.
//!
//! Lê `data/manifest.json`, `target/aether_transplant_hv.json`,
//! `target/aether_e3.json` e `target/aether_transplant_uso.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Shard FHRR no manifesto.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ShardInfo {
    #[serde(default)]
    pub relation: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub count: u64,
}

/// Origem (doação) + artefato ETL.
#[derive(Clone, Debug, Default)]
pub struct DonationOrigin {
    pub donor: String,
    pub revision: String,
    pub dim_in: u32,
    pub dim_hv: u32,
    pub words: u32,
    pub seed: u64,
    pub count: u64,
    pub facts: u64,
    pub whitened: bool,
    pub sha256_bank: String,
    pub sha256_embeds: String,
    pub shards: Vec<ShardInfo>,
    pub conflicts_planted: u64,
    pub conflicts_detected: u64,
    pub authority_max: u8,
    pub created: String,
    pub claim: String,
}

/// Destino no organismo (leitor T1/T2).
#[derive(Clone, Debug)]
pub struct TransplantDest {
    pub bank_path: String,
    pub idx_path: String,
    pub hv_api: &'static str,
    pub sgdb_bridge: &'static str,
    pub certificate: &'static str,
    pub t0_untouched: bool,
    pub authority_cap: u8,
}

impl Default for TransplantDest {
    fn default() -> Self {
        Self {
            bank_path: "data/hv/bank.bin".into(),
            idx_path: "data/hv/bank.idx".into(),
            hv_api: "src/core/hv.rs · HvBank::top_k_into",
            sgdb_bridge: "src/knowledge/sgdb_bridge.rs",
            certificate: "Evidence(Hypervector) · decide_with_evidence",
            t0_untouched: true,
            authority_cap: 240,
        }
    }
}

/// Gates numéricos do probe INTAKE.
#[derive(Clone, Debug, Default)]
pub struct TransplantGates {
    pub atoms: u64,
    pub spearman: f64,
    pub jl_distortion: f64,
    pub recall_at_10: f64,
    pub latency_ms: f64,
    pub allocs_query: u64,
    pub conflict_rate: f64,
    pub conflicts_detected: u64,
    pub conflicts_planted: u64,
    pub determinism: bool,
    pub rebuild_ok: bool,
    pub pass: bool,
    pub path: PathBuf,
}

/// Estatísticas E3 (geometria / FHRR / analogia).
#[derive(Clone, Debug, Default)]
pub struct E3Stats {
    pub spearman: f64,
    pub rel_recall: f64,
    pub analogy_r1: f64,
    pub analogy_r5: f64,
    pub false_confident: f64,
    pub verifier_ms: f64,
    pub facts: u64,
    pub present: bool,
}

/// Um cenário de usabilidade (U1…U6).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct UsoCase {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub pass: bool,
    #[serde(default)]
    pub score: String,
}

/// Lote de testes de uso.
#[derive(Clone, Debug, Default)]
pub struct UsoReport {
    pub cases: Vec<UsoCase>,
    pub pass_all: bool,
    pub present: bool,
}

/// Visão agregada para a UI do hub.
#[derive(Clone, Debug, Default)]
pub struct TransplantView {
    pub origin: Option<DonationOrigin>,
    pub dest: TransplantDest,
    pub gates: Option<TransplantGates>,
    pub e3: E3Stats,
    pub uso: UsoReport,
    pub load_notes: Vec<String>,
}

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    donor: String,
    #[serde(default)]
    revision: String,
    #[serde(default)]
    dim_in: u32,
    #[serde(default)]
    dim_hv: u32,
    #[serde(default)]
    words: u32,
    #[serde(default)]
    seed: u64,
    #[serde(default)]
    count: u64,
    #[serde(default)]
    facts: u64,
    #[serde(default)]
    whitened: bool,
    #[serde(default)]
    sha256_bank: String,
    #[serde(default)]
    sha256_embeds: String,
    #[serde(default)]
    shards: Vec<ShardInfo>,
    #[serde(default)]
    conflicts_planted: u64,
    #[serde(default)]
    conflicts_detected: u64,
    #[serde(default)]
    authority_max_transplant: u8,
    #[serde(default)]
    created: String,
    #[serde(default)]
    claim: String,
}

#[derive(Deserialize)]
struct RawGates {
    #[serde(default)]
    atoms: u64,
    #[serde(default)]
    spearman: f64,
    #[serde(default)]
    jl_distortion: f64,
    #[serde(default)]
    recall_at_10: f64,
    #[serde(default)]
    latency_ms: f64,
    #[serde(default)]
    allocs_query: u64,
    #[serde(default)]
    conflict_rate: f64,
    #[serde(default)]
    conflicts_detected: u64,
    #[serde(default)]
    conflicts_planted: u64,
    #[serde(default)]
    determinism: bool,
    #[serde(default)]
    rebuild_ok: bool,
    #[serde(default)]
    pass: bool,
}

#[derive(Deserialize)]
struct RawE3 {
    #[serde(default)]
    spearman: f64,
    #[serde(default)]
    rel_recall_clean: f64,
    #[serde(default)]
    analogy_r1: f64,
    #[serde(default)]
    analogy_r5: f64,
    #[serde(default)]
    false_confident: f64,
    #[serde(default)]
    verifier_ms: f64,
    #[serde(default)]
    facts: u64,
}

#[derive(Deserialize)]
struct RawUso {
    #[serde(default)]
    cases: Vec<UsoCase>,
    #[serde(default)]
    pass_all: bool,
}

fn workspace_root() -> PathBuf {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(m);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Paths padrão dos artefatos.
pub fn transplant_paths() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = workspace_root();
    (
        prefer_round_manifest(&root),
        prefer_round_file(
            &root,
            "aether_transplant_hv.json",
            "target/aether_transplant_hv.json",
        ),
        prefer_round_file(&root, "aether_e3.json", "target/aether_e3.json"),
        prefer_round_file(
            &root,
            "aether_transplant_uso.json",
            "target/aether_transplant_uso.json",
        ),
    )
}

/// Prefere `data/rounds/20261117_b1`, depois `bg0`, senão `data/manifest.json`.
fn prefer_round_manifest(root: &Path) -> PathBuf {
    for round in ["20261117_b1", "20261117_bg0"] {
        let p = root.join("data/rounds").join(round).join("manifest.json");
        if p.exists() {
            return p;
        }
    }
    root.join("data/manifest.json")
}

fn prefer_round_file(root: &Path, name: &str, fallback: &str) -> PathBuf {
    for round in ["20261117_b1", "20261117_bg0"] {
        let p = root.join("data/rounds").join(round).join(name);
        if p.exists() {
            return p;
        }
    }
    root.join(fallback)
}

/// Carrega a visão de doação/transplante (faltas viram notas, sem panic).
pub fn load_transplant_view() -> TransplantView {
    let (manifest_p, gates_p, e3_p, uso_p) = transplant_paths();
    let mut view = TransplantView {
        dest: TransplantDest::default(),
        ..Default::default()
    };
    view.load_notes
        .push(format!("rodada: {}", manifest_p.display()));

    match load_manifest(&manifest_p) {
        Ok(o) => view.origin = Some(o),
        Err(e) => view.load_notes.push(format!("manifest: {e}")),
    }
    match load_gates(&gates_p) {
        Ok(g) => view.gates = Some(g),
        Err(e) => view.load_notes.push(format!("gates: {e}")),
    }
    match load_e3(&e3_p) {
        Ok(e) => view.e3 = e,
        Err(e) => view.load_notes.push(format!("e3: {e}")),
    }
    match load_uso(&uso_p) {
        Ok(u) => view.uso = u,
        Err(e) => view.load_notes.push(format!("uso: {e}")),
    }
    view
}

fn load_manifest(path: &Path) -> Result<DonationOrigin, String> {
    let raw: RawManifest = read_json(path)?;
    Ok(DonationOrigin {
        donor: raw.donor,
        revision: raw.revision,
        dim_in: raw.dim_in,
        dim_hv: raw.dim_hv,
        words: raw.words,
        seed: raw.seed,
        count: raw.count,
        facts: raw.facts,
        whitened: raw.whitened,
        sha256_bank: raw.sha256_bank,
        sha256_embeds: raw.sha256_embeds,
        shards: raw.shards,
        conflicts_planted: raw.conflicts_planted,
        conflicts_detected: raw.conflicts_detected,
        authority_max: if raw.authority_max_transplant == 0 {
            240
        } else {
            raw.authority_max_transplant
        },
        created: raw.created,
        claim: raw.claim,
    })
}

fn load_gates(path: &Path) -> Result<TransplantGates, String> {
    let raw: RawGates = read_json(path)?;
    Ok(TransplantGates {
        atoms: raw.atoms,
        spearman: raw.spearman,
        jl_distortion: raw.jl_distortion,
        recall_at_10: raw.recall_at_10,
        latency_ms: raw.latency_ms,
        allocs_query: raw.allocs_query,
        conflict_rate: raw.conflict_rate,
        conflicts_detected: raw.conflicts_detected,
        conflicts_planted: raw.conflicts_planted,
        determinism: raw.determinism,
        rebuild_ok: raw.rebuild_ok,
        pass: raw.pass,
        path: path.to_path_buf(),
    })
}

fn load_e3(path: &Path) -> Result<E3Stats, String> {
    let raw: RawE3 = read_json(path)?;
    Ok(E3Stats {
        spearman: raw.spearman,
        rel_recall: raw.rel_recall_clean,
        analogy_r1: raw.analogy_r1,
        analogy_r5: raw.analogy_r5,
        false_confident: raw.false_confident,
        verifier_ms: raw.verifier_ms,
        facts: raw.facts,
        present: true,
    })
}

fn load_uso(path: &Path) -> Result<UsoReport, String> {
    let raw: RawUso = read_json(path)?;
    Ok(UsoReport {
        cases: raw.cases,
        pass_all: raw.pass_all,
        present: true,
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Err(format!("ausente ({})", path.display()));
    }
    let s = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// Texto longo para a coluna Detalhe (origem).
pub fn origin_detail(o: &DonationOrigin) -> String {
    let mut shards = String::new();
    for s in &o.shards {
        shards.push_str(&format!(
            "  · {}  offset={}  n={}\n",
            s.relation, s.offset, s.count
        ));
    }
    format!(
        "DOAÇÃO (origem / ETL)\n\
         donor: {}\n\
         revision: {}\n\
         claim: {}\n\
         criado: {}\n\
         \n\
         dim_in={} → dim_hv={} ({} × u64)\n\
         seed={}  whitened={}\n\
         átomos={}  fatos={}\n\
         conflitos plantados/detectados={}/{}\n\
         authority máx. transplantado={}\n\
         \n\
         sha256_bank:\n  {}\n\
         sha256_embeds:\n  {}\n\
         \n\
         shards FHRR (≤48/relação):\n{}",
        o.donor,
        o.revision,
        o.claim,
        o.created,
        o.dim_in,
        o.dim_hv,
        o.words,
        o.seed,
        o.whitened,
        o.count,
        o.facts,
        o.conflicts_planted,
        o.conflicts_detected,
        o.authority_max,
        o.sha256_bank,
        o.sha256_embeds,
        shards
    )
}

/// Texto longo para a coluna Detalhe (destino).
pub fn dest_detail(d: &TransplantDest) -> String {
    format!(
        "TRANSPLANTE (destino AETHER)\n\
         bank: {}\n\
         idx:  {}\n\
         API:  {}\n\
         SGDB: {}\n\
         Cert: {}\n\
         \n\
         T0 / step_into intocado: {}\n\
         authority cap: ≤ {}\n\
         HITL exclusivo: 255\n\
         Camada: T1 (~1 ms) / T2 (SGDB 10–50 ms)\n\
         Formato: AETHV1 · 1,28 KB/átomo\n\
         Playbook: docs/DOACAO_PLAYBOOK.md",
        d.bank_path,
        d.idx_path,
        d.hv_api,
        d.sgdb_bridge,
        d.certificate,
        d.t0_untouched,
        d.authority_cap
    )
}

/// Texto longo dos gates INTAKE.
pub fn gates_detail(g: &TransplantGates) -> String {
    format!(
        "GATES INTAKE_TRANSPLANT\n\
         pass={}\n\
         átomos={}\n\
         Spearman={:.4}  (alvo ≥ 0,95)\n\
         Recall@10={:.4}  (alvo ≥ 0,90)\n\
         JL dist={:.4}  (alvo ≤ 0,12)\n\
         latência={:.4} ms  (alvo ≤ 1,5)\n\
         allocs={}  (alvo 0)\n\
         conflitos={}/{}  rate={:.3}\n\
         determinismo={}  rebuild={}\n\
         \n\
         artefato: {}",
        g.pass,
        g.atoms,
        g.spearman,
        g.recall_at_10,
        g.jl_distortion,
        g.latency_ms,
        g.allocs_query,
        g.conflicts_detected,
        g.conflicts_planted,
        g.conflict_rate,
        g.determinism,
        g.rebuild_ok,
        g.path.display()
    )
}

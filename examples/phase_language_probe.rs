//! E1 — Phase-Language Head (PLH) sharded, `D = 160`.
//!
//! A lei medida no E1b (`M_max ≈ 0,4·D` ⇒ 64 fatos em D=160) proíbe empilhar
//! muito numa superposição plana. O PLH resolve com **shards por relação**:
//! `24 shards × 41–42 fatos = 1.000 fatos` em D=160, cada shard bem abaixo do
//! teto (≤48), com working set total < 500 KB (L2-residente).
//!
//! # Binding não-comutativo
//!
//! O `FhrrStore` liga `e ⊗ r ⊗ v` por **soma de fases** (comutativa). Para
//! separar `A → B` de `B → A`, o **valor** é permutado ciclicamente por papel:
//!
//! ```text
//! fato = (subj ⊕ ANCHOR_SUBJ, rel ⊕ ANCHOR_VERB, ρ¹(obj ⊕ ANCHOR_OBJ))
//! ```
//!
//! `ρ¹` desloca as fases uma posição; trocar sujeito e objeto muda o vetor
//! armazenado, então as duas ordens ocupam pontos diferentes do espaço.
//!
//! # Zero alocação no caminho de consulta
//!
//! As pistas `(subj ⊕ âncora, rel ⊕ âncora)` são escritas em buffers
//! pré-alocados ([`add_into`]) e a consulta usa [`FhrrStore::query_into`] com
//! `FhrrScratch` criado uma vez. O alocador contador fica armado durante as
//! 1.000 consultas e deve marcar **0**.
//!
//! # O que **não** foi feito (e por quê)
//!
//! O spec pedia **ressonador ACF** entre fatores. Com âncoras de papel e a
//! consulta `(sujeito, relação)` fornecida, a fatoração é **um único unbinding**
//! — não há produto de fatores desconhecidos para iterar. O ressonador resolve
//! o caso em que *nenhum* fator é conhecido; esse caso é **E1-2** e não foi
//! simulado aqui para não apresentar um número que não mede o que promete.
//!
//! ```text
//! cargo run --release --example phase_language_probe
//! ```
//!
//! Saída: gates + `target/aether_e1_plh.json`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

use aether_engine::core::fhrr::{FhrrHit, FhrrScratch, FhrrStore};

// ---------------------------------------------------------------------------
// Alocador contador
// ---------------------------------------------------------------------------

static HOT_ALLOCS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// ---------------------------------------------------------------------------
// Parâmetros
// ---------------------------------------------------------------------------

const D: usize = 160;
const TRACES: usize = 6;
const SHARDS: usize = 24;
const FACTS: usize = 1_000;
const VOCAB: usize = 64;
const MAX_PER_SHARD: usize = 48;

// ---------------------------------------------------------------------------
// Fase: soma sem alocação, permutação, átomos
// ---------------------------------------------------------------------------

fn atom(seed: u64, n: usize) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..n).map(|_| rng.gen::<f32>() * TAU).collect()
}

/// `out = a ⊕ b` (soma de fases módulo 2π), sem alocar.
fn add_into(a: &[f32], b: &[f32], out: &mut [f32]) {
    for ((o, x), y) in out.iter_mut().zip(a.iter()).zip(b.iter()) {
        *o = (x + y).rem_euclid(TAU);
    }
}

/// `ρ^k`: permutação cíclica das fases (quebra comutatividade).
fn permute(v: &[f32], k: usize) -> Vec<f32> {
    let n = v.len();
    (0..n).map(|i| v[(i + n - (k % n)) % n]).collect()
}

/// SNR de uma consulta: `(melhor − média dos outros) / desvio`, sem alocar.
fn snr_from_sims(sims: &[f32]) -> f32 {
    if sims.len() < 2 {
        return f32::INFINITY;
    }
    let best = sims.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let (mut sum, mut n, mut skipped) = (0.0f32, 0usize, false);
    for &s in sims {
        if !skipped && (s - best).abs() < 1e-6 {
            skipped = true;
            continue;
        }
        sum += s;
        n += 1;
    }
    if n == 0 {
        return f32::INFINITY;
    }
    let mean = sum / n as f32;

    let (mut var, mut skipped2) = (0.0f32, false);
    for &s in sims {
        if !skipped2 && (s - best).abs() < 1e-6 {
            skipped2 = true;
            continue;
        }
        var += (s - mean) * (s - mean);
    }
    let sd = (var / n as f32).sqrt().max(1e-9);
    (best - mean) / sd
}

// ---------------------------------------------------------------------------
// PLH
// ---------------------------------------------------------------------------

struct Shard {
    store: FhrrStore,
}

struct Plh {
    anchor_subj: Vec<f32>,
    anchor_verb: Vec<f32>,
    anchor_obj: Vec<f32>,
    subjects: Vec<Vec<f32>>,
    relations: Vec<Vec<f32>>,
    shards: Vec<Shard>,
}

impl Plh {
    fn new(seed: u64) -> Self {
        let anchor_subj = atom(seed ^ 0xA1, D);
        let anchor_verb = atom(seed ^ 0xA2, D);
        let anchor_obj = atom(seed ^ 0xA3, D);
        let subjects: Vec<Vec<f32>> = (0..VOCAB)
            .map(|i| atom(seed ^ (0x1000 + i as u64), D))
            .collect();
        let relations: Vec<Vec<f32>> = (0..SHARDS)
            .map(|i| atom(seed ^ (0x2000 + i as u64), D))
            .collect();
        let shards = (0..SHARDS)
            .map(|_| Shard {
                store: FhrrStore::with_traces(D, TRACES).expect("store"),
            })
            .collect();

        Self {
            anchor_subj,
            anchor_verb,
            anchor_obj,
            subjects,
            relations,
            shards,
        }
    }

    /// Grava `(subj, rel, obj)` no shard da relação. Devolve `(shard, posição)`.
    fn insert(&mut self, subj: &[f32], rel_idx: usize, obj: &[f32]) -> (usize, usize) {
        let mut entity = vec![0.0f32; D];
        let mut relation = vec![0.0f32; D];
        add_into(subj, &self.anchor_subj, &mut entity);
        add_into(&self.relations[rel_idx], &self.anchor_verb, &mut relation);
        let mut obj_role = vec![0.0f32; D];
        add_into(obj, &self.anchor_obj, &mut obj_role);
        let value = permute(&obj_role, 1);

        let shard = rel_idx % SHARDS;
        let pos = self.shards[shard].store.len();
        self.shards[shard]
            .store
            .add_fact(&entity, &relation, &value)
            .expect("add_fact");
        (shard, pos)
    }

    /// Escreve as pistas nas buffers pré-alocadas (zero alocação).
    fn prepare_cue(&self, subj: &[f32], rel_idx: usize, entity: &mut [f32], relation: &mut [f32]) {
        add_into(subj, &self.anchor_subj, entity);
        add_into(&self.relations[rel_idx], &self.anchor_verb, relation);
    }

    fn query_prepared(
        &self,
        rel_idx: usize,
        entity: &[f32],
        relation: &[f32],
        mask: &[bool],
        sc: &mut FhrrScratch,
    ) -> FhrrHit {
        let shard = rel_idx % SHARDS;
        self.shards[shard]
            .store
            .query_into(entity, relation, mask, sc)
            .expect("query")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seed = 0xE1_0001u64;
    let mut plh = Plh::new(seed);
    let mask = vec![true; D];

    // --- população: 1000 fatos, relações cicladas (≤42 por shard) ---
    // A memória endereça por papel (e⊗r): dentro de um shard, (sujeito, relação)
    // precisa ser ÚNICO, senão duas frases colidem no mesmo papel. Aqui o sujeito
    // é `i / SHARDS`, único por shard (0..41 < VOCAB).
    let mut facts: Vec<(usize, usize, usize)> = Vec::with_capacity(FACTS);
    for i in 0..FACTS {
        let rel = i % SHARDS;
        let subj = i / SHARDS;
        let obj = atom(seed ^ (0x9000 + i as u64), D);
        let subject = plh.subjects[subj].clone();
        plh.insert(&subject, rel, &obj);
        facts.push((subj, rel, i));
    }
    let total: usize = plh.shards.iter().map(|s| s.store.len()).sum();
    let max_shard = plh
        .shards
        .iter()
        .map(|s| s.store.len())
        .max()
        .unwrap_or(0);

    // buffers pré-alocados do caminho quente
    let mut entity = vec![0.0f32; D];
    let mut relation = vec![0.0f32; D];
    let mut sc = plh.shards[0].store.scratch(MAX_PER_SHARD);

    // --- Gate 1: recall@1 sharded (zero alocação) ---
    let mut hits = 0usize;
    let mut snr_sum = 0.0f32;
    let mut snr_n = 0usize;

    let t0 = Instant::now();
    let before = HOT_ALLOCS.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    for &(subj, rel, obj_id) in &facts {
        plh.prepare_cue(&plh.subjects[subj], rel, &mut entity, &mut relation);
        let hit = plh.query_prepared(rel, &entity, &relation, &mask, &mut sc);
        let expected = obj_id / SHARDS; // posição dentro do shard
        if hit.value_index == expected {
            hits += 1;
        }
        let s = snr_from_sims(sc.sims());
        if s.is_finite() {
            snr_sum += s;
            snr_n += 1;
        }
    }
    ARMED.with(|a| a.set(false));
    let hot_allocs = HOT_ALLOCS.load(Ordering::Relaxed) - before;
    let elapsed = t0.elapsed();
    let recall = hits as f64 / FACTS as f64;
    let latency_us = elapsed.as_secs_f64() * 1e6 / FACTS as f64;
    let snr = if snr_n > 0 { snr_sum / snr_n as f32 } else { 0.0 };

    // --- baseline: superposição plana de 1000 fatos em D=160 ---
    let mut flat = FhrrStore::with_traces(D, TRACES).expect("flat");
    for &(subj, rel, obj_id) in &facts {
        let obj = atom(seed ^ (0x9000 + obj_id as u64), D);
        let mut entity = vec![0.0f32; D];
        let mut relation = vec![0.0f32; D];
        add_into(&plh.subjects[subj], &plh.anchor_subj, &mut entity);
        add_into(&plh.relations[rel], &plh.anchor_verb, &mut relation);
        let mut obj_role = vec![0.0f32; D];
        add_into(&obj, &plh.anchor_obj, &mut obj_role);
        let value = permute(&obj_role, 1);
        flat.add_fact(&entity, &relation, &value).expect("flat add");
    }
    let mut flat_sc = flat.scratch(FACTS);
    let mut flat_hits = 0usize;
    for (i, &(subj, rel, _)) in facts.iter().enumerate() {
        let mut entity = vec![0.0f32; D];
        let mut relation = vec![0.0f32; D];
        add_into(&plh.subjects[subj], &plh.anchor_subj, &mut entity);
        add_into(&plh.relations[rel], &plh.anchor_verb, &mut relation);
        let hit = flat
            .query_into(&entity, &relation, &mask, &mut flat_sc)
            .expect("flat q");
        if hit.value_index == i {
            flat_hits += 1;
        }
    }
    let flat_recall = flat_hits as f64 / FACTS as f64;

    // --- Gate 2: não-comutatividade estrita ---
    let mut nc = Plh::new(seed ^ 0x0C);
    let a = atom(seed ^ 0xAA, D);
    let b = atom(seed ^ 0xBB, D);
    let (_, pos_ab) = nc.insert(&a, 0, &b);
    let (_, pos_ba) = nc.insert(&b, 0, &a);
    let mut nc_sc = nc.shards[0].store.scratch(MAX_PER_SHARD);
    let mut e1 = vec![0.0f32; D];
    let mut r1 = vec![0.0f32; D];
    nc.prepare_cue(&a, 0, &mut e1, &mut r1);
    let ab = nc.query_prepared(0, &e1, &r1, &mask, &mut nc_sc);
    nc.prepare_cue(&b, 0, &mut e1, &mut r1);
    let ba = nc.query_prepared(0, &e1, &r1, &mask, &mut nc_sc);
    let nc_ok = ab.value_index == pos_ab && ba.value_index == pos_ba && pos_ab != pos_ba;

    // --- Gate 4: abstenção (in-vocab vs fora do vocabulário) ---
    let mut sc2 = plh.shards[0].store.scratch(MAX_PER_SHARD);
    let mut entity = vec![0.0f32; D];
    let mut relation = vec![0.0f32; D];

    let mut role_in = Vec::with_capacity(200);
    for k in 0..200 {
        // sujeitos 0..41 são os realmente armazenados (FACTS/SHARDS por shard)
        let subj = k % (FACTS / SHARDS);
        let rel = k % SHARDS;
        plh.prepare_cue(&plh.subjects[subj], rel, &mut entity, &mut relation);
        let hit = plh.query_prepared(rel, &entity, &relation, &mask, &mut sc2);
        role_in.push(hit.role_similarity);
    }

    let oov_entity = atom(seed ^ 0xDEAD, D);
    let oov_relation = atom(seed ^ 0xBEEF, D);
    let shard0 = &plh.shards[0].store;
    let mut role_oov = Vec::with_capacity(200);
    for _ in 0..200 {
        let hit = shard0
            .query_into(&oov_entity, &oov_relation, &mask, &mut sc2)
            .expect("oov");
        role_oov.push(hit.role_similarity);
    }

    let threshold = 0.5f32;
    let abstain_oov =
        role_oov.iter().filter(|s| **s < threshold).count() as f64 / role_oov.len() as f64;
    let false_abstain =
        role_in.iter().filter(|s| **s < threshold).count() as f64 / role_in.len() as f64;

    // --- relatório ---
    println!();
    println!("AETHER · E1 — Phase-Language Head sharded (D={D})");
    println!("=====================================================");
    println!(
        "shards={SHARDS} fatos={total} max/shard={max_shard} (teto {MAX_PER_SHARD}) traces={TRACES}"
    );
    println!(
        "working set ≈ {:.1} KB",
        (D * TRACES * 8 * SHARDS) as f64 / 1024.0
    );
    println!();
    println!("  Gate 1  recall@1 sharded ({FACTS} fatos) : {:.4}", recall);
    println!("          baseline plano (D=160)           : {:.4}", flat_recall);
    println!("  Gate 2  não-comutatividade A→B vs B→A    : {}", pass(nc_ok));
    println!("  Gate 3  SNR                              : {snr:.2}σ");
    println!("          latência/consulta                : {latency_us:.2} µs");
    println!("          alocações no hot-path            : {hot_allocs}");
    println!(
        "  Gate 4  abstenção OOV                    : {:.1}% (limiar {threshold:.2})",
        100.0 * abstain_oov
    );
    println!(
        "          falso-abster em in-vocab         : {:.1}%",
        100.0 * false_abstain
    );
    println!();

    let g1 = recall >= 0.99;
    let g2 = nc_ok;
    let g3 = snr > 5.0 && latency_us < 15.0 && hot_allocs == 0;
    let g4 = abstain_oov >= 0.95 && false_abstain <= 0.05;
    println!("  G1 sharded recall ≥ 99%      : {}", pass(g1));
    println!("  G2 não-comutatividade 100%   : {}", pass(g2));
    println!("  G3 SNR > 5σ · <15 µs · 0 B   : {}", pass(g3));
    println!("  G4 abstenção ≥ 95% · FP ≤ 5% : {}", pass(g4));
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e1_plh.json";
    let json = format!(
        "{{\n  \"dim\": {D}, \"traces\": {TRACES}, \"shards\": {SHARDS},\n  \"facts\": {total}, \"max_per_shard\": {max_shard},\n  \"recall_sharded\": {recall:.4},\n  \"recall_flat\": {flat_recall:.4},\n  \"non_commutative\": {nc_ok},\n  \"snr_sigma\": {snr:.3},\n  \"latency_us\": {latency_us:.3},\n  \"hot_allocs\": {hot_allocs},\n  \"abstain_oov\": {abstain_oov:.4},\n  \"false_abstain\": {false_abstain:.4}\n}}\n"
    );
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

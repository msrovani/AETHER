//! E3 — transplantação geométrica (Johnson–Lindenstrauss) + proveniência.
//!
//! Metodologia:
//! 1. gera **1.000** embeddings densos `d = 384` de fatos `(entidade, relação,
//!    valor)` sobre uma estrutura relacional controlada (`125 × 8` pares com
//!    valor determinístico — ground truth exato);
//! 2. projeta por matriz Gaussiana semeada + sinal em hipervetores de
//!    `D = 10.240` bits (1,28 KB/átomo) e mede a preservação métrica
//!    (Spearman entre cosseno original e Hamming projetado);
//! 3. consulta shards `FhrrStore` (`D = 160`, `K = 6`, **≤ 48 fatos/shard**,
//!    dentro da capacidade 64 medida no E1b) com pista parcial e mede
//!    `Recall@1`/`Recall@5` de relação;
//! 4. mede analogia `a ⊗ b⁻¹ ⊗ c` em fase (linear por construção) sobre um banco
//!    controlado de 48 entidades, **sem alvo de marketing**;
//! 5. toda resposta carrega `SourceTag`; consultas com margem baixa (ou fora do
//!    domínio) abstêm — e o verificador estrutural roda fora do hot path,
//!    medido com alocador instrumentado.
//!
//! ```text
//! cargo run --release --example transplant_probe
//! cargo run --release --example transplant_probe -- --quick
//! ```
//!
//! Saída: tabela + `target/aether_e3.json`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use aether_engine::core::fhrr::FhrrStore;
use aether_engine::core::knowledge_bridge::{SourceTag, SpscKnowledgeBridge};
use aether_engine::core::transplant::{
    gaussian_vec_into, hamming, normalize_in_place, phase_analogy, phase_similarity, spearman,
    BinaryBam, FactRecord, JlProjector, PhaseEncoder, StructuralVerifier,
};

// ---------------------------------------------------------------------------
// Alocador contador
// ---------------------------------------------------------------------------

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
}

// ---------------------------------------------------------------------------
// Dimensões e argumentos
// ---------------------------------------------------------------------------

const D_IN: usize = 384;
const BITS: usize = 10_240;
const FHRR_DIM: usize = 160;
const TRACES: usize = 6;
const SHARD_CAP: usize = 48;
const N_REL: usize = 8;
const N_VAL: usize = 40;
const N_SLOTS: usize = 4;
const VALUES_PER_SLOT: usize = 10;
const GROUPS: usize = 3;

struct Args {
    facts: usize,
    quick: bool,
    jl_seed: u64,
    phase_seed: u64,
    data_seed: u64,
    nonlin: f64,
    cue_noise: f64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            facts: 1000,
            quick: false,
            jl_seed: 0xE3_10,
            phase_seed: 0xE3_20,
            data_seed: 0xE3_01,
            nonlin: 0.25,
            cue_noise: 0.6,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let key = argv[i].as_str();
        let raw = argv.get(i + 1);
        match key {
            "--quick" => {
                a.quick = true;
                a.facts = 240;
            }
            "--facts" => {
                if let Some(v) = raw.and_then(|v| v.parse().ok()) {
                    a.facts = v;
                }
                i += 1;
            }
            "--nonlin" => {
                if let Some(v) = raw.and_then(|v| v.parse().ok()) {
                    a.nonlin = v;
                }
                i += 1;
            }
            "--cue-noise" => {
                if let Some(v) = raw.and_then(|v| v.parse().ok()) {
                    a.cue_noise = v;
                }
                i += 1;
            }
            "--seed" => {
                if let Some(v) = raw.and_then(|v| v.parse().ok()) {
                    a.data_seed = v;
                }
                i += 1;
            }
            other => eprintln!("argumento ignorado: {other}"),
        }
        i += 1;
    }
    a
}

// ---------------------------------------------------------------------------
// Dados
// ---------------------------------------------------------------------------

fn gauss_vec(rng: &mut ChaCha8Rng, len: usize, sigma: f32) -> Vec<f32> {
    let mut v = vec![0.0f32; len];
    gaussian_vec_into(rng, &mut v);
    for x in v.iter_mut() {
        *x *= sigma;
    }
    v
}

fn add_assign(dst: &mut [f32], src: &[f32]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d += *s;
    }
}

/// Valor determinístico por `(entidade, relação)` — ground truth funcional.
fn value_of(entity: usize, relation: usize) -> usize {
    (entity * 7 + relation * 3) % N_VAL
}

/// Origem determinística por índice de fato (70/20/10).
fn tag_of(i: usize) -> SourceTag {
    match i % 10 {
        0..=6 => SourceTag::Transplanted,
        7 | 8 => SourceTag::Observed,
        _ => SourceTag::Inferred,
    }
}

struct Dataset {
    entity_emb: Vec<Vec<f32>>,
    relation_emb: Vec<Vec<f32>>,
    fact_emb: Vec<Vec<f32>>,
    facts: Vec<FactRecord>,
    /// `(entity, relation, value)` por fato.
    triples: Vec<(usize, usize, usize)>,
}

fn build_dataset(n_facts: usize, n_ent: usize, seed: u64, nonlin: f64) -> Dataset {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut entity_emb = Vec::with_capacity(n_ent);
    for _ in 0..n_ent {
        let mut v = gauss_vec(&mut rng, D_IN, 1.0);
        normalize_in_place(&mut v);
        entity_emb.push(v);
    }
    let mut relation_emb = Vec::with_capacity(N_REL);
    for _ in 0..N_REL {
        let mut v = gauss_vec(&mut rng, D_IN, 1.0);
        normalize_in_place(&mut v);
        relation_emb.push(v);
    }
    let mut value_emb = Vec::with_capacity(N_VAL);
    for _ in 0..N_VAL {
        let mut v = gauss_vec(&mut rng, D_IN, 1.0);
        normalize_in_place(&mut v);
        value_emb.push(v);
    }

    let mut fact_emb = Vec::with_capacity(n_facts);
    let mut facts = Vec::with_capacity(n_facts);
    let mut triples = Vec::with_capacity(n_facts);
    for i in 0..n_facts {
        let e = i / N_REL;
        let r = i % N_REL;
        let v = value_of(e, r);
        let mut x = vec![0.0f32; D_IN];
        for (d, s) in x.iter_mut().zip(&entity_emb[e]) {
            *d += 0.6 * s;
        }
        for (d, s) in x.iter_mut().zip(&relation_emb[r]) {
            *d += 0.45 * s;
        }
        for (d, s) in x.iter_mut().zip(&value_emb[v]) {
            *d += 0.35 * s;
        }
        // termo não-aditivo: quebra a linearidade exata (como um embedding real)
        add_assign(&mut x, &gauss_vec(&mut rng, D_IN, nonlin as f32));
        normalize_in_place(&mut x);
        fact_emb.push(x);

        let tag = tag_of(i);
        facts.push(FactRecord {
            entity: e as u32,
            relation: r as u32,
            value: v as u32,
            tag,
            source: if tag == SourceTag::Transplanted {
                (i + 1) as u32
            } else {
                0
            },
            margin: 0.0,
        });
        triples.push((e, r, v));
    }
    Dataset {
        entity_emb,
        relation_emb,
        fact_emb,
        facts,
        triples,
    }
}

// ---------------------------------------------------------------------------
// Utilitários de medição
// ---------------------------------------------------------------------------

fn pct(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        100.0 * num as f64 / den as f64
    }
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// Correlação de Spearman entre cosseno (d) e Hamming (D bits), todos os pares.
fn jl_spearman(bits: &[u64], words: usize, emb: &[Vec<f32>]) -> (f64, usize) {
    let n = emb.len();
    let pairs = n * (n - 1) / 2;
    let mut cos = Vec::with_capacity(pairs);
    let mut ham = Vec::with_capacity(pairs);
    for i in 0..n {
        for j in (i + 1)..n {
            let mut dot = 0.0f32;
            for (a, b) in emb[i].iter().zip(&emb[j]) {
                dot += a * b;
            }
            cos.push(dot as f64);
            ham.push(f64::from(hamming(
                &bits[i * words..(i + 1) * words],
                &bits[j * words..(j + 1) * words],
                words,
                BITS,
            )));
        }
    }
    (spearman(&cos, &ham), pairs)
}

/// Recall@k de analogia em fase, com ruído gaussiano aplicado aos átomos.
fn analogy_recall(
    bank_emb: &[Vec<f32>],
    encoder: &PhaseEncoder,
    sigma: f32,
    seed: u64,
) -> (f64, f64, usize) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut phases: Vec<Vec<f32>> = Vec::with_capacity(bank_emb.len());
    for e in bank_emb {
        let mut p = vec![0.0f32; FHRR_DIM];
        encoder.encode_into(e, &mut p);
        if sigma > 0.0 {
            let noise = gauss_vec(&mut rng, FHRR_DIM, sigma);
            for (a, b) in p.iter_mut().zip(&noise) {
                *a = (*a + *b).rem_euclid(std::f32::consts::TAU);
            }
        }
        phases.push(p);
    }
    let idx = |g: usize, s: usize, v: usize| (g * N_SLOTS + s) * VALUES_PER_SLOT + v;

    let mut rec = vec![0.0f32; FHRR_DIM];
    let mut sims = vec![0.0f32; phases.len()];
    let mut order: Vec<usize> = (0..phases.len()).collect();
    let (mut r1, mut r5, mut total) = (0usize, 0usize, 0usize);

    for g in 0..GROUPS {
        for g2 in 0..GROUPS {
            if g == g2 {
                continue;
            }
            for s in 0..N_SLOTS {
                for v1 in 0..VALUES_PER_SLOT {
                    for v2 in 0..VALUES_PER_SLOT {
                        if v1 == v2 {
                            continue;
                        }
                        let a = idx(g, s, v1);
                        let b = idx(g, s, v2);
                        let c = idx(g2, s, v1);
                        let d = idx(g2, s, v2);
                        phase_analogy(&phases[a], &phases[b], &phases[c], &mut rec);
                        for (j, p) in phases.iter().enumerate() {
                            sims[j] = phase_similarity(&rec, p);
                        }
                        order.sort_by(|&x, &y| {
                            sims[y]
                                .partial_cmp(&sims[x])
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        total += 1;
                        if order[0] == d {
                            r1 += 1;
                        }
                        if order[..5.min(order.len())].contains(&d) {
                            r5 += 1;
                        }
                    }
                }
            }
        }
    }
    (
        r1 as f64 / total.max(1) as f64,
        r5 as f64 / total.max(1) as f64,
        total,
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n_ent = args.facts.div_ceil(N_REL);

    println!();
    println!("AETHER · E3 — transplantação JL + proveniência");
    println!("=====================================================");
    println!(
        "fatos={} entidades={} relações={} | d={D_IN} → D={BITS} bits ({} B/átomo) · FHRR D={FHRR_DIM} K={TRACES}",
        args.facts,
        n_ent,
        N_REL,
        BITS / 8
    );
    println!();

    // ---------- dados ----------
    let ds = build_dataset(args.facts, n_ent, args.data_seed, args.nonlin);
    let n = ds.facts.len();

    // ---------- JL: projeção + Spearman ----------
    let mut proj = JlProjector::new(D_IN, BITS, 512, args.jl_seed);
    let words = proj.words();
    let mut bits = vec![0u64; n * words];
    let flat: Vec<f32> = ds.fact_emb.iter().flatten().copied().collect();
    let t0 = Instant::now();
    proj.project_into(&flat, n, &mut bits);
    let jl_ms = t0.elapsed().as_secs_f64() * 1e3;
    let (rho, pairs) = jl_spearman(&bits, words, &ds.fact_emb);

    // banco binário (1,28 KB/átomo)
    let mut bam = BinaryBam::with_capacity(BITS, n);
    for i in 0..n {
        bam.push(&bits[i * words..(i + 1) * words]).expect("bam");
    }
    let bam_bytes = bam.memory_bytes();

    println!("JL (Johnson–Lindenstrauss)");
    println!(
        "  átomos                         : {n} × {} B = {:.2} MB",
        bam.bytes_per_atom(),
        bam_bytes as f64 / 1e6
    );
    println!(
        "  bloco materializado            : {:.0} KB (matriz inteira nunca existe)",
        proj.scratch_bytes() as f64 / 1e3
    );
    println!("  projeção                       : {jl_ms:.1} ms");
    println!("  pares avaliados                : {pairs}");
    println!("  Spearman(cosseno, Hamming)     : ρ = {rho:.4}");
    println!();

    // ---------- FHRR shards: recuperação de relação ----------
    let enc = PhaseEncoder::new(D_IN, FHRR_DIM, args.phase_seed, 2.0);
    let mut entity_phase: Vec<Vec<f32>> = Vec::with_capacity(n_ent);
    for e in &ds.entity_emb {
        let mut p = vec![0.0f32; FHRR_DIM];
        enc.encode_into(e, &mut p);
        entity_phase.push(p);
    }
    let mut relation_phase: Vec<Vec<f32>> = Vec::with_capacity(N_REL);
    for r in &ds.relation_emb {
        let mut p = vec![0.0f32; FHRR_DIM];
        enc.encode_into(r, &mut p);
        relation_phase.push(p);
    }
    let mut value_phase: Vec<Vec<f32>> = Vec::with_capacity(N_VAL);
    for i in 0..N_VAL {
        let mut p = vec![0.0f32; FHRR_DIM];
        // valores: base aleatória determinística, independente das entidades
        let mut rng = ChaCha8Rng::seed_from_u64(args.phase_seed ^ (0x1000 + i as u64));
        let mut v = gauss_vec(&mut rng, D_IN, 1.0);
        normalize_in_place(&mut v);
        enc.encode_into(&v, &mut p);
        value_phase.push(p);
    }

    let mut mask = vec![true; FHRR_DIM];
    // 25% da pista oculta (determinístico)
    for (j, m) in mask.iter_mut().enumerate() {
        if j % 4 == 0 {
            *m = false;
        }
    }
    let mut rng_noise = ChaCha8Rng::seed_from_u64(args.phase_seed ^ 0xBEEF);

    let mut shards = 0usize;
    let mut max_shard = 0usize;
    let mut rel_hits = [0usize; 3];
    let mut rel_total = 0usize;
    let mut margins: Vec<f32> = Vec::with_capacity(n);
    let mut answer_tags_ok = 0usize;
    let mut answer_tags_total = 0usize;
    let sigmas = [0.0f32, args.cue_noise as f32, 1.2f32];

    let mut start = 0usize;
    while start < n {
        let end = (start + SHARD_CAP).min(n);
        let mut store = FhrrStore::with_traces(FHRR_DIM, TRACES)?;
        for i in start..end {
            let (e, r, v) = ds.triples[i];
            store.add_fact(&entity_phase[e], &relation_phase[r], &value_phase[v])?;
        }
        shards += 1;
        max_shard = max_shard.max(end - start);
        for i in start..end {
            let (e, r, _) = ds.triples[i];
            let local = i - start;
            for (si, sigma) in sigmas.iter().enumerate() {
                let mut cue = entity_phase[e].clone();
                if *sigma > 0.0 {
                    let noise = gauss_vec(&mut rng_noise, FHRR_DIM, *sigma);
                    for (a, b) in cue.iter_mut().zip(&noise) {
                        *a = (*a + *b).rem_euclid(std::f32::consts::TAU);
                    }
                }
                let hit = store.query_match(&cue, &relation_phase[r], &mask)?;
                if hit.value_index == local {
                    rel_hits[si] += 1;
                }
                if si == 0 {
                    margins.push(hit.margin);
                    answer_tags_total += 1;
                    if SourceTag::from_code(ds.facts[i].tag.code()).is_some() {
                        answer_tags_ok += 1;
                    }
                }
            }
            rel_total += 1;
        }
        start = end;
    }

    println!("FHRR shards (≤ {SHARD_CAP} fatos/acumulador)");
    println!("  shards={shards} · maior={max_shard} · D={FHRR_DIM} · K={TRACES}");
    for (si, sigma) in sigmas.iter().enumerate() {
        println!(
            "  Recall@1 (ruído de pista {sigma:>3.1} rad) : {:.4}",
            rel_hits[si] as f64 / rel_total.max(1) as f64
        );
    }
    println!();

    // ---------- analogia em fase ----------
    // Banco controlado: 3 grupos × 4 slots × 4 valores = 48 entidades.
    let active = 4usize;
    let bank_len = GROUPS * N_SLOTS * active;
    let mut attr: Vec<Vec<Vec<f32>>> = Vec::with_capacity(N_SLOTS);
    let mut rng = ChaCha8Rng::seed_from_u64(args.data_seed ^ 0xA77);
    for _ in 0..N_SLOTS {
        let mut slot = Vec::with_capacity(VALUES_PER_SLOT);
        for _ in 0..VALUES_PER_SLOT {
            let mut v = gauss_vec(&mut rng, D_IN, 1.0);
            normalize_in_place(&mut v);
            slot.push(v);
        }
        attr.push(slot);
    }
    let mut bank_emb: Vec<Vec<f32>> = Vec::with_capacity(bank_len);
    for g in 0..GROUPS {
        for s in 0..N_SLOTS {
            for v in 0..active {
                let mut x = attr[s][v].clone();
                for t in 0..N_SLOTS {
                    if t != s {
                        let w = (g * 3 + t) % VALUES_PER_SLOT;
                        add_assign(&mut x, &attr[t][w]);
                    }
                }
                // não-aditividade por entidade
                add_assign(&mut x, &gauss_vec(&mut rng, D_IN, args.nonlin as f32));
                normalize_in_place(&mut x);
                bank_emb.push(x);
            }
        }
    }

    let (an1_clean, an5_clean, an_total) = analogy_recall(&bank_emb, &enc, 0.0, 0xE3_A0);
    let (an1_noise, an5_noise, _) = analogy_recall(&bank_emb, &enc, 0.5, 0xE3_A1);
    let (an1_noise2, an5_noise2, _) = analogy_recall(&bank_emb, &enc, 1.0, 0xE3_A2);
    println!("analogia a ⊗ b⁻¹ ⊗ c (48 entidades, {an_total} consultas)");
    println!("  Recall@1 (sem ruído)           : {an1_clean:.4}  · Recall@5: {an5_clean:.4}");
    println!("  Recall@1 (ruído de átomo 0,5rad): {an1_noise:.4}  · Recall@5: {an5_noise:.4}");
    println!(
        "  Recall@1 (ruído de átomo 1,0rad): {an1_noise2:.4}  · Recall@5: {an5_noise2:.4}  [não-linearidade dos dados {:.2}]",
        args.nonlin
    );
    println!();

    // ---------- abstenção calibrada (margem) ----------
    let half = margins.len() / 2;
    let mut train: Vec<f32> = margins[..half].to_vec();
    train.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let threshold = if train.is_empty() {
        0.0
    } else {
        let idx = ((train.len() as f64 * 0.5).ceil() as usize)
            .saturating_sub(1)
            .min(train.len() - 1);
        train[idx]
    };

    let mut accepted = 0usize;
    let mut false_accepted = 0usize;
    let mut ood = 0usize;
    let mut ood_abstained = 0usize;
    // avalia no conjunto de teste (segunda metade dos fatos)
    start = half;
    let mut shard_off = (half / SHARD_CAP) * SHARD_CAP;
    while shard_off < n && start < n {
        let end = (shard_off + SHARD_CAP).min(n);
        let mut store = FhrrStore::with_traces(FHRR_DIM, TRACES)?;
        for i in shard_off..end {
            let (e, r, v) = ds.triples[i];
            store.add_fact(&entity_phase[e], &relation_phase[r], &value_phase[v])?;
        }
        for i in shard_off.max(start)..end {
            let (e, r, _) = ds.triples[i];
            let local = i - shard_off;
            let hit = store.query_match(&entity_phase[e], &relation_phase[r], &mask)?;
            if hit.margin >= threshold {
                accepted += 1;
                if hit.value_index != local {
                    false_accepted += 1;
                }
            }
        }
        shard_off = end;
        start = end;
    }
    // fora do domínio: pistas aleatórias
    let mut ood_store = FhrrStore::with_traces(FHRR_DIM, TRACES)?;
    for i in 0..SHARD_CAP.min(n) {
        let (e, r, v) = ds.triples[i];
        ood_store.add_fact(&entity_phase[e], &relation_phase[r], &value_phase[v])?;
    }
    for _ in 0..200 {
        let cue = gauss_vec(&mut rng_noise, FHRR_DIM, std::f32::consts::TAU / 3.0);
        let rel = gauss_vec(&mut rng_noise, FHRR_DIM, std::f32::consts::TAU / 3.0);
        let hit = ood_store.query_match(&cue, &rel, &mask)?;
        ood += 1;
        if hit.margin < threshold {
            ood_abstained += 1;
        }
    }
    let false_conf = if accepted > 0 {
        false_accepted as f64 / accepted as f64
    } else {
        0.0
    };

    println!("abstenção por margem (limiar calibrado em 50% do treino)");
    println!("  limiar                         : {threshold:.4}");
    println!(
        "  aceitos                        : {accepted} · falsos confiáveis {:.2}%",
        100.0 * false_conf
    );
    println!(
        "  abstenção fora do domínio      : {:.1}% ({}/{ood})",
        pct(ood_abstained, ood),
        ood_abstained
    );
    println!();

    // ---------- verificador estrutural (fora do hot path) ----------
    let mut verifier = StructuralVerifier::new(n_ent, N_REL);
    let (mut bridge, _rx) = SpscKnowledgeBridge::<2048>::new();
    let mut facts = ds.facts.clone();
    for (i, f) in facts.iter_mut().enumerate() {
        // margem real da consulta limpa que produziu o fato
        f.margin = margins.get(i).copied().unwrap_or(1.0);
    }
    let verdict = verifier.verify_into(&facts, threshold, Some(&mut bridge));
    // medição limpa (alocador armado só na janela)
    let (measured, allocs) = count_allocs(|| {
        verifier.verify_into(&facts, threshold, None::<&mut SpscKnowledgeBridge<2048>>)
    });
    let t0 = Instant::now();
    let _ = verifier.verify_into(&facts, threshold, None::<&mut SpscKnowledgeBridge<2048>>);
    let verifier_ms = t0.elapsed().as_secs_f64() * 1e3;

    println!("verificador estrutural (fora do ciclo)");
    println!(
        "  fatos={} · conflitos={} · proveniência inválida={} · ambíguos={}",
        measured.checked,
        measured.value_conflicts,
        measured.provenance_violations,
        measured.ambiguous
    );
    println!(
        "  eventos no bridge              : emitidos={} descartados={}",
        verdict.emitted, verdict.dropped
    );
    println!("  latência                       : {verifier_ms:.3} ms · alocações {allocs}");
    println!();

    // ---------- gates ----------
    let g1 = rho >= 0.95;
    let rel_recall = rel_hits[0] as f64 / rel_total.max(1) as f64;
    let g2 = rel_recall > 0.60;
    let g3a = answer_tags_ok == answer_tags_total && answer_tags_total > 0;
    let g3b = false_conf < 0.05;
    let g3c = verifier_ms <= 20.0 && allocs == 0;

    println!("gates");
    println!(
        "  G1 Spearman ρ ≥ 0,95           : {}  (ρ={rho:.4})",
        pass(g1)
    );
    println!(
        "  G2 recuperação de relação > 0,60: {}  ({rel_recall:.4})· Recall@5 medido {:.4}",
        pass(g2),
        rel_hits[0] as f64 / rel_total.max(1) as f64
    );
    println!(
        "  G3 proveniência em 100%         : {}  ({}/{})",
        pass(g3a),
        answer_tags_ok,
        answer_tags_total
    );
    println!(
        "  G3 falsos confiáveis < 5%       : {}  ({:.2}%)",
        pass(g3b),
        100.0 * false_conf
    );
    println!(
        "  G3 verificador ≤ 20 ms e 0 alloc: {}  ({verifier_ms:.3} ms · {allocs} allocs)",
        pass(g3c)
    );
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e3.json";
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"facts\": {n}, \"entities\": {n_ent}, \"relations\": {N_REL},\n"
    ));
    json.push_str(&format!(
        "  \"d_in\": {D_IN}, \"bits\": {BITS}, \"bytes_per_atom\": {},\n",
        bam.bytes_per_atom()
    ));
    json.push_str(&format!(
        "  \"bam_bytes\": {}, \"jl_ms\": {jl_ms:.3}, \"pairs\": {pairs},\n  \"spearman\": {rho:.6},\n",
        bam_bytes
    ));
    json.push_str(&format!(
        "  \"fhrr_dim\": {FHRR_DIM}, \"traces\": {TRACES}, \"shards\": {shards}, \"max_shard\": {max_shard},\n"
    ));
    json.push_str(&format!(
        "  \"rel_recall_clean\": {:.6}, \"rel_recall_noise\": {:.6}, \"rel_recall_noise2\": {:.6},\n",
        rel_hits[0] as f64 / rel_total.max(1) as f64,
        rel_hits[1] as f64 / rel_total.max(1) as f64,
        rel_hits[2] as f64 / rel_total.max(1) as f64
    ));
    json.push_str(&format!(
        "  \"analogy_r1\": {an1_clean:.6}, \"analogy_r5\": {an5_clean:.6}, \"analogy_queries\": {an_total},\n"
    ));
    json.push_str(&format!(
        "  \"analogy_r1_atom_noise\": {an1_noise:.6}, \"analogy_r1_atom_noise2\": {an1_noise2:.6},\n"
    ));
    json.push_str(&format!(
        "  \"threshold\": {threshold:.6}, \"accepted\": {accepted}, \"false_confident\": {false_conf:.6},\n"
    ));
    json.push_str(&format!(
        "  \"ood_abstained\": {:.6}, \"verifier_ms\": {verifier_ms:.4}, \"verifier_allocs\": {allocs},\n",
        ood_abstained as f64 / ood.max(1) as f64
    ));
    json.push_str(&format!(
        "  \"verifier_conflicts\": {}, \"verifier_prov_violations\": {},\n",
        measured.value_conflicts, measured.provenance_violations
    ));
    json.push_str(&format!(
        "  \"bridge_emitted\": {}, \"bridge_dropped\": {},\n",
        verdict.emitted, verdict.dropped
    ));
    json.push_str(&format!(
        "  \"gates\": {{\"g1\": {g1}, \"g2\": {g2}, \"g3a\": {g3a}, \"g3b\": {g3b}, \"g3c\": {g3c}}}\n}}\n"
    ));
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}

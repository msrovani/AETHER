//! E3 — gates da transplantação geométrica e da proveniência.
//!
//! * **G1** — preservação métrica JL: Spearman `ρ ≥ 0,95` entre cosseno
//!   original (`d = 384`) e Hamming binarizado (`D = 10.240` bits), com
//!   `1.280 bytes/átomo` e memória do banco `1,28 KB × N`;
//! * **G2** — recuperação de relação em shards `FhrrStore` (`D = 160`, `K = 6`,
//!   ≤ 48 fatos/shard) `> 0,60` com ruído de pista; analogia medida, sem alvo;
//! * **G3** — 100% das respostas com `SourceTag` válida; falsos confiáveis
//!   `< 5%`; verificador estrutural `≤ 20 ms` e **0 alocação**.
//!
//! Referências: arXiv:1906.08340 / arXiv:2201.11691 (JL), Vol. III §7 e R3/R10.

#![cfg(all(feature = "m1", feature = "m5"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::fhrr::FhrrStore;
use aether_engine::core::knowledge_bridge::{
    KnowledgeSink, NullSink, SourceTag, SpscKnowledgeBridge,
};
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
// Dimensões
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

fn gauss_vec(rng: &mut ChaCha8Rng, len: usize, sigma: f32) -> Vec<f32> {
    let mut v = vec![0.0f32; len];
    gaussian_vec_into(rng, &mut v);
    for x in v.iter_mut() {
        *x *= sigma;
    }
    v
}

fn unit(rng: &mut ChaCha8Rng, len: usize) -> Vec<f32> {
    let mut v = gauss_vec(rng, len, 1.0);
    normalize_in_place(&mut v);
    v
}

fn add_assign(dst: &mut [f32], src: &[f32]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d += *s;
    }
}

// ---------------------------------------------------------------------------
// G1 — JL preserva a métrica
// ---------------------------------------------------------------------------

#[test]
fn g1_jl_preserva_metrico_e_orcamento_de_bytes() {
    let n = 160usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0x0E31);
    let mut emb: Vec<Vec<f32>> = Vec::with_capacity(n);
    for _ in 0..n {
        emb.push(unit(&mut rng, D_IN));
    }
    let flat: Vec<f32> = emb.iter().flatten().copied().collect();

    let mut proj = JlProjector::new(D_IN, BITS, 512, 0x0E311);
    assert_eq!(proj.words() * 8, BITS / 8);
    assert_eq!(proj.bytes_per_atom(), 1280, "1,28 KB por átomo");
    assert!(proj.scratch_bytes() <= 1_250_000, "bloco deve caber na L2");

    let words = proj.words();
    let mut bits = vec![0u64; n * words];
    proj.project_into(&flat, n, &mut bits);

    let mut cos = Vec::with_capacity(n * (n - 1) / 2);
    let mut ham = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let mut dot = 0.0f32;
            for (a, b) in emb[i].iter().zip(&emb[j]) {
                dot += a * b;
            }
            cos.push(1.0 - dot as f64);
            ham.push(f64::from(hamming(
                &bits[i * words..(i + 1) * words],
                &bits[j * words..(j + 1) * words],
                words,
                BITS,
            )));
        }
    }
    let rho = spearman(&cos, &ham);
    assert!(rho.is_finite(), "Spearman indefinido");
    assert!(rho >= 0.95, "G1: ρ = {rho:.4} < 0,95");

    // o banco binário ocupa exatamente 1,28 KB/átomo
    let mut bam = BinaryBam::with_capacity(BITS, n);
    for i in 0..n {
        bam.push(&bits[i * words..(i + 1) * words]).expect("bam");
    }
    assert_eq!(bam.memory_bytes(), n * 1280);
    // cleanup exato: o vizinho mais próximo de um átomo é ele mesmo
    for i in (0..n).step_by(17) {
        let (j, d) = bam.nearest(bam.atom(i));
        assert_eq!(j, i);
        assert_eq!(d, 0);
    }
}

// ---------------------------------------------------------------------------
// G2 — relação e analogia
// ---------------------------------------------------------------------------

fn value_of(entity: usize, relation: usize) -> usize {
    (entity * 7 + relation * 3) % N_VAL
}

#[test]
fn g2_recuperacao_de_relacao_em_shards() {
    let n_ent = 25usize; // 25 × 8 = 200 fatos
    let n_facts = n_ent * N_REL;
    const _: () = assert!(SHARD_CAP <= 64, "capacidade medida no E1b (D=160, K=6)");

    let mut rng = ChaCha8Rng::seed_from_u64(0x0E32);
    let entity_emb: Vec<Vec<f32>> = (0..n_ent).map(|_| unit(&mut rng, D_IN)).collect();
    let relation_emb: Vec<Vec<f32>> = (0..N_REL).map(|_| unit(&mut rng, D_IN)).collect();

    let enc = PhaseEncoder::new(D_IN, FHRR_DIM, 0x0E322, 2.0);
    let phase = |e: &[f32]| {
        let mut p = vec![0.0f32; FHRR_DIM];
        enc.encode_into(e, &mut p);
        p
    };
    let entity_ph: Vec<Vec<f32>> = entity_emb.iter().map(|e| phase(e)).collect();
    let relation_ph: Vec<Vec<f32>> = relation_emb.iter().map(|e| phase(e)).collect();
    // Átomo de valor **por fato** (como no E1b): valores repetidos com átomos
    // idênticos tornam o argmax ambíguo e a margem degenerada.
    let value_ph: Vec<Vec<f32>> = (0..n_facts)
        .map(|_| {
            (0..FHRR_DIM)
                .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
                .collect()
        })
        .collect();

    // Pista completa: a consulta é (entidade, relação); mascarar posições só
    // reduz a capacidade efetiva, e o E1b já mediu o regime de 48 fatos.
    let mask = vec![true; FHRR_DIM];

    let mut rng = ChaCha8Rng::seed_from_u64(0x0E323);
    let (mut clean, mut noisy, mut total) = (0usize, 0usize, 0usize);
    let mut start = 0usize;
    while start < n_facts {
        let end = (start + SHARD_CAP).min(n_facts);
        let mut store = FhrrStore::with_traces(FHRR_DIM, TRACES).expect("store");
        for (i, vp) in value_ph.iter().enumerate().take(end).skip(start) {
            let (e, r) = (i / N_REL, i % N_REL);
            store
                .add_fact(&entity_ph[e], &relation_ph[r], vp)
                .expect("add");
        }
        for i in start..end {
            let (e, r) = (i / N_REL, i % N_REL);
            let local = i - start;
            let hit = store
                .query_match(&entity_ph[e], &relation_ph[r], &mask)
                .expect("query");
            if hit.value_index == local {
                clean += 1;
            }
            // pista com ruído de 0,6 rad (simula embedding imperfeito)
            let mut cue = entity_ph[e].clone();
            let noise = gauss_vec(&mut rng, FHRR_DIM, 0.6);
            for (a, b) in cue.iter_mut().zip(&noise) {
                *a = (*a + *b).rem_euclid(std::f32::consts::TAU);
            }
            let hit = store
                .query_match(&cue, &relation_ph[r], &mask)
                .expect("query");
            if hit.value_index == local {
                noisy += 1;
            }
            total += 1;
        }
        start = end;
    }

    let clean_rate = clean as f64 / total as f64;
    let noisy_rate = noisy as f64 / total as f64;
    assert!(
        clean_rate >= 0.95,
        "G2: Recall@1 limpo = {clean_rate:.4} (esperado ≥ 0,95)"
    );
    assert!(
        noisy_rate > 0.60,
        "G2: recuperação de relação com ruído = {noisy_rate:.4} (alvo > 0,60)"
    );
}

/// Banco controlado de 48 entidades e consultas `a:b :: c:d`.
fn analogy_recall(bank_emb: &[Vec<f32>], enc: &PhaseEncoder, sigma: f32, seed: u64) -> (f64, f64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut phases: Vec<Vec<f32>> = Vec::with_capacity(bank_emb.len());
    for e in bank_emb {
        let mut p = vec![0.0f32; FHRR_DIM];
        enc.encode_into(e, &mut p);
        if sigma > 0.0 {
            let noise = gauss_vec(&mut rng, FHRR_DIM, sigma);
            for (a, b) in p.iter_mut().zip(&noise) {
                *a = (*a + *b).rem_euclid(std::f32::consts::TAU);
            }
        }
        phases.push(p);
    }
    let idx = |g: usize, s: usize, v: usize| (g * N_SLOTS + s) * 4 + v;
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
                for v1 in 0..4 {
                    for v2 in 0..4 {
                        if v1 == v2 {
                            continue;
                        }
                        let (a, b, c, d) =
                            (idx(g, s, v1), idx(g, s, v2), idx(g2, s, v1), idx(g2, s, v2));
                        phase_analogy(&phases[b], &phases[a], &phases[c], &mut rec);
                        for (j, p) in phases.iter().enumerate() {
                            sims[j] = phase_similarity(&rec, p);
                        }
                        order.sort_by(|&x, &y| {
                            sims[y]
                                .partial_cmp(&sims[x])
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        total += 1;
                        r1 += usize::from(order[0] == d);
                        r5 += usize::from(order[..5.min(order.len())].contains(&d));
                    }
                }
            }
        }
    }
    (r1 as f64 / total as f64, r5 as f64 / total as f64)
}

#[test]
fn g2_analogia_em_fase_e_medida_sem_alvo() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x0E33);
    let mut attr: Vec<Vec<Vec<f32>>> = Vec::with_capacity(N_SLOTS);
    for _ in 0..N_SLOTS {
        let mut slot = Vec::with_capacity(VALUES_PER_SLOT);
        for _ in 0..VALUES_PER_SLOT {
            slot.push(unit(&mut rng, D_IN));
        }
        attr.push(slot);
    }
    let mut bank: Vec<Vec<f32>> = Vec::with_capacity(GROUPS * N_SLOTS * 4);
    for g in 0..GROUPS {
        for s in 0..N_SLOTS {
            for v in 0..4 {
                let mut x = attr[s][v].clone();
                for t in 0..N_SLOTS {
                    if t != s {
                        add_assign(&mut x, &attr[t][(g * 3 + t) % VALUES_PER_SLOT]);
                    }
                }
                // não-aditividade controlada (embedding real não é aditivo).
                // Sem normalizar: normalizar é não-linear e destrói a identidade
                // a:b :: c:d que a analogia em fase explora.
                add_assign(
                    &mut x,
                    &gauss_vec(&mut rng, D_IN, 0.25 / (D_IN as f32).sqrt()),
                );
                bank.push(x);
            }
        }
    }
    let enc = PhaseEncoder::new(D_IN, FHRR_DIM, 0x0E333, 2.0);
    let (r1, r5) = analogy_recall(&bank, &enc, 0.0, 0x0E334);
    assert!(r1.is_finite() && r5 >= r1);
    assert!(r1 >= 0.80, "analogia Recall@1 = {r1:.4} (esperado ≥ 0,80)");
    // ruído alto degrada de forma mensurável, não explode
    let (r1n, _) = analogy_recall(&bank, &enc, 1.2, 0x0E335);
    assert!(r1n <= r1 + 1e-9, "ruído não pode melhorar o recall");
}

// ---------------------------------------------------------------------------
// G3 — proveniência, abstenção e verificador
// ---------------------------------------------------------------------------

fn fact(i: usize) -> FactRecord {
    let tag = match i % 10 {
        0..=6 => SourceTag::Transplanted,
        7 | 8 => SourceTag::Observed,
        _ => SourceTag::Inferred,
    };
    FactRecord {
        entity: (i / N_REL) as u32,
        relation: (i % N_REL) as u32,
        value: value_of(i / N_REL, i % N_REL) as u32,
        tag,
        source: if tag == SourceTag::Transplanted {
            (i + 1) as u32
        } else {
            0
        },
        margin: 0.5,
    }
}

#[test]
fn g3_verificador_zero_alloc_e_20ms() {
    let n_ent = 125usize; // 1000 fatos
    let facts: Vec<FactRecord> = (0..n_ent * N_REL).map(fact).collect();
    assert_eq!(facts.len(), 1000);

    let mut verifier = StructuralVerifier::new(n_ent, N_REL);
    // warmup
    let _ = verifier.verify_into(&facts, 0.1, None::<&mut NullSink>);

    let (verdict, allocs) =
        count_allocs(|| verifier.verify_into(&facts, 0.1, None::<&mut NullSink>));
    let t0 = Instant::now();
    let _ = verifier.verify_into(&facts, 0.1, None::<&mut NullSink>);
    let ms = t0.elapsed().as_secs_f64() * 1e3;

    assert_eq!(allocs, 0, "verificador alocou {allocs} vezes");
    assert!(ms <= 20.0, "G3: verificador levou {ms:.3} ms (> 20 ms)");
    assert_eq!(verdict.checked, 1000);
    assert!(
        verdict.is_valid(),
        "grafo deveria ser consistente: {verdict:?}"
    );
    assert_eq!(verdict.provenance_violations, 0);
    // 100% dos fatos têm tag válida por construção (enum) — confirma o inverso
    for f in &facts {
        assert!(SourceTag::from_code(f.tag.code()).is_some());
    }

    // o verificador **detecta** o que deve detectar
    let mut bad = facts[..4].to_vec();
    bad.push(FactRecord {
        entity: facts[0].entity,
        relation: facts[0].relation,
        value: facts[0].value.wrapping_add(1),
        tag: SourceTag::Observed,
        source: 0,
        margin: 0.5,
    });
    bad.push(FactRecord {
        entity: 7,
        relation: 3,
        value: 9,
        tag: SourceTag::Transplanted,
        source: 0, // sem fonte ⇒ inválido
        margin: 0.5,
    });
    let v = verifier.verify_into(&bad, 0.1, None::<&mut NullSink>);
    assert_eq!(v.value_conflicts, 1);
    assert_eq!(v.provenance_violations, 1);
    assert!(!v.is_valid());
}

#[test]
fn g3_bridge_emite_zero_alloc_quando_ativo() {
    let n_ent = 20usize;
    let facts: Vec<FactRecord> = (0..n_ent * N_REL).map(fact).collect();
    let mut verifier = StructuralVerifier::new(n_ent, N_REL);
    let (mut bridge, mut rx) = SpscKnowledgeBridge::<256>::new();
    let (verdict, allocs) = count_allocs(|| verifier.verify_into(&facts, 0.1, Some(&mut bridge)));
    assert_eq!(allocs, 0);
    assert_eq!(verdict.emitted, facts.len() as u64);
    assert_eq!(verdict.dropped, 0);
    assert!(verdict.is_valid());
    // o consumidor recebe os eventos (proveniência carimbada)
    let mut seen = 0usize;
    while let Some(ev) = rx.try_pop() {
        assert!(SourceTag::from_code(ev.source).is_some());
        seen += 1;
    }
    assert_eq!(seen, facts.len());
    assert!(bridge.is_active());
}

#[test]
fn g3_abstencao_por_margem_tem_poucos_falsos_confiaveis() {
    // Duas abstenções independentes:
    //  · **evidência** (`role_similarity`) detecta pista fora do domínio;
    //  · **margem** detecta cleanup ambíguo dentro do domínio.
    // A margem sozinha não detecta OOD: o unbinding sempre "acha" um valor
    // para o melhor papel, mesmo quando o papel não existe.
    let mut rng = ChaCha8Rng::seed_from_u64(0x0E34);
    let dim = FHRR_DIM;
    let mut store = FhrrStore::with_traces(dim, TRACES).expect("store");
    let mask = vec![true; dim];
    let mut in_domain = Vec::new();
    let mut in_role = Vec::new();
    let mut region = Vec::new();
    for _ in 0..SHARD_CAP {
        let mut e = vec![0.0f32; dim];
        let mut r = vec![0.0f32; dim];
        let mut v = vec![0.0f32; dim];
        for j in 0..dim {
            e[j] = rng.gen::<f32>() * std::f32::consts::TAU;
            r[j] = rng.gen::<f32>() * std::f32::consts::TAU;
            v[j] = rng.gen::<f32>() * std::f32::consts::TAU;
        }
        store.add_fact(&e, &r, &v).expect("add");
        region.push((e, r));
    }
    for (e, r) in &region {
        let hit = store.query_match(e, r, &mask).expect("q");
        in_domain.push(hit.margin);
        in_role.push(hit.role_similarity);
    }
    in_domain.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    in_role.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // limiar de ambiguidade: 50% de cobertura; evidência: percentil 5 do domínio
    let cut = in_domain[in_domain.len() / 2];
    let evidence_cut = in_role[in_role.len() / 20];

    let mut ood = 0usize;
    let mut abstained = 0usize;
    let mut false_confident = 0usize;
    let mut accepted = 0usize;
    for _ in 0..200 {
        let mut e = vec![0.0f32; dim];
        let mut r = vec![0.0f32; dim];
        for j in 0..dim {
            e[j] = rng.gen::<f32>() * std::f32::consts::TAU;
            r[j] = rng.gen::<f32>() * std::f32::consts::TAU;
        }
        let hit = store.query_match(&e, &r, &mask).expect("q");
        ood += 1;
        if hit.role_similarity < evidence_cut || hit.margin < cut {
            abstained += 1;
        } else {
            accepted += 1;
            // OOD aceito é, por definição, um "confiante e errado".
            false_confident += 1;
        }
    }
    let rate = abstained as f64 / ood as f64;
    assert!(
        rate >= 0.95,
        "abstenção fora do domínio = {:.1}% (esperado ≥ 95%)",
        100.0 * rate
    );
    let false_conf = if accepted > 0 {
        false_confident as f64 / accepted as f64
    } else {
        0.0
    };
    assert!(
        false_conf < 0.05,
        "falsos confiáveis = {:.1}%",
        100.0 * false_conf
    );
    // dentro do domínio, o limiar de margem aceita ~50% (por construção)
    let inside = in_domain.iter().filter(|m| **m >= cut).count();
    assert!(
        inside * 2 >= in_domain.len(),
        "limiar de cobertura deveria aceitar ≥ 50% do domínio"
    );
}

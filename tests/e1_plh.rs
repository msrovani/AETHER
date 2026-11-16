//! E1 — gates do Phase-Language Head sharded.
//!
//! Versão compacta do `examples/phase_language_probe.rs`: 240 fatos em 24
//! shards (10 por shard), para rodar em teste sem custo.
//!
//! Verifica: recall sharded ≥ 99% (vs superposição plana), não-comutatividade
//! estrita e abstenção por evidência. Os números completos (1.000 fatos) ficam em
//! `target/aether_e1_plh.json`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

use aether_engine::core::fhrr::FhrrStore;

const D: usize = 160;
const TRACES: usize = 6;
const SHARDS: usize = 24;

fn atom(seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..D).map(|_| rng.gen::<f32>() * TAU).collect()
}

fn add_into(a: &[f32], b: &[f32], out: &mut [f32]) {
    for ((o, x), y) in out.iter_mut().zip(a.iter()).zip(b.iter()) {
        *o = (x + y).rem_euclid(TAU);
    }
}

fn permute(v: &[f32], k: usize) -> Vec<f32> {
    (0..v.len())
        .map(|i| v[(i + v.len() - (k % v.len())) % v.len()])
        .collect()
}

fn build_fact(
    subj: &[f32],
    rel: &[f32],
    obj: &[f32],
    anchor: &[Vec<f32>],
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut entity = vec![0.0f32; D];
    let mut relation = vec![0.0f32; D];
    let mut obj_role = vec![0.0f32; D];
    add_into(subj, &anchor[0], &mut entity);
    add_into(rel, &anchor[1], &mut relation);
    add_into(obj, &anchor[2], &mut obj_role);
    (entity, relation, permute(&obj_role, 1))
}

#[test]
fn phasor_cue_matches_phase_cue() {
    // `query_phasor_into` tem de devolver exatamente o mesmo que `query_into`.
    let seed = 0xE1_0004u64;
    let anchor = vec![atom(seed ^ 0xA1), atom(seed ^ 0xA2), atom(seed ^ 0xA3)];
    let subj = atom(seed ^ 0x11);
    let rel = atom(seed ^ 0x22);
    let obj = atom(seed ^ 0x33);

    let mut store = FhrrStore::with_traces(D, TRACES).expect("store");
    let (e, r, v) = build_fact(&subj, &rel, &obj, &anchor);
    store.add_fact(&e, &r, &v).expect("add");
    let (e2, r2, v2) = build_fact(&subj, &rel, &atom(seed ^ 0x44), &anchor);
    store.add_fact(&e2, &r2, &v2).expect("add2");

    let mask = vec![true; D];
    let mut sc = store.scratch(4);
    let by_phase = store.query_into(&e, &r, &mask, &mut sc).expect("q1");

    // cue = exp(i·(entity + relation)), sem `sin_cos` por consulta
    let mut cue_re = vec![0.0f32; D];
    let mut cue_im = vec![0.0f32; D];
    for j in 0..D {
        let (s, c) = (e[j] + r[j]).sin_cos();
        cue_re[j] = c;
        cue_im[j] = s;
    }
    let by_phasor = store
        .query_phasor_into(&cue_re, &cue_im, &mask, &mut sc)
        .expect("q2");

    assert_eq!(by_phase.value_index, by_phasor.value_index);
    assert_eq!(by_phase.fact_index, by_phasor.fact_index);
    assert!(
        (by_phase.similarity - by_phasor.similarity).abs() < 1e-5,
        "similaridade divergiu: {} vs {}",
        by_phase.similarity,
        by_phasor.similarity
    );
}

#[test]
fn sharding_holds_480_facts_and_flat_collapses() {
    let seed = 0xE1_0001u64;
    let anchor = vec![atom(seed ^ 0xA1), atom(seed ^ 0xA2), atom(seed ^ 0xA3)];
    let subjects: Vec<Vec<f32>> = (0..32).map(|i| atom(seed ^ (0x1000 + i as u64))).collect();
    let relations: Vec<Vec<f32>> = (0..SHARDS)
        .map(|i| atom(seed ^ (0x2000 + i as u64)))
        .collect();

    let mut shards: Vec<FhrrStore> = (0..SHARDS)
        .map(|_| FhrrStore::with_traces(D, TRACES).expect("shard"))
        .collect();
    let mut flat = FhrrStore::with_traces(D, TRACES).expect("flat");

    let facts = 480usize;
    for i in 0..facts {
        let rel = i % SHARDS;
        let subj = i / SHARDS;
        let obj = atom(seed ^ (0x9000 + i as u64));
        let (e, r, v) = build_fact(&subjects[subj], &relations[rel], &obj, &anchor);
        shards[rel].add_fact(&e, &r, &v).expect("shard add");
        flat.add_fact(&e, &r, &v).expect("flat add");
    }

    let mask = vec![true; D];
    let mut sc = shards[0].scratch(64);
    let mut hits = 0usize;
    for i in 0..facts {
        let rel = i % SHARDS;
        let subj = i / SHARDS;
        let obj = atom(seed ^ (0x9000 + i as u64));
        let (e, r, _v) = build_fact(&subjects[subj], &relations[rel], &obj, &anchor);
        let hit = shards[rel].query_into(&e, &r, &mask, &mut sc).expect("q");
        if hit.value_index == i / SHARDS {
            hits += 1;
        }
    }
    let recall = hits as f64 / facts as f64;
    assert!(recall >= 0.99, "recall sharded {recall} < 0.99");

    let mut flat_sc = flat.scratch(facts);
    let mut flat_hits = 0usize;
    for i in 0..facts {
        let rel = i % SHARDS;
        let subj = i / SHARDS;
        let obj = atom(seed ^ (0x9000 + i as u64));
        let (e, r, _v) = build_fact(&subjects[subj], &relations[rel], &obj, &anchor);
        let hit = flat.query_into(&e, &r, &mask, &mut flat_sc).expect("fq");
        if hit.value_index == i {
            flat_hits += 1;
        }
    }
    let flat_recall = flat_hits as f64 / facts as f64;
    assert!(
        flat_recall < 0.3,
        "superposição plana deveria colapsar, veio {flat_recall}"
    );
}

#[test]
fn permutation_makes_order_matter() {
    let seed = 0xE1_0002u64;
    let anchor = vec![atom(seed ^ 0xA1), atom(seed ^ 0xA2), atom(seed ^ 0xA3)];
    let a = atom(seed ^ 0xAA);
    let b = atom(seed ^ 0xBB);
    let rel = atom(seed ^ 0xCC);

    let mut store = FhrrStore::with_traces(D, TRACES).expect("store");
    let (e1, r1, v1) = build_fact(&a, &rel, &b, &anchor);
    let (e2, r2, v2) = build_fact(&b, &rel, &a, &anchor);
    store.add_fact(&e1, &r1, &v1).expect("ab");
    store.add_fact(&e2, &r2, &v2).expect("ba");

    let mask = vec![true; D];
    let mut sc = store.scratch(4);
    let ab = store.query_into(&e1, &r1, &mask, &mut sc).expect("q1");
    let ba = store.query_into(&e2, &r2, &mask, &mut sc).expect("q2");
    assert_eq!(ab.value_index, 0, "A→B deveria recuperar o primeiro fato");
    assert_eq!(ba.value_index, 1, "B→A deveria recuperar o segundo fato");
    assert_ne!(ab.value_index, ba.value_index);
}

#[test]
fn out_of_vocabulary_abstains() {
    let seed = 0xE1_0003u64;
    let anchor = vec![atom(seed ^ 0xA1), atom(seed ^ 0xA2), atom(seed ^ 0xA3)];
    let subjects: Vec<Vec<f32>> = (0..16).map(|i| atom(seed ^ (0x1000 + i))).collect();
    let rel = atom(seed ^ 0x2000);

    let mut store = FhrrStore::with_traces(D, TRACES).expect("store");
    for (i, subject) in subjects.iter().enumerate() {
        let obj = atom(seed ^ (0x9000 + i as u64));
        let (e, r, v) = build_fact(subject, &rel, &obj, &anchor);
        store.add_fact(&e, &r, &v).expect("add");
    }

    let mask = vec![true; D];
    let mut sc = store.scratch(16);

    // in-vocab: evidência alta
    let (e, r, _) = build_fact(&subjects[3], &rel, &atom(seed ^ 0x9999), &anchor);
    let hit = store.query_into(&e, &r, &mask, &mut sc).expect("in");
    assert!(
        hit.role_similarity > 0.9,
        "evidência in-vocab baixa: {}",
        hit.role_similarity
    );

    // fora do vocabulário: abstenção
    let oov_e = atom(seed ^ 0xDEAD);
    let oov_r = atom(seed ^ 0xBEEF);
    let hit = store
        .query_into(&oov_e, &oov_r, &mask, &mut sc)
        .expect("oov");
    assert!(
        hit.role_similarity < 0.5,
        "OOV deveria abster, evidência {}",
        hit.role_similarity
    );
}

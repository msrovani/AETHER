//! scratch (temporary)
use aether_engine::core::bam_phase::{BamHolographicVector, BamMask};
use aether_engine::core::bam_store::BamHolographicStore;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

const D: usize = 160;

fn atom(rng: &mut ChaCha8Rng) -> BamHolographicVector<D> {
    let phases: Vec<f32> = (0..D).map(|_| rng.gen::<f32>() * TAU).collect();
    BamHolographicVector::<D>::from_radians(&phases).unwrap()
}

fn main() {
    for &(facts, k) in &[(48usize, 6usize), (48, 10), (48, 13)] {
        let mut rng = ChaCha8Rng::seed_from_u64(0xA17E_0001 ^ k as u64);
        let mut store = BamHolographicStore::<D>::new(k).unwrap();
        let mut atoms = Vec::new();
        for _ in 0..facts {
            let e = atom(&mut rng);
            let r = atom(&mut rng);
            let v = atom(&mut rng);
            store.add_fact(&e, &r, &v);
            atoms.push((e, r));
        }
        let mask = BamMask::<D>::all();
        let mut sc = store.scratch(facts);
        let mut hits = 0;
        let mut sim_sum = 0.0f32;
        let mut role_sum = 0.0f32;
        for (k2, (e, r)) in atoms.iter().enumerate() {
            let hit = store.query_into(e, r, &mask, &mut sc).unwrap();
            if hit.value_index == k2 {
                hits += 1;
            }
            sim_sum += hit.similarity;
            role_sum += hit.role_similarity;
        }
        println!(
            "facts={facts} K={k} recall={:.3} sim={:.3} role={:.3} mem={}KB",
            hits as f64 / facts as f64,
            sim_sum / facts as f32,
            role_sum / facts as f32,
            store.memory_bytes() / 1024
        );
    }

    // flat 1000
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let mut store = BamHolographicStore::<D>::new(6).unwrap();
    let mut atoms = Vec::new();
    for _ in 0..1000 {
        let e = atom(&mut rng);
        let r = atom(&mut rng);
        let v = atom(&mut rng);
        store.add_fact(&e, &r, &v);
        atoms.push((e, r));
    }
    let mask = BamMask::<D>::all();
    let mut sc = store.scratch(1000);
    let mut hits = 0;
    let mut n = 0;
    for (i, (e, r)) in atoms.iter().enumerate().step_by(5) {
        let hit = store.query_into(e, r, &mask, &mut sc).unwrap();
        if hit.value_index == i {
            hits += 1;
        }
        n += 1;
    }
    println!("flat 1000 recall={:.4}", hits as f64 / n as f64);
}

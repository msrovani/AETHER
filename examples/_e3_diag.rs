//! Probe descartável: mede correlação cosseno × Hamming do JL.
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::transplant::{
    gaussian_vec_into, hamming, normalize_in_place, spearman, JlProjector,
};

const D_IN: usize = 384;
const BITS: usize = 10_240;

fn unit(rng: &mut ChaCha8Rng, len: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; len];
    gaussian_vec_into(rng, &mut v);
    normalize_in_place(&mut v);
    v
}

fn main() {
    let n = 160usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0x0E31);
    let mut emb: Vec<Vec<f32>> = Vec::with_capacity(n);
    for _ in 0..n {
        emb.push(unit(&mut rng, D_IN));
    }
    let flat: Vec<f32> = emb.iter().flatten().copied().collect();
    let mut proj = JlProjector::new(D_IN, BITS, 512, 0x0E311);
    let words = proj.words();
    let mut bits = vec![0u64; n * words];
    proj.project_into(&flat, n, &mut bits);

    let mut cos = Vec::new();
    let mut ham = Vec::new();
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
    println!("rho(1-cos, ham) = {:.4}", spearman(&cos, &ham));
    let sims: Vec<f64> = cos.iter().map(|c| 1.0 - c).collect();
    println!("rho(cos, ham)   = {:.4}", spearman(&sims, &ham));
    for k in 0..5 {
        println!("par {k}: cos_dist={:.4} sim={:.4} ham={:.0}", cos[k], sims[k], ham[k]);
    }
    let mean_ham: f64 = ham.iter().sum::<f64>() / ham.len() as f64;
    println!("ham médio = {:.1} (esperado ~5120)", mean_ham);
}

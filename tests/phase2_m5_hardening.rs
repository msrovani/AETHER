#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 32;

fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (std::f32::consts::TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(std::f32::consts::TAU)
        })
        .collect()
}

fn random_pattern(n: usize, seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..n)
        .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
        .collect()
}

fn mask_exact_hidden(seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..N).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; N];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn ang_err(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool]) -> f32 {
    let mut s = 0.0;
    let mut c = 0;
    for i in 0..N {
        if mask[i] {
            continue;
        }
        s += ang_err(recovered[i], target[i]).abs();
        c += 1;
    }
    s / c as f32
}

fn report(label: &str, patterns: &[Vec<f32>], seeds: u64) {
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    };
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in patterns {
        mem.imprint(p, 40);
    }
    let k = patterns.len();
    let mut means = vec![0.0f32; k];
    let mut worst = vec![0.0f32; k];
    for seed in 0..seeds {
        for (idx, p) in patterns.iter().enumerate() {
            let mask = mask_exact_hidden(seed, 12);
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(p, &mask, 600, 0.05);
            let e = mean_err(&rec, p, &mask);
            means[idx] += e;
            worst[idx] = worst[idx].max(e);
        }
    }
    println!(
        "[{label} P={k}] mean {} | worst {}",
        (0..k)
            .map(|i| format!("{:.4}", means[i] / seeds as f32))
            .collect::<Vec<_>>()
            .join(" "),
        (0..k)
            .map(|i| format!("{:.4}", worst[i]))
            .collect::<Vec<_>>()
            .join(" ")
    );
}

#[test]
fn probe_stats() {
    let p0 = harmonic_pattern(N, 0);
    let p1 = harmonic_pattern(N, 1);
    let p2 = harmonic_pattern(N, 2);
    let p3 = harmonic_pattern(N, 3);

    // Família do invariante
    report("harm 0,1,2", &[p0.clone(), p1.clone(), p2.clone()], 40);
    report("harm 0..3", &[p0.clone(), p1.clone(), p2.clone(), p3.clone()], 20);
    report("harm 0,1", &[p0.clone(), p1.clone()], 40);

    // Controle: três padrões que diferem por rotação rígida (crosstalk nulo
    // por construção: Δψ é idêntico para todos os pares em todos os padrões).
    let tau3 = std::f32::consts::TAU / 3.0;
    let q1: Vec<f32> = p0.iter().map(|v| (v + tau3).rem_euclid(std::f32::consts::TAU)).collect();
    let q2: Vec<f32> = p0.iter().map(|v| (v + 2.0 * tau3).rem_euclid(std::f32::consts::TAU)).collect();
    report("rotacoes rigidas", &[p0.clone(), q1, q2], 40);

    // Padrões aleatórios (referência da capacidade clássica)
    let r0 = random_pattern(N, 11);
    let r1 = random_pattern(N, 12);
    let r2 = random_pattern(N, 13);
    report("aleatorios", &[r0, r1, r2], 40);
}

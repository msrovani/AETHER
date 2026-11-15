//! SONDA TEMPORARIA (BUFFY) — diagnostico do recall do padrao harmonico 2.
//! Hipotese: o erro vem de herdar o estado deixado pelo recall anterior
//! (recall e stateful), nao do acoplamento gravado.
//! Sera removida apos a investigacao.

#![cfg(feature = "m6")]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const N: usize = 32;
const HIDDEN: usize = 12;

fn harmonic(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (std::f32::consts::TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(std::f32::consts::TAU)
        })
        .collect()
}

fn ang(a: f32, b: f32) -> f32 {
    ((a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs()
}

fn mask_exact(seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..N).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; N];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn mean_err(rec: &[f32], tgt: &[f32], mask: &[bool]) -> f32 {
    let mut s = 0.0;
    let mut c = 0usize;
    for i in 0..tgt.len() {
        if mask[i] {
            continue;
        }
        s += ang(rec[i], tgt[i]);
        c += 1;
    }
    s / c as f32
}

fn cfg() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

/// `cold = true` zera as fases antes de cada recall (estado limpo).
/// `cold = false` reproduz o comportamento anterior: herda o estado do recall
/// imediatamente anterior, na mesma seed.
fn sweep(cold: bool, order: &[usize]) -> Vec<(f32, f32)> {
    let pats: Vec<Vec<f32>> = order.iter().map(|k| harmonic(N, *k)).collect();
    let mut mem = ComplexPhaseMemory::with_config(N, cfg());
    for p in &pats {
        mem.imprint(p, 40);
    }
    let seeds = 30u64;
    let mut sum = vec![0.0f32; pats.len()];
    let mut worst = vec![0.0f32; pats.len()];
    for seed in 0..seeds {
        let mask = mask_exact(seed, HIDDEN);
        for (k, p) in pats.iter().enumerate() {
            if cold {
                mem.reset_phases(0.0);
            }
            let (rec, _e) = mem.recall(p, &mask, 600, 0.05);
            let e = mean_err(&rec, p, &mask);
            sum[k] += e;
            worst[k] = worst[k].max(e);
        }
    }
    (0..pats.len())
        .map(|k| (sum[k] / seeds as f32, worst[k]))
        .collect()
}

#[test]
fn sonda_causa_do_erro() {
    println!("\n=== SONDA: causa do erro do padrao 2 ===");
    for (label, cold) in [("COLD (reset 0 antes de cada recall)", true), (
        "WARM (herda recall anterior, mesmo seed)",
        false,
    )] {
        let r = sweep(cold, &[0, 1, 2]);
        println!("{label}:");
        for (k, (m, w)) in r.iter().enumerate() {
            println!("   p{k}: media={m:.6} pior={w:.6}");
        }
    }
    // Ordem invertida: se o erro for contaminacao, p2 melhora ao vir primeiro.
    println!("COLD, ordem 2,1,0:");
    for (k, (m, w)) in sweep(true, &[2, 1, 0]).iter().enumerate() {
        println!("   pos{k}: media={m:.6} pior={w:.6}");
    }
    println!("WARM, ordem 2,1,0:");
    for (k, (m, w)) in sweep(false, &[2, 1, 0]).iter().enumerate() {
        println!("   pos{k}: media={m:.6} pior={w:.6}");
    }
    println!("=== fim ===\n");
}

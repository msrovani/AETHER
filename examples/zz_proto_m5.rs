//! PROTOTIPO TEMPORARIO (BUFFY) — diagnostico de custo do `recall` (M5).
//!
//! Mede duas coisas que a especificacao de auditoria cobra:
//!   (a) latencia x numero de passos, e a precisao alcancada em cada caso;
//!   (b) se a soma de acoplamento `sum_j W_ij sin(phi_j - phi_i - delta_ij)`
//!       pode ser reescrita como parte imaginaria de um produto matriz-vetor
//!       complexo, que tira UM `sin` por aresta e deixa so flops.
//!
//! A reescrita e exata (identidade trigonometrica), entao a comparacao com o
//! `recall` real e a prova: mesma fisica, custo diferente.

use std::time::Instant;

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 200;
const FACTS: usize = 50;
const TAU: f32 = std::f32::consts::TAU;

fn ang(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % TAU;
    if d > TAU / 2.0 {
        d -= TAU;
    } else if d < -TAU / 2.0 {
        d += TAU;
    }
    d.abs()
}

fn mask_exact(seed: u64, n: usize, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; n];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        let (ca, sa) = a[i].sin_cos();
        let (cb, sb) = b[i].sin_cos();
        dot += ca * cb + sa * sb;
        na += 1.0;
        nb += 1.0;
    }
    dot / (na * nb).sqrt()
}

/// Replica `integrate` do M5 trocando a soma com `sin` por aresta por
/// `Im(e^{-i phi_i} * sum_j A_ij e^{i phi_j})`, com `A_ij = W_ij e^{-i delta_ij}`
/// precalculado uma vez. Sem alocacao extra no laco.
struct FastKernel {
    n: usize,
    cr: Vec<f32>,  // W cos(delta)
    ci: Vec<f32>,  // -W sin(delta)
    phases: Vec<f32>,
    cos_p: Vec<f32>,
    sin_p: Vec<f32>,
}

impl FastKernel {
    fn new(mem: &ComplexPhaseMemory) -> Self {
        let n = mem.n;
        let mut cr = vec![0.0f32; n * n];
        let mut ci = vec![0.0f32; n * n];
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let idx = i * n + j;
                let (c, s) = mem.phase_shifts[idx].sin_cos();
                cr[idx] = mem.weights[idx] * c;
                ci[idx] = -mem.weights[idx] * s;
            }
        }
        Self {
            n,
            cr,
            ci,
            phases: vec![0.0; n],
            cos_p: vec![0.0; n],
            sin_p: vec![0.0; n],
        }
    }

    fn run(&mut self, cue: &[f32], mask: &[bool], steps: usize, dt: f32, clamp: f32) {
        let n = self.n;
        let inv_n = 1.0 / n as f32;
        for i in 0..n {
            if mask[i] {
                self.phases[i] = cue[i].rem_euclid(TAU);
            }
        }
        for _ in 0..steps {
            for j in 0..n {
                let (c, s) = self.phases[j].sin_cos();
                self.cos_p[j] = c;
                self.sin_p[j] = s;
            }
            for i in 0..n {
                let mut re = 0.0f32;
                let mut im = 0.0f32;
                let base = i * n;
                for j in 0..n {
                    let idx = base + j;
                    re += self.cr[idx] * self.cos_p[j] - self.ci[idx] * self.sin_p[j];
                    im += self.cr[idx] * self.sin_p[j] + self.ci[idx] * self.cos_p[j];
                }
                let (cpi, spi) = self.phases[i].sin_cos();
                let mut drive = (im * cpi - re * spi) * inv_n;
                if mask[i] {
                    let err = cue[i] - self.phases[i];
                    drive += clamp * err.sin();
                }
                let v = self.phases[i] + drive * dt;
                self.phases[i] = v.rem_euclid(TAU);
            }
        }
    }
}

fn main() {
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.001,
    };
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    let mut rng = ChaCha8Rng::seed_from_u64(0xFAC7);
    let mut facts: Vec<Vec<f32>> = Vec::new();
    for _ in 0..FACTS {
        let p: Vec<f32> = (0..N).map(|_| rng.gen::<f32>() * TAU).collect();
        facts.push(p);
    }
    for f in &facts {
        mem.imprint(f, 8);
    }
    let mut kernel = FastKernel::new(&mem);

    // Mesmas 5 consultas para as duas implementacoes (40% ocluido).
    let queries: Vec<(usize, Vec<f32>, Vec<bool>)> = (0..5)
        .map(|q| {
            let truth = (rng.gen::<u32>() as usize) % FACTS;
            let mask = mask_exact(0x7000 + q as u64, N, N * 40 / 100);
            let mut cue = vec![0.0f32; N];
            for i in 0..N {
                if mask[i] {
                    cue[i] = facts[truth][i] + (rng.gen::<f32>() - 0.5) * 0.5;
                }
            }
            (truth, cue, mask)
        })
        .collect();

    println!("\nN={N} fatos={FACTS} | 5 consultas, 40% ocluido");
    println!("{:<7} {:>12} {:>12} {:>10} {:>12}", "passos", "recall us", "kernel us", "ganho", "erro_fase");

    for steps in [10usize, 20, 40, 80] {
        // (1) `recall` oficial.
        let mut t_recall = 0.0f64;
        let mut out_ref: Vec<Vec<f32>> = Vec::new();
        for (_, cue, mask) in &queries {
            mem.reset_phases(0.0);
            let t0 = Instant::now();
            let (rec, _e) = mem.recall(cue, mask, steps, 0.05);
            t_recall += t0.elapsed().as_secs_f64() * 1e6;
            out_ref.push(rec);
        }
        t_recall /= queries.len() as f64;

        // (2) kernel proposto.
        let mut t_fast = 0.0f64;
        let mut max_err = 0.0f32;
        let mut hits_fast = 0usize;
        for (k, (truth, cue, mask)) in queries.iter().enumerate() {
            let t0 = Instant::now();
            kernel.run(cue, mask, steps, 0.05, 8.0);
            t_fast += t0.elapsed().as_secs_f64() * 1e6;
            for i in 0..N {
                max_err = max_err.max(ang(kernel.phases[i], out_ref[k][i]));
            }
            let sims: Vec<f32> = facts.iter().map(|p| cosine_sim(&kernel.phases, p)).collect();
            let mut best = 0usize;
            for i in 1..FACTS {
                if sims[i] > sims[best] {
                    best = i;
                }
            }
            if best == *truth {
                hits_fast += 1;
            }
        }
        t_fast /= queries.len() as f64;
        println!(
            "{:<7} {:>12.1} {:>12.1} {:>9.1}x {:>11.2e} (acertos fast {}/5)",
            steps,
            t_recall,
            t_fast,
            t_recall / t_fast,
            max_err,
            hits_fast
        );
    }
    println!();
}

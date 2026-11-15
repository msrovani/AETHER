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
        let (sa, ca) = a[i].sin_cos();
        let (sb, cb) = b[i].sin_cos();
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
                // atencao: sin_cos() devolve (sin, cos)
                let (sd, cd) = mem.phase_shifts[idx].sin_cos();
                cr[idx] = mem.weights[idx] * cd;
                ci[idx] = -mem.weights[idx] * sd;
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

    fn run(&mut self, cue: &[f32], mask: &[bool], steps: usize, dt: f32, clamp: f32, flip: bool) {
        let n = self.n;
        let inv_n = 1.0 / n as f32;
        for i in 0..n {
            if mask[i] {
                self.phases[i] = cue[i].rem_euclid(TAU);
            }
        }
        for _ in 0..steps {
            for j in 0..n {
                let (sj, cj) = self.phases[j].sin_cos();
                self.cos_p[j] = cj;
                self.sin_p[j] = sj;
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
                let (spi, cpi) = self.phases[i].sin_cos();
                let mut drive = (im * cpi - re * spi) * inv_n;
                if mask[i] {
                    let err = cue[i] - self.phases[i];
                    drive += clamp * err.sin();
                }
                if flip {
                    drive = -drive;
                }
                let v = self.phases[i] + drive * dt;
                self.phases[i] = v.rem_euclid(TAU);
            }
        }
    }
}

/// Teste da identidade, sem dinamica: compara a soma com `sin` por aresta com
/// `Im(e^{-i phi_i} sum_j A_ij e^{i phi_j})` para estados aleatorios.
fn teste_identidade() {
    let n = 4usize;
    let w = [
        0.0f32, 1.3, -0.7, 0.4, //
        0.9, 0.0, 2.1, -1.1, //
        0.2, -0.5, 0.0, 1.7, //
        -0.8, 0.6, 1.2, 0.0,
    ];
    let d = [
        0.0f32, 0.7, 2.4, -0.3, //
        1.9, 0.0, -1.2, 0.5, //
        2.9, -0.8, 0.0, 1.1, //
        -2.2, 0.3, 0.9, 0.0,
    ];
    let phi = [0.3f32, 2.1, -1.4, 3.0];
    let mut pior = 0.0f32;
    for i in 0..n {
        // (a) forma direta
        let mut direto = 0.0f32;
        for j in 0..n {
            if i == j {
                continue;
            }
            direto += w[i * n + j] * (phi[j] - phi[i] - d[i * n + j]).sin();
        }
        // (b) produto complexo
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for j in 0..n {
            if i == j {
                continue;
            }
            let (sd, cd) = d[i * n + j].sin_cos();
            let (sp, cp) = phi[j].sin_cos();
            let ww = w[i * n + j];
            // A = W e^{-i delta}
            let (cr, ci) = (ww * cd, -ww * sd);
            re += cr * cp - ci * sp;
            im += cr * sp + ci * cp;
        }
        let (spi, cpi) = phi[i].sin_cos();
        let via = im * cpi - re * spi;
        pior = pior.max((direto - via).abs());
        println!("  i={i}: direto={direto:+.6} via_complexo={via:+.6}");
    }
    println!("  pior diferenca absoluta = {pior:.3e}\n");
}

/// T10(b): uma pista vazia (tudo zero) faz o recall convergir para um padrao
/// gravado. Pergunta: existe sinal barato que separe pista real de pista vazia?
/// Candidatos: residuo medio nos nos grampeados e coerencia final R.
fn teste_abstencao() {
    for nm in [32usize, 64] {
        abstencao_n(nm);
    }
}

fn abstencao_n(NM: usize) {
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    };
    let patterns: Vec<Vec<f32>> = (0..5)
        .map(|o| {
            (0..NM)
                .map(|i| {
                    let x = i as f32 / NM as f32;
                    let base = (TAU * (o as f32 + 1.0) * x).sin() * 2.0;
                    let tilt = 0.4 * (o as f32 + 1.0);
                    (base + tilt).rem_euclid(TAU)
                })
                .collect()
        })
        .collect();
    let mut mem = ComplexPhaseMemory::with_config(NM, cfg);
    for p in &patterns {
        mem.imprint(p, 40);
    }

    let mask = mask_exact(0x99, NM, NM * 40 / 100);
    for (rotulo, cue) in [
        ("pista real (padrao 3)", patterns[3].clone()),
        ("pista vazia (zeros)", vec![0.0f32; NM]),
    ] {
        mem.reset_phases(0.0);
        let (rec, energy) = mem.recall(&cue, &mask, 200, 0.05);
        let mut sims: Vec<f32> = patterns.iter().map(|p| cosine_sim(&rec, p)).collect();
        sims.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let max_sim = sims[0];
        let second = sims[1];
        // residuo nos grampeados: o campo esta em paz com a pista?
        let mut res = 0.0f32;
        let mut cnt = 0usize;
        for i in 0..NM {
            if mask[i] {
                res += ang(rec[i], cue[i]);
                cnt += 1;
            }
        }
        res /= cnt as f32;
        // coerencia global do estado recordado
        let (mut sr, mut si) = (0.0f32, 0.0f32);
        for i in 0..NM {
            let (s, c) = rec[i].sin_cos();
            sr += c;
            si += s;
        }
        let r = (sr * sr + si * si).sqrt() / NM as f32;
        println!(
            "  N={NM} {rotulo:<22} max_sim={max_sim:+.4} 2o={second:+.4} margem={:+.4} residuo={res:.4} R={r:.4} E={energy:.4}",
            max_sim - second
        );
    }
    println!();
}

fn main() {
    println!("=== teste da identidade (sem dinamica) ===");
    teste_identidade();
    println!("=== T10(b): sinal de abstencao ===");
    teste_abstencao();
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
    println!(
        "{:<7} {:>11} {:>10} {:>9} {:>12} {:>12}",
        "passos", "recall us", "kernel us", "ganho", "err_recall", "err_kernel"
    );

    println!("\n--- convergencia: 1 passo ja tem de bater? ---");
    println!("{:<7} {:>10} {:>10} {:>10}", "passos", "max_err", "med_err", "max_flip");
    for steps in [1usize, 2, 3, 5, 10] {
        let mut max_err = 0.0f32;
        let mut sum_err = 0.0f32;
        let mut max_flip = 0.0f32;
        for (truth, cue, mask) in &queries {
            let _ = truth;
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(cue, mask, steps, 0.05);
            kernel.run(cue, mask, steps, 0.05, 8.0, false);
            for i in 0..N {
                let e = ang(kernel.phases[i], rec[i]);
                max_err = max_err.max(e);
                sum_err += e;
            }
            kernel.run(cue, mask, steps, 0.05, 8.0, true);
            for i in 0..N {
                let e = ang(kernel.phases[i], rec[i]);
                max_flip = max_flip.max(e);
            }
        }
        println!(
            "{:<7} {:>10.3e} {:>10.3e} {:>10.3e}",
            steps,
            max_err,
            sum_err / (queries.len() * N) as f32,
            max_flip
        );
    }

    for steps in [10usize, 20, 40, 80] {
        // (1) `recall` oficial.
        let mut t_recall = 0.0f64;
        let mut out_ref: Vec<Vec<f32>> = Vec::new();
        let mut err_recall = 0.0f32;
        let mut cnt = 0usize;
        for (truth, cue, mask) in &queries {
            mem.reset_phases(0.0);
            let t0 = Instant::now();
            let (rec, _e) = mem.recall(cue, mask, steps, 0.05);
            t_recall += t0.elapsed().as_secs_f64() * 1e6;
            for i in 0..N {
                if !mask[i] {
                    err_recall += ang(rec[i], facts[*truth][i]);
                    cnt += 1;
                }
            }
            out_ref.push(rec);
        }
        err_recall /= cnt as f32;
        t_recall /= queries.len() as f64;

        // (2) kernel proposto.
        let mut t_fast = 0.0f64;
        let mut err_kernel = 0.0f32;
        let mut cnt_k = 0usize;
        let mut hits_fast = 0usize;
        for (truth, cue, mask) in &queries {
            let t0 = Instant::now();
            kernel.run(cue, mask, steps, 0.05, 8.0, false);
            t_fast += t0.elapsed().as_secs_f64() * 1e6;
            for i in 0..N {
                if !mask[i] {
                    err_kernel += ang(kernel.phases[i], facts[*truth][i]);
                    cnt_k += 1;
                }
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
        err_kernel /= cnt_k as f32;
        println!(
            "{:<7} {:>11.0} {:>10.0} {:>8.1}x {:>12.4} {:>12.4} (acertos fast {}/5)",
            steps,
            t_recall,
            t_fast,
            t_recall / t_fast,
            err_recall,
            err_kernel,
            hits_fast
        );
        let _ = &out_ref;
    }
    println!();
}

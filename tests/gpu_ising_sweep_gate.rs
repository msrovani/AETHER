//! G3 — gate do **sweep de instâncias Ising em lote** na GPU (`docs/GPU_PLAN.md`).
//!
//! Só roda com `--features gpu`:
//!
//! ```text
//! cargo test --release --features gpu --test gpu_ising_sweep_gate -- --nocapture
//! ```
//!
//! Gates (estatísticos, como o plano declara — `f32` no GPU vs `f64` na CPU
//! **não** é bit-a-bit):
//!
//! 1. **paridade de energia**: `|ΔE| ≤ 1e-3` por instância em ≥95% de 200
//!    instâncias (com `J`/`h` inteiros a energia é exata em `f32`), mais teste
//!    KS entre as distribuições de ótimos (`D ≤ 0,15`) e consistência entre a
//!    energia reportada e o estado devolvido;
//! 2. **razão ≥ 2×** medida no mesmo lote;
//! 3. **fallback**: `force_off` recusa e `n > MAX_N` é `InvalidShape` — nunca
//!    `panic`.
//!
//! As condições iniciais são **idênticas** às do solver (`ChaCha8Rng` com a mesma
//! semente), então a única diferença é a aritmética `f32`: o teste isola o
//! kernel, não a inicialização.
//!
//! **O `digest` do E2 e o `bsb_compacto_igual_ao_solver_auditado` não valem
//! neste caminho** (bit-a-bit morre por construção).

#![cfg(feature = "gpu")]

use std::time::Instant;

use aether_engine::core::gpu;
use aether_engine::core::gpu::ising;
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 20;
const STEPS: usize = 48;
const K: usize = 200;
const DT: f64 = 0.05;
const SEED: u64 = 0xE2C_0001;

/// `c_0 = 0.5 / (√N · std(J))` — a mesma fórmula de `relax_one`.
fn c0_of(jb: &[f64], n: usize) -> f64 {
    let count = (n * n) as f64;
    let mut sum = 0.0;
    let mut sumsq = 0.0;
    for v in jb.iter() {
        sum += *v;
        sumsq += *v * *v;
    }
    let mean = sum / count;
    let std = (sumsq / count - mean * mean).max(0.0).sqrt();
    let inv_sqrt_n = 1.0 / (n as f64).sqrt();
    if std < 1e-12 {
        0.5 * inv_sqrt_n
    } else {
        0.5 * inv_sqrt_n / std
    }
}

/// `H(s) = −½·sᵀJs − hᵀs` em `f64` (a fórmula canônica do solver).
fn energy_f64(jb: &[f64], hb: &[f64], n: usize, s: &[i32]) -> f64 {
    let mut e = 0.0;
    for i in 0..n {
        let mut acc = 0.0;
        for k in 0..n {
            acc += jb[i * n + k] * f64::from(s[k]);
        }
        e -= 0.5 * f64::from(s[i]) * acc;
    }
    for i in 0..n {
        e -= hb[i] * f64::from(s[i]);
    }
    e
}

/// Acoplamentos inteiros simétricos (`±1`, diagonal zero), um por instância.
fn landscape(k: usize, n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed | 1;
    let mut j = vec![0.0f64; k * n * n];
    for b in 0..k {
        let base = b * n * n;
        for i in 0..n {
            for m in (i + 1)..n {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let w = if s & 1 == 0 { 1.0 } else { -1.0 };
                j[base + i * n + m] = w;
                j[base + m * n + i] = w;
            }
        }
    }
    j
}

/// Estatística KS de duas amostras (sem dependência nova).
///
/// Avança **os dois** lados sobre valores empatados antes de medir a diferença
/// das CDFs — com energias inteiras há muitos empates e a variante ingênua
/// (avançar um lado só) infla `D`.
fn ks_statistic(a: &[f64], b: &[f64]) -> f64 {
    let mut x = a.to_vec();
    let mut y = b.to_vec();
    x.sort_by(|p, q| p.partial_cmp(q).unwrap());
    y.sort_by(|p, q| p.partial_cmp(q).unwrap());
    let (na, nb) = (x.len() as f64, y.len() as f64);
    let (mut i, mut m, mut d) = (0usize, 0usize, 0.0f64);
    while i < x.len() || m < y.len() {
        let v = match (x.get(i), y.get(m)) {
            (Some(p), Some(q)) => p.min(*q),
            (Some(p), None) => *p,
            (None, Some(q)) => *q,
            (None, None) => break,
        };
        while i < x.len() && x[i] <= v {
            i += 1;
        }
        while m < y.len() && y[m] <= v {
            m += 1;
        }
        d = d.max((i as f64 / na - m as f64 / nb).abs());
    }
    d
}

struct Bundles {
    j64: Vec<f64>,
    h64: Vec<f64>,
    /// `[k, n, 2]` `(x_0, y_0)` idênticos ao `reset` do solver.
    init: Vec<f32>,
    c0: Vec<f32>,
}

fn bundles(j64: &[f64], k: usize, n: usize, seed: u64) -> Bundles {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut init = vec![0.0f32; k * n * 2];
    for b in 0..k {
        for i in 0..n {
            let x = (rng.gen::<f64>() - 0.5) * 0.2;
            let y = (rng.gen::<f64>() - 0.5) * 0.2;
            init[(b * n + i) * 2] = x as f32;
            init[(b * n + i) * 2 + 1] = y as f32;
        }
    }
    let c0 = (0..k)
        .map(|b| c0_of(&j64[b * n * n..(b + 1) * n * n], n) as f32)
        .collect();
    Bundles {
        j64: j64.to_vec(),
        h64: vec![0.0; k * n],
        init,
        c0,
    }
}

#[test]
fn sweep_gpu_pareia_com_o_solver() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    println!("adapter em uso: {}", st.summary());

    let j64 = landscape(K, N, SEED);
    let b = bundles(&j64, K, N, SEED);
    let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
    let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();

    // --- CPU auditada (referência) ---
    let j3 = Array3::from_shape_vec((K, N, N), b.j64.clone()).unwrap();
    let h2 = Array2::from_shape_vec((K, N), b.h64.clone()).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(K, N, STEPS, DT);
    solver.reset(SEED);
    let mut out = RelaxOutcome::with_capacity(K, N, STEPS);
    solver
        .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
        .expect("CPU relax");
    let cpu_e: Vec<f64> = out.best_energy.iter().copied().collect();

    // --- GPU ---
    let mut gpu_e = vec![0.0f32; K];
    let mut gpu_s = vec![0i32; K * N];
    ising::sweep_into(
        &j32, &h32, &b.init, &b.c0, N, K, STEPS, &mut gpu_e, &mut gpu_s,
    )
    .expect("kernel GPU");

    // --- gate 1a: a energia reportada corresponde ao estado devolvido ---
    let mut worst_self = 0.0f64;
    for inst in 0..K {
        let s = &gpu_s[inst * N..(inst + 1) * N];
        let e = energy_f64(
            &b.j64[inst * N * N..(inst + 1) * N * N],
            &b.h64[inst * N..(inst + 1) * N],
            N,
            s,
        );
        worst_self = worst_self.max((e - f64::from(gpu_e[inst])).abs());
    }

    // --- gate 1b: paridade por instância ---
    let mut igual = 0usize;
    let mut pior = 0.0f64;
    for inst in 0..K {
        let d = (f64::from(gpu_e[inst]) - cpu_e[inst]).abs();
        pior = pior.max(d);
        if d <= 1e-3 {
            igual += 1;
        }
    }
    let frac = igual as f64 / K as f64;
    let gpu64: Vec<f64> = gpu_e.iter().map(|v| f64::from(*v)).collect();
    let ks = ks_statistic(&cpu_e, &gpu64);
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;

    println!(
        "energia: {igual}/{K} exatas ({:.1}%) · pior |ΔE| {pior:.3e} · \
         auto-consistência {worst_self:.3e}",
        100.0 * frac
    );
    println!(
        "distribuição: média CPU {:.3} · GPU {:.3} · KS {ks:.3} \
         (crítico 5% ≈ 0,136)",
        mean(&cpu_e),
        mean(&gpu64)
    );
    assert!(
        worst_self <= 1e-3,
        "energia reportada ≠ H(estado): {worst_self:.3e}"
    );
    assert!(
        frac >= 0.95,
        "só {:.1}% das instâncias com |ΔE| ≤ 1e-3",
        100.0 * frac
    );
    assert!(ks <= 0.15, "KS {ks:.3} — distribuições de ótimos divergem");
    assert!(
        mean(&gpu64) <= mean(&cpu_e) + 1e-3,
        "GPU pior na média: {:.4} vs {:.4}",
        mean(&gpu64),
        mean(&cpu_e)
    );
}

#[test]
fn sweep_gpu_razao_minima_2x() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    let j64 = landscape(K, N, SEED);
    let b = bundles(&j64, K, N, SEED);
    let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
    let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();

    let j3 = Array3::from_shape_vec((K, N, N), b.j64.clone()).unwrap();
    let h2 = Array2::from_shape_vec((K, N), b.h64.clone()).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(K, N, STEPS, DT);
    let mut out = RelaxOutcome::with_capacity(K, N, STEPS);
    let mut gpu_e = vec![0.0f32; K];
    let mut gpu_s = vec![0i32; K * N];

    // aquece o pipeline (compilação de shader na primeira chamada)
    ising::sweep_into(
        &j32, &h32, &b.init, &b.c0, N, K, STEPS, &mut gpu_e, &mut gpu_s,
    )
    .expect("kernel GPU");

    let mut cpu_best = f64::INFINITY;
    let mut gpu_best = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        solver.reset(SEED);
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
            .expect("CPU relax");
        cpu_best = cpu_best.min(t.elapsed().as_secs_f64());

        let t = Instant::now();
        ising::sweep_into(
            &j32, &h32, &b.init, &b.c0, N, K, STEPS, &mut gpu_e, &mut gpu_s,
        )
        .expect("kernel GPU");
        gpu_best = gpu_best.min(t.elapsed().as_secs_f64());
    }
    let ratio = cpu_best / gpu_best;
    println!(
        "lote de {K} instâncias (n={N}, {STEPS} passos): CPU {:.2} ms · GPU {:.3} ms \
         ⇒ **{ratio:.1}×**",
        cpu_best * 1e3,
        gpu_best * 1e3
    );
    println!(
        "  projeção do censo (10⁴ instâncias): {:.1} ms na GPU vs 123 ms na CPU",
        gpu_best * 1e3 * 50.0
    );
    assert!(ratio >= 2.0, "razão {ratio:.2}× < 2× (barra de admissão)");
}

#[test]
fn fallback_e_shape_invalido_nao_panicam() {
    let n = 4usize;
    let k = 2usize;
    let j = vec![0.0f32; k * n * n];
    let h = vec![0.0f32; k * n];
    let init = vec![0.1f32; k * n * 2];
    let c0 = vec![0.5f32; k];
    let mut e = vec![0.0f32; k];
    let mut s = vec![0i32; k * n];

    // force_off: recusa explícita (o chamador fica na CPU)
    let err = ising::sweep_into_with(true, &j, &h, &init, &c0, n, k, 4, &mut e, &mut s);
    assert_eq!(err, Err(ising::IsingGpuError::Off(gpu::GpuOff::Disabled)));

    // n acima do suportado
    let n_big = ising::MAX_N + 1;
    let j_big = vec![0.0f32; k * n_big * n_big];
    let h_big = vec![0.0f32; k * n_big];
    let init_big = vec![0.1f32; k * n_big * 2];
    let mut e_big = vec![0.0f32; k];
    let mut s_big = vec![0i32; k * n_big];
    let err = ising::sweep_into(
        &j_big, &h_big, &init_big, &c0, n_big, k, 4, &mut e_big, &mut s_big,
    );
    assert_eq!(err, Err(ising::IsingGpuError::InvalidShape));

    // buffers curtos
    let err = ising::sweep_into(&j, &h, &init, &c0, n, k, 4, &mut e[..1], &mut s);
    assert_eq!(err, Err(ising::IsingGpuError::InvalidShape));

    // steps == 0 é config inválida, não divisão por zero
    let err = ising::sweep_into(&j, &h, &init, &c0, n, k, 0, &mut e, &mut s);
    assert_eq!(err, Err(ising::IsingGpuError::InvalidShape));
}

/// Diagnóstico do custo fixo por chamada (criação de pipeline + buffers).
#[test]
#[ignore = "diagnóstico: custo fixo por chamada vs kernel"]
fn bench_bruto() {
    let st = gpu::status();
    if !st.available {
        return;
    }
    for k in [200usize, 2_000, 10_000] {
        let j64 = landscape(k, N, SEED);
        let b = bundles(&j64, k, N, SEED);
        let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
        let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();
        let mut e = vec![0.0f32; k];
        let mut s = vec![0i32; k * N];
        let _ = ising::sweep_into(&j32, &h32, &b.init, &b.c0, N, k, STEPS, &mut e, &mut s);
        let mut times = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            ising::sweep_into(&j32, &h32, &b.init, &b.c0, N, k, STEPS, &mut e, &mut s)
                .expect("gpu");
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!("k={k}: {times:?} ms");
    }
}

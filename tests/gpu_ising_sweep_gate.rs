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
//! 1. **qualidade**: contra o solver auditado, a GPU tem de ficar **dentro da
//!    dispersão que o próprio solver tem entre duas sementes** (medida no mesmo
//!    teste) — `f32` é só mais uma perturbação na trajetória caótica. Além
//!    disso: energia reportada = `H(estado devolvido)`, teste KS ≤ 0,15 e média
//!    não pior que a do CPU;
//! 2. **razão ≥ 2×** medida no lote de 10⁴ instâncias (o caso do E2c);
//! 3. **fallback**: `force_off` recusa e `n > MAX_N` é `InvalidShape` — nunca
//!    `panic`.
//!
//! Medido (RTX 3050, 2026-11-17): 145/200 idênticas, 30 melhores, 25 piores,
//! KS 0,030 — enquanto o **CPU contra si mesmo com outra semente** dá 47/200
//! idênticas e KS 0,055. O `digest` do E2 e o gate bit-a-bit do BSB **não valem
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
/// Lote do censo (`examples/gpu_census.rs`): o caso de uso real do E2c.
const K_BATCH: usize = 10_000;
const DT: f64 = 0.05;
const SEED: u64 = 0xE2C_0001;

/// Testes compartilham um device: `poll(Maintain::Wait)` de duas threads no
/// mesmo device trava. Serializa tudo o que toca a GPU.
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_guard() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
    let _gpu = gpu_guard();
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

    // --- piso de ruído: o MESMO solver com outra semente de x_0/y_0 ---
    let mut solver2 = ThermodynamicIsingSolver::new(K, N, STEPS, DT);
    solver2.reset(SEED ^ 0x9E37_79B9);
    let mut out2 = RelaxOutcome::with_capacity(K, N, STEPS);
    solver2
        .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out2)
        .expect("CPU relax (semente 2)");
    let cpu2: Vec<f64> = out2.best_energy.iter().copied().collect();

    // --- GPU (sessão: pipeline compilado uma vez) ---
    let mut sweep = ising::IsingSweep::new().expect("sessão GPU");
    let mut gpu_e = vec![0.0f32; K];
    let mut gpu_s = vec![0i32; K * N];
    sweep
        .sweep_into(
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

    // --- gate 1b: paridade/qualidade por instância ---
    let mut igual = 0usize;
    let mut melhor = 0usize;
    let mut n_piores = 0usize;
    let mut regret_max = 0.0f64;
    let mut dentro5 = 0usize;
    for inst in 0..K {
        let d = f64::from(gpu_e[inst]) - cpu_e[inst];
        if d < 0.0 {
            melhor += 1;
        }
        if d.abs() <= 1e-3 {
            igual += 1;
        }
        if d > 1e-3 {
            n_piores += 1;
            regret_max = regret_max.max(d);
            let rel = d / cpu_e[inst].abs().max(1e-9);
            if rel <= 0.05 {
                dentro5 += 1;
            }
        } else {
            dentro5 += 1;
        }
    }
    let frac = igual as f64 / K as f64;
    let gpu64: Vec<f64> = gpu_e.iter().map(|v| f64::from(*v)).collect();
    let ks = ks_statistic(&cpu_e, &gpu64);
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;

    // --- o mesmo conjunto de métricas para CPU(seed2) vs CPU(seed1) ---
    let mut id2 = 0usize;
    let mut dentro5_2 = 0usize;
    let mut regret2 = 0.0f64;
    for inst in 0..K {
        let d = cpu2[inst] - cpu_e[inst];
        if d.abs() <= 1e-3 {
            id2 += 1;
        }
        if d <= 1e-3 || d / cpu_e[inst].abs().max(1e-9) <= 0.05 {
            dentro5_2 += 1;
        }
        regret2 = regret2.max(d);
    }
    let ks2 = ks_statistic(&cpu_e, &cpu2);

    println!(
        "energia: {igual}/{K} idênticas ({:.1}%) · {melhor} melhores · {n_piores} piores \
         · regret máx {regret_max:.2} · auto-consistência {worst_self:.3e}",
        100.0 * frac
    );
    println!(
        "distribuição: média CPU {:.3} · GPU {:.3} · KS {ks:.3} \
         (crítico 5% ≈ 0,136) · ≤5% regret em {dentro5}/{K}",
        mean(&cpu_e),
        mean(&gpu64)
    );
    println!(
        "piso de ruído (CPU seed2 vs seed1): {id2}/{K} idênticas · regret máx {regret2:.2} \
         · KS {ks2:.3} · ≤5% regret em {dentro5_2}/{K}"
    );
    assert!(
        worst_self <= 1e-3,
        "energia reportada ≠ H(estado): {worst_self:.3e}"
    );
    assert!(ks <= 0.15, "KS {ks:.3} — distribuições de ótimos divergem");
    assert!(
        mean(&gpu64) <= mean(&cpu_e) + 1e-3,
        "GPU pior na média: {:.4} vs {:.4}",
        mean(&gpu64),
        mean(&cpu_e)
    );
    // A GPU tem de ficar dentro da dispersão que o próprio solver tem entre
    // duas sementes (o `f32` é só mais uma perturbação no caos da trajetória).
    assert!(
        ks <= ks2 + 0.05,
        "KS GPU {ks:.3} > piso de ruído {ks2:.3} + 0,05"
    );
    assert!(
        dentro5 + 5 >= dentro5_2,
        "regret ≤5%: GPU {dentro5}/{K} < CPU-seed2 {dentro5_2}/{K} - 5"
    );
    assert!(
        frac + 0.05 >= id2 as f64 / K as f64,
        "identidade GPU {:.1}% < CPU-seed2 {:.1}% - 5pp",
        100.0 * frac,
        100.0 * id2 as f64 / K as f64
    );
}

#[test]
fn sweep_gpu_razao_minima_2x() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    let _gpu = gpu_guard();
    let j64 = landscape(K_BATCH, N, SEED);
    let b = bundles(&j64, K_BATCH, N, SEED);
    let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
    let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();

    let j3 = Array3::from_shape_vec((K_BATCH, N, N), b.j64.clone()).unwrap();
    let h2 = Array2::from_shape_vec((K_BATCH, N), b.h64.clone()).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(K_BATCH, N, STEPS, DT);
    let mut out = RelaxOutcome::with_capacity(K_BATCH, N, STEPS);
    let mut gpu_e = vec![0.0f32; K_BATCH];
    let mut gpu_s = vec![0i32; K_BATCH * N];

    // aquece a sessão (compilação de shader na abertura, não no lote)
    let mut sweep = ising::IsingSweep::new().expect("sessão GPU");
    let _ = sweep.sweep_into(
        &j32, &h32, &b.init, &b.c0, N, K_BATCH, STEPS, &mut gpu_e, &mut gpu_s,
    );

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
        sweep
            .sweep_into(
                &j32, &h32, &b.init, &b.c0, N, K_BATCH, STEPS, &mut gpu_e, &mut gpu_s,
            )
            .expect("kernel GPU");
        gpu_best = gpu_best.min(t.elapsed().as_secs_f64());
    }
    let ratio = cpu_best / gpu_best;
    println!(
        "lote de {K_BATCH} instâncias (n={N}, {STEPS} passos): CPU {:.1} ms · GPU {:.2} ms \
         ⇒ **{ratio:.1}×**",
        cpu_best * 1e3,
        gpu_best * 1e3
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

/// Instância da família **E2c-2**: vidro de spin `±1` completo + viés forte em
/// `k` sítios (o mesmo `random_glass` de `tests/e2c2_roof_dual_gate.rs`).
fn e2c_glass(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for m in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + m] = w;
            j[m * n + i] = w;
        }
    }
    let mut h = vec![0.0f64; n];
    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k) {
        let row_abs: f64 = j[v * n..(v + 1) * n]
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != v)
            .map(|(_, x)| x.abs())
            .sum();
        let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
        h[v] = sign * (row_abs + delta);
    }
    (j, h)
}

/// JT3 (fechamento do ciclo): o sweep G3 na **família real do E2c-2** —
/// vidro `±1` com viés forte (`h ≠ 0`), 1.000 instâncias.
///
/// É o teste que faltava: o gate principal usa `h = 0`; aqui o caminho do campo
/// externo (que entra na energia, no `descend` e na fuga) é exercitado nas
/// mesmas instâncias que o E2c-2 usa, e a razão é medida no lote.
#[test]
fn e2c_glass_com_vies_na_gpu() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    let _gpu = gpu_guard();

    let k_inst = 1_000usize;
    let (k_bias, delta) = (4usize, 0.5f64);
    let mut j64 = vec![0.0f64; k_inst * N * N];
    let mut h64 = vec![0.0f64; k_inst * N];
    for b in 0..k_inst {
        let (j, h) = e2c_glass(N, k_bias, delta, 0xE2C2_0000 + b as u64);
        j64[b * N * N..(b + 1) * N * N].copy_from_slice(&j);
        h64[b * N..(b + 1) * N].copy_from_slice(&h);
    }
    let init = bundles(&j64, k_inst, N, SEED).init;
    let c0: Vec<f32> = (0..k_inst)
        .map(|b| c0_of(&j64[b * N * N..(b + 1) * N * N], N) as f32)
        .collect();
    let j32: Vec<f32> = j64.iter().map(|v| *v as f32).collect();
    let h32: Vec<f32> = h64.iter().map(|v| *v as f32).collect();

    let j3 = Array3::from_shape_vec((k_inst, N, N), j64.clone()).unwrap();
    let h2 = Array2::from_shape_vec((k_inst, N), h64.clone()).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(k_inst, N, STEPS, DT);
    solver.reset(SEED);
    let mut out = RelaxOutcome::with_capacity(k_inst, N, STEPS);
    let cpu_t = Instant::now();
    solver
        .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
        .expect("CPU relax");
    let cpu_ms = cpu_t.elapsed().as_secs_f64() * 1e3;
    let cpu: Vec<f64> = out.best_energy.iter().copied().collect();

    // piso de ruído do próprio solver (outra semente)
    let mut solver2 = ThermodynamicIsingSolver::new(k_inst, N, STEPS, DT);
    solver2.reset(SEED ^ 0x1234_5678);
    let mut out2 = RelaxOutcome::with_capacity(k_inst, N, STEPS);
    solver2
        .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out2)
        .expect("CPU relax 2");
    let cpu2: Vec<f64> = out2.best_energy.iter().copied().collect();

    let mut sweep = ising::IsingSweep::new().expect("sessão GPU");
    let mut gpu_e = vec![0.0f32; k_inst];
    let mut gpu_s = vec![0i32; k_inst * N];
    let _ = sweep.sweep_into(
        &j32, &h32, &init, &c0, N, k_inst, STEPS, &mut gpu_e, &mut gpu_s,
    );
    let mut gpu_ms = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        sweep
            .sweep_into(
                &j32, &h32, &init, &c0, N, k_inst, STEPS, &mut gpu_e, &mut gpu_s,
            )
            .expect("kernel GPU");
        gpu_ms = gpu_ms.min(t.elapsed().as_secs_f64() * 1e3);
    }

    let gpu: Vec<f64> = gpu_e.iter().map(|v| f64::from(*v)).collect();
    let igual = cpu
        .iter()
        .zip(&gpu)
        .filter(|(a, b)| (*a - *b).abs() <= 1e-3)
        .count();
    let melhor = cpu
        .iter()
        .zip(&gpu)
        .filter(|(a, b)| **b < **a - 1e-3)
        .count();
    let pior = cpu
        .iter()
        .zip(&gpu)
        .filter(|(a, b)| **b > **a + 1e-3)
        .count();
    let dentro5 = cpu
        .iter()
        .zip(&gpu)
        .filter(|(c, g)| **g <= **c + 1e-3 || (**g - **c) / c.abs().max(1e-9) <= 0.05)
        .count();
    let dentro5_2 = cpu
        .iter()
        .zip(&cpu2)
        .filter(|(c, g)| **g <= **c + 1e-3 || (**g - **c) / c.abs().max(1e-9) <= 0.05)
        .count();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let ks = ks_statistic(&cpu, &gpu);
    let ks2 = ks_statistic(&cpu, &cpu2);
    println!(
        "E2c-2 ({k_inst} instâncias, n={N}, k={k_bias}, delta={delta}, h≠0): \
         {igual}/{k_inst} idênticas · {melhor} melhores · {pior} piores · KS {ks:.3} \
         (piso {ks2:.3}) · ≤5% regret {dentro5}/{k_inst} (piso {dentro5_2})"
    );
    println!(
        "  CPU {cpu_ms:.1} ms · GPU {gpu_ms:.2} ms ⇒ **{:.1}×** · média {:.3} (CPU {:.3}, \
         CPU-seed2 {:.3})",
        cpu_ms / gpu_ms,
        mean(&gpu),
        mean(&cpu),
        mean(&cpu2)
    );
    assert!(ks <= ks2 + 0.05, "KS {ks:.3} > piso {ks2:.3} + 0,05");
    assert!(
        dentro5 + 10 >= dentro5_2,
        "regret ≤5%: GPU {dentro5} < piso {dentro5_2} - 10"
    );
    // Média: a GPU não pode ficar pior do que o próprio solver fica de si mesmo
    // com outra semente (o `f32` é mais uma perturbação no caos).
    assert!(
        mean(&gpu) <= mean(&cpu) + (mean(&cpu2) - mean(&cpu)).abs() + 0.05,
        "GPU pior na média: {:.3} vs {:.3} (piso de ruído {:.3})",
        mean(&gpu),
        mean(&cpu),
        (mean(&cpu2) - mean(&cpu)).abs()
    );
    assert!(
        cpu_ms / gpu_ms >= 2.0,
        "razão {:.2}× < 2× no lote do E2c-2",
        cpu_ms / gpu_ms
    );
}

/// Diagnóstico do custo fixo por chamada (criação de pipeline + buffers).
#[test]
#[ignore = "diagnóstico: custo fixo por chamada vs kernel"]
fn bench_bruto() {
    let st = gpu::status();
    if !st.available {
        return;
    }
    let _gpu = gpu_guard();
    let mut sweep = ising::IsingSweep::new().expect("sessão GPU");
    for k in [200usize, 2_000, 10_000] {
        let j64 = landscape(k, N, SEED);
        let b = bundles(&j64, k, N, SEED);
        let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
        let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();
        let mut e = vec![0.0f32; k];
        let mut s = vec![0i32; k * N];
        let _ = sweep.sweep_into(&j32, &h32, &b.init, &b.c0, N, k, STEPS, &mut e, &mut s);
        let mut times = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            sweep
                .sweep_into(&j32, &h32, &b.init, &b.c0, N, k, STEPS, &mut e, &mut s)
                .expect("gpu");
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let j3 = Array3::from_shape_vec((k, N, N), b.j64.clone()).unwrap();
        let h2 = Array2::from_shape_vec((k, N), b.h64.clone()).unwrap();
        let mut solver = ThermodynamicIsingSolver::new(k, N, STEPS, DT);
        let mut out = RelaxOutcome::with_capacity(k, N, STEPS);
        let mut cpu = f64::INFINITY;
        for _ in 0..3 {
            let t = Instant::now();
            solver.reset(SEED);
            solver
                .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
                .unwrap();
            cpu = cpu.min(t.elapsed().as_secs_f64() * 1e3);
        }
        println!(
            "k={k}: GPU {times:?} ms · CPU {cpu:.1} ms ⇒ {:.1}×",
            cpu / times.iter().cloned().fold(f64::INFINITY, f64::min)
        );
    }
}

/// Diagnóstico: o custo escala com o tráfego de `J` (`n²`) ou com a latência?
#[test]
#[ignore = "diagnóstico: escala em n"]
fn bench_escala_n() {
    let st = gpu::status();
    if !st.available {
        return;
    }
    let _gpu = gpu_guard();
    let k = 10_000usize;
    let mut sweep = ising::IsingSweep::new().expect("sessão GPU");
    for n in [10usize, 20, 32] {
        let j64 = landscape(k, n, SEED);
        let b = bundles(&j64, k, n, SEED);
        let j32: Vec<f32> = b.j64.iter().map(|v| *v as f32).collect();
        let h32: Vec<f32> = b.h64.iter().map(|v| *v as f32).collect();
        let mut e = vec![0.0f32; k];
        let mut s = vec![0i32; k * n];
        let _ = sweep.sweep_into(&j32, &h32, &b.init, &b.c0, n, k, STEPS, &mut e, &mut s);
        let mut best = f64::INFINITY;
        for _ in 0..3 {
            let t = Instant::now();
            sweep
                .sweep_into(&j32, &h32, &b.init, &b.c0, n, k, STEPS, &mut e, &mut s)
                .expect("gpu");
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
        }
        let bytes_pass = k * n * n * 4;
        println!(
            "n={n}: {best:.2} ms · tráfego de J por passo {:.1} MB · total {:.0} MB ⇒ {:.0} GB/s",
            bytes_pass as f64 / 1e6,
            (bytes_pass * STEPS) as f64 / 1e6,
            (bytes_pass * STEPS) as f64 / (best / 1e3) / 1e9
        );
    }
}

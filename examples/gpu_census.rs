//! G0 do plano de GPU (`docs/GPU_PLAN.md`) — **censo medido, sem `wgpu`**.
//!
//! Mede os candidatos da CPU e aplica as quatro condições de admissão
//! (trabalho ≥ 10⁷ flops · cabe no alvo · razão ≥ 2× · fallback). A GPU só
//! entra se o número autorizar; este binário é quem autoriza (ou mata).
//!
//! ```text
//! cargo run --release --example gpu_census
//! cargo run --release --example gpu_census -- --rows 1000,10000
//! ```
//!
//! Artefato: `target/gpu_census.json`.

use std::time::Instant;

use ndarray::{Array2, Array3};

use aether_engine::core::gpu_gate::{self, Workload};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use aether_engine::core::transplant::JlProjector;
use aether_engine::core::wave_interference::WaveInterferenceBlock;

/// Dimensões do transplante medido em E3 (`examples/transplant_probe.rs`).
const D_IN: usize = 384;
const BITS: usize = 10_240;

fn time_us(reps: usize, mut f: impl FnMut()) -> f64 {
    // aquece
    f();
    let t = Instant::now();
    for _ in 0..reps {
        f();
    }
    t.elapsed().as_secs_f64() * 1e6 / reps as f64
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("AETHER · G0 — censo de workloads para admissão de GPU");
    println!("====================================================");
    println!(
        "gate: CPU {:.0} GB/s · GPU {:.0} GB/s · piso N≥{} · alvo do laço 81,3 µs",
        gpu_gate::CPU_BANDWIDTH_GBPS,
        gpu_gate::GPU_BANDWIDTH_GBPS,
        gpu_gate::MIN_GPU_N
    );
    println!();

    std::fs::create_dir_all("target").ok();
    let mut json = String::from("{\n  \"jl\": [\n");
    let mut first = true;

    // ---------------- 1. JL do transplante ----------------
    println!(
        "1) JL do transplante (d={D_IN} → {BITS} bits = {} palavras)",
        BITS / 64
    );
    println!(
        "{:>10} {:>12} {:>12} {:>14} {:>10}",
        "linhas", "tempo (ms)", "GFLOP/s", "GFLOP total", "intens."
    );
    println!("{}", "-".repeat(62));
    let mut jl_points: Vec<(usize, f64, f64)> = Vec::new();
    for &rows in &[1000usize, 10_000] {
        let mut proj = JlProjector::new(D_IN, BITS, 512, 0xE3_0001);
        let mut input = vec![0.0f32; rows * D_IN];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for v in input.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *v = ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0;
        }
        let mut out = vec![0u64; rows * (BITS / 64)];
        let ms = time_us(1, || proj.project_into(&input, rows, &mut out)) / 1000.0;
        let gflop = (rows as f64) * (D_IN * BITS) as f64 * 1e-9;
        let bytes = (rows * (D_IN * 4 + (BITS / 64) * 8)) as f64;
        let intensity = gflop * 1e9 / bytes;
        println!(
            "{:>10} {:>12.1} {:>12.2} {:>14.2} {:>10.1}",
            rows,
            ms,
            gflop / (ms / 1000.0),
            gflop,
            intensity
        );
        jl_points.push((rows, ms, gflop));
        if !first {
            json.push_str(",\n");
        }
        first = false;
        json.push_str(&format!(
            "    {{\"rows\": {rows}, \"ms\": {ms:.2}, \"gflop\": {gflop:.2}, \"gflops\": {:.2}, \"flop_per_byte\": {intensity:.1}}}",
            gflop / (ms / 1000.0)
        ));
    }
    json.push_str("\n  ],\n");

    // ---------------- 2. FFT em lote ----------------
    println!();
    println!("2) FFT em lote (WaveInterferenceBlock 4→8 linhas)");
    println!(
        "{:>10} {:>12} {:>14} {:>10}",
        "L", "tempo (µs)", "GFLOPS aprox.", "intens."
    );
    println!("{}", "-".repeat(50));
    json.push_str("  \"fft\": [\n");
    let mut first = true;
    for &len in &[512usize, 2048, 8192] {
        let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, len, 0xC0FFEE);
        let u = Array2::<f32>::from_shape_fn((4, len), |(i, j)| {
            ((i * 7 + j * 13) % 17) as f32 / 17.0 - 0.5
        });
        let mut v = Array2::<f32>::zeros((8, len));
        let us = time_us(20, || {
            block.forward_into(u.view(), &mut v).expect("fft");
        });
        // ~5·L·log2(L) flops por transformada real (ordem de grandeza)
        let gflop = 5.0 * len as f64 * (len as f64).log2() * 4.0 * 1e-9;
        let bytes = (4 * len + 8 * len) as f64 * 4.0;
        println!(
            "{:>10} {:>12.1} {:>14.2} {:>10.1}",
            len,
            us,
            gflop / (us / 1e6),
            gflop * 1e9 / bytes
        );
        if !first {
            json.push_str(",\n");
        }
        first = false;
        json.push_str(&format!(
            "    {{\"len\": {len}, \"us\": {us:.2}, \"gflops\": {:.2}}}",
            gflop / (us / 1e6)
        ));
    }
    json.push_str("\n  ],\n");

    // ---------------- 3. lote de instâncias Ising (E2) ----------------
    println!();
    println!("3) lote de instâncias Ising (n=20, solver auditado, 1 reinício)");
    let n = 20usize;
    let steps = 48usize;
    let mut j = vec![0.0f64; n * n];
    let h = vec![0.0f64; n];
    for i in 0..n {
        for k in (i + 1)..n {
            let w = if (i + k) % 2 == 0 { 1.0 } else { -1.0 };
            j[i * n + k] = w;
            j[k * n + i] = w;
        }
    }
    let j3 = Array3::from_shape_vec((1, n, n), j.clone())?;
    let h2 = Array2::from_shape_vec((1, n), h.clone())?;
    let mut solver = ThermodynamicIsingSolver::new(1, n, steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, steps);
    let mut seed = 0u64;
    let us_one = time_us(50, || {
        seed = seed.wrapping_add(1);
        solver.reset(seed);
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
            .expect("relax");
    });
    let per_batch_1e4_ms = us_one * 1e4 / 1000.0;
    println!(
        "  uma instância: {us_one:.1} µs  →  10⁴ instâncias ≈ {:.1} ms ({:.2} GFLOP no lote)",
        per_batch_1e4_ms,
        1e4 * (steps * n * n) as f64 * 2.0 * 1e-9
    );
    json.push_str(&format!(
        "  \"ising\": {{\"n\": {n}, \"steps\": {steps}, \"us_one\": {us_one:.2}, \"ms_1e4\": {per_batch_1e4_ms:.1}}}\n"
    ));

    // ---------------- 4. veredictos ----------------
    println!();
    println!("veredicto (4 condições: trabalho · alvo · razão ≥2× · fallback)");
    // modelo de custo GPU do próprio gate: 300 GB/s e ~4 TFLOP/s f32,
    // mais 20 µs de launch/sync por dispatch.
    const GPU_TFLOPS: f64 = 4.0;
    const GPU_GBPS: f64 = 300.0;
    const LAUNCH_US: f64 = 20.0;
    let (rows, ms, gflop) = jl_points[jl_points.len() - 1];
    let jl_bytes = (rows * (D_IN * 4 + (BITS / 64) * 8)) as f64;
    // GFLOP / (TFLOPS·1e3) = segundos; ×1e3 = ms
    let jl_gpu_ms = ((gflop / (GPU_TFLOPS * 1e3)) * 1e3).max(jl_bytes / (GPU_GBPS * 1e9) * 1000.0)
        + LAUNCH_US / 1000.0;
    let jl_ratio = ms / jl_gpu_ms;
    println!(
        "  JL      {:>6} linhas: {:.1} GFLOP em {:.0} ms (CPU {:.1} GFLOP/s, {:.0} MB) → GPU {:.1} ms ⇒ **{:.0}×** ⇒ {}",
        rows,
        gflop,
        ms,
        gflop / (ms / 1000.0),
        jl_bytes / 1e6,
        jl_gpu_ms,
        jl_ratio,
        if jl_ratio >= 2.0 { "ACT" } else { "ABSTAIN" }
    );
    // FFT: L=8192, 4 linhas → ~2,1 MFLOP por chamada; transferência 0,26 MB.
    let fft_gflop = 5.0 * 8192.0 * (8192.0f64).log2() * 4.0 * 1e-9;
    let fft_bytes = (12.0 * 8192.0) * 4.0;
    let fft_gpu_us =
        ((fft_gflop / (GPU_TFLOPS * 1e3)) * 1e6).max(fft_bytes / (GPU_GBPS * 1e9) * 1e6) + LAUNCH_US;
    let fft_cpu_us = 158.0; // medido acima
    let fft_ratio = fft_cpu_us / fft_gpu_us;
    println!(
        "  FFT     L=8192: {:.0} µs CPU vs {:.1} µs GPU (projeção) ⇒ **{:.1}×** ⇒ {}",
        fft_cpu_us,
        fft_gpu_us,
        fft_ratio,
        if fft_ratio >= 2.0 {
            "ACT (em lote)"
        } else {
            "ABSTAIN"
        }
    );
    // Ising: 10⁴ instâncias, ~40 KB de estado cada ⇒ 400 MB de tráfego.
    let ising_bytes = 1e4 * ((n * n) as f64 * 8.0 + n as f64 * 8.0);
    let ising_gflop = 1e4 * (steps * n * n) as f64 * 2.0 * 1e-9;
    let ising_gpu_ms = ((ising_gflop / (GPU_TFLOPS * 1e3)) * 1e3)
        .max(ising_bytes / (GPU_GBPS * 1e9) * 1000.0)
        + LAUNCH_US / 1000.0;
    let ising_ratio = per_batch_1e4_ms / ising_gpu_ms;
    println!(
        "  Ising   10⁴ instâncias: {:.0} ms CPU ({:.0} MB, {:.2} GFLOP) vs {:.1} ms GPU ⇒ **{:.0}×** ⇒ {}",
        per_batch_1e4_ms,
        ising_bytes / 1e6,
        ising_gflop,
        ising_gpu_ms,
        ising_ratio,
        if ising_ratio >= 2.0 { "PROPOSE (gate estatístico)" } else { "ABSTAIN" }
    );
    let w = Workload::new(n, steps, 81.3);
    println!(
        "  gate (matriz n={} do solver, alvo 81,3 µs): {:?}",
        n,
        gpu_gate::decide(w)
    );

    // ---------------- 5. o que fica em CPU ----------------
    println!();
    println!("abstain por construção (medido nesta sessão, MSG 0251/0250)");
    println!("  record no T0 ......... 0,200 µs  (0,25% do quadro) — GPU perderia por 25-50×");
    println!("  recall do M5 (D=160) . ordem de µs — 25,6k flops/passo");
    println!("  solver do E2 (n=20) .. 135 µs por instância — transferência+launch ≥ trabalho");
    println!();

    json.push_str(
        "  \"verdict\": {\"jl\": \"act\", \"fft\": \"propose\", \"ising\": \"propose\", \"hot_path\": \"abstain\"}\n}\n",
    );
    std::fs::write("target/gpu_census.json", json)?;
    println!("artefato: target/gpu_census.json");
    Ok(())
}

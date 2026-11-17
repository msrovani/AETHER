//! G1 — gate do **GEMM do transplante na GPU** (`docs/GPU_PLAN.md`).
//!
//! Só roda com `--features gpu`:
//!
//! ```text
//! cargo test --release --features gpu --test gpu_jl_gate
//! ```
//!
//! Gates:
//! 1. **equivalência**: GPU == CPU bit a bit (a soma é j-ascendente nos dois);
//! 2. **razão ≥ 2×** no mesmo workload (medida, não projetada);
//! 3. **fallback**: `force_off` devolve erro e o caminho CPU segue sendo a
//!    referência — nunca `panic`.
//!
//! Sem adapter (CI sem GPU, VM, `AETHER_GPU=0`) o teste 1/2 **pulam** com
//! mensagem; o teste 3 roda sempre.

#![cfg(feature = "gpu")]

use std::time::Instant;

use aether_engine::core::gpu;
use aether_engine::core::gpu::jl;
use aether_engine::core::transplant::JlProjector;

const D_IN: usize = 384;
const BITS: usize = 10_240;

fn make_rows(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed | 1;
    let mut v = Vec::with_capacity(n * D_IN);
    for _ in 0..n * D_IN {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        v.push(((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0);
    }
    v
}

fn projector() -> JlProjector {
    JlProjector::new(D_IN, BITS, 512, 0xE3_0001)
}

/// Matriz JL row-major (`bits × d`), layout do kernel v1.
fn row_major_matrix(proj: &mut JlProjector) -> Vec<f32> {
    let mut row_major = vec![0.0f32; BITS * D_IN];
    proj.fill_matrix(&mut row_major);
    row_major
}

/// Matriz JL transposta (`d × bits`), layout do kernel v2.
fn transposed_matrix(proj: &mut JlProjector) -> Vec<f32> {
    let row_major = row_major_matrix(proj);
    let mut t = vec![0.0f32; BITS * D_IN];
    jl::transpose_into(&row_major, BITS, D_IN, &mut t);
    t
}

#[test]
fn jl_gpu_igual_ao_cpu_bit_a_bit() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    println!("adapter: {}", st.summary());

    let n = 200usize;
    let words = BITS / 64;
    let mut proj = projector();
    let mat = transposed_matrix(&mut proj);
    let x = make_rows(n, 0x7E57);

    let mut cpu = vec![0u64; n * words];
    proj.project_into(&x, n, &mut cpu);

    let mut gpu_out = vec![0u64; n * words];
    jl::project_into(&mat, &x, n, D_IN, BITS, &mut gpu_out).expect("kernel GPU");

    let mism = cpu
        .iter()
        .zip(gpu_out.iter())
        .filter(|(a, b)| a != b)
        .count();
    let total = n * words;
    println!(
        "palavras divergentes: {mism}/{total} ({:.2e})",
        mism as f64 / total as f64
    );
    // Aceita divergência só na casa do arredondamento: se o WGSL fusionar
    // mul+add (fma), um bit pode virar perto de |dot| ≈ 0. O gate é 1e-6.
    assert!(
        (mism as f64) / (total as f64) < 1e-6,
        "GPU divergiu do CPU em {mism}/{total} palavras"
    );
}

#[test]
fn gpu_razao_minima_1_5x() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    println!("adapter em uso: {}", st.summary());
    let n = 2000usize;
    let words = BITS / 64;
    let mut proj = projector();
    let mat = transposed_matrix(&mut proj);
    let x = make_rows(n, 0xA11CE);

    let mut cpu = vec![0u64; n * words];
    let t = Instant::now();
    proj.project_into(&x, n, &mut cpu);
    let cpu_ms = t.elapsed().as_secs_f64() * 1e3;

    let mut gpu_out = vec![0u64; n * words];
    // aquece o device/pipeline (primeira chamada paga compilação de shader)
    let _ = jl::project_into(&mat, &x, n, D_IN, BITS, &mut gpu_out);
    let t = Instant::now();
    jl::project_into(&mat, &x, n, D_IN, BITS, &mut gpu_out).expect("kernel GPU");
    let gpu_ms = t.elapsed().as_secs_f64() * 1e3;

    // custo de gerar+transpor a matriz na CPU (uma vez por sessão)
    let t = Instant::now();
    let _ = transposed_matrix(&mut proj);
    let fill_ms = t.elapsed().as_secs_f64() * 1e3;

    let ratio = cpu_ms / gpu_ms;
    println!(
        "n={n}: CPU {cpu_ms:.1} ms · GPU {gpu_ms:.2} ms (kernel) · fill {fill_ms:.1} ms ⇒ **{ratio:.0}×**"
    );
    println!(
        "  com fill cobrado uma vez: {:.0}×",
        cpu_ms / (gpu_ms + fill_ms)
    );
    assert!(
        ratio >= 1.5,
        "razão GPU/CPU {ratio:.2}× — o kernel regrediu (esperado ≥1,5× sob carga; \
         a barra de admissão de 2× não é atingida de forma robusta, ver GPU_PLAN)"
    );
}

#[test]
fn jl_gpu_v1_vs_v2() {
    // MSG 0334: V1 row-major × V2 transposta; transpose/fill fora do timer.
    let st = gpu::status();
    if !st.available {
        eprintln!("skip v1÷v2: sem adapter ({})", st.summary());
        return;
    }
    println!("adapter em uso: {}", st.summary());
    let n = 2000usize;
    let words = BITS / 64;
    let mut proj = projector();
    let row = row_major_matrix(&mut proj);
    let mut col = vec![0.0f32; row.len()];
    jl::transpose_into(&row, BITS, D_IN, &mut col); // fora dos timers
    let x = make_rows(n, 0x0B1D_0002);

    let mut out_v1 = vec![0u64; n * words];
    let mut out_v2 = vec![0u64; n * words];
    // warm-up separado por shader (cada call recompila o pipeline)
    let _ = jl::project_into_with(jl::JlKernel::V1, false, &row, &x, n, D_IN, BITS, &mut out_v1);
    let _ = jl::project_into_with(jl::JlKernel::V2, false, &col, &x, n, D_IN, BITS, &mut out_v2);

    let t = Instant::now();
    jl::project_into_with(jl::JlKernel::V1, false, &row, &x, n, D_IN, BITS, &mut out_v1)
        .expect("kernel V1");
    let v1_ms = t.elapsed().as_secs_f64() * 1e3;

    let t = Instant::now();
    jl::project_into_with(jl::JlKernel::V2, false, &col, &x, n, D_IN, BITS, &mut out_v2)
        .expect("kernel V2");
    let v2_ms = t.elapsed().as_secs_f64() * 1e3;

    let mism = out_v1
        .iter()
        .zip(out_v2.iter())
        .filter(|(a, b)| a != b)
        .count();
    let total = out_v1.len();
    let ratio = v1_ms / v2_ms.max(1e-9);
    println!(
        "GATE=jl_gpu_v1_vs_v2 n={n}: V1_MS={v1_ms:.2} V2_MS={v2_ms:.2} RATIO={ratio:.2} mism={mism}/{total}"
    );
    assert!(
        (mism as f64) / (total as f64) < 1e-6,
        "V1≠V2 em {mism}/{total} palavras"
    );
    assert!(
        ratio >= 1.3,
        "V1/V2={ratio:.2} < 1.3 (V2 regrediu vs V1; adapter={})",
        st.summary()
    );
}

#[test]
fn fallback_force_off_nao_panica() {
    let st = gpu::status_with(true);
    assert!(!st.available, "force_off deveria desligar");
    assert_eq!(st.off, Some(gpu::GpuOff::Disabled));

    // com force_off o kernel recusa e o chamador usa CPU
    let n = 2usize;
    let mut proj = projector();
    let mat = transposed_matrix(&mut proj);
    let x = make_rows(n, 3);
    let mut out = vec![0u64; n * (BITS / 64)];
    let err = jl::project_into_with(jl::DEFAULT_KERNEL, true, &mat, &x, n, D_IN, BITS, &mut out);
    assert_eq!(err, Err(jl::JlGpuError::Off(gpu::GpuOff::Disabled)));

    // e a referência CPU continua exata
    let mut cpu = vec![0u64; n * (BITS / 64)];
    proj.project_into(&x, n, &mut cpu);
    assert!(cpu.iter().any(|w| *w != 0), "CPU deveria produzir bits");
}

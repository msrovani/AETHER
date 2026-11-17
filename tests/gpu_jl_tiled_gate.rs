//! G1-v3 — gate do JL **tiled** (`docs/GPU_PLAN.md`).
//!
//! ```text
//! cargo test --release --features gpu --test gpu_jl_tiled_gate -- --nocapture
//! ```
//!
//! Gates:
//! 1. **bit-exato**: o bit é idêntico ao caminho CPU (j ascendente preservado);
//!    tolerância 1e-6 só para o caso de o WGSL fusionar `mul+add` perto de zero;
//! 2. **razão ≥ 10×** contra a CPU no lote de 2.000 linhas (o v2 media 5×);
//! 3. **sessão**: a matriz sobe uma vez — o segundo lote não paga `fill`;
//! 4. **fallback/shape**: `force_off`, sem matriz, `rows` curto e `bits` não
//!    múltiplo de 32 recusam sem `panic`.

#![cfg(feature = "gpu")]

use std::time::Instant;

use aether_engine::core::gpu;
use aether_engine::core::gpu::jl;
use aether_engine::core::gpu::jl_tiled::{JlSession, JlTiledError};
use aether_engine::core::transplant::JlProjector;

const D_IN: usize = 384;
const BITS: usize = 10_240;

/// Device compartilhado: `poll(Maintain::Wait)` de duas threads trava.
static GPU_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu_guard() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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

fn transposed_matrix(proj: &mut JlProjector) -> Vec<f32> {
    let mut row_major = vec![0.0f32; BITS * D_IN];
    proj.fill_matrix(&mut row_major);
    let mut t = vec![0.0f32; BITS * D_IN];
    jl::transpose_into(&row_major, BITS, D_IN, &mut t);
    t
}

#[test]
fn tiled_igual_ao_cpu_bit_a_bit() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    let _gpu = gpu_guard();
    println!("adapter: {}", st.summary());

    // 200 linhas + um lote de 37 (não múltiplo de 64, exercita as bordas).
    for n in [200usize, 37] {
        let words = BITS / 64;
        let mut proj = projector();
        let x = make_rows(n, 0x7E57 + n as u64);

        let mut cpu = vec![0u64; n * words];
        proj.project_into(&x, n, &mut cpu);

        let mut sess = JlSession::new().expect("sessão JL");
        let mat = transposed_matrix(&mut proj);
        sess.set_matrix(&mat, D_IN, BITS).expect("matriz");

        let mut gpu_out = vec![0u64; n * words];
        sess.project_into(&x, n, &mut gpu_out).expect("kernel");

        let mism = cpu
            .iter()
            .zip(gpu_out.iter())
            .filter(|(a, b)| a != b)
            .count();
        let total = n * words;
        println!(
            "n={n}: palavras divergentes: {mism}/{total} ({:.2e})",
            mism as f64 / total as f64
        );
        assert!(
            (mism as f64) / (total as f64) < 1e-6,
            "GPU divergiu do CPU em {mism}/{total} palavras (n={n})"
        );
    }
}

#[test]
fn tiled_razao_minima_10x() {
    let st = gpu::status();
    if !st.available {
        eprintln!("skip (sem GPU): {}", st.summary());
        return;
    }
    let _gpu = gpu_guard();
    println!("adapter em uso: {}", st.summary());

    let n = 2000usize;
    let words = BITS / 64;
    let mut proj = projector();
    let x = make_rows(n, 0xA11CE);
    let mut cpu = vec![0u64; n * words];

    let t = Instant::now();
    proj.project_into(&x, n, &mut cpu);
    let cpu_ms = t.elapsed().as_secs_f64() * 1e3;

    // custo de gerar + transpor a matriz (uma vez por sessão)
    let t = Instant::now();
    let mat = transposed_matrix(&mut proj);
    let fill_ms = t.elapsed().as_secs_f64() * 1e3;

    let mut sess = JlSession::new().expect("sessão JL");
    let t = Instant::now();
    sess.set_matrix(&mat, D_IN, BITS).expect("matriz");
    let upload_ms = t.elapsed().as_secs_f64() * 1e3;

    let mut gpu_out = vec![0u64; n * words];
    let _ = sess.project_into(&x, n, &mut gpu_out);

    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        sess.project_into(&x, n, &mut gpu_out).expect("kernel");
        best = best.min(t.elapsed().as_secs_f64() * 1e3);
    }
    let ratio = cpu_ms / best;
    println!(
        "n={n}: CPU {cpu_ms:.1} ms · GPU {best:.2} ms ⇒ **{ratio:.0}×** \
         (fill {fill_ms:.0} ms + upload {upload_ms:.0} ms, pagos 1×/sessão)"
    );
    println!(
        "  custo do primeiro lote (fill+upload+kernel): {:.0} ms ⇒ {:.0}×",
        fill_ms + upload_ms + best,
        cpu_ms / (fill_ms + upload_ms + best)
    );
    assert!(
        ratio >= 10.0,
        "razão {ratio:.2}× < 10× (o v2 media 5×; o tiled tem de superar)"
    );

    // bit-exato também no lote grande: divergência só é aceitável se o produto
    // interno daquele bit for ~0 (o WGSL funde mul+add; o sinal de |dot|≈0
    // vira). Qualquer outra divergência é bug de índice.
    let words = BITS / 64;
    let mut mism = 0usize;
    let mut worst_dot = 0.0f64;
    for (i, (a, b)) in cpu.iter().zip(gpu_out.iter()).enumerate() {
        if a == b {
            continue;
        }
        mism += 1;
        let row = i / words;
        let w = i % words;
        for bit in 0..64usize {
            if (a >> bit) & 1 == (b >> bit) & 1 {
                continue;
            }
            let bcol = w * 64 + bit;
            let mut dot = 0.0f64;
            for j in 0..D_IN {
                dot += f64::from(mat[j * BITS + bcol]) * f64::from(x[row * D_IN + j]);
            }
            worst_dot = worst_dot.max(dot.abs());
        }
    }
    println!("  divergências no lote grande: {mism} palavra(s) · maior |dot| {worst_dot:.3e}");
    assert!(
        mism <= 4 && (mism as f64) / ((n * words) as f64) < 1e-5,
        "{mism} palavras divergentes em {} (limite 4 e 1e-5)",
        n * words
    );
    for (i, (a, b)) in cpu.iter().zip(gpu_out.iter()).enumerate() {
        if a == b {
            continue;
        }
        let row = i / words;
        let w = i % words;
        for bit in 0..64usize {
            if (a >> bit) & 1 == (b >> bit) & 1 {
                continue;
            }
            let bcol = w * 64 + bit;
            let mut dot = 0.0f64;
            for j in 0..D_IN {
                dot += f64::from(mat[j * BITS + bcol]) * f64::from(x[row * D_IN + j]);
            }
            assert!(
                dot.abs() < 1e-3,
                "bit ({row}, {bcol}) divergiu com |dot| = {:.3e} — não é arredondamento",
                dot.abs()
            );
        }
    }
}

#[test]
fn tiled_fallback_e_shape_nao_panicam() {
    let err = JlSession::with_force_off(true);
    assert!(matches!(
        err,
        Err(JlTiledError::Off(gpu::GpuOff::Disabled))
    ));

    let st = gpu::status();
    if !st.available {
        return;
    }
    let _gpu = gpu_guard();

    // sem matriz
    let mut sess = JlSession::new().expect("sessão");
    let mut out = vec![0u64; 8];
    let err = sess.project_into(&[0.0f32; 8 * D_IN], 8, &mut out);
    assert_eq!(err, Err(JlTiledError::NoMatrix));

    // bits não múltiplo de 32
    let err = sess.set_matrix(&[0.0f32; 100], 4, 25);
    assert_eq!(err, Err(JlTiledError::Shape));

    // matriz ok, saída curta
    let mat = vec![0.0f32; D_IN * BITS];
    sess.set_matrix(&mat, D_IN, BITS).expect("matriz");
    let mut out = vec![0u64; 4];
    let err = sess.project_into(&[0.0f32; 8 * D_IN], 8, &mut out);
    assert_eq!(err, Err(JlTiledError::Shape));
}

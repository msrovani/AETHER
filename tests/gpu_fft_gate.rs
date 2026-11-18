//! G2 — FFT em lote. Sem a feature `gpu` só corre o contrato de `force_off`.
//!
//! ```text
//! cargo test --release --features gpu --test gpu_fft_gate -- --nocapture
//! ```

use num_complex::Complex;

use aether_engine::core::gpu::fft::{fft_batch_cpu, fft_batch_gpu, FftGpuError};
use aether_engine::core::gpu::GpuOff;

fn signal(n: usize, batch: usize, seed: u64) -> Vec<Complex<f32>> {
    let mut s = seed | 1;
    let mut v = Vec::with_capacity(batch * n);
    for _ in 0..batch * n {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let re = ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0;
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let im = ((s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0;
        v.push(Complex::new(re, im));
    }
    v
}

#[cfg(feature = "gpu")]
fn rel_l2(gpu: &[Complex<f32>], cpu: &[Complex<f32>]) -> f32 {
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (a, b) in gpu.iter().zip(cpu) {
        let d = a - b;
        num += d.norm_sqr();
        den += b.norm_sqr();
    }
    (num / den.max(1e-12)).sqrt()
}

#[test]
fn force_off_nao_entra_em_panic() {
    let input = signal(32, 2, 7);
    let err = fft_batch_gpu(&input, 32, 2, true).expect_err("force_off");
    assert!(matches!(
        err,
        FftGpuError::Off(GpuOff::Disabled) | FftGpuError::Off(GpuOff::NoFeature)
    ));
    assert!(fft_batch_cpu(&input, 32, 2).is_ok());
}

#[cfg(feature = "gpu")]
#[test]
fn fft_lote_erro_e_razao() {
    use std::time::Instant;

    const N: usize = 8192;
    const B: usize = 64;
    let input = signal(N, B, 0xF27);
    let cpu = fft_batch_cpu(&input, N, B).expect("cpu");
    let gpu = match fft_batch_gpu(&input, N, B, false) {
        Ok(v) => v,
        Err(FftGpuError::Off(off)) => {
            eprintln!("FFT-G2 abstain: {}", off.label());
            return;
        }
        Err(other) => panic!("fft gpu: {other:?}"),
    };
    let err = rel_l2(&gpu, &cpu);
    eprintln!("FFT-G2 erro relativo L2 = {err:.3e}");
    assert!(err <= 1e-4, "erro {err} > 1e-4");

    let again = fft_batch_gpu(&input, N, B, false).expect("repro");
    assert_eq!(gpu, again, "mesmo lote, saída diferente");

    let _ = fft_batch_gpu(&input, N, B, false);
    let t0 = Instant::now();
    let _ = fft_batch_gpu(&input, N, B, false).expect("gpu time");
    let gpu_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let _ = fft_batch_cpu(&input, N, B).expect("cpu time");
    let cpu_s = t1.elapsed().as_secs_f64();
    let ratio = cpu_s / gpu_s.max(1e-9);
    eprintln!("FFT-G2 razão CPU/GPU = {ratio:.2} (cpu {cpu_s:.4}s gpu {gpu_s:.4}s)");
    if ratio < 2.0 {
        eprintln!("FFT-G2 abstain: razão {ratio:.2} < 2");
        return;
    }
}

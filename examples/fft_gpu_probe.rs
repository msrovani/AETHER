//! Probe G1/G2 — crossover CPU×GPU do FFT em lote (MSG 0436).
//!
//! Mede, no mesmo processo: (a) custo fixo por chamada (20 chamadas B=64,
//! primeira descartada), (b) varredura de batch 64..2048 em L=8192 comparando
//! `fft_batch_cpu` (rustfft) com `fft_batch_gpu` no mesmo host.
//! Saída: target/fft_gpu_probe.json. Zero decisão — só números.

#[cfg(not(feature = "gpu"))]
fn main() {
    eprintln!("fft_gpu_probe: compile com --features gpu");
    std::process::exit(2);
}

#[cfg(feature = "gpu")]
fn main() {
    use num_complex::Complex;
    use std::time::Instant;

    use aether_engine::core::gpu::fft::{fft_batch_cpu, fft_batch_gpu};

    fn signal(len: usize, seed: u64) -> Vec<Complex<f32>> {
        let mut x = 0x9E3779B97F4A7C15u64.wrapping_mul(seed | 1);
        let mut v = Vec::with_capacity(len);
        for _ in 0..len {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let re = (x as f64 / u64::MAX as f64 - 0.5) as f32;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let im = (x as f64 / u64::MAX as f64 - 0.5) as f32;
            v.push(Complex::new(re, im));
        }
        v
    }

    fn timed<F: FnMut() -> T, T>(mut f: F) -> f64 {
        let t0 = Instant::now();
        let _ = f();
        t0.elapsed().as_secs_f64() * 1e3
    }

    fn serde_json_case(batch: usize, cpu: f64, gpu: f64, gpu_p50: f64, ratio: f64) -> String {
        format!(
            "{{\"batch\":{batch},\"cpu_min_ms\":{cpu:.4},\"gpu_min_ms\":{gpu:.4},\"gpu_p50_ms\":{gpu_p50:.4},\"ratio_min\":{ratio:.4}}}"
        )
    }

    let n = 8192usize;
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    eprintln!("fft_gpu_probe host={host} n={n}");

    // (a) custo fixo por chamada: 20 chamadas B=64, primeira é warmup.
    let mut calls = Vec::new();
    {
        let input = signal(n * 64, 1);
        for _ in 0..20 {
            let ms = timed(|| fft_batch_gpu(&input, n, 64, false));
            calls.push(ms);
        }
    }
    eprintln!("calls_ms[0..20]={calls:?}");
    let mut steady = calls[1..].to_vec();
    steady.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let fixed_p50 = steady[steady.len() / 2];
    eprintln!("custo_fixo_p50_ms={fixed_p50:.3}");

    // (b) varredura de batch: razão CPU/GPU e2e por lote.
    let mut cases = Vec::new();
    for batch in [64usize, 256, 1024, 2048] {
        let input = signal(n * batch, batch as u64 + 7);

        let mut cpu_ms = Vec::new();
        for _ in 0..3 {
            cpu_ms.push(timed(|| fft_batch_cpu(&input, n, batch)));
        }
        cpu_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // warmup GPU (device+pipeline) descartado.
        let _ = fft_batch_gpu(&input, n, batch, false);
        let mut gpu_ms = Vec::new();
        for _ in 0..5 {
            gpu_ms.push(timed(|| fft_batch_gpu(&input, n, batch, false)));
        }
        gpu_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let cpu = cpu_ms[0];
        let gpu = gpu_ms[0];
        let ratio = cpu / gpu;
        eprintln!(
            "batch={batch} cpu_min={cpu:.3}ms gpu_min={gpu:.3}ms gpu_p50={:.3}ms ratio={ratio:.2}x",
            gpu_ms[gpu_ms.len() / 2]
        );
        cases.push(serde_json_case(
            batch,
            cpu,
            gpu,
            gpu_ms[gpu_ms.len() / 2],
            ratio,
        ));
    }

    let json = format!(
        "{{\"host\":\"{host}\",\"n\":{n},\"fixed_p50_ms\":{fixed_p50:.4},\"first_call_ms\":{:.4},\"cases\":[{}],\"created\":\"2026-11-17\"}}\n",
        calls[0],
        cases.join(",")
    );
    let _ = std::fs::create_dir_all("target");
    std::fs::write("target/fft_gpu_probe.json", json).expect("artefato");
    eprintln!("artefato=target/fft_gpu_probe.json");
}

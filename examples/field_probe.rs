//! `field_probe` — aponta o `WaveInterferenceBlock` para um sinal REAL da
//! máquina e mostra o sistema reagindo fisicamente ao ambiente, sem uma linha
//! de texto ou token envolvido.
//!
//! Sinal contínuo usado (zero dependências):
//! * jitter de relógio dos acordares de thread contra o deadline de 60 Hz;
//! * pressão de CPU (custo relativo de uma unidade fixa de trabalho).
//!
//! A cada quadro, o bloco de interferência espectral processa o campo e o
//! exemplo imprime a entropia antes/depois — o colapso de incerteza físico.
//!
//! ```bash
//! cargo run --release --example field_probe -- --duration 20
//! # em outro terminal: stress a CPU (ex.: compilando algo grande) e veja a
//! # entropia e a amplitude da onda reagirem.
//! ```

use std::time::{Duration, Instant};

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::Array2;

/// Semente xorshift64* determinística.
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0
    }
}

/// Transdutor mínimo: jitter + pressão de CPU → onda contínua.
struct Probe {
    window: usize,
    ring: Vec<f32>,
    pos: usize,
    next: Instant,
    origin: Instant,
    baseline_ns: f64,
    rng: Rng,
}

impl Probe {
    fn new(window: usize, seed: u64) -> Self {
        let mut p = Self {
            window,
            ring: vec![0.0; window],
            pos: 0,
            next: Instant::now(),
            origin: Instant::now(),
            baseline_ns: 1.0,
            rng: Rng(seed ^ 0x9E37_79B9_7F4A_7C15),
        };
        let mut acc = 0.0;
        for _ in 0..8 {
            let t0 = Instant::now();
            let _ = p.busy(120_000);
            acc += t0.elapsed().as_secs_f64() * 1e9;
        }
        p.baseline_ns = (acc / 8.0).max(1.0);
        p
    }

    fn busy(&mut self, iters: u64) -> u64 {
        let mut x = self.rng.0;
        let mut acc = 0u64;
        let mut i = 0;
        while i < iters {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            acc = acc.wrapping_add(x);
            i += 1;
        }
        self.rng.0 = x;
        acc
    }

    fn sample(&mut self) -> f32 {
        let now = Instant::now();
        let jitter_ns = now.saturating_duration_since(self.next).as_secs_f64() * 1e9;
        let t0 = Instant::now();
        let _ = self.busy(40_000);
        let work_ns = t0.elapsed().as_secs_f64() * 1e9;
        let load = (work_ns / self.baseline_ns - 1.0).clamp(-0.5, 4.0);

        let carrier = (now.duration_since(self.origin).as_secs_f64() * 6.0) as f32;
        let wave = (jitter_ns.ln().max(0.0) as f32) * 0.02
            + load as f32 * 0.5
            + self.rng.next_f32() * 0.05
            + carrier.sin() * 0.08;

        self.ring[self.pos] = wave;
        self.pos = (self.pos + 1) % self.window;
        wave
    }

    fn tick(&mut self, hz: f64) {
        self.next += Duration::from_secs_f64(1.0 / hz);
        if self.next < Instant::now() {
            self.next = Instant::now();
        }
    }

    fn field(&self, channels: usize) -> Array2<f32> {
        let mut out = Array2::<f32>::zeros((channels, self.window));
        for c in 0..channels {
            let gain = 1.0 + c as f32 * 0.5;
            for k in 0..self.window {
                let idx = (self.pos + self.window - 1 + k) % self.window;
                out[[c, k]] = self.ring[idx] * gain;
            }
        }
        out
    }

    /// Amplitude pico-a-pico recente (telemetria bruta).
    fn amplitude(&self) -> f32 {
        let min = self.ring.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = self.ring.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        max - min
    }
}

fn main() {
    let mut duration = 20.0f64;
    let mut hz = 60.0f64;
    let mut window = 128usize;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--duration" => {
                i += 1;
                duration = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(20.0);
            }
            "--hz" => {
                i += 1;
                hz = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(60.0);
            }
            "--window" => {
                i += 1;
                window = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(128);
            }
            other => {
                eprintln!("flag desconhecida: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    println!("field_probe — sinal físico real (jitter + pressão de CPU) → Módulo 2");
    println!("hz={hz} janela={window} duração={duration}s\n");

    let mut probe = Probe::new(window, 0xA37E_0004);
    let mut block = WaveInterferenceBlock::with_max_len(2, 4, 32, window, 0xC0FFEE);
    let mut u = Array2::<f32>::zeros((2, window));
    let mut v = Array2::<f32>::zeros((4, window));

    let started = Instant::now();
    let mut frames = 0u64;
    let mut last_print = Instant::now();

    while started.elapsed().as_secs_f64() < duration {
        let now = Instant::now();
        if now < probe.next {
            std::thread::sleep(probe.next - now);
        }
        probe.tick(hz);
        let _ = probe.sample();
        u.assign(&probe.field(2));

        block.forward_into(u.view(), &mut v).expect("forward");
        let h_raw = block.compute_spectral_entropy(u.view()).expect("entropia");
        let h_wave = block.compute_wave_entropy();

        frames += 1;
        if last_print.elapsed() >= Duration::from_millis(500) {
            println!(
                "t={:>6.1}s quadros={:>5} | amplitude={:.3} | entropia cru={:.3} → interferida={:.3} | Δ={:+.3}",
                started.elapsed().as_secs_f64(),
                frames,
                probe.amplitude(),
                h_raw,
                h_wave,
                h_wave - h_raw
            );
            last_print = Instant::now();
        }
    }

    println!("\nfield_probe encerrado.");
}

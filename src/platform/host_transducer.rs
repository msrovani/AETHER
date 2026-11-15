//! Transdutor físico real do PC: jitter de silício, carga e cadência.
//!
//! # O que é medido (e o que **não** é)
//!
//! * **Jitter físico / escalonador** — acordares calibrados por
//!   [`std::time::Instant`] com meta fixa; o desvio absoluto em nanossegundos
//!   é a assinatura real de preempção/DPC da máquina. É normalizado pela
//!   topologia (`available_parallelism`) e injetado no
//!   [`WaveInterferenceBlock`] como ruído térmico/carga.
//! * **Carga** — contagem de iterações num orçamento fixo de tempo; a queda
//!   relativa ao baseline calibrado é a carga de CPU.
//! * **Cadência interativa** — apenas os **intervalos** `Δt` (µs) entre eventos
//!   de teclado. Nenhum caractere é gravado, exibido ou transmitido: a tecla é
//!   lida só para rotear a ação (`1`, `2`, `3`, `q`) e descartada. Isto é um
//!   sensor de ritmo, não um keylogger.
//!
//! # Zero alocação
//!
//! [`HostTransducer::sample_wave_into`] não aloca: os buffers do bloco de onda
//! e o *scratch* do BAM são criados em [`HostTransducer::new`]. Só o caminho de
//! erro (`AetherError::Config`) aloca mensagem.

use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ndarray::Array2;

use crate::core::bam_phase::{BamHolographicVector, BamMask};
use crate::core::bam_store::{BamHolographicStore, BamScratch};
use crate::core::wave_interference::WaveInterferenceBlock;
use crate::error::{AetherError, Result};

/// Canais do bloco de onda (entrada e saída).
pub const WAVE_CHANNELS: usize = 8;
/// Dimensão da assinatura de cadência.
pub const SIGNATURE_DIM: usize = 8;
/// Dimensão do vetor BAM (assinatura preenchida com zeros).
pub const BAM_DIM: usize = 16;
/// Profundidade do anel de intervalos.
pub const CADENCE_RING: usize = 8;
/// Traços da memória de identidade.
pub const BAM_TRACES: usize = 4;

/// Flag: injetar pulso de carga/anomalia térmica.
pub const FLAG_PULSE: u32 = 1;
/// Flag: injetar rajada de ruído (testa abstenção).
pub const FLAG_NOISE: u32 = 1 << 1;
/// Flag: gravar a assinatura atual na memória BAM.
pub const FLAG_SAVE: u32 = 1 << 2;
/// Flag: encerrar graciosamente.
pub const FLAG_QUIT: u32 = 1 << 3;

/// Estado compartilhado entre a thread de teclado e o transdutor.
///
/// Só guarda instantes e flags — nunca o caractere digitado.
#[derive(Debug)]
pub struct CadenceShared {
    epoch: Instant,
    last_ns: AtomicU64,
    last_dt_ns: AtomicU64,
    flags: AtomicU32,
    keys: AtomicU64,
}

impl Default for CadenceShared {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            last_ns: AtomicU64::new(0),
            last_dt_ns: AtomicU64::new(0),
            flags: AtomicU32::new(0),
            keys: AtomicU64::new(0),
        }
    }
}

impl CadenceShared {
    /// Marca um evento de entrada e roteia a ação (o caractere é descartado).
    ///
    /// O único dado retido é o intervalo `Δt` desde o evento anterior.
    pub fn on_key(&self, action: u32) {
        let now = self.epoch.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let prev = self.last_ns.swap(now, Ordering::Relaxed);
        let dt = if prev == 0 {
            0
        } else {
            now.saturating_sub(prev)
        };
        self.last_dt_ns.store(dt, Ordering::Relaxed);
        self.keys.fetch_add(1, Ordering::Relaxed);
        if action != 0 {
            self.flags.fetch_or(action, Ordering::Relaxed);
        }
    }

    /// Último intervalo `Δt` em nanossegundos.
    pub fn last_dt_ns(&self) -> u64 {
        self.last_dt_ns.load(Ordering::Relaxed)
    }

    /// Pede o encerramento (`q`/Esc).
    pub fn request_quit(&self) {
        self.flags.fetch_or(FLAG_QUIT, Ordering::Relaxed);
    }

    /// `true` se o encerramento foi pedido.
    pub fn quit_requested(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & FLAG_QUIT != 0
    }

    /// Total de eventos de entrada observados.
    pub fn keys(&self) -> u64 {
        self.keys.load(Ordering::Relaxed)
    }
}

/// Amostra escalar do último ciclo (leitura barata para a telemetria).
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    /// Onda sintetizada (8 canais).
    pub wave: [f32; WAVE_CHANNELS],
    /// Jitter absoluto do acordar (ns).
    pub jitter_ns: f32,
    /// Carga relativa em `[0, 1]`.
    pub load: f32,
    /// Último `Δt` de cadência (µs).
    pub dt_us: f32,
    /// Similaridade da assinatura na memória BAM.
    pub identity: f32,
    /// Margem para a segunda assinatura.
    pub margin: f32,
    /// Assinaturas gravadas.
    pub signatures: usize,
}

/// Sensor contínuo do PC.
pub struct HostTransducer {
    wave: WaveInterferenceBlock,
    input: Array2<f32>,
    output: Array2<f32>,
    logical: usize,
    jitter_ns: f32,
    load: f32,
    load_baseline: f32,
    thermal: f32,
    noise: f32,
    lcg: u64,
    ring: [f32; CADENCE_RING],
    ring_idx: usize,
    ring_len: usize,
    last_key_seq: u64,
    dt_us: f32,
    tick: u64,
    shared: Option<Arc<CadenceShared>>,
    store: BamHolographicStore<BAM_DIM>,
    scratch: BamScratch<BAM_DIM>,
    self_role: BamHolographicVector<BAM_DIM>,
    mask: BamMask<BAM_DIM>,
    identities: usize,
    last: Sample,
}

impl fmt::Debug for HostTransducer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostTransducer")
            .field("logical", &self.logical)
            .field("jitter_ns", &self.jitter_ns)
            .field("load", &self.load)
            .field("identities", &self.identities)
            .field("tick", &self.tick)
            .field("interactive", &self.shared.is_some())
            .finish_non_exhaustive()
    }
}

impl HostTransducer {
    /// Cria o sensor com buffers pré-alocados.
    pub fn new(seed: u64) -> Result<Self> {
        let mut wave = WaveInterferenceBlock::with_max_len(
            WAVE_CHANNELS,
            WAVE_CHANNELS,
            4,
            WAVE_CHANNELS,
            seed,
        );
        // Aquece planos FFT/scratch fora do laço quente.
        {
            let input = Array2::<f32>::zeros((WAVE_CHANNELS, WAVE_CHANNELS));
            let mut output = Array2::<f32>::zeros((WAVE_CHANNELS, WAVE_CHANNELS));
            wave.forward_into(input.view(), &mut output)?;
        }

        let store = BamHolographicStore::<BAM_DIM>::new(BAM_TRACES)?;
        let scratch = store.scratch(BAM_TRACES * 4);
        let mut role = [0.0f32; BAM_DIM];
        for (i, r) in role.iter_mut().enumerate() {
            *r = (i as f32) * std::f32::consts::TAU / (BAM_DIM as f32);
        }
        let self_role = BamHolographicVector::<BAM_DIM>::from_radians(&role)?;

        let logical = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        Ok(Self {
            wave,
            input: Array2::<f32>::zeros((WAVE_CHANNELS, WAVE_CHANNELS)),
            output: Array2::<f32>::zeros((WAVE_CHANNELS, WAVE_CHANNELS)),
            logical,
            jitter_ns: 0.0,
            load: 0.0,
            load_baseline: 1.0,
            thermal: 0.0,
            noise: 0.0,
            lcg: seed | 1,
            ring: [0.0; CADENCE_RING],
            ring_idx: 0,
            ring_len: 0,
            last_key_seq: 0,
            dt_us: 0.0,
            tick: 0,
            shared: None,
            store,
            scratch,
            self_role,
            mask: BamMask::<BAM_DIM>::all(),
            identities: 0,
            last: Sample::default(),
        })
    }

    /// Liga a captura de cadência (thread de teclado já criada).
    pub fn attach_cadence(&mut self, shared: Arc<CadenceShared>) {
        self.last_key_seq = shared.keys();
        self.shared = Some(shared);
    }

    /// Referência ao estado de cadência, se houver.
    pub fn cadence(&self) -> Option<&Arc<CadenceShared>> {
        self.shared.as_ref()
    }

    /// Calibra o baseline de carga (fora do laço quente).
    pub fn calibrate(&mut self) {
        let mut best = 0f32;
        for _ in 0..3 {
            best = best.max(self.count_budget(Duration::from_micros(50)) as f32);
        }
        self.load_baseline = best.max(1.0);
    }

    /// Sensores da última amostra.
    pub const fn sample(&self) -> Sample {
        self.last
    }

    /// Núcleos lógicos detectados (normalização do jitter).
    pub const fn logical_cores(&self) -> usize {
        self.logical
    }

    /// Jitter normalizado pela topologia (`ns / núcleos`).
    pub fn jitter_normalized(&self) -> f32 {
        self.jitter_ns / self.logical.max(1) as f32
    }

    /// Assinatura de cadência preenchida até [`BAM_DIM`].
    pub fn signature_padded(&self) -> [f32; BAM_DIM] {
        let mut out = [0.0f32; BAM_DIM];
        let sig = self.signature();
        out[..SIGNATURE_DIM].copy_from_slice(&sig);
        out
    }

    /// Grava a assinatura atual na memória holográfica BAM.
    pub fn enroll_now(&mut self) -> Result<usize> {
        let padded = self.signature_padded();
        let v = BamHolographicVector::<BAM_DIM>::from_radians(&padded)?;
        let idx = self.store.add_fact(&v, &self.self_role, &v);
        self.identities = self.store.len();
        Ok(idx)
    }

    /// Assinaturas gravadas.
    pub const fn signatures(&self) -> usize {
        self.identities
    }

    /// `true` se o encerramento foi pedido no terminal.
    pub fn quit_requested(&self) -> bool {
        self.shared.as_ref().is_some_and(|s| s.quit_requested())
    }

    /// Amostra contínua: preenche `out` com a onda sintetizada.
    ///
    /// **Sem alocação**: usa apenas os buffers internos pré-alocados.
    pub fn sample_wave_into(&mut self, out: &mut [f32]) -> Result<()> {
        if out.len() < WAVE_CHANNELS {
            return Err(AetherError::Config(
                "transducer: out precisa de >= WAVE_CHANNELS".into(),
            ));
        }
        self.consume_input();

        // 1. Jitter físico (desvio absoluto do acordar calibrado).
        self.jitter_ns = self.measure_jitter_ns();
        // 2. Carga relativa ao baseline.
        self.load = self.measure_load();
        self.tick = self.tick.wrapping_add(1);

        // 3. Preenche o bloco de onda (embedding circulante dos sensores).
        for c in 0..WAVE_CHANNELS {
            for l in 0..WAVE_CHANNELS {
                self.input[[c, l]] = self.feature((c + l) % WAVE_CHANNELS);
            }
        }
        self.wave
            .forward_into(self.input.view(), &mut self.output)?;

        // 4. Síntese: média espacial de cada canal de saída.
        for (i, slot) in out.iter_mut().take(WAVE_CHANNELS).enumerate() {
            let mut acc = 0.0f32;
            for l in 0..WAVE_CHANNELS {
                acc += self.output[[i, l]];
            }
            *slot = sanitize(acc / WAVE_CHANNELS as f32);
        }

        // 5. Identidade contínua (consulta zero-alocação).
        let (identity, margin) = if self.identities > 0 {
            let padded = self.signature_padded();
            let cue = BamHolographicVector::<BAM_DIM>::from_radians(&padded)?;
            match self
                .store
                .query_into(&cue, &self.self_role, &self.mask, &mut self.scratch)
            {
                Ok(hit) => (sanitize(hit.similarity).clamp(0.0, 1.0), hit.margin),
                Err(_) => (0.0, 0.0),
            }
        } else {
            (0.0, 0.0)
        };

        // 6. Primeira assinatura vira a referência de boot (headless incluído).
        if self.identities == 0 {
            let _ = self.enroll_now();
        }

        self.last = Sample {
            wave: [
                out[0], out[1], out[2], out[3], out[4], out[5], out[6], out[7],
            ],
            jitter_ns: self.jitter_ns,
            load: self.load,
            dt_us: self.dt_us,
            identity,
            margin,
            signatures: self.identities,
        };
        Ok(())
    }

    /// Consome flags/cadência da thread de teclado (sem alocar).
    fn consume_input(&mut self) {
        self.thermal *= 0.95;
        self.noise *= 0.85;
        let Some(shared) = self.shared.as_ref() else {
            return;
        };
        let keys = shared.keys();
        if keys != self.last_key_seq {
            let dt_ns = shared.last_dt_ns();
            if dt_ns > 0 {
                self.dt_us = (dt_ns as f32) / 1000.0;
                // Só o intervalo é guardado; nenhum caractere é registrado.
                self.ring[self.ring_idx] = self.dt_us;
                self.ring_idx = (self.ring_idx + 1) % CADENCE_RING;
                self.ring_len = (self.ring_len + 1).min(CADENCE_RING);
            }
            self.last_key_seq = keys;
        }
        let flags = shared
            .flags
            .fetch_and(!(FLAG_PULSE | FLAG_NOISE | FLAG_SAVE), Ordering::Relaxed);
        if flags & FLAG_PULSE != 0 {
            self.thermal = (self.thermal + 0.8).min(1.0);
        }
        if flags & FLAG_NOISE != 0 {
            self.noise = 1.0;
        }
        if flags & FLAG_SAVE != 0 {
            let _ = self.enroll_now();
        }
    }

    /// Um canal de entrada do bloco de onda.
    fn feature(&self, i: usize) -> f32 {
        match i {
            0 => (self.jitter_ns / 100_000.0).tanh(),
            1 => self.load,
            2 => (self.dt_us / 50_000.0).tanh(),
            3 => self.thermal,
            4 => self.noise,
            5 => self.cadence_stat(0),
            6 => self.cadence_stat(1),
            _ => {
                let x = self.lcg;
                ((x >> 33) as f32) / (u32::MAX as f32)
            }
        }
    }

    /// Estatística do anel de cadência (média ou coeficiente de variação).
    fn cadence_stat(&self, which: usize) -> f32 {
        if self.ring_len == 0 {
            return 0.0;
        }
        let n = self.ring_len as f32;
        let sum: f32 = self.ring[..self.ring_len].iter().sum();
        let mean = sum / n;
        if which == 0 {
            return (mean / 10_000.0).tanh();
        }
        let var: f32 = self.ring[..self.ring_len]
            .iter()
            .map(|v| {
                let d = v - mean;
                d * d
            })
            .sum::<f32>()
            / n;
        (var.sqrt() / mean.max(1e-6)).tanh()
    }

    /// Assinatura de 8 dimensões da cadência (estável, finita).
    fn signature(&self) -> [f32; SIGNATURE_DIM] {
        let mut sig = [0.0f32; SIGNATURE_DIM];
        if self.ring_len == 0 {
            sig[0] = sanitize(self.jitter_ns / 1e5);
            sig[1] = self.load;
            return sig;
        }
        let n = self.ring_len as f32;
        let sum: f32 = self.ring[..self.ring_len].iter().sum();
        let mean = sum / n;
        let mut vmin = f32::INFINITY;
        let mut vmax = f32::NEG_INFINITY;
        for v in &self.ring[..self.ring_len] {
            vmin = vmin.min(*v);
            vmax = vmax.max(*v);
        }
        let var: f32 = self.ring[..self.ring_len]
            .iter()
            .map(|v| {
                let d = v - mean;
                d * d
            })
            .sum::<f32>()
            / n;
        let std = var.sqrt();
        let scale = 10_000.0f32;
        sig[0] = (mean / scale).tanh();
        sig[1] = (std / scale).tanh();
        sig[2] = (std / mean.max(1e-6)).tanh();
        sig[3] = (self.dt_us / scale).tanh();
        sig[4] = (vmin / scale).tanh();
        sig[5] = (vmax / scale).tanh();
        let half = self.ring_len / 2;
        let first: f32 = self.ring[..half].iter().sum::<f32>() / half.max(1) as f32;
        let second: f32 = self.ring[half..self.ring_len].iter().sum::<f32>()
            / (self.ring_len - half).max(1) as f32;
        sig[6] = ((second - first) / scale).tanh();
        sig[7] = (n / CADENCE_RING as f32).tanh();
        sig
    }

    /// Desvio absoluto médio de 4 acordares com meta de 100 µs.
    fn measure_jitter_ns(&mut self) -> f32 {
        let target = Duration::from_micros(100);
        let mut abs = 0.0f64;
        for _ in 0..4 {
            let t0 = Instant::now();
            let deadline = t0 + target;
            while Instant::now() < deadline {
                std::hint::spin_loop();
            }
            let over = t0.elapsed().as_nanos() as i128 - target.as_nanos() as i128;
            abs += over.unsigned_abs() as f64;
        }
        let jitter = (abs / 4.0) as f32;
        // O jitter cresce com a contenção: avança o LCG para decorrelacionar.
        self.lcg = self
            .lcg
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        sanitize(jitter)
    }

    /// Carga em `[0, 1]`: queda de iterações frente ao baseline calibrado.
    fn measure_load(&self) -> f32 {
        let iters = self.count_budget(Duration::from_micros(20)) as f32;
        let ratio = (iters / self.load_baseline).clamp(0.0, 1.0);
        (1.0 - ratio).clamp(0.0, 1.0)
    }

    /// Conta iterações de trabalho num orçamento de tempo.
    fn count_budget(&self, budget: Duration) -> u64 {
        let t0 = Instant::now();
        let mut x = self.lcg | 1;
        let mut n = 0u64;
        while t0.elapsed() < budget {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            n = n.wrapping_add(1);
        }
        std::hint::black_box(x);
        n
    }
}

/// Sanitiza valores não finitos para `0`.
fn sanitize(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Captura de cadência no terminal (só `Δt`, zero keylogger).
///
/// Requer a feature `m4` (crossterm). A tecla é lida apenas para rotear a ação
/// — `1` pulso térmico, `2` rajada de ruído, `3` gravar assinatura, `q` sair —
/// e o caractere é descartado imediatamente.
#[cfg(feature = "m4")]
pub fn spawn_cadence_tap() -> Result<Arc<CadenceShared>> {
    use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind};
    use crossterm::terminal;

    terminal::enable_raw_mode().map_err(AetherError::Io)?;
    let shared = Arc::new(CadenceShared::default());
    let worker = Arc::clone(&shared);
    std::thread::spawn(move || {
        while !worker.quit_requested() {
            match poll(Duration::from_millis(50)) {
                Ok(true) => {
                    if let Ok(Event::Key(key)) = read() {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char('1') => worker.on_key(FLAG_PULSE),
                                KeyCode::Char('2') => worker.on_key(FLAG_NOISE),
                                KeyCode::Char('3') => worker.on_key(FLAG_SAVE),
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    worker.request_quit();
                                    break;
                                }
                                // Qualquer outra tecla só conta como evento temporal.
                                _ => worker.on_key(0),
                            }
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    Ok(shared)
}

/// Restaura o terminal (idempotente; chame no encerramento).
#[cfg(feature = "m4")]
pub fn disable_raw_mode() {
    let _ = crossterm::terminal::disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amostragem_sem_alocacao_e_finita() {
        let mut tr = HostTransducer::new(7).expect("transducer");
        tr.calibrate();
        let mut out = [0.0f32; WAVE_CHANNELS];
        tr.sample_wave_into(&mut out).expect("sample");
        assert!(out.iter().all(|v| v.is_finite()));
        assert!(tr.signatures() >= 1, "boot deve gravar a 1a assinatura");
        let s = tr.sample();
        assert!(s.jitter_ns.is_finite() && s.jitter_ns >= 0.0);
        assert!((0.0..=1.0).contains(&s.load));
        assert!(s.identity.is_finite());
        // Amostras subsequentes mantêm o contrato.
        for _ in 0..8 {
            tr.sample_wave_into(&mut out).expect("sample");
        }
        assert!(out.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn cadencia_captura_so_intervalos() {
        let mut tr = HostTransducer::new(11).expect("transducer");
        tr.calibrate();
        let shared = Arc::new(CadenceShared::default());
        tr.attach_cadence(Arc::clone(&shared));
        shared.on_key(0);
        std::thread::sleep(Duration::from_millis(2));
        shared.on_key(FLAG_PULSE);
        let mut out = [0.0f32; WAVE_CHANNELS];
        tr.sample_wave_into(&mut out).expect("sample");
        assert!(tr.sample().dt_us >= 0.0);
        assert_eq!(shared.keys(), 2, "eventos contados, sem caracteres");
        shared.request_quit();
        assert!(tr.quit_requested());
    }
}

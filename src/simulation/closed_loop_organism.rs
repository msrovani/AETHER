//! Módulo 4 — Organismo cognitivo de loop fechado.
//!
//! Liga os três pilares num fluxo contínuo, sem tokens:
//!
//! 1. **Sensores**: 3 nós Kuramoto acoplados em `127.0.0.1` (`R > 0.95`).
//! 2. **Perturbação física**: transiente ondulatório (obstáculo + assinatura
//!    acústica) injetado no `Nó_Ambiente` como `ξ ≠ 0`.
//! 3. **Propagação → onda**: o deslizamento de fase visto no `Nó_Bancada` vira
//!    campo contínuo (projeção fasorial `cos θ`/`sin θ`) e alimenta o
//!    `WaveInterferenceBlock`, que filtra por interferência destrutiva.
//! 4. **Colapso termodinâmico**: o vetor de viés `h(t)` deforma oamiltoniano e
//!    o `ThermodynamicIsingSolver` relaxa até `s* ∈ {-1,+1}^N`.
//!
//! Nada aqui trafega texto: entre nós só existe fase; entre o sensor e o
//! solver só existe sinal contínuo.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use ndarray::{Array1, Array2, Array3};
use num_complex::Complex32;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::core::kuramoto_field::{KuramotoConfig, KuramotoPhaseNode, NodeHandle};
use crate::core::thermodynamic_solver::{IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver};
use crate::core::wave_interference::WaveInterferenceBlock;
use crate::error::{AetherError, Result};

/// Nomes lógicos dos nós sensoriais (apenas rótulo de exibição; não trafega).
pub const NODE_NAMES: [&str; 3] = ["AMBIENTE", "MOVEL", "BANCADA"];

/// Configuração do organismo.
#[derive(Clone, Debug)]
pub struct OrganismConfig {
    /// Dimensão `N` do Ising (número de atuadores).
    pub decision_dim: usize,
    /// Canais de fase por nó (sempre 4 no protocolo de 40 bytes).
    pub sensor_channels: usize,
    /// Amostras no buffer circular do campo sensorial.
    pub window: usize,
    /// Passos de relaxamento por quadro.
    pub steps: usize,
    /// Taxa do loop principal (Hz).
    pub hz: f64,
    /// Momento da perturbação física.
    pub perturb_after: Duration,
    /// Semente do RNG (reprodutibilidade bit-a-bit).
    pub seed: u64,
    /// Coeficiente de acoplamento de Kuramoto.
    pub k_coupling: f64,
    /// Ganho do campo externo `h(t)` no hamiltoniano.
    pub field_gain: f64,
    /// Frequências naturais dos osciladores (rad/s).
    pub omega: [f64; 4],
    /// Ruído do sensor (desvio padrão em rad).
    ///
    /// É a **largura de banda** do sensor: o ruído branco de medição é o piso
    /// espectral que ocupa todos os bins. Com um fasor de fase quase-estático
    /// (`ω ≈ 6 rad/s`, 60 Hz, janela de 128 amostras) o campo é *narrowband* e
    /// um piso ínfimo (0.03) deixa a interferência **espalhar** energia dentro
    /// dos 32 modos retidos, elevando `H_wave` acima de `H_raw`. Com o piso
    /// físico de 0.35 (RMS ≈ 0.20 no fasor unitário, ≈ 0.2 rad ≈ 11° de erro
    /// de fase, SNR ≈ 11 dB — a fase segue como sinal principal) a energia em
    /// `k ≥ modes` passa a existir, o truncamento modal remove essa banda e a
    /// interferência **reduz** a entropia, como a ARCHITECTURE §8 exige.
    pub sensor_noise: f64,
    /// Frequência da assinatura acústica (Hz).
    pub acoustic_hz: f64,
    /// Amplitude da assinatura acústica (rad/s sobre `dθ/dt`).
    pub acoustic_amplitude: f32,
}

impl Default for OrganismConfig {
    fn default() -> Self {
        Self {
            decision_dim: 64,
            sensor_channels: 4,
            window: 128,
            steps: 200,
            hz: 60.0,
            perturb_after: Duration::from_secs(3),
            seed: 0xA37E_0001,
            k_coupling: 8.0,
            field_gain: 0.25,
            omega: [6.0, 6.02, 5.98, 6.01],
            sensor_noise: 0.35,
            acoustic_hz: 10.0,
            acoustic_amplitude: 20.0,
        }
    }
}

/// Estado exibido/medido em um quadro.
#[derive(Clone, Debug)]
pub struct OrganismFrame {
    /// Tempo desde o start.
    pub elapsed: Duration,
    /// Coerência de fase global `R(t)` (média dos nós).
    pub order_parameter: f64,
    /// `R(t)` por nó.
    pub r_per_node: [f64; 3],
    /// Entropia espectral do campo cru (antes da interferência).
    pub entropy_raw: f64,
    /// Entropia espectral do campo transferido (após a interferência).
    pub entropy_wave: f64,
    /// Energia do estado emergente `s*`.
    pub energy: f64,
    /// Energia do estado inicial `s_0` do último relaxamento.
    pub energy_initial: f64,
    /// Curva `H(t)` do último relaxamento (`steps+1` amostras).
    pub energy_trace: Vec<f64>,
    /// Estado emergente `s*`.
    pub states: Vec<i8>,
    /// Fase atual do `Nó_Bancada`.
    pub phase_bancada: [f64; 4],
    /// A perturbação já foi injetada?
    pub perturbed: bool,
    /// Quadros descartados por estouro de orçamento.
    pub dropped_frames: u32,
    /// Custo do último quadro (µs).
    pub frame_cost_us: u64,
}

impl OrganismFrame {
    /// Quadro **pré-dimensionado** para o caminho quente.
    ///
    /// `energy_trace` já nasce com `steps + 1` amostras e `states` com `n`
    /// atuadores, exatamente as formas que [`ClosedLoopOrganism::step_into`]
    /// exige. Assim o loop de 60 Hz reutiliza o mesmo quadro e só preenche os
    /// campos (`copy_from_slice`), sem realocar `Vec`.
    pub fn with_capacity(steps: usize, n: usize) -> Self {
        Self {
            elapsed: Duration::ZERO,
            order_parameter: 0.0,
            r_per_node: [0.0; 3],
            entropy_raw: 0.0,
            entropy_wave: 0.0,
            energy: 0.0,
            energy_initial: 0.0,
            energy_trace: vec![0.0; steps + 1],
            states: vec![0; n],
            phase_bancada: [0.0; 4],
            perturbed: false,
            dropped_frames: 0,
            frame_cost_us: 0,
        }
    }
}

/// Organismo cognitivo integrado.
pub struct ClosedLoopOrganism {
    cfg: OrganismConfig,
    handles: Vec<NodeHandle>,
    wave: WaveInterferenceBlock,
    solver: ThermodynamicIsingSolver,
    j: Array2<f64>,
    h: Array1<f64>,
    readout: Array3<Complex32>,
    ring: Array2<f32>,
    ring_pos: usize,
    ring_filled: usize,
    u: Array2<f32>,
    v: Array2<f32>,
    out: RelaxOutcome,
    /// Scratch da leitura espectral (`h(t)` antes da normalização).
    raw: Array1<f64>,
    started: Instant,
    perturbed: bool,
    dropped_frames: u32,
    rng: ChaCha8Rng,
}

impl std::fmt::Debug for ClosedLoopOrganism {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClosedLoopOrganism")
            .field("cfg", &self.cfg)
            .field("nodes", &self.handles.len())
            .field("j", &self.j.dim())
            .field("window", &self.ring.ncols())
            .field("perturbed", &self.perturbed)
            .field("dropped_frames", &self.dropped_frames)
            .finish()
    }
}

impl ClosedLoopOrganism {
    /// Monta o organismo: 3 nós, bloco de onda, solver e paisagem `J`.
    pub fn new(cfg: OrganismConfig) -> Result<Self> {
        if cfg.sensor_channels != 4 {
            return Err(AetherError::Config(format!(
                "sensor_channels deve ser 4 (protocolo de 40 bytes), veio {}",
                cfg.sensor_channels
            )));
        }
        if cfg.decision_dim == 0 || cfg.window < 8 {
            return Err(AetherError::Config(
                "decision_dim > 0 e window >= 8 sao obrigatorios".into(),
            ));
        }

        let channels = cfg.sensor_channels;
        let c_in = channels * 2; // cos/sin de cada canal de fase
        let c_out = 8;
        let modes = 32;

        // --- 1. nós sensoriais (endereços efêmeros, peers resolvidos depois) ---
        let mut nodes: Vec<KuramotoPhaseNode> = Vec::with_capacity(NODE_NAMES.len());
        for id in 0..NODE_NAMES.len() as u32 {
            let node_cfg = KuramotoConfig {
                node_id: id,
                bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
                peers: Vec::new(),
                omega: cfg.omega,
                k_coupling: cfg.k_coupling,
                hz: cfg.hz,
                adjacency: None,
                perturb_duration: Duration::from_millis(250),
                multicast: None,
            };
            nodes.push(KuramotoPhaseNode::new(node_cfg)?);
        }
        let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
        for (i, node) in nodes.iter_mut().enumerate() {
            let peers: Vec<SocketAddrV4> = addrs
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, a)| *a)
                .collect();
            node.set_peers(peers);
        }
        let handles: Vec<NodeHandle> = nodes.into_iter().map(|n| n.spawn()).collect();

        // --- 2. paisagem de energia fixa (J simétrica, diagonal zero) ---
        let n = cfg.decision_dim;
        let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
        let scale = 1.0 / (n as f64).sqrt();
        let mut j = Array2::<f64>::zeros((n, n));
        for i in 0..n {
            for k in (i + 1)..n {
                let v: f64 = rng.gen::<f64>() * 2.0 - 1.0;
                let v = v * scale;
                j[[i, k]] = v;
                j[[k, i]] = v;
            }
        }

        // --- 3. cabeça de leitura espectral (base complexa) ---
        // Cada atuador i é um ressonador: h_i = Re( Σ_{o,k} W[i,o,k] · Ŷ[o,k] ).
        let mut readout = Array3::<Complex32>::zeros((n, c_out, modes));
        let norm = 1.0f32 / (c_out * modes) as f32;
        for i in 0..n {
            for o in 0..c_out {
                for k in 0..modes {
                    let amp = norm * (0.5 + rng.gen::<f32>());
                    let ang = rng.gen::<f32>() * std::f32::consts::TAU;
                    readout[[i, o, k]] = Complex32::new(amp * ang.cos(), amp * ang.sin());
                }
            }
        }

        let wave = WaveInterferenceBlock::with_max_len(c_in, c_out, modes, cfg.window, cfg.seed);
        let solver = ThermodynamicIsingSolver::new(1, n, cfg.steps, 0.05);
        let out = RelaxOutcome::with_capacity(1, n, cfg.steps);
        let window = cfg.window;

        Ok(Self {
            cfg,
            handles,
            wave,
            solver,
            j,
            h: Array1::<f64>::zeros(n),
            readout,
            ring: Array2::<f32>::zeros((c_in, window)),
            ring_pos: 0,
            ring_filled: 0,
            u: Array2::<f32>::zeros((c_in, window)),
            v: Array2::<f32>::zeros((c_out, window)),
            out,
            raw: Array1::<f64>::zeros(n),
            started: Instant::now(),
            perturbed: false,
            dropped_frames: 0,
            rng,
        })
    }

    /// Número de nós.
    pub fn node_count(&self) -> usize {
        self.handles.len()
    }

    /// Configuração corrente.
    pub fn config(&self) -> &OrganismConfig {
        &self.cfg
    }

    /// Estado emergente atual.
    pub fn states(&self) -> &[i8] {
        let n = self.out.states.ncols();
        match self.out.states.as_slice() {
            Some(s) if n > 0 => &s[..n],
            _ => &[],
        }
    }

    /// Um quadro completo do organismo.
    ///
    /// Aloca o quadro (`OrganismFrame::with_capacity`) e delega para
    /// [`Self::step_into`]; o caminho quente sem alocação é `step_into`.
    pub fn step(&mut self) -> Result<OrganismFrame> {
        let mut frame = OrganismFrame::with_capacity(self.cfg.steps, self.cfg.decision_dim);
        self.step_into(&mut frame)?;
        Ok(frame)
    }

    /// Computa um quadro **dentro de `out`**, sem alocar após o warmup.
    ///
    /// Exige que `out` já tenha as formas canônicas
    /// (`energy_trace.len() == steps + 1`, `states.len() == decision_dim`);
    /// qualquer divergência retorna [`AetherError::Shape`] **sem**
    /// redimensionar (o warmup deve usar [`OrganismFrame::with_capacity`]).
    ///
    /// Todos os buffers internos (`RelaxOutcome`, `raw`, plano/scratch da onda)
    /// são reutilizados; os dois `Vec` do quadro são preenchidos por
    /// `copy_from_slice`.
    pub fn step_into(&mut self, out: &mut OrganismFrame) -> Result<()> {
        let t0 = Instant::now();
        let steps = self.cfg.steps;
        let n = self.cfg.decision_dim;

        // --- validação de formas (sem alocar/redimensionar no caminho feliz) ---
        if out.energy_trace.len() != steps + 1 || out.states.len() != n {
            return Err(AetherError::shape(
                [steps + 1, n],
                [out.energy_trace.len(), out.states.len()],
            ));
        }

        let channels = self.cfg.sensor_channels;
        let window = self.cfg.window;

        // --- amostragem dos nós ---
        let mut r_per_node = [0.0f64; 3];
        let mut r_sum = 0.0f64;
        let mut r_count = 0.0f64;
        let mut bancada_phase = [0.0f64; 4];
        let mut have_bancada = false;
        for (i, handle) in self.handles.iter().enumerate() {
            if let Some(f) = handle.latest() {
                r_per_node[i] = f.order_parameter;
                r_sum += f.order_parameter;
                r_count += 1.0;
                if i == 2 {
                    bancada_phase = f.phase;
                    have_bancada = true;
                }
            }
        }
        let order_parameter = if r_count > 0.0 { r_sum / r_count } else { 0.0 };

        // --- injeção da perturbação física ---
        if !self.perturbed && self.started.elapsed() >= self.cfg.perturb_after {
            self.perturbed = true;
            let xi = [
                self.cfg.acoustic_amplitude,
                self.cfg.acoustic_amplitude,
                0.0,
            ];
            let _ = self.handles[0].perturb_oscillatory(xi, self.cfg.acoustic_hz as f32);
        }

        // --- campo sensorial: projeção fasorial do Nó_Bancada ---
        if have_bancada {
            let pos = self.ring_pos;
            for c in 0..channels {
                let th = bancada_phase[c];
                let (s, co) = th.sin_cos();
                let noise = self.rng.gen::<f32>() * 2.0 - 1.0;
                self.ring[[2 * c, pos]] = (co as f32) + noise * self.cfg.sensor_noise as f32;
                self.ring[[2 * c + 1, pos]] = (s as f32) + noise * self.cfg.sensor_noise as f32;
            }
            self.ring_pos = (pos + 1) % window;
            self.ring_filled = (self.ring_filled + 1).min(window);
        }

        // copia o buffer circular na ordem temporal (mais antigo -> mais novo)
        let start = (self.ring_pos + window - self.ring_filled) % window;
        for c in 0..self.ring.nrows() {
            for k in 0..self.ring_filled {
                self.u[[c, k]] = self.ring[[c, (start + k) % window]];
            }
            for k in self.ring_filled..window {
                self.u[[c, k]] = 0.0;
            }
        }

        // --- interferência espectral ---
        self.wave.forward_into(self.u.view(), &mut self.v)?;
        let entropy_raw = self.wave.compute_spectral_entropy(self.u.view())?;
        let entropy_wave = self.wave.compute_wave_entropy();

        // --- leitura espectral -> vetor de viés h(t) ---
        {
            let spec = self.wave.wave_spectrum();
            let keep = self.wave.modes().min(spec.ncols());
            let c_out = spec.nrows();
            for i in 0..n {
                let mut acc = 0.0f64;
                for o in 0..c_out {
                    for k in 0..keep {
                        let w = self.readout[[i, o, k]];
                        let y = spec[[o, k]];
                        // Re(W * Y)
                        acc += (w.re * y.re - w.im * y.im) as f64;
                    }
                }
                self.raw[i] = acc;
            }
            // normaliza (média zero, desvio unitário) e aplica o ganho
            let mean = self.raw.iter().sum::<f64>() / n as f64;
            let var = self
                .raw
                .iter()
                .map(|x| (x - mean) * (x - mean))
                .sum::<f64>()
                / n as f64;
            let sd = var.sqrt().max(1e-12);
            for i in 0..n {
                self.h[i] = (self.raw[i] - mean) / sd * self.cfg.field_gain;
            }
        }

        // --- colapso termodinâmico ---
        let h2 = self.h.view().into_shape_with_order((1, n)).unwrap();
        let j3 = self.j.view().into_shape_with_order((1, n, n)).unwrap();
        self.solver
            .relax_into(IsingLandscape::new(j3, h2), &mut self.out)?;

        // --- preenche o quadro (sem realocar os Vec de `out`) ---
        let trace = self.out.energy_trace.row(0);
        let trace_slice = trace
            .as_slice()
            .ok_or_else(|| AetherError::Config("out.energy_trace deve ser C-contiguo".into()))?;
        out.energy_trace.copy_from_slice(trace_slice);

        let st = self.out.states.row(0);
        let st_slice = st
            .as_slice()
            .ok_or_else(|| AetherError::Config("out.states deve ser C-contiguo".into()))?;
        out.states.copy_from_slice(st_slice);

        out.elapsed = self.started.elapsed();
        out.order_parameter = order_parameter;
        out.r_per_node = r_per_node;
        out.entropy_raw = entropy_raw;
        out.entropy_wave = entropy_wave;
        out.energy = self.out.best_energy[0];
        out.energy_initial = self.out.energy_trace[[0, 0]];
        out.phase_bancada = bancada_phase;
        out.perturbed = self.perturbed;
        out.dropped_frames = self.dropped_frames;
        out.frame_cost_us = t0.elapsed().as_micros() as u64;
        Ok(())
    }

    /// Roda o organismo até `deadline` (ou para sempre se `None`), desenhando
    /// um quadro por período. Quadros que estouram o orçamento são **pulados**
    /// (sem acúmulo de atraso) e contabilizados em `dropped_frames`.
    pub fn run(
        &mut self,
        deadline: Option<Duration>,
        sink: &mut dyn crate::simulation::panel::PanelSink,
    ) -> Result<()> {
        let period = Duration::from_secs_f64(1.0 / self.cfg.hz);
        let mut next = Instant::now();
        loop {
            let now = Instant::now();
            if now < next {
                std::thread::sleep(next - now);
            }
            let frame = self.step()?;
            let cost = Duration::from_micros(frame.frame_cost_us);
            if cost > period {
                self.dropped_frames += 1;
                next = Instant::now() + period;
            } else {
                next += period;
                if next < Instant::now() {
                    next = Instant::now() + period;
                }
            }
            sink.draw(&frame)?;
            if let Some(d) = deadline {
                if self.started.elapsed() >= d {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Encerra os nós (consome o handle).
    pub fn shutdown(&mut self) {
        for h in self.handles.drain(..) {
            let _ = h.stop();
        }
    }
}

/// Mantem o organismo vivo enquanto o handle existe.
impl Drop for ClosedLoopOrganism {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Impressao compacta de um quadro (usada pelo painel simples).
pub fn format_frame(f: &OrganismFrame) -> String {
    let bits: String = f
        .states
        .iter()
        .map(|s| if *s > 0 { '1' } else { '0' })
        .collect();
    format!(
        "t={:>6.2}s R={:.4} H={:+.4} (H0={:+.4}) S_raw={:.3} S_wave={:.3} s*={} cost={}us drops={}",
        f.elapsed.as_secs_f64(),
        f.order_parameter,
        f.energy,
        f.energy_initial,
        f.entropy_raw,
        f.entropy_wave,
        bits,
        f.frame_cost_us,
        f.dropped_frames
    )
}

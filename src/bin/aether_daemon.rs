//! Módulo 8 — Daemon AETHER encarnado: transdutor físico real + enxame LAN.
//!
//! Substitui os sinais sintéticos da Fase 1 por telemetria física real da
//! máquina e acopla instâncias na rede local por **UDP multicast binário de
//! 40 bytes** — sem HTTP, sem JSON, sem um único token trafegando.
//!
//! Transdução (zero strings no sinal):
//! * **jitter de relógio** — atraso dos acordares de thread contra o deadline
//!   de 60 Hz (a pressão do scheduler é física e contínua);
//! * **pressão de CPU** — duração de uma unidade de trabalho fixa
//!   (calibração de *busy-loop*): sob carga, o mesmo trabalho leva mais tempo;
//! * **fluxo de rede** — bytes de fase recebidos por janela (o tráfego do
//!   próprio enxame é a variável física observada; em Unix `/proc/net/dev`
//!   complementa).
//!
//! Ciclo cognitivo a 60 Hz: onda física + fases dos vizinhos →
//! `WaveInterferenceBlock` → Surpresa F(t) →
//!   F alto  ⇒ `ThermodynamicDreamer::imagine_futures(T = 0.45)`;
//!   F baixo ⇒ colapso (T = 0.01) + imprint na `ComplexPhaseMemory`;
//! → `MorphogeneticField::evolve_topology` → telemetria viva no terminal.
//!
//! ```text
//! cargo run --release --bin aether-daemon
//! # em outra máquina (ou terminal) da mesma rede: as fases acoplam sozinhas
//! ```

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use ndarray::Array2;

use aether_engine::core::kuramoto_field::{KuramotoConfig, KuramotoPhaseNode, NodeHandle};
use aether_engine::core::wave_interference::WaveInterferenceBlock;
use aether_engine::error::Result;
use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use aether_engine::thermodynamic_dreamer::{DreamerConfig, ThermodynamicDreamer};

/// Grupo multicast do enxame (mesma LAN).
pub const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);
/// Porta do enxame.
pub const MULTICAST_PORT: u16 = 7777;

/// Configuração do daemon.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    /// Duração da execução (`None` = até interromper).
    pub duration: Option<Duration>,
    /// Taxa do ciclo cognitivo.
    pub hz: f64,
    /// Limiar de surpresa que dispara a imaginação contrafactual.
    pub alert_free_energy: f32,
    /// Temperatura de imaginação.
    pub dream_temperature: f64,
    /// Temperatura de ação.
    pub action_temperature: f64,
    /// Semente determinística.
    pub seed: u64,
    /// Janela do campo sensorial.
    pub window: usize,
    /// Dimensão da memória de fase / topologia.
    pub cognition_dim: usize,
    /// Amostras do sonhador.
    pub dream_samples: usize,
    /// Passos do sonhador.
    pub dream_steps: usize,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            duration: Some(Duration::from_secs(30)),
            hz: 60.0,
            alert_free_energy: 0.35,
            dream_temperature: 0.45,
            action_temperature: 0.01,
            seed: 0xA37E_0003,
            window: 128,
            cognition_dim: 32,
            dream_samples: 16,
            dream_steps: 120,
        }
    }
}

/// Semente xorshift64* determinística do transdutor.
struct TransducerRng(u64);

impl TransducerRng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0
    }
}

/// Transdutor físico: converte telemetria real do host em onda contínua.
pub struct HardwareTransducer {
    window: usize,
    ring: Vec<f32>,
    pos: usize,
    next: Instant,
    origin: Instant,
    baseline_ns: f64,
    /// Jitter do último acordar (ns), medido **antes** de avançar o prazo.
    last_jitter_ns: f64,
    last_net: u64,
    rng: TransducerRng,
}

/// Iterações da unidade fixa de trabalho — as mesmas na calibração e na
/// amostra (auditoria QA/M8: baseline e amostra com contagens diferentes
/// deixavam `load` pinado no piso).
const WORK_ITERS: u64 = 40_000;

impl HardwareTransducer {
    /// Cria o transdutor e calibra o custo da unidade fixa de trabalho.
    pub fn new(window: usize, seed: u64) -> Self {
        let mut t = Self {
            window,
            ring: vec![0.0f32; window],
            pos: 0,
            next: Instant::now(),
            origin: Instant::now(),
            baseline_ns: 1.0,
            last_jitter_ns: 0.0,
            last_net: 0,
            rng: TransducerRng(seed ^ 0x9E37_79B9_7F4A_7C15),
        };
        let mut acc = 0.0f64;
        for _ in 0..8 {
            let t0 = Instant::now();
            let _ = t.busy_work(WORK_ITERS);
            acc += t0.elapsed().as_secs_f64() * 1e9;
        }
        t.baseline_ns = (acc / 8.0).max(1.0);
        t
    }

    /// Unidade de trabalho fixa — o custo mede a pressão real do host.
    #[inline]
    fn busy_work(&mut self, iters: u64) -> u64 {
        let mut x = self.rng.0;
        let mut acc = 0u64;
        let mut i = 0u64;
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

    /// Captura o jitter do acordar corrente (o atraso contra o prazo previsto)
    /// e **só depois** avança o prazo — medir depois do avanço dava sempre ~0.
    pub fn tick_deadline(&mut self, hz: f64) {
        let now = Instant::now();
        self.last_jitter_ns = now.saturating_duration_since(self.next).as_secs_f64() * 1e9;
        self.next += Duration::from_secs_f64(1.0 / hz);
        if self.next < now {
            self.next = now;
        }
    }

    /// Amostra um instante físico; `net_bytes` é o contador cumulativo de
    /// bytes de fase observados pelo enxame.
    pub fn sample(&mut self, net_bytes: u64) -> f32 {
        let now = Instant::now();
        let jitter_ns = self.last_jitter_ns;

        let t0 = Instant::now();
        let _ = self.busy_work(WORK_ITERS);
        let work_ns = t0.elapsed().as_secs_f64() * 1e9;
        let load = (work_ns / self.baseline_ns - 1.0).clamp(-0.5, 4.0);

        let net_delta = net_bytes.saturating_sub(self.last_net);
        self.last_net = net_bytes;
        let net_wave = (net_delta as f64 / 40.0 / 60.0).clamp(0.0, 8.0);

        let carrier = (now.duration_since(self.origin).as_secs_f64() * 60.0).fract() as f32;
        let wave = (jitter_ns.ln().max(0.0) as f32) * 0.02
            + load as f32 * 0.4
            + net_wave as f32 * 0.15
            + self.rng.next_f32() * 0.05
            + (carrier * 0.10472).sin() * 0.05;

        self.ring[self.pos] = wave;
        self.pos = (self.pos + 1) % self.window;
        wave
    }

    /// Prazo corrente (usado pelo laço principal).
    pub fn deadline(&self) -> Instant {
        self.next
    }

    /// Campo contínuo atual `[C_in, L]`.
    pub fn field(&self, channels: usize) -> Array2<f32> {
        let mut out = Array2::<f32>::zeros((channels, self.window));
        for c in 0..channels {
            let shift = c as f32 * 0.7;
            for k in 0..self.window {
                let idx = (self.pos + self.window - 1 + k) % self.window;
                out[[c, k]] = self.ring[idx] * (1.0 + shift * 0.1) + shift * 0.01;
            }
        }
        out
    }
}

/// Telemetria de um ciclo.
#[derive(Clone, Copy, Debug)]
pub struct DaemonTelemetry {
    /// Latência do ciclo (µs).
    pub frame_us: u64,
    /// Coerência de fase da LAN `R(t)`.
    pub lan_coherence: f64,
    /// Peers distintos ouvidos no último ciclo (auditoria: `R > 0.9` só é
    /// válido com peers > 0).
    pub peers_heard: u32,
    /// Quadros descartados por estouro do orçamento de 60 Hz.
    pub dropped_frames: u64,
    /// Energia livre (erro absoluto + metabolismo) atual.
    pub free_energy: f32,
    /// Surpresa relativa ao baseline (dispara o sonho quando > limiar).
    pub surprise: f32,
    /// Energia da memória de fase.
    pub memory_energy: f32,
    /// Incerteza epistêmica do conjunto amostrado.
    pub epistemic_uncertainty: f64,
    /// Densidade topológica ativa.
    pub topological_density: f32,
    /// Estava sonhando neste ciclo?
    pub dreaming: bool,
    /// Número de ciclos.
    pub cycles: u64,
}

/// Estado do organismo encarnado.
pub struct AetherDaemon {
    cfg: DaemonConfig,
    node: Option<NodeHandle>,
    wave: WaveInterferenceBlock,
    dreamer: ThermodynamicDreamer,
    memory: ComplexPhaseMemory,
    field: MorphogeneticField,
    transducer: HardwareTransducer,
    c_in: usize,
    j_field: Vec<f32>,
    u: Array2<f32>,
    v: Array2<f32>,
    cycles: u64,
    /// Média móvel do erro relativo de reconstrução (baseline de surpresa).
    baseline_nmse: f32,
    /// Quadros que estouraram o orçamento de 60 Hz (cumulativo).
    dropped_frames: u64,
}

impl std::fmt::Debug for AetherDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AetherDaemon")
            .field("cfg", &self.cfg)
            .field("cycles", &self.cycles)
            .finish()
    }
}

impl AetherDaemon {
    /// Monta o organismo encarnado (nó multicast incluído).
    pub fn new(cfg: DaemonConfig) -> Result<Self> {
        let cognition_dim = cfg.cognition_dim;
        let window = cfg.window;
        let seed = cfg.seed;
        let hz = cfg.hz;
        let c_in = 2;
        let c_out = 4;
        let modes = 32;

        // Validação estrutural (panic=abort não perdoa config inválida).
        if hz <= 0.0 || !hz.is_finite() {
            return Err(aether_engine::error::AetherError::Config(
                "hz deve ser > 0 e finito".into(),
            ));
        }
        if cfg.dream_samples == 0 || cfg.dream_steps == 0 || cognition_dim < 2 {
            return Err(aether_engine::error::AetherError::Config(
                "dream_samples/dream_steps > 0 e cognition_dim >= 2 sao obrigatorios".into(),
            ));
        }

        // Nó de fase em multicast: descoberta automática na LAN. node_id é
        // único por instância (auditoria QA/M8: id fixo colapsava N instâncias
        // num único slot e invalidava R(LAN)).
        let node_cfg = KuramotoConfig {
            node_id: unique_node_id(),
            bind: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTICAST_PORT),
            peers: vec![SocketAddrV4::new(MULTICAST_GROUP, MULTICAST_PORT)],
            omega: [6.0, 6.01, 5.99, 6.005],
            k_coupling: 8.0,
            hz,
            adjacency: None,
            perturb_duration: Duration::from_millis(250),
            multicast: Some(MULTICAST_GROUP),
        };
        let node = Some(KuramotoPhaseNode::new(node_cfg)?.spawn());

        // Paisagem inicial determinística (xorshift64*).
        let mut rng = TransducerRng(cfg.seed ^ 0x5050);
        let scale = 1.0 / (cognition_dim as f32).sqrt();
        let mut j_field = vec![0.0f32; cognition_dim * cognition_dim];
        for i in 0..cognition_dim {
            for k in (i + 1)..cognition_dim {
                let v = ((rng.next_u64() >> 40) as f32 / (1u32 << 23) as f32 - 1.0) * scale * 0.5;
                j_field[i * cognition_dim + k] = v;
                j_field[k * cognition_dim + i] = v;
            }
        }

        Ok(Self {
            cfg,
            node,
            wave: WaveInterferenceBlock::with_max_len(c_in, c_out, modes, window, seed),
            dreamer: ThermodynamicDreamer::new(
                cognition_dim,
                DreamerConfig {
                    a0: 1.0,
                    gamma_damp: 0.35,
                    beta_chaos: 0.55,
                    seed: seed ^ 0xD12E,
                },
            ),
            memory: ComplexPhaseMemory::with_config(
                cognition_dim,
                PhaseMemoryConfig {
                    omega0: 0.0,
                    clamp_strength: 8.0,
                    eta_w: 0.12,
                    eta_delta: 0.08,
                    gamma_w: 0.002,
                },
            ),
            field: MorphogeneticField::new(cognition_dim, &j_field, MorphogenesisConfig::default()),
            transducer: HardwareTransducer::new(window, seed),
            c_in,
            j_field,
            u: Array2::<f32>::zeros((c_in, window)),
            v: Array2::<f32>::zeros((c_out, window)),
            cycles: 0,
            baseline_nmse: 0.0,
            dropped_frames: 0,
        })
    }

    /// Um ciclo cognitivo completo.
    pub fn cycle(&mut self) -> Result<DaemonTelemetry> {
        let t0 = Instant::now();
        self.transducer.tick_deadline(self.cfg.hz);

        // 1) onda física + fases dos vizinhos
        let frame = self
            .node
            .as_ref()
            .and_then(|n| n.latest())
            .unwrap_or_else(zero_frame);
        let _ = self.transducer.sample(frame.rx_bytes as u64);
        let fresh = self.transducer.field(self.c_in);
        self.u.assign(&fresh);

        // 2) predição espectral + surpresa
        self.wave.forward_into(self.u.view(), &mut self.v)?;
        let entropy_raw = self
            .wave
            .compute_spectral_entropy(self.u.view())
            .unwrap_or(0.0) as f32;
        let entropy_wave = self.wave.compute_wave_entropy() as f32;

        // Surpresa = erro relativo de reconstrução ACIMA do baseline + variação
        // de entropia. O |ΔH|/8 sozinho ficava limitado a 0.125 e nunca cruzava
        // o limiar de sonho; o erro relativo responde a mudanças reais do sinal.
        let rows = self.c_in.min(self.v.nrows());
        let mut se = 0.0f32;
        let mut su = 0.0f32;
        for c in 0..rows {
            for k in 0..self.cfg.window {
                let u = self.u[[c, k]];
                let d = u - self.v[[c, k]];
                se += d * d;
                su += u * u;
            }
        }
        let nmse = se / (su + 1e-6);
        let abs_error = nmse.clamp(0.0, 4.0);
        let entropy_surprise = ((entropy_raw - entropy_wave).abs() / 8.0).clamp(0.0, 0.5);
        let surprise = ((nmse - self.baseline_nmse).max(0.0) + entropy_surprise).clamp(0.0, 4.0);
        self.baseline_nmse = 0.95 * self.baseline_nmse + 0.05 * nmse;

        // spins contínuos derivados da fase local (entrada da morfogênese),
        // mapeados dos 4 canais de fase para a dimensão cognitiva completa
        let spins: Vec<f32> = (0..self.cfg.cognition_dim)
            .map(|i| (frame.phase[i % 4].cos() * 0.9) as f32)
            .collect();
        // A morfogênese vê o erro absoluto de reconstrução (mantém topologia
        // viva); o sonho é disparado pela SURPRESA relativa ao baseline.
        let free_energy = self.field.evolve_topology(abs_error, &spins, 0.05);

        // 3) decisão: sonhar (surpresa alta) ou colapsar (ação)
        let dreaming = surprise > self.cfg.alert_free_energy;
        let temperature = if dreaming {
            self.cfg.dream_temperature
        } else {
            self.cfg.action_temperature
        };
        let bias: Vec<f32> = (0..self.cfg.cognition_dim)
            .map(|i| (frame.phase[i % 4].sin() * 0.15) as f32)
            .collect();
        let ensemble = self.dreamer.imagine_futures(
            &self.j_field,
            &bias,
            temperature,
            self.cfg.dream_samples,
            self.cfg.dream_steps,
            0.05,
        );

        // 4) grava o padrão estável na memória de fase
        let first_sample = ensemble.samples.first();
        let pattern: Vec<f32> = (0..self.cfg.cognition_dim)
            .map(|i| {
                let bit = first_sample
                    .and_then(|sample| sample.get(i))
                    .copied()
                    .unwrap_or(-1);
                if bit > 0 {
                    std::f32::consts::FRAC_PI_4
                } else {
                    std::f32::consts::FRAC_PI_4 + std::f32::consts::PI
                }
            })
            .collect();
        self.memory.imprint(&pattern, 2);
        let memory_energy = self.memory.phase_energy();

        // 5) telemetria
        self.cycles += 1;
        Ok(DaemonTelemetry {
            frame_us: t0.elapsed().as_micros() as u64,
            lan_coherence: frame.order_parameter,
            peers_heard: frame.peers_heard,
            dropped_frames: self.dropped_frames,
            free_energy,
            surprise,
            memory_energy,
            epistemic_uncertainty: ensemble.epistemic_uncertainty,
            topological_density: self.field.density(),
            dreaming,
            cycles: self.cycles,
        })
    }

    /// Roda até o prazo (ou para sempre), reportando via callback.
    ///
    /// Quadros que estouram o orçamento de 60 Hz são contabilizados em
    /// `dropped_frames` (sem acúmulo de atraso).
    pub fn run(&mut self, mut report: impl FnMut(&DaemonTelemetry)) -> Result<()> {
        let started = Instant::now();
        let mut last_report = Instant::now();
        let period = Duration::from_secs_f64(1.0 / self.cfg.hz);
        loop {
            let now = Instant::now();
            let deadline = self.transducer.deadline();
            if now < deadline {
                std::thread::sleep(deadline - now);
            }
            let t0 = Instant::now();
            let telemetry = self.cycle()?;
            if t0.elapsed() > period {
                self.dropped_frames += 1;
            }
            if last_report.elapsed() >= Duration::from_millis(250) {
                report(&telemetry);
                last_report = Instant::now();
            }
            if let Some(d) = self.cfg.duration {
                if started.elapsed() >= d {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Encerra o nó de fase (o `Drop` de `NodeHandle` faz o join).
    pub fn shutdown(&mut self) {
        if let Some(node) = self.node.take() {
            let _ = node.stop();
        }
    }
}

impl Drop for AetherDaemon {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Frame zerado (estado inicial sem pacote recebido).
fn zero_frame() -> aether_engine::core::kuramoto_field::Frame {
    aether_engine::core::kuramoto_field::Frame {
        phase: [0.0; 4],
        order_parameter: 0.0,
        rx_packets: 0,
        rx_bytes: 0,
        tx_packets: 0,
        peers_heard: 0,
        last_latency_ns: 0,
    }
}

/// `node_id` único por instância: mistura pid + nanossegundos do boot do
/// processo. Duas instâncias nunca colapsam no mesmo slot (auditoria QA/M8).
fn unique_node_id() -> u32 {
    let pid = std::process::id() as u64;
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = pid ^ t.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x >> 32) as u32 | 0xD000_0000
}

/// Parse de CLI + validação (auditoria QA/M8: args ignorados eram bug).
fn parse_cli() -> std::result::Result<DaemonConfig, String> {
    let mut cfg = DaemonConfig::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        match flag.as_str() {
            "--help" | "-h" => {
                println!("aether-daemon [opções]");
                println!("  --duration <s>   duração em segundos (aceita 30 ou 30s)");
                println!("  --hz <f>         taxa do ciclo (padrão 60)");
                println!("  --seed <u64>     semente determinística");
                println!("  --alert <f>      limiar de surpresa para sonhar (padrão 0.35)");
                println!("  --window <n>     janela do campo sensorial [8, 8192]");
                println!("  --dim <n>        dimensão cognitiva [2, 256]");
                println!("  --samples <n>    amostras do sonhador (>0)");
                println!("  --steps <n>      passos do sonhador (>0)");
                println!("  --help           esta ajuda");
                std::process::exit(0);
            }
            "--duration" => {
                i += 1;
                let raw = args.get(i).ok_or("faltou valor para --duration")?;
                let v = raw
                    .strip_suffix('s')
                    .or_else(|| raw.strip_suffix('S'))
                    .unwrap_or(raw);
                let secs: f64 = v.parse().map_err(|e| format!("--duration inválido: {e}"))?;
                if !secs.is_finite() || secs <= 0.0 {
                    return Err("--duration precisa ser > 0 e finito".into());
                }
                cfg.duration = Some(Duration::from_secs_f64(secs));
            }
            "--hz" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --hz")?;
                cfg.hz = v.parse().map_err(|e| format!("--hz inválido: {e}"))?;
                if !cfg.hz.is_finite() || cfg.hz <= 0.0 || cfg.hz > 10_000.0 {
                    return Err("--hz fora de (0, 10000] e finito".into());
                }
            }
            "--seed" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --seed")?;
                cfg.seed = v.parse().map_err(|e| format!("--seed inválido: {e}"))?;
            }
            "--alert" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --alert")?;
                cfg.alert_free_energy = v.parse().map_err(|e| format!("--alert inválido: {e}"))?;
                if !cfg.alert_free_energy.is_finite() || cfg.alert_free_energy <= 0.0 {
                    return Err("--alert precisa ser > 0 e finito".into());
                }
            }
            "--samples" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --samples")?;
                cfg.dream_samples = v.parse().map_err(|e| format!("--samples inválido: {e}"))?;
                if cfg.dream_samples == 0 || cfg.dream_samples > 1024 {
                    return Err("--samples fora de [1, 1024]".into());
                }
            }
            "--steps" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --steps")?;
                cfg.dream_steps = v.parse().map_err(|e| format!("--steps inválido: {e}"))?;
                if cfg.dream_steps == 0 || cfg.dream_steps > 10_000 {
                    return Err("--steps fora de [1, 10000]".into());
                }
            }
            "--window" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --window")?;
                cfg.window = v.parse().map_err(|e| format!("--window inválido: {e}"))?;
                if cfg.window < 8 || cfg.window > 8192 {
                    return Err("--window fora de [8, 8192]".into());
                }
            }
            "--dim" => {
                i += 1;
                let v = args.get(i).ok_or("faltou valor para --dim")?;
                cfg.cognition_dim = v.parse().map_err(|e| format!("--dim inválido: {e}"))?;
                if cfg.cognition_dim < 2 || cfg.cognition_dim > 256 {
                    return Err("--dim fora de [2, 256]".into());
                }
            }
            other => return Err(format!("flag desconhecida: {other} (use --help)")),
        }
        i += 1;
    }
    // validação estrutural (panic=abort não perdoa config inválida)
    if cfg.dream_samples == 0 || cfg.dream_steps == 0 {
        return Err("dream_samples e dream_steps devem ser > 0".into());
    }
    if !cfg.alert_free_energy.is_finite() || cfg.alert_free_energy <= 0.0 {
        return Err("--alert precisa ser > 0 e finito".into());
    }
    Ok(cfg)
}

/// Fases a partir de spins (para a memória de fase).
#[allow(dead_code)]
fn phases_from_spins(s: &[i8], n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            if s[i] > 0 {
                std::f32::consts::FRAC_PI_4
            } else {
                std::f32::consts::FRAC_PI_4 + std::f32::consts::PI
            }
        })
        .collect()
}

fn main() -> Result<()> {
    let cfg = match parse_cli() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("erro de uso: {e}");
            std::process::exit(2);
        }
    };
    println!("AETHER daemon — organismo encarnado (multicast {MULTICAST_GROUP}:{MULTICAST_PORT})");
    let mut daemon = AetherDaemon::new(cfg)?;
    daemon.run(|t| {
        println!(
            "ciclo {:>6} | R(LAN)={:.3} peers={} | F={:.3} S={:.3} | E_mem={:+.3} | U={:.3} | topologia={:.1}% | {} | {}µs | drops={}",
            t.cycles,
            t.lan_coherence,
            t.peers_heard,
            t.free_energy,
            t.surprise,
            t.memory_energy,
            t.epistemic_uncertainty,
            t.topological_density * 100.0,
            if t.dreaming { "SONHANDO" } else { "AÇÃO" },
            t.frame_us,
            t.dropped_frames
        );
    })?;
    println!("daemon encerrado.");
    Ok(())
}

//! Módulo 3 — Acoplamento de fase P2P (Kuramoto) por datagramas UDP binários.
//!
//! Contrato do fio: **exatamente 40 bytes**, little-endian, sem texto, sem
//! `serde`, sem `tokio`. Cada nó é uma thread dedicada que:
//!
//! 1. drena o socket até `WouldBlock` (nenhum datagrama fica para trás);
//! 2. integra Kuramoto semi-implícito em `f64` com `dt = 1/hz`:
//!    `dθ_d/dt = ω_d + (K/N_ch) Σ_peers Σ_n A[d][n] sin(θ_peer_n − θ_d)`
//!    (mais a perturbação local ξ, quando ativa);
//! 3. publica o [`PhasePacket`] local a todos os peers;
//! 4. publica um [`Frame`] sem bloquear (`Arc<Mutex<Option<Frame>>>`).
//!
//! Nada aqui transmite comandos: o único canal entre nós é fase. A perturbação
//! nunca é anunciada — ela quebra a simetria e propaga **pelo termo
//! `sin(θ_n − θ_m)`**, exatamente como na especificação.
//!
//! `timestamp_ns` usa uma base monotônica compartilhada ([`std::time::Instant`]
//! em [`OnceLock`]) e serve **apenas** para telemetria de latência; nenhuma
//! correção de fase depende dele (os nós não têm relógio comum).
//!
//! Decisões de engenharia:
//! * `socket2` para `SO_REUSEADDR`, `SO_RCVBUF`/`SO_SNDBUF` = 4 MiB e modo
//!   não-bloqueante; o socket é então convertido para [`std::net::UdpSocket`]
//!   (mesmo descritor, sem `unsafe`).
//! * O laço quente não aloca: buffers de stack fixos, arrays `[f64; 4]`, sem
//!   `Vec` recriado e sem `String` (nomes de thread são criados uma única vez,
//!   no `new`/`spawn`).
//! * Contabilização: `rx_bytes` só soma datagramas de **40 bytes válidos**, de
//!   modo que `rx_bytes % 40 == 0` é sempre verdadeiro por construção.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use socket2::{Domain, Protocol, Socket, Type};

use crate::error::{AetherError, Result};

/// Tamanho do pacote de fase no fio (bytes).
pub const PACKET_SIZE: usize = 40;

/// Número de canais de fase por nó (fixo pelo protocolo).
const CHANNELS: usize = 4;

/// Tamanho do buffer de recepção do kernel (4 MiB, ARCHITECTURE §3).
const SOCKET_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// Buffer de recepção de stack: maior que 40 para *detectar* datagramas
/// inválidos (um buffer de 40 truncaria silenciosamente 41..=64 bytes).
const RECV_BUF_LEN: usize = 64;

const TAU: f64 = std::f64::consts::TAU;

// ---------------------------------------------------------------------------
// Pacote de fase
// ---------------------------------------------------------------------------

/// Pacote de fase de 40 bytes (little-endian no fio).
///
/// Layout `#[repr(C)]` (sem *padding*, `size = 40`, `align = 8`):
///
/// | offset | campo         | bytes |
/// |-------:|---------------|------:|
/// | 0      | `timestamp_ns`| 8     |
/// | 8      | `node_id`     | 4     |
/// | 12     | `phase`       | 16    |
/// | 28     | `xi`          | 12    |
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhasePacket {
    /// Carimbo monotônico de envio (telemetria de latência apenas).
    pub timestamp_ns: u64,
    /// Identificador do nó emissor.
    pub node_id: u32,
    /// Fase dos 4 canais em `[0, 2π)`.
    pub phase: [f32; 4],
    /// Perturbação local recente (3 primeiros canais).
    pub xi: [f32; 3],
}

// INV-K1: o contrato do fio é verificado em tempo de compilação.
const _: () = assert!(std::mem::size_of::<PhasePacket>() == PACKET_SIZE);
const _: () = assert!(std::mem::align_of::<PhasePacket>() == 8);

impl PhasePacket {
    /// Serializa em 40 bytes little-endian explícitos (sem `unsafe`).
    pub fn encode(&self) -> [u8; PACKET_SIZE] {
        let mut out = [0u8; PACKET_SIZE];
        out[0..8].copy_from_slice(&self.timestamp_ns.to_le_bytes());
        out[8..12].copy_from_slice(&self.node_id.to_le_bytes());
        for (d, value) in self.phase.iter().enumerate() {
            let off = 12 + d * 4;
            out[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (d, value) in self.xi.iter().enumerate() {
            let off = 28 + d * 4;
            out[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        out
    }

    /// Desserializa um datagrama de exatamente 40 bytes.
    ///
    /// Rejeita qualquer outro tamanho com [`AetherError::BadPacketLen`]
    /// (INV-K2).
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() != PACKET_SIZE {
            return Err(AetherError::BadPacketLen {
                len: buf.len(),
                expected: PACKET_SIZE,
            });
        }

        let mut word8 = [0u8; 8];
        word8.copy_from_slice(&buf[0..8]);
        let timestamp_ns = u64::from_le_bytes(word8);

        let mut word4 = [0u8; 4];
        word4.copy_from_slice(&buf[8..12]);
        let node_id = u32::from_le_bytes(word4);

        let mut phase = [0.0f32; 4];
        for (d, slot) in phase.iter_mut().enumerate() {
            let off = 12 + d * 4;
            word4.copy_from_slice(&buf[off..off + 4]);
            *slot = f32::from_bits(u32::from_le_bytes(word4));
        }

        let mut xi = [0.0f32; 3];
        for (d, slot) in xi.iter_mut().enumerate() {
            let off = 28 + d * 4;
            word4.copy_from_slice(&buf[off..off + 4]);
            *slot = f32::from_bits(u32::from_le_bytes(word4));
        }

        Ok(Self {
            timestamp_ns,
            node_id,
            phase,
            xi,
        })
    }
}

// ---------------------------------------------------------------------------
// Configuração e quadro
// ---------------------------------------------------------------------------

/// Configuração de um nó de fase.
#[derive(Clone, Debug)]
pub struct KuramotoConfig {
    /// Identificador do nó (vai no pacote; não é comando).
    pub node_id: u32,
    /// Endereço de bind (`127.0.0.1:0` nos testes — porta efêmera).
    pub bind: SocketAddrV4,
    /// Peers para broadcast (todos com todos nos testes).
    pub peers: Vec<SocketAddrV4>,
    /// Frequências naturais dos 4 canais (rad/s).
    pub omega: [f64; 4],
    /// Coeficiente de acoplamento `K` (padrão 8.0).
    pub k_coupling: f64,
    /// Taxa do laço (Hz); `dt = 1/hz`.
    pub hz: f64,
    /// Matriz de acoplamento entre canais; `None` ⇒ identidade.
    pub adjacency: Option<[[f64; 4]; 4]>,
    /// Duração de uma perturbação injetada.
    pub perturb_duration: Duration,
    /// Grupo multicast a entrar (`IP_ADD_MEMBERSHIP`), para descoberta P2P
    /// automática na LAN (Módulo 8). `None` = unicast puro.
    pub multicast: Option<Ipv4Addr>,
}

/// Estado observável de um nó em um instante.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Fase local dos 4 canais em `[0, 2π)`.
    pub phase: [f64; 4],
    /// Parâmetro de ordem médio `R(t) ∈ [0,1]`.
    pub order_parameter: f64,
    /// Datagramas válidos recebidos (cumulativo).
    pub rx_packets: u32,
    /// Bytes válidos recebidos (cumulativo; sempre múltiplo de 40).
    pub rx_bytes: u32,
    /// Datagramas enviados com sucesso (cumulativo).
    pub tx_packets: u32,
    /// `now − timestamp_ns` do último pacote válido (ns, saturating).
    pub last_latency_ns: u64,
}

// ---------------------------------------------------------------------------
// Relógio monotônico compartilhado (telemetria)
// ---------------------------------------------------------------------------

fn monotonic_base() -> &'static Instant {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now)
}

/// Nanossegundos desde a base monotônica do processo.
fn now_ns() -> u64 {
    let elapsed = monotonic_base().elapsed().as_nanos();
    elapsed.min(u64::MAX as u128) as u64
}

// ---------------------------------------------------------------------------
// Estado de peer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct PeerSlot {
    addr: SocketAddrV4,
    /// `node_id` aprendido do primeiro pacote deste peer (None até ouvir).
    node_id: Option<u32>,
    phase: [f64; CHANNELS],
    heard: bool,
}

// ---------------------------------------------------------------------------
// Comandos para a thread do nó
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Cmd {
    Perturb { xi: [f32; 3], freq_hz: f32 },
    Stop,
}

// ---------------------------------------------------------------------------
// Nó
// ---------------------------------------------------------------------------

/// Nó de fase Kuramoto com socket UDP dedicado.
#[derive(Debug)]
pub struct KuramotoPhaseNode {
    cfg: KuramotoConfig,
    socket: UdpSocket,
    local_addr: SocketAddrV4,
    phase: [f64; CHANNELS],
    adjacency: [[f64; CHANNELS]; CHANNELS],
    peer_slots: Vec<PeerSlot>,
    xi: [f32; 3],
    perturb_remaining: f64,
    perturb_time: f64,
    perturb_freq_hz: f32,
    rx_packets: u32,
    rx_bytes: u32,
    tx_packets: u32,
    last_latency_ns: u64,
}

impl KuramotoPhaseNode {
    /// Cria o nó: bind com tuning de socket e fases iniciais aleatórias.
    ///
    /// As fases iniciais vêm de um `ChaCha8` semeado por `node_id`, o que dá
    /// desalinhamento máximo **e** reprodutibilidade sem adicionar `seed` ao
    /// contrato de configuração.
    pub fn new(cfg: KuramotoConfig) -> Result<Self> {
        validate_config(&cfg)?;

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        // Tuning obrigatório (ARCHITECTURE §3). Falha não é fatal: o kernel
        // pode limitar o buffer — a física tolera perda de amostra.
        let _ = socket.set_recv_buffer_size(SOCKET_BUFFER_BYTES);
        let _ = socket.set_send_buffer_size(SOCKET_BUFFER_BYTES);
        socket.bind(&SocketAddr::V4(cfg.bind).into())?;
        socket.set_nonblocking(true)?;

        // Multicast (Módulo 8): entra no grupo ANTES da conversão para
        // `UdpSocket`, para que a associação valha para o mesmo socket.
        if let Some(group) = cfg.multicast {
            let _ = socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED);
            let _ = socket.set_multicast_loop_v4(true);
        }

        let socket: UdpSocket = socket.into();
        let local_addr = match socket.local_addr()? {
            SocketAddr::V4(addr) => addr,
            SocketAddr::V6(addr) => {
                return Err(AetherError::Config(format!(
                    "bind IPv6 inesperado para protocolo IPv4: {addr}"
                )))
            }
        };

        let adjacency = cfg.adjacency.unwrap_or([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);

        let peer_slots = cfg
            .peers
            .iter()
            .map(|addr| PeerSlot {
                addr: *addr,
                node_id: None,
                phase: [0.0; CHANNELS],
                heard: false,
            })
            .collect();

        let phase = initial_phases(cfg.node_id);

        Ok(Self {
            cfg,
            socket,
            local_addr,
            phase,
            adjacency,
            peer_slots,
            xi: [0.0; 3],
            perturb_remaining: 0.0,
            perturb_time: 0.0,
            perturb_freq_hz: 0.0,
            rx_packets: 0,
            rx_bytes: 0,
            tx_packets: 0,
            last_latency_ns: 0,
        })
    }

    /// Endereço efetivamente associado (porta real quando `bind` usou `:0`).
    pub fn local_addr(&self) -> SocketAddrV4 {
        self.local_addr
    }

    /// Substitui a lista de peers (antes do `spawn` ou via comando, em execução).
    ///
    /// Slots novos nascem "não ouvidos": até receber de cada peer configurado,
    /// [`Frame::order_parameter`] reporta `0.0` — nunca um `1.0` falso.
    pub fn set_peers(&mut self, peers: Vec<SocketAddrV4>) {
        self.cfg.peers = peers;
        self.rebuild_peer_slots();
    }

    /// Injeta uma perturbação DC local (`ξ` somado à fase por
    /// `perturb_duration`). Nada é anunciado no fio: só a fase muda.
    pub fn inject_perturbation(&mut self, xi: [f32; 3]) {
        self.start_perturbation(xi, 0.0);
    }

    /// Injeta uma perturbação oscilatória: `ξ · sin(2π f t)` somado à fase.
    /// Mesmo canal físico da DC; muda apenas a assinatura temporal.
    pub fn inject_oscillatory_perturbation(&mut self, xi: [f32; 3], freq_hz: f32) {
        self.start_perturbation(xi, freq_hz);
    }

    /// Avança um passo `dt` (dreno → integração → broadcast → quadro).
    pub fn step(&mut self, dt: f64) -> Result<Frame> {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(AetherError::Config(format!(
                "dt invalido: {dt} (deve ser finito e > 0)"
            )));
        }
        Ok(self.tick(dt))
    }

    /// Move o nó para uma thread de SO dedicada e devolve o handle de controle.
    pub fn spawn(self) -> NodeHandle {
        let node_id = self.cfg.node_id;
        let (cmd_tx, cmd_rx) = unbounded::<Cmd>();
        let state = Arc::new(Mutex::new(None::<Frame>));
        let state_thread = Arc::clone(&state);

        let join = thread::spawn(move || run_loop(self, cmd_rx, state_thread));

        NodeHandle {
            node_id,
            cmd_tx,
            state,
            join: Some(join),
        }
    }

    // -- internos ---------------------------------------------------------

    fn start_perturbation(&mut self, xi: [f32; 3], freq_hz: f32) {
        self.xi = if xi.iter().all(|value| value.is_finite()) {
            xi
        } else {
            [0.0f32; 3]
        };
        self.perturb_freq_hz = if freq_hz.is_finite() && freq_hz > 0.0 {
            freq_hz
        } else {
            0.0
        };
        self.perturb_remaining = self.cfg.perturb_duration.as_secs_f64();
        self.perturb_time = 0.0;
    }

    fn rebuild_peer_slots(&mut self) {
        self.peer_slots.clear();
        self.peer_slots
            .extend(self.cfg.peers.iter().map(|addr| PeerSlot {
                addr: *addr,
                node_id: None,
                phase: [0.0; CHANNELS],
                heard: false,
            }));
    }

    fn apply_cmd(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Stop => return false,
            Cmd::Perturb { xi, freq_hz } => self.start_perturbation(xi, freq_hz),
        }
        true
    }

    /// Drena todos os datagramas disponíveis até `WouldBlock`.
    fn drain(&mut self) {
        let mut buf = [0u8; RECV_BUF_LEN];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    if len != PACKET_SIZE {
                        // Datagrama inválido: não entra em `rx_bytes`.
                        continue;
                    }
                    let packet = match PhasePacket::decode(&buf[..len]) {
                        Ok(packet) => packet,
                        Err(_) => continue,
                    };
                    let received_at = now_ns();
                    self.last_latency_ns = received_at.saturating_sub(packet.timestamp_ns);
                    self.rx_packets = self.rx_packets.wrapping_add(1);
                    self.rx_bytes = self.rx_bytes.wrapping_add(PACKET_SIZE as u32);
                    let addr = match from {
                        SocketAddr::V4(addr) => addr,
                        SocketAddr::V6(_) => continue,
                    };
                    // Casamento por `node_id` (o pacote carrega a identidade).
                    // Em multicast a origem não é o endereço do grupo, então o
                    // endereço só é usado como fallback (unicast puro).
                    let can_learn = self.peer_slots.len() < 64;
                    let slot = self
                        .peer_slots
                        .iter_mut()
                        .find(|s| s.node_id == Some(packet.node_id));
                    let slot = match slot {
                        Some(slot) => Some(slot),
                        None => {
                            let by_addr = self
                                .peer_slots
                                .iter_mut()
                                .find(|s| s.addr == addr && s.node_id.is_none());
                            match by_addr {
                                Some(s) => {
                                    s.node_id = Some(packet.node_id);
                                    Some(s)
                                }
                                // topologia aprendida no fio: peer novo entra
                                // como slot próprio (limite físico de segurança)
                                None if can_learn => {
                                    self.peer_slots.push(PeerSlot {
                                        addr,
                                        node_id: Some(packet.node_id),
                                        phase: [0.0; CHANNELS],
                                        heard: true,
                                    });
                                    None
                                }
                                None => None,
                            }
                        }
                    };
                    if let Some(slot) = slot {
                        for (dst, src) in slot.phase.iter_mut().zip(packet.phase.iter()) {
                            *dst = f64::from(*src);
                        }
                        slot.heard = true;
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                // Erros transitórios de ICMP/porta fechada (Windows) não podem
                // derrubar o laço: o próximo tick tenta de novo.
                Err(_) => break,
            }
        }
    }

    fn tick(&mut self, dt: f64) -> Frame {
        self.drain();
        let now = now_ns();

        let k_over_channels = self.cfg.k_coupling / CHANNELS as f64;
        let mut coupling = [0.0f64; CHANNELS];
        let mut heard = 0usize;

        for slot in &self.peer_slots {
            if !slot.heard {
                continue;
            }
            heard += 1;
            for (d, theta_d) in self.phase.iter().enumerate() {
                let mut sum = 0.0f64;
                for (n, theta_peer) in slot.phase.iter().enumerate() {
                    sum += self.adjacency[d][n] * (theta_peer - theta_d).sin();
                }
                coupling[d] += sum;
            }
        }

        let (shape, perturbing) = if self.perturb_remaining > 0.0 {
            let shape = if self.perturb_freq_hz > 0.0 {
                (TAU * f64::from(self.perturb_freq_hz) * self.perturb_time).sin()
            } else {
                1.0
            };
            (shape, true)
        } else {
            (0.0, false)
        };

        let mut perturb_rate = [0.0f64; CHANNELS];
        if perturbing {
            for (d, xi_d) in self.xi.iter().enumerate() {
                perturb_rate[d] = f64::from(*xi_d) * shape;
            }
        }

        let mut next_phase = [0.0f64; CHANNELS];
        for (d, theta) in self.phase.iter().enumerate() {
            let rate = self.cfg.omega[d] + k_over_channels * coupling[d] + perturb_rate[d];
            next_phase[d] = (theta + rate * dt).rem_euclid(TAU);
        }
        self.phase = next_phase;

        if perturbing {
            self.perturb_time += dt;
            self.perturb_remaining = (self.perturb_remaining - dt).max(0.0);
        }

        let order_parameter = self.order_parameter(heard);

        let packet = PhasePacket {
            timestamp_ns: now,
            node_id: self.cfg.node_id,
            phase: [
                self.phase[0] as f32,
                self.phase[1] as f32,
                self.phase[2] as f32,
                self.phase[3] as f32,
            ],
            xi: self.xi,
        };
        let bytes = packet.encode();
        for peer in &self.cfg.peers {
            if self.socket.send_to(&bytes, peer).is_ok() {
                self.tx_packets = self.tx_packets.wrapping_add(1);
            }
        }

        Frame {
            phase: self.phase,
            order_parameter,
            rx_packets: self.rx_packets,
            rx_bytes: self.rx_bytes,
            tx_packets: self.tx_packets,
            last_latency_ns: self.last_latency_ns,
        }
    }

    /// `R = média_d |1/N Σ_j e^{iθ_j^d}|` sobre `self` + pacotes de peers.
    ///
    /// Enquanto algum peer **configurado** não foi ouvido, retorna `0.0`
    /// (INV-K5: o teste não pode convergir falsamente).
    fn order_parameter(&self, heard: usize) -> f64 {
        if !self.cfg.peers.is_empty() && heard < self.cfg.peers.len() {
            return 0.0;
        }
        let total = (heard + 1) as f64;
        let mut r_sum = 0.0f64;
        for (d, theta) in self.phase.iter().enumerate() {
            let (sin_self, cos_self) = theta.sin_cos();
            let mut re = cos_self;
            let mut im = sin_self;
            for slot in &self.peer_slots {
                if slot.heard {
                    re += slot.phase[d].cos();
                    im += slot.phase[d].sin();
                }
            }
            r_sum += (re * re + im * im).sqrt() / total;
        }
        (r_sum / CHANNELS as f64).clamp(0.0, 1.0)
    }
}

fn validate_xi(xi: [f32; 3]) -> Result<()> {
    if xi.iter().any(|value| !value.is_finite()) {
        return Err(AetherError::Config(
            "xi contem valor nao finito".to_string(),
        ));
    }
    Ok(())
}

fn validate_config(cfg: &KuramotoConfig) -> Result<()> {
    if !cfg.hz.is_finite() || cfg.hz <= 0.0 {
        return Err(AetherError::Config(format!(
            "hz invalido: {} (deve ser finito e > 0)",
            cfg.hz
        )));
    }
    if !cfg.k_coupling.is_finite() {
        return Err(AetherError::Config(format!(
            "k_coupling invalido: {}",
            cfg.k_coupling
        )));
    }
    if cfg.omega.iter().any(|w| !w.is_finite()) {
        return Err(AetherError::Config(
            "omega contem valor nao finito".to_string(),
        ));
    }
    if let Some(adj) = cfg.adjacency {
        if adj.iter().flatten().any(|a| !a.is_finite()) {
            return Err(AetherError::Config(
                "adjacency contem valor nao finito".to_string(),
            ));
        }
    }
    Ok(())
}

/// Fases iniciais pseudo-aleatórias (ChaCha8) determinísticas por `node_id`:
/// desalinhamento máximo e reprodução bit-a-bit entre execuções.
fn initial_phases(node_id: u32) -> [f64; CHANNELS] {
    use rand::Rng;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let seed = 0xA37E_0000_0000_0001u64 ^ u64::from(node_id).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut phase = [0.0f64; CHANNELS];
    for theta in phase.iter_mut() {
        *theta = rng.gen::<f64>() * TAU;
    }
    phase
}

// ---------------------------------------------------------------------------
// Laço da thread
// ---------------------------------------------------------------------------

fn run_loop(mut node: KuramotoPhaseNode, cmds: Receiver<Cmd>, state: Arc<Mutex<Option<Frame>>>) {
    // Backlog máximo de ticks executados em rajada após um oversleep do
    // agendador (Windows arredonda sleeps para ~15 ms); acima disso o relógio
    // é ressincronizado em vez de acumular atraso.
    const MAX_CATCHUP: u32 = 8;

    let period = Duration::from_secs_f64(1.0 / node.cfg.hz);
    let mut next = Instant::now();

    loop {
        // Espera o próximo tick em fatias de <= 1 ms, atendendo comandos.
        loop {
            let now = Instant::now();
            if now >= next {
                break;
            }
            let wait = (next - now).min(Duration::from_millis(1));
            match cmds.recv_timeout(wait) {
                Ok(Cmd::Stop) => return,
                Ok(cmd) => {
                    if !node.apply_cmd(cmd) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }

        // Ticks devidos (permite recuperar oversleep sem alterar dt = 1/hz).
        let mut executed = 0u32;
        while Instant::now() >= next {
            let frame = node.tick(period.as_secs_f64());
            if let Ok(mut slot) = state.lock() {
                *slot = Some(frame);
            }
            next += period;
            executed += 1;
            if executed >= MAX_CATCHUP {
                next = Instant::now() + period;
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

/// Controle remoto de um nó em execução.
#[derive(Debug)]
pub struct NodeHandle {
    node_id: u32,
    cmd_tx: Sender<Cmd>,
    state: Arc<Mutex<Option<Frame>>>,
    join: Option<thread::JoinHandle<()>>,
}

impl NodeHandle {
    /// Identificador do nó (o mesmo que vai no pacote).
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    /// Último quadro publicado (não bloqueia: tenta o lock e devolve cópia).
    pub fn latest(&self) -> Option<Frame> {
        self.state.lock().ok().and_then(|slot| *slot)
    }

    /// Perturbação DC (`ξ` somado à fase durante `perturb_duration`).
    pub fn perturb(&self, xi: [f32; 3]) -> Result<()> {
        validate_xi(xi)?;
        self.send(Cmd::Perturb { xi, freq_hz: 0.0 })
    }

    /// Perturbação oscilatória assinatura acústica: `ξ · sin(2π f t)`.
    pub fn perturb_oscillatory(&self, xi: [f32; 3], freq_hz: f32) -> Result<()> {
        validate_xi(xi)?;
        if !freq_hz.is_finite() || freq_hz < 0.0 {
            return Err(AetherError::Config(format!("freq_hz invalido: {freq_hz}")));
        }
        self.send(Cmd::Perturb { xi, freq_hz })
    }

    fn send(&self, cmd: Cmd) -> Result<()> {
        self.cmd_tx.send(cmd).map_err(|_| {
            AetherError::Degenerate("no Kuramoto ja encerrado: comando descartado".to_string())
        })
    }

    /// Sinaliza parada e faz `join` da thread do nó.
    pub fn stop(mut self) -> Result<()> {
        let _ = self.cmd_tx.send(Cmd::Stop);
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| {
                AetherError::Io(std::io::Error::other(
                    "thread do no Kuramoto terminou em panic",
                ))
            })?;
        }
        Ok(())
    }
}

impl Drop for NodeHandle {
    fn drop(&mut self) {
        // Sem `stop()` explícito: encerra a thread em vez de vazá-la.
        let _ = self.cmd_tx.send(Cmd::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

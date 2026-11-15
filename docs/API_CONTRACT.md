# AETHER — Contrato de API Canônico (v1.0)

> Documento normativo. Todos os agentes implementam **exatamente** estas assinaturas.
> Desvios são permitidos **apenas** se o agente postar `PROPOSTA:API_DESVIO` no fórum
> e o ORC-0 aprovar. O objetivo é permitir a integração do Módulo 4 sem retrabalho.

Convenção geral:

* Números contínuos: `f64` na fronteira física (solver), `f32` no domínio de onda e no wire.
* Nenhuma API recebe `String`/`&str` como **entrada de sinal**. Strings só em mensagens de erro.
* Todo estado mutável de hot path é pré-alocado; nada cresce durante a operação.
* Erros: `crate::error::{AetherError, Result}`.

---

## M1 — `src/core/thermodynamic_solver.rs` (feature `m1`)

```rust
use ndarray::{Array1, Array2, Array3, ArrayView2, ArrayView3};

/// Paisagem de energia de um lote de instancias de Ising.
#[derive(Clone, Copy)]
pub struct IsingLandscape<'a> {
    pub j: ArrayView3<'a, f64>,   // [B, N, N] simetrica, diagonal zero
    pub h: ArrayView2<'a, f64>,   // [B, N]
}

impl<'a> IsingLandscape<'a> {
    pub fn new(j: ArrayView3<'a, f64>, h: ArrayView2<'a, f64>) -> Self;
}

/// Resultado do relaxamento termodinamico.
#[derive(Debug, Clone)]
pub struct RelaxOutcome {
    pub states: Array2<i8>,         // [B, N] estado emergente = argmin da trajetoria
    pub initial_states: Array2<i8>, // [B, N] sign(x_0) usado de fato
    pub energy_trace: Array2<f64>,  // [B, steps+1] H(s_0), H(s_1), ...
    pub best_energy: Array1<f64>,   // [B] H do estado emergente
}

impl RelaxOutcome {
    pub fn with_capacity(batch: usize, n: usize, steps: usize) -> Self;
}

#[derive(Debug)]
pub struct ThermodynamicIsingSolver { /* ... */ }

impl ThermodynamicIsingSolver {
    pub fn new(batch: usize, n: usize, steps: usize, dt: f64) -> Self;
    pub fn reset(&mut self, seed: u64);
    /// Conveniencia: aloca o resultado.
    pub fn relax(&mut self, land: IsingLandscape<'_>) -> Result<RelaxOutcome>;
    /// Hot path: reutiliza buffers, **zero alocacao**.
    pub fn relax_into(&mut self, land: IsingLandscape<'_>, out: &mut RelaxOutcome) -> Result<()>;
    /// H(s) = -0.5 s' J s - h' s  (J [N,N], h [N], s: N valores em {-1,+1})
    pub fn energy(&self, j: &ArrayView2<'_, f64>, h: &ArrayView2<'_, f64>, s: &[i8]) -> f64;
}
```

Invariantes que M1 **deve** garantir:

* `INV-T1` `best_energy[b] <= energy_trace[b][0]` para todo `b` (elitismo).
* `INV-T2` `states[b] ∈ {-1,+1}^N`.
* `INV-T3` `energy_trace[b]` tem exatamente `steps+1` amostras.
* `INV-T4` `relax_into` nao aloca (verificado por contador de alocacao).
* `INV-T5` determinismo: `reset(seed)` + `relax` ⇒ mesma saida bit-a-bit.

## M2 — `src/core/wave_interference.rs` (feature `m2`)

```rust
use ndarray::{Array2, Array3, ArrayView2, ArrayView3};
use num_complex::Complex32;

#[derive(Debug)]
pub struct WaveInterferenceBlock { /* ... */ }

impl WaveInterferenceBlock {
    pub fn new(c_in: usize, c_out: usize, modes: usize, seed: u64) -> Self;
    pub fn with_max_len(c_in: usize, c_out: usize, modes: usize, max_len: usize, seed: u64) -> Self;

    /// u: [C_in, L] -> v: [C_out, L]
    pub fn forward(&mut self, u: ArrayView2<'_, f32>) -> Result<Array2<f32>>;
    /// Hot path: sem alocacao.
    pub fn forward_into(&mut self, u: ArrayView2<'_, f32>, out: &mut Array2<f32>) -> Result<()>;
    /// u: [B, C_in, L] -> v: [B, C_out, L]
    pub fn forward_batched(&mut self, u: ArrayView3<'_, f32>) -> Result<Array3<f32>>;

    /// Entropia espectral normalizada do campo de entrada, em [0,1].
    pub fn compute_spectral_entropy(&self, u: ArrayView2<'_, f32>) -> Result<f64>;
    /// Entropia do espectro JA transferido (caminho de onda), em [0,1].
    pub fn compute_wave_entropy(&self) -> f64;

    /// Espectro transferido [C_out, bins] (caminho de onda, sem projecao local).
    pub fn wave_spectrum(&self) -> ArrayView2<'_, Complex32>;
    /// Pesos da matriz de transferencia, ordem C [C_out, C_in, modes]
    /// (leitura: prova de invariancia de L). A implementacao devolve um
    /// slice contiguo; `to_owned()` reconstroi o Array3.
    pub fn r_phi(&self) -> &[Complex32];
    /// Pesos da projecao local 1x1, ordem C [C_out, C_in] (leitura).
    pub fn w_local(&self) -> &[f32];
    pub fn modes(&self) -> usize;
    pub fn c_in(&self) -> usize;
    pub fn c_out(&self) -> usize;
}
```

Invariantes que M2 **deve** garantir:

* `INV-W1` pesos (`r_phi`, `w_local`) **nao mudam** com `L` (mesmos valores, bit-a-bit).
* `INV-W2` `forward` aceita `L` = 128, 512, 2048, 4096 sem erro de dimensao.
* `INV-W3` saida tem shape `[C_out, L]` e todos os valores sao finitos.
* `INV-W4` `compute_spectral_entropy` de um campo coerente < de um campo de ruido.
* `INV-W5` o bloco atua como filtro de banda: entropia do espectro transferido
  (`compute_wave_entropy`) <= entropia do campo cru (com ruido de banda larga).

## M3 — `src/core/kuramoto_field.rs` (feature `m3`)

```rust
use std::net::SocketAddrV4;

pub const PACKET_SIZE: usize = 40;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhasePacket {
    pub timestamp_ns: u64,  // 8 B
    pub node_id: u32,      // 4 B
    pub phase: [f32; 4],   // 16 B
    pub xi: [f32; 3],      // 12 B
}                          // total = 40 B, align 8

impl PhasePacket {
    pub fn encode(&self) -> [u8; PACKET_SIZE];          // little-endian explicito
    pub fn decode(buf: &[u8]) -> Result<Self>;          // len != 40 => BadPacketLen
}

#[derive(Clone, Debug)]
pub struct KuramotoConfig {
    pub node_id: u32,
    pub bind: SocketAddrV4,
    pub peers: Vec<SocketAddrV4>,
    pub omega: [f64; 4],
    pub k_coupling: f64,
    pub hz: f64,
    pub adjacency: Option<[[f64; 4]; 4]>,
    pub perturb_duration: std::time::Duration,
    /// Grupo multicast a entrar (`IP_ADD_MEMBERSHIP`), Módulo 8. `None` = unicast.
    pub multicast: Option<std::net::Ipv4Addr>,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub phase: [f64; 4],
    pub order_parameter: f64,   // R(t) em [0,1]
    pub rx_packets: u32,
    pub rx_bytes: u32,
    pub tx_packets: u32,
    pub last_latency_ns: u64,
}

#[derive(Debug)]
pub struct KuramotoPhaseNode { /* ... */ }

impl KuramotoPhaseNode {
    pub fn new(cfg: KuramotoConfig) -> Result<Self>;
    pub fn local_addr(&self) -> SocketAddrV4;
    pub fn set_peers(&mut self, peers: Vec<SocketAddrV4>);
    pub fn inject_perturbation(&mut self, xi: [f32; 3]);
    pub fn inject_oscillatory_perturbation(&mut self, xi: [f32; 3], freq_hz: f32);
    pub fn step(&mut self, dt: f64) -> Result<Frame>;
    pub fn spawn(self) -> NodeHandle;
}

#[derive(Debug)]
pub struct NodeHandle { /* ... */ }

impl NodeHandle {
    pub fn node_id(&self) -> u32;
    pub fn latest(&self) -> Option<Frame>;
    pub fn perturb(&self, xi: [f32; 3]) -> Result<()>;
    /// Perturbacao OSCILATORIA (assinatura acustica): xi * sin(2*pi*f*t) durante
    /// `perturb_duration`. Mesmo canal fisico, so muda a assinatura temporal.
    pub fn perturb_oscillatory(&self, xi: [f32; 3], freq_hz: f32) -> Result<()>;
    pub fn stop(self) -> Result<()>;
}
```

Invariantes que M3 **deve** garantir:

* `INV-K1` `size_of::<PhasePacket>() == 40` e `align_of == 8` (const assert no fonte).
* `INV-K2` `decode` rejeita 39 e 41 bytes.
* `INV-K3` `encode`/`decode` round-trip preserva todos os campos.
* `INV-K4` o wire e **exclusivamente** UDP binario de 40 bytes (nenhum byte textual).
* `INV-K5` 4 nos com fases aleatorias atingem `R(t) > 0.95` em < 3 s.

---

## M4 — `src/simulation/closed_loop_organism.rs` (feature `m4`, dono ORC-0)

Consome M1/M2/M3 conforme o contrato acima. Nao pode alterar assinaturas publicas
dos modulos sem `PROPOSTA:API_DESVIO` aprovada.

```rust
pub fn step(&mut self) -> Result<OrganismFrame>;                    // aloca o quadro
pub fn step_into(&mut self, out: &mut OrganismFrame) -> Result<()>;  // zero alocacao
pub fn run(&mut self, deadline: Option<Duration>, sink: &mut dyn PanelSink) -> Result<()>;
```

`PanelSink` vive em `src/simulation/panel.rs`:

```rust
pub trait PanelSink {
    fn draw(&mut self, frame: &OrganismFrame) -> Result<()>;
    fn teardown(&mut self) -> Result<()> { Ok(()) }
}
```

---

## Fase 2 (M5–M7)

O contrato normativo da fase 2 é `ARCHITECTURE_PHASE2.md` §3. Desvios
implementados e aprovados:

* **M5** `recall()` retorna `(Vec<f32>, f32)` (normativo) e aloca; o caminho
  **zero-alloc** é `recall_into(...)`. `sparsity_ratio`/`density` e
  `try_imprint`/`try_recall` são extensões. O `imprint` usa projeção complexa
  (pseudo-inversa de Gram) e atinge p0/p1/p2 < 0.001 rad no invariante de
  3 padrões/40% mascarado (ver §3/M5).
* **M6** `temperature`/`dt` são `f64` (convenção física); há
  `imagine_futures_into(...)` zero-alloc e `try_imagine_futures(...)`.
* **M7** `sparsity_ratio()` = fração dissolvida; `density()` = fração ativa.
  `try_evolve_topology(...)` devolve `Result`; o wrapper `evolve_topology`
  retorna 0.0 em erro.
* **M8** CLI e telemetria em `src/bin/aether_daemon.rs`; não é API de
  biblioteca.

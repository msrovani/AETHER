# PROJETO AETHER: Protótipo de Computação Pós-Token Baseada em Física Contínua (Edição Rust)

> Esta é uma adaptação da especificação original (Python/NumPy/PyTorch) para **Rust**, com foco em
> execução nativa, previsibilidade de memória e **latência mínima entre sockets UDP**.
> A matemática e os invariantes físicos são idênticos aos da especificação original; muda o
> substrato de execução, o modelo de concorrência e as garantias de tempo.

---

## 1. Diretriz Arquitetural e Restrições Rígidas (Leia Antes de Codificar)

Você está construindo um motor cognitivo experimental inspirado em física de sistemas dinâmicos,
óptica ondulatória e sincronização de osciladores.

**PROIBIÇÕES ABSOLUTAS (não use em hipótese alguma):**

- **ZERO Tokenização:** sem BPE, WordPiece, vocabulários discretos, `String`/`&str` como entrada
  interna ou embeddings de token. Toda entrada é sinal contínuo (`f32`/`f64`): formas de onda,
  séries temporais multivariadas ou campos espaciais.
- **ZERO Transformers / Atenção Discreta:** sem `MultiheadAttention`, sem KV-cache, sem
  arquiteturas autoregressivas, sem `candle-transformers`, sem `tch`/`burn` com blocos de atenção.
- **ZERO Protocolos Semânticos de Rede:** entre nós distribuídos é proibido trafegar JSON, XML,
  Protobuf semântico, REST, gRPC ou comandos explícitos (`{"action": ...}`). Proibido inclusive o
  uso de `serde`, `serde_json`, `prost`/`tonic`, `postcard` ou `bincode` em qualquer caminho de rede.
  A comunicação ocorre **exclusivamente** por campos de fase de osciladores em datagramas UDP
  binários de tamanho fixo (40 bytes).
- **ZERO Backpropagation Global Obrigatório:** o motor de decisão opera por relaxamento
  termodinâmico (minimização de Hamiltoniano). Nenhum `Tensor` com `requires_grad`, nenhum
  otimizador de gradiente, nenhuma camada densa treinada por descida de gradiente é necessária para
  o funcionamento do sistema.
- **ZERO Alocação no Caminho Quente (hot path):** após o *warmup*, o laço de simulação a 60 Hz não
  pode alocar memória no heap. `Vec`/`Array` são dimensionados e reutilizados na inicialização.
- **ZERO `unsafe` fora do módulo de codec:** `#![forbid(unsafe_code)]` no crate; a única exceção é
  `core::kuramoto_field::codec`, que pode usar `#[allow(unsafe_code)]` com `bytemuck` para
  serialização *zero-copy* — e mesmo lá `unsafe` deve ser evitado se `bytemuck::pod_read_unaligned`
  e `bytemuck::bytes_of` resolverem.
- **ZERO Runtime assíncrono no laço de fase:** nada de `tokio`/`async-std` no Módulo 3. Timers e
  reatores de terceiros introduzem *jitter* de agendamento incompatível com o orçamento de latência
  (Seção 4). O laço é *bare-metal*: uma thread de SO por nó + `mio::Poll` + sockets não-bloqueantes.

**Stack permitida (versões mínimas, travadas em `Cargo.lock`):**

| Dependência | Versão | Papel |
|---|---|---|
| `rustc` | 1.78+ (edition 2021) | toolchain |
| `ndarray` | 0.16 | tensores densos batelados, views sem cópia |
| `rayon` | 1.10 | paralelismo por *chunks* nas dimensões de lote |
| `rustfft` + `realfft` | 6.x / 3.x | FFT real 1D/2D com planos pré-compilados |
| `bytemuck` | 1 | cast binário do pacote de 40 bytes |
| `socket2` | 0.5 | `SO_RCVBUF`, `SO_REUSEADDR`, `MSG_DONTWAIT`, buffer de envio |
| `mio` | 1.0 | reator de eventos (epoll/kqueue/IOCP/`WSAPoll`) |
| `crossbeam-channel` | 0.5 | passagem de estado entre threads sem *locks* |
| `rand` + `rand_chacha` | 0.8 / 0.3 | RNG determinístico (`ChaCha8Rng` com semente fixa) |
| `ratatui` + `crossterm` | 0.29 / 0.28 | painel de terminal do Módulo 4 |
| `thiserror` | 2 | erros tipados do crate |
| `criterion` | 0.5 (dev) | benchmarks com IC estatístico |

**Opcional / por feature:**
`candle-core` atrás de `--features cuda` (apenas como backend tensorial alternativo para o solver,
nunca como transformer); `wide` ou `std::simd` (nightly, `--features simd`) para vetorização manual
do integrador; `cudarc` para `rustfft` em GPU fica fora de escopo no v1;
a rota de GPU aprovada (cross-vendor, AMD/Intel/NVIDIA) é `wgpu` atrás de
`--features gpu` — ver `docs/HW_TARGET.md` §3/§4.

---

## 2. Estrutura de Diretórios a Ser Criada

Crate único com binário de simulação — mantém o layout original e evita *overhead* de *link* entre
crates; a separação de `features` já garante isolamento de build.

```text
aether_engine/
├── ARCHITECTURE.md
├── Cargo.toml               # [lib] + [[bin]] aether-organism, perfis release tunados
├── Cargo.lock               # commitado (binário determinístico)
├── .cargo/config.toml       # rustflags: -C target-cpu=native, codegen-units=1, LTO
├── rust-toolchain.toml      # canal mínimo travado
├── src/
│   ├── lib.rs               # #![forbid(unsafe_code)] + re-exports + abas de erro
│   ├── core/
│   │   ├── mod.rs
│   │   ├── thermodynamic_solver.rs   # Módulo 1 — Ballistic Simulated Bifurcation
│   │   ├── wave_interference.rs      # Módulo 2 — Operador Neural de Fourier
│   │   └── kuramoto_field.rs         # Módulo 3 — Sincronização de fase P2P (UDP)
│   ├── simulation/
│   │   ├── mod.rs
│   │   ├── closed_loop_organism.rs   # Módulo 4 — integração em tempo real
│   │   └── panel.rs                  # painel ratatui (60 Hz)
│   └── bin/
│       └── organism.rs               # entrypoint: `cargo run --release --bin organism`
├── tests/
│   ├── physical_invariants.rs        # os 3 invariantes obrigatórios
│   └── alloc_invariants.rs           # alocação zero no hot path (allocator contador)
├── benches/
│   ├── solver.rs
│   ├── wave.rs
│   └── kuramoto_latency.rs
└── examples/
    └── field_probe.rs                # injeta sinal real (mic/telemetria) no Módulo 2
```

**Perfis de build (obrigatório em `Cargo.toml`):**

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
overflow-checks = false
debug = 1            # mantém símbolos para profiling sem custo de execução

[profile.bench]
inherits = "release"
debug = 1
```

```toml
# .cargo/config.toml  (Linux/macOS; no Windows usar apenas o primeiro e ver nota)
[build]
rustflags = ["-C", "target-cpu=native"]
# NOTA: em CI (GitHub Actions) NÃO usar target-cpu=native — usar "x86-64-v2" para portabilidade
# dos artefatos de teste. Ver Seção 9.
```

---

## 3. Contratos de Performance e de Latência (verificáveis, não aspiracionais)

Toda meta abaixo é medida com `criterion` (mediana e p99) em hardware de referência declarado em
`benches/README.md` (CPU, núcleos, governor, `isolcpus`). Rodar benchmarks com o servidor de TUI
desligado e `taskset`/`nice -n -20` quando disponível.

| Caminho | Meta | Instrumento |
|---|---|---|
| Passo do integrador bSB, `N=64`, lote 1 | **≤ 50 µs** (p99) | `benches/solver.rs` |
| Relaxamento completo, 200 passos, `N=64` | **≤ 10 ms** | `benches/solver.rs` |
| FFT ida+volta, `C=8`, `L=2048` | **≤ 300 µs** | `benches/wave.rs` |
| `send_to` → `recv_from` loopback, pacote 40 B | **≤ 200 µs** (p99) | `benches/kuramoto_latency.rs` |
| Quadro completo do organismo (3 nós + onda + colapso) | **≤ 8 ms** (60 Hz com folga) | `benches/` + medidor interno |
| Alocações por quadro após warmup | **0** | `tests/alloc_invariants.rs` |

Regras de engenharia que sustentam essas metas:

1. **Buffers pré-alocados e reutilizados:** `x`, `y`, `j_scratch`, `fft_in`, `fft_out`, vetores de
   estados e a série de energia vivem na struct e são `clear()`+reuso, nunca recriados.
2. **Views, não cópias:** `ArrayView3`/`ArrayView2` em todas as assinaturas públicas de entrada;
   só o Módulo 3 copia bytes (é o contrato do fio).
3. **Planos FFT cacheados:** `RealFftPlanner` fica na struct; para `L` desconhecido de antemão, um
   `HashMap<usize, Arc<dyn RealToComplex<f32>>>` com *guarda* de tamanho máximo (ver 5.3).
4. **Sem `Box<dyn>` no hot path:** despacho estático; `dyn` só na borda de I/O.
5. **Afinidade e *spin* controlado:** cada nó Kuramoto pode ser fixado em um núcleo
   (`core_affinity`, atrás de `--features affinity`); o `Poll` usa *timeout* de 1 ms e
   `Event::readable` → dreno total do socket antes de dormir (`recursive drain` até `WouldBlock`).
6. **Socket tuning obrigatório:** `SO_RCVBUF` e `SO_SNDBUF` = 4 MiB (ou `net.core.rmem_max`),
   `SO_REUSEADDR`, `IP_TOS = 0x10` (baixa latência) e `MSG_DONTWAIT` na recepção. Em Linux,
   `recvmmsg`/`sendmmsg` atrás de `--features mmsg` (via `libc`), com *fallback* `recv_from` no
   Windows/macOS (`WSAPoll`/`kqueue` via `mio`).
7. **Relógio monotônico:** `std::time::Instant` para o escalonamento dos 60 Hz;
   `clock_gettime(CLOCK_MONOTONIC_RAW)` (Linux) para o carimbo `timestamp_ns` do pacote. Nunca
   `SystemTime` no pacote.
8. **Sem panic no hot path:** resultados `Result<_, AetherError>` na borda; dentro do laço,
   invariantes garantidos por construção (dimensões validadas no `new`).

---

## 4. Especificação Matemática e de Implementação por Módulo

### Módulo 1: `src/core/thermodynamic_solver.rs` — Relaxamento Termodinâmico

Solver de **Bifurcação Simulada Balística (bSB)** para otimização combinatória e tomada de decisão
como relaxamento de um Hamiltoniano de Ising:

$$H(\mathbf{s}) = -\frac{1}{2}\sum_{i,j} J_{ij}\, s_i s_j - \sum_i h_i s_i, \qquad s_i \in \{-1,+1\}$$

**Dinâmica contínua (integração simplética de Euler):**

$$\frac{dx_i}{dt} = a_0\, y_i, \qquad
\frac{dy_i}{dt} = -\big[a_0 - a(t)\big] x_i + c_0\Big(\sum_{j=1}^{N} J_{ij} x_j + h_i\Big)$$

Paredes perfeitamente inelásticas: se $|x_i| > 1$, então $x_i \leftarrow \operatorname{sign}(x_i)$ e
$y_i \leftarrow 0$.

Parâmetros: $a_0 = 1.0$; rampa linear $a(t) = \frac{t}{T_{\max}} a_0$ de $0$ a $a_0$;
$c_0 = \dfrac{0.5}{\sqrt{N}\,\operatorname{std}(J)}$, com `std(J)` calculado **por instância do
lote** (nunca global) e *clamp* em `std < 1e-12` → `c0 = 0.5 / sqrt(N)`.

**Contrato público (Rust):**

```rust
pub struct IsingLandscape<'a> {
    pub j: ndarray::ArrayView3<'a, f64>,   // [B, N, N], simétrico, diagonal zero
    pub h: ndarray::ArrayView2<'a, f64>,   // [B, N]
}

pub struct RelaxOutcome {
    pub states: ndarray::Array2<i8>,       // [B, N] em {-1, +1}
    pub energy_trace: ndarray::Array2<f64>,// [B, steps+1] — H(s0), H(s1), ...
    pub best_energy: ndarray::Array1<f64>, // [B] energia do melhor estado visitado
}

pub struct ThermodynamicIsingSolver {
    a0: f64, dt: f64, steps: usize,
    x: ndarray::Array2<f64>,  // [B, N] posições
    y: ndarray::Array2<f64>,  // [B, N] momentos
    hx: ndarray::Array2<f64>, // [B, N] scratch do gradiente: J·x + h
    x_best: ndarray::Array2<f64>,
    e_best: ndarray::Array1<f64>,
}

impl ThermodynamicIsingSolver {
    pub fn new(batch: usize, n: usize, steps: usize, dt: f64) -> Self; // aloca tudo
    pub fn reset(&mut self, seed: u64);                                 // x,y ~ U(-0.1,0.1)*rand_sym
    pub fn relax(&mut self, land: IsingLandscape<'_>) -> Result<RelaxOutcome, AetherError>;
    pub fn energy(&self, j: &ndarray::ArrayView2<'_, f64>, s: &[i8]) -> f64;
}
```

**Decisões de implementação obrigatórias:**

- **Elitismo explícito:** a cada passo, calcule $H(\operatorname{sign}(x))$ (escalar por item do
  lote) e guarde o melhor estado em `x_best`/`e_best`. O retorno é o **argmin da trajetória**, não o
  estado final bruto. Isso é o que garante o invariante 1 (Seção 8) de forma determinística:
  $H(\mathbf{s}_{\text{final}}) \le H(\mathbf{s}_0)$ sempre, com desigualdade estrita na prática.
- **Produto matriz-vetor sem `nalgebra` `DMatrix` intermediária:** para $N \le 128$, escreva o
  `gemv` manualmente em laço `for i { acc = h[i]; for j { acc += j[i*N+j]*x[j] } }` com `ndarray`
  em *slices* planos e `#[inline(always)]`, vetorizado pelo LLVM; para $N > 128$ use
  `faer`/`matrixmultiply` opcional (`--features faer`).
- **Paralelismo:** `rayon` sobre o eixo `B` (`par_chunks_mut` de linhas), nunca sobre `N` (o
  gradiente é dependente do estado global da instância).
- **Precisão:** integração em `f64`. Só converta para `f32` na fronteira com o Módulo 4 (vetor
  $\mathbf{h}(t)$) e nos pacotes UDP.
- **Reprodutibilidade:** `ChaCha8Rng` semeado por `reset(seed)`; o teste do invariante usa a mesma
  semente e obtém bit-a-bit o mesmo resultado.

---

### Módulo 2: `src/core/wave_interference.rs` — Interferência Espectral

Camada de **Operador Neural de Fourier 1D/2D** onde o sinal sensorial propaga e combina no domínio
da frequência, sem matriz de atenção espacial:

$$v(x) = \sigma\Big( W_{\text{local}}\, u(x) + \mathcal{F}^{-1}\big\{ R_\Phi(k)\odot
\mathcal{F}\{u\}\big\}(x) \Big)$$

**Mecânica física:**

1. `realfft::RealFftPlanner` converte $u \in \mathbb{R}^{B\times C_{in}\times L}$ em espectro
   `[B, C_in, L/2+1]` complexo.
2. Truncamento modal: mantêm-se os primeiros `modes` bins (padrão **32**), zerando o resto — isso
   descarta ruído de alta frequência e mantém $O(L\log L)$ com custo independente de `L` na
   modulação.
3. Matriz de transferência complexa $R_\Phi(k) \in \mathbb{C}^{C_{in}\times C_{out}\times K_{\max}}$
   modula **fase** (atraso) e **amplitude** (ganho/atenuação). Produto contraído sobre `C_in` e
   sobre os `modes`, com `ndarray`/`gemm` complexo (`num-complex::Complex32`).
4. `irfft` de volta ao domínio contínuo, somado à projeção local $1\times1$
   (`W_local: [C_out, C_in]`), seguido de ativação (`tanh`, não-ReLU: queremos simetria de fase).

**Contrato público:**

```rust
pub struct WaveInterferenceBlock {
    modes: usize,
    w_local: ndarray::Array2<f32>,              // [C_out, C_in]
    r_phi: ndarray::Array3<num_complex::Complex32>, // [C_out, C_in, modes] — fase + amplitude
    kernel_cache: HashMap<usize, Arc<dyn realfft::RealToComplex<f32>>>,
    inv_cache: HashMap<usize, Arc<dyn realfft::ComplexToReal<f32>>>,
    spectrum: ndarray::Array3<num_complex::Complex32>,
    time_domain: ndarray::Array3<f32>,
}

impl WaveInterferenceBlock {
    pub fn new(c_in: usize, c_out: usize, modes: usize, seed: u64) -> Self;
    /// u: [C_in, L] ou [B, C_in, L]. Invariante: mesmo `self` processa qualquer `L`.
    pub fn forward(&mut self, u: ndarray::ArrayView2<'_, f32>) -> Result<ndarray::Array2<f32>, AetherError>;
    pub fn forward_batched(&mut self, u: ndarray::ArrayView3<'_, f32>) -> Result<ndarray::Array3<f32>, AetherError>;
    /// Entropia espectral normalizada H = -Σ p_k ln p_k / ln K, com p_k = |U_k|² / Σ|U|².
    pub fn compute_spectral_entropy(&self, u: ndarray::ArrayView2<'_, f32>) -> Result<f64, AetherError>;
}
```

**Decisões de implementação obrigatórias:**

- **Invariância de resolução por construção:** $R_\Phi$ é indexado apenas por *bin* `0..modes`, sem
  qualquer parâmetro dependente de `L`. Logo os pesos têm forma fixa
  `[C_out, C_in, modes]` e processam `L = 128`, `512`, `2048` ou `4096` sem alteração. O
  `kernel_cache` recebe o plano de cada `L` na primeira chamada (alocação é permitida aí, não no
  laço).
- **`realfft` exige comprimento par** (e bom desempenho em tamanhos compostos por 2/3/5). Para `L`
  ímpar ou "ruim", faça *zero-padding* para a próxima potência de dois elegível e **registre isso na
  documentação** (`forward` retorna o `L` efetivo no doc-comment; o invariante 2 só testa pares).
- **Honestidade sobre equivariância:** repetir bins fixos não é equivariância a escala. Se
  `--features spectral-reparam` estiver ligada, `R_Phi` é amostrada em frequências **normalizadas**
  `k/K` (interpolação linear entre bins vizinhos), tornando a resposta aproximadamente invariante a
  mudança de taxa de amostragem. Documente a diferença nos dois modos.
- **Sem `Vec<Vec<f32>>`:** tudo `ndarray` contíguo em ordem C; `as_slice_mut().unwrap()` nos laços
  internos para eliminar checagens de *bounds* no caminho quente.
- **Inicialização:** $R_\Phi$ inicializado com amplitudes ~ $\mathcal{N}(0, 1/\sqrt{C_{in}\cdot
  modes})$ e fases ~ $\mathcal{U}(0, 2\pi)$; $W_{local}$ com Xavier uniforme. Nada de
  `requires_grad` — o bloco é usado como *filtro físico* em tempo de execução (a adaptação, se
  houver, é por regra local de Hebb/Oja opcional em `--features plasticity`, nunca por backprop).

---

### Módulo 3: `src/core/kuramoto_field.rs` — Acoplamento de Fase P2P (UDP binário)

Sincronização entre nós de hardware pelo **modelo de Kuramoto**:

$$\frac{d\theta_m^{(d)}}{dt} = \omega_m^{(d)} + \frac{K}{N}\sum_{n=1}^{N} A_{mn}\sin\big(\theta_n^{(d)}
- \theta_m^{(d)}\big) + \xi_m^{(d)}(t)$$

**Protocolo binário — 40 bytes, `#[repr(C)]`, little-endian, sem texto:**

```rust
pub const PACKET_SIZE: usize = 40;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PhasePacket {
    pub node_id: u32,              // 4 bytes
    pub timestamp_ns: u64,         // 8 bytes — CLOCK_MONOTONIC_RAW
    pub phase: [f32; 4],           // 16 bytes — θ ∈ [0, 2π)^4
    pub xi: [f32; 3],              // 12 bytes — perturbação local ξ
}
// total = 40 bytes
const _: () = assert!(std::mem::size_of::<PhasePacket>() == PACKET_SIZE);
const _: () = assert!(std::mem::align_of::<PhasePacket>() == 8);

impl PhasePacket {
    pub fn encode(&self) -> [u8; PACKET_SIZE];               // bytemuck::bytes_of + cópia
    pub fn decode(buf: &[u8]) -> Result<Self, AetherError>;  // len == 40 obrigatório
}
```

`decode` **rejeita** qualquer datagrama com `len != 40` (retorna `AetherError::BadPacketLen`). Em
`--features mmsg`, `sendmmsg`/`recvmmsg` operam direto sobre `[u8; 40]` com `iovec` no stack —
zero cópia e zero alocação.

**Contrato público:**

```rust
pub struct KuramotoConfig {
    pub node_id: u32,
    pub bind: std::net::SocketAddrV4,        // 127.0.0.1:0 no teste (porta efêmera)
    pub peers: Vec<std::net::SocketAddrV4>,
    pub omega: [f64; 4],                     // frequências naturais (rad/s)
    pub k_coupling: f64,                     // K padrão 8.0
    pub hz: f64,                             // 60.0
    pub adjacency: Option<[[f64; 4]; 4]>,    // A_mn entre os 4 canais; padrão identidade
}

pub struct KuramotoPhaseNode { /* socket, poll, buffers, histórico */ }

impl KuramotoPhaseNode {
    pub fn new(cfg: KuramotoConfig) -> Result<Self, AetherError>; // bind + tuning de socket
    pub fn local_addr(&self) -> std::net::SocketAddrV4;
    pub fn inject_perturbation(&mut self, xi: [f32; 3]);           // ξ_m ≠ 0
    pub fn step(&mut self, dt: f64) -> Result<Frame, AetherError>;  // avança e dreno os pacotes
    pub fn spawn(self) -> NodeHandle;                               // thread dedicada + mio::Poll
}

pub struct Frame {
    pub phase: [f64; 4],
    pub order_parameter: f64,   // R(t) ∈ [0,1]
    pub rx_packets: u32,
    pub rx_bytes: u32,
    pub last_latency_ns: u64,   // now - timestamp_ns (indicador de jitter)
}

pub struct NodeHandle { /* JoinHandle + crossbeam::Receiver<Frame> + Sender<Cmd> */ }
impl NodeHandle {
    pub fn latest(&self) -> Option<Frame>;
    pub fn perturb(&self, xi: [f32; 3]) -> Result<(), AetherError>;
    pub fn stop(self) -> std::thread::Result<()>;
}
```

**Parâmetro de ordem global:**

$$R(t) = \Big|\tfrac{1}{N}\sum_{n=1}^{N} e^{i\theta_n(t)}\Big| \in [0,1]$$

calculado por canal `d` e também como média dos 4 canais (o valor reportado em `Frame` é a média
ponderada uniforme).

**Decisões de implementação obrigatórias:**

- **Uma thread de SO por nó, `mio::Poll` com `timeout = 1 ms`:** o laço (a) drena todos os
  datagramas disponíveis até `WouldBlock`, (b) integra o passo de Kuramoto com `dt = 1/hz`
  (semi-implícito, `f64`), (c) emite o pacote para todos os `peers`, (d) publica `Frame` no canal
  sem bloquear (`try_send`, descartando o mais antigo se o consumidor atrasar — a física tolera
  perda de amostra, não bloqueio).
- **Propagação sem mensagens semânticas:** o único efeito de `inject_perturbation` é somar $\xi_m$ à
  fase local durante `t_perturb` (~250 ms). A quebra de simetria se propaga **pelo termo
  $\sin(\theta_n-\theta_m)$** — o pacote nunca diz "perturbação"; ele carrega fase e energia.
- **`SO_RCVBUF`/`SO_SNDBUF` via `socket2::Socket::set_recv_buffer_size`**, `set_nonblocking(true)`,
  `set_reuse_address(true)`; no Linux, `IP_TOS` via `libc` sob `cfg(target_os = "linux")`.
- **`timestamp_ns`**: no envio, `Instant::now()` convertido para base monotônica; na recepção,
  guarda-se a diferença; **não** se usa o timestamp para correção de fase (os nós não têm relógio
  comum) — é apenas telemetria de latência.
- **Janela de aquecimento no teste:** a coerência é medida depois do primeiro segundo (evita
  transiente de *bind*/ARP no localhost).

---

### Módulo 4: `src/simulation/closed_loop_organism.rs` — Organismo Cognitivo Integrado

Fluxo contínuo, sem tokens, a 60 Hz:

1. **Sensores contínuos:** 3 nós Kuramoto (`Nó_Ambiente`, `Nó_Móvel`, `Nó_Bancada`) acoplados em
   `127.0.0.1` com `K = 8.0`, atingindo $R > 0.95$.
2. **Perturbação física:** após a convergência, injeta-se no `Nó_Ambiente` um transiente ondulatório
   (pacote de $\xi \neq 0$ + rajada de 8–40 Hz sintetizada em `f32`).
3. **Propagação de fase → onda:** o deslizamento de fase observado no `Nó_Bancada` (série temporal
   dos 4 canais de $\theta$, amostrada a 60 Hz) é convertido em campo
   `[C_in=4, L=256]` (buffer circular de 256 amostras) e alimenta `WaveInterferenceBlock`, que
   filtra por interferência destrutiva e produz o vetor de viés externo
   $\mathbf{h}(t) \in \mathbb{R}^{N}$ (redução da última camada por `modes` → `N`).
4. **Colapso termodinâmico:** $\mathbf{h}(t)$ deforma a paisagem de energia
   ($J$ fixo, `h` dinâmico) e o `ThermodynamicIsingSolver` relaxa em `steps ≤ 200` até
   $\mathbf{s}^* \in \{-1,+1\}^{N}$.
5. **Painel:** `ratatui` imprime, a cada quadro: `R(t)`, entropia espectral, curva $H(t)$
   (sparkline dos 200 passos), $\mathbf{s}^*$ (bits), alocações/quadro (deve ser 0) e jitter p99
   dos pacotes.

**Contrato público:**

```rust
pub struct OrganismConfig {
    pub decision_dim: usize,      // N do Ising, padrão 64
    pub sensor_channels: usize,   // 4
    pub window: usize,            // 256 amostras
    pub steps: usize,             // 200
    pub hz: f64,                  // 60.0
    pub perturb_after: std::time::Duration, // 2 s
    pub seed: u64,
}

pub struct ClosedLoopOrganism { /* nós, bloco de onda, solver, painel */ }
impl ClosedLoopOrganism {
    pub fn new(cfg: OrganismConfig) -> Result<Self, AetherError>;
    /// Um quadro: [R(t), entropia, h(t), curva H(t), s*].
    pub fn step(&mut self, frame: &[Frame]) -> Result<OrganismFrame, AetherError>;
    pub fn run(&mut self, deadline: Option<std::time::Duration>) -> Result<(), AetherError>;
}
```

**Orçamento por quadro (8 ms):** drenar/passo dos 3 nós ≤ 1 ms · onda (L=256) ≤ 400 µs ·
relaxamento (N=64, 200 passos, lote 1) ≤ 4 ms · painel ≤ 1 ms · folga ≥ 1.5 ms.

Se o orçamento estourar, o quadro é **pulado** (não há acúmulo de atraso) e o contador
`dropped_frames` é exibido no painel. Nenhum `sleep` maior que o restante do período.

---

## 5. Critérios de Aceitação — `tests/physical_invariants.rs`

Todos os testes rodam com `cargo test --release` (o perfil `debug` é lento demais para o Invariante 3
e não reflete o produto). Sementes fixas, nenhuma dependência de rede externa, tudo em `127.0.0.1`
com portas efêmeras.

### 5.1 Invariante Termodinâmico

> A energia final $H(\mathbf{s}_{\text{final}})$ deve ser **estritamente menor** que a energia
> inicial $H(\mathbf{s}_0)$ em **100%** de **50** instâncias aleatórias de $J$ simétrica ($N = 64$).

```rust
#[test]
fn invariante_01_colapso_termodinamico() {
    const N: usize = 64;
    const CASES: usize = 50;
    let mut solver = ThermodynamicIsingSolver::new(1, N, 200, 0.05);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0xA37E_0001);
    for case in 0..CASES {
        let mut j = ndarray::Array2::<f64>::zeros((N, N));
        // J simétrico, entradas N(0, 1/sqrt(N)), diagonal = 0
        // ... preenchimento determinístico ...
        let h = ndarray::Array1::<f64>::zeros(N);
        let out = solver.relax(j.view()).expect("relax");
        let e0 = solver.energy(&j.view(), &s0_of(&out));   // H do estado inicial gravado
        let ef = out.best_energy[0];
        assert!(ef < e0, "caso {case}: H_final={ef} >= H_inicial={e0}");
        assert!(ef.is_finite());
    }
}
```

*Nota de implementação:* `RelaxOutcome::energy_trace[0]` é $H(\operatorname{sign}(x_0))$ com o
$x_0$ efetivamente usado — não recalcule com outro estado, ou o teste mede outra coisa.

### 5.2 Invariante de Resolução Ondulatória

> O `WaveInterferenceBlock` deve processar tensores de comprimento $L=128$, $L=512$ e $L=2048$ com
> **exatamente os mesmos parâmetros treináveis**, sem erro de dimensão.

```rust
#[test]
fn invariante_02_resolucao_independente() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let pesos_antes = block.r_phi.clone();
    for &l in &[128usize, 512, 2048] {
        let u = deterministic_field(4, l);
        let v = block.forward(u.view()).expect("forward");
        assert_eq!(v.shape(), &[8, l]);
        assert!(v.iter().all(|x| x.is_finite()));
        assert_eq!(block.r_phi, pesos_antes, "pesos não podem mudar com L");
    }
    // entropia espectral: campo coerente < campo aleatório
    let h_coherent = block.compute_spectral_entropy(sine_field(4, 512).view()).unwrap();
    let h_noise = block.compute_spectral_entropy(deterministic_field(4, 512).view()).unwrap();
    assert!(h_coherent < h_noise);
}
```

### 5.3 Invariante de Sincronização sem Mensagens

> 4 instâncias de `KuramotoPhaseNode` comunicando-se **exclusivamente** por datagramas UDP de
> **40 bytes**, com fases iniciais aleatórias, devem atingir coerência global $R(t) > 0.95$ em menos
> de **3 s**.

```rust
#[test]
fn invariante_03_sincronizacao_udp() {
    let nodes = (0..4).map(|id| KuramotoPhaseNode::new(cfg_loopback(id, 0xF00D + id as u64)))
        .collect::<Vec<_>>();
    let handles: Vec<_> = nodes.into_iter().map(|n| n.spawn()).collect();
    let t0 = std::time::Instant::now();
    let mut converged = false;
    while t0.elapsed() < std::time::Duration::from_secs(3) {
        if let Some(f) = handles.iter().filter_map(|h| h.latest()).reduce(mean_frame) {
            assert_eq!(f.rx_bytes % 40, 0, "todo datagrama deve ter exatamente 40 bytes");
            if f.order_parameter > 0.95 { converged = true; break; }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for h in handles { h.stop().unwrap(); }
    assert!(converged, "R(t) não atingiu 0.95 em 3 s");
}
```

**Anti-flakiness:** tolerância de amostragem (10 ms), `K = 8.0`, `dt = 1/60`, fases iniciais de
`ChaCha8Rng` com semente fixa, portas efêmeras (`:0`) e `SO_REUSEADDR`. Se o runner de CI tiver
apenas 1 núcleo, o teste usa `#[ignore]` sob `AETHER_SINGLE_CORE=1` — **documentado**, não
silenciado.

### 5.4 Invariantes adicionais de engenharia (obrigatórios, custo baixo)

`tests/alloc_invariants.rs`:

- **Alocação zero:** `#[global_allocator]` contador; após warmup, `X` quadros consecutivos do
  organismo devem registrar **0** alocações. Falha se `alloc_count` variar.
- **Codec:** `PhasePacket::encode/decode` ida-e-volta preserva todos os campos;
  `decode` de `39` e `41` bytes retorna `Err`; `size_of::<PhasePacket>() == 40` (já garantido por
  `const assert`, o teste evita que alguém remova a asserção).
- **Determinismo:** duas execuções do solver com a mesma semente produzem `states` idênticos
  bit-a-bit.

---

## 6. Benchmarks (`cargo bench`) — metas da Seção 3

- `benches/solver.rs`: `N ∈ {32, 64, 128}`, `B ∈ {1, 8, 64}`, 200 passos; relatar mediana e p99 por
  passo e por relaxamento.
- `benches/wave.rs`: `L ∈ {128, 512, 2048, 8192}`, `C_in=4, C_out=8, modes=32`; ida+volta e
  `compute_spectral_entropy`.
- `benches/kuramoto_latency.rs`: ping-pong de 100k pacotes de 40 B em loopback; histograma
  (`hdrhistogram`) com p50/p99/p999 e comparação `recv_from` vs `recvmmsg` (`--features mmsg`).
- `benches/organism.rs`: quadro completo (sem TUI), 600 quadros, medindo p99 e contagem de quadros
  pulados.

---

## 7. Portabilidade e CI

- **Linux (alvo primário):** `epoll`/`mio`, `recvmmsg`, `CLOCK_MONOTONIC_RAW`, `core_affinity`,
  `target-cpu=native` local.
- **Windows (alvo secundário, obrigatório compilar):** `WSAPoll` via `mio`, sem `recvmmsg`
  (fallback `recv_from`), `Instant` de *performance counter*. O Invariante 3 roda em
  `127.0.0.1` sem ajustes.
- **macOS:** `kqueue` via `mio`; `target-cpu=native` não se aplica — usar padrão.
- **CI (GitHub Actions):** `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` ·
  `cargo test --release` · `cargo test --release --features simd` (nightly, `continue-on-error`) ·
  benchmarks **não** rodam no CI (ruído); rodam em runner dedicado com `criterion --save-baseline`.
  Em CI, `RUSTFLAGS="-C target-cpu=x86-64-v2"` para reprodutibilidade.

---

## 8. Definição de Pronto (Definition of Done)

A tarefa só está concluída quando, em **uma única execução limpa**:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release            # 3 invariantes físicos + alocação zero + codec + determinismo
cargo run --release --features m4 --bin aether-organism -- --duration 10s --plain
bash scripts/check_organism.sh 10s   # asserts: R>0.95, entropia menor, H(s*) < H(s0)
```

produzem saída exitosa, e o painel do Módulo 4 exibe, na ordem: $R(t) > 0.95$ antes da perturbação,
queda visível de $R$ no instante da perturbação, recuperação da coerência, entropia espectral menor
após a interferência e $H(\mathbf{s}^*)$ estritamente abaixo de $H(\mathbf{s}_0)$.

**Proibições de verificação:** não é permitido (a) marcar teste como `#[ignore]` sem
`AETHER_SINGLE_CORE`, (b) afrouxar o limiar de 0.95, (c) reduzir as 50 instâncias, (d) substituir o
UDP por canais em memória "para o teste passar", (e) usar `unsafe` para ganhar desempenho fora do
módulo de codec. Cada um desses atalhos invalida o protótipo.

---

## 9. Prompt de Handoff para o Agente

> Leia `ARCHITECTURE.md` e implemente os módulos 1 a 4 em Rust **um por vez**, começando por
> `src/core/thermodynamic_solver.rs`. Após cada módulo, rode `cargo test --release` e
> `cargo clippy --all-targets -- -D warnings`; só avance quando o invariante correspondente da
> Seção 5 passar. Não simplifique a integração simplética (paredes inelásticas, rampa `a(t)`,
> $c_0$ normalizado por instância), não substitua `rustfft`/`realfft`, não introduza `tokio` no
> Módulo 3 e não troque o pacote de 40 bytes por `serde`. Se uma meta de latência da Seção 3 não for
> atingida, meça com `criterion` e relate o número obtido — não maquie o número.

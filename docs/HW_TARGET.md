# HW_TARGET — plataforma de hardware alvo (v1)

Fixação do hardware onde o AETHER roda e é medido. Decisão: **por enquanto, só
máquinas de mesa — PC, notebook e servidor**, com **CPU x86-64 multithread** e
**GPU de qualquer um dos três grandes vendors (AMD / Intel / NVIDIA)**. Mobile,
Android Auto/CarPlay e hardware dedicado (CAN/CSI) ficam **fora do escopo v1**
(ver `docs/TRANSDUCER_IO.md` para o estado dos contratos desses produtos).

## 1. Classes de máquina suportadas

| Classe | Configuração típica | Papel no AETHER |
|---|---|---|
| **Notebook** | 4–8 núcleos, 16 GiB RAM, iGPU (AMD Radeon / Intel Arc-Xe) | desenvolvimento, painel TUI, daemon em enxame (nó de rede) |
| **Desktop / workstation** | 8–16 núcleos, 32 GiB RAM, dGPU ou iGPU | hardware de referência de benchmarks, organismo de loop fechado a 60 Hz |
| **Servidor** | 16+ núcleos, ECC, governor `performance`, `isolcpus` opcional | medição de p99 estável, enxame multi-nó, backtest/dreamer em lote |

Mínimo absoluto: **2 núcleos físicos, 4 GiB RAM, placa de rede com multicast
(IPv4), SO com relógio monotônico**. A meta de 60 Hz do organismo (`≤ 8 ms`
por quadro) é contratada na **classe desktop**; notebook e servidor precisam
atingi-la em CPU — a GPU é aceleração, não desculpa.

## 2. CPU

* **ISA:** x86-64, **multithread obrigatório** (SMT welcome, mas as metas são
  medidas com `rayon` em *chunks* de lote, não em magia de frequência).
* **Baseline portátil:** `x86-64-v2` (SSE4.2/POPCNT). Artefatos de CI e bins
  distribuídos compilam para v2; build local usa `-C target-cpu=native`
  (ver `.cargo/config.toml` e `ARCHITECTURE.md` §2).
* **Não depender de:** AVX-512 (emulação no Windows de Intel de consumo),
  E-cores/P-cores híbridos com afinidade manual — o escalonador é quem decide;
  `--features affinity` existe para `isolcpus` em servidor, não é requisito.
* **Governor (Linux):** `performance` durante benchmark; anotado em
  `benches/README.md`. No Windows: plano "Alto desempenho".

## 3. GPU — um motor, três vendors

A regra é **não acoplar o motor a nenhum vendor**. Toda aceleração de GPU vive
atrás de `--features gpu` e fala **uma única API neutra**:

| Vendor | Caminho nativo | Caminho neutro (contrato do AETHER) |
|---|---|---|
| NVIDIA | CUDA (`cudarc`, opcional, *per-vendor*) | **Vulkan via `wgpu`** |
| AMD | ROCm/HIP (Linux) | **Vulkan via `wgpu`** |
| Intel | oneAPI/SYCL (opcional) | **Vulkan via `wgpu`** (Arc e iGPU Xe) |

Princípios:

1. **A rota suportada é `wgpu`** (Vulkan no Windows/Linux; DX12 e Metal são
   *fallbacks* do próprio `wgpu`, não alvos de teste). Um único backend abrange
   os três vendors — mesmo binário, mesmo shader WGSL.
2. **CUDA/ROCm/SYCL são acelerações opcionais por feature**, nunca requisito
   do motor. O crate sem `gpu` funciona completo (estado atual: CPU-only,
   `cudarc` para rustfft fora de escopo no v1 — `ARCHITECTURE.md` §1).
3. **Carga apta a GPU:** FFT em lote grande (`L ≥ 8192`), amostragem
   Boltzmann do `ThermodynamicDreamer` (milhares de caminhos em paralelo),
   backtest de morfogênese. **A rota de 60 Hz do organismo permanece em CPU**
   até a GPU provar determinismo aceitável (ver §4).
4. **Fallback obrigatório:** ausência de GPU (ou driver ausente) degrada para
   a rota CPU com um aviso — **nunca** um `panic`. O motor não sabe o que está
   plugado, na GPU como no transdutor.

## 4. Contrato de determinismo CPU ↔ GPU

* **CPU é a referência bit-a-bit.** Todo resultado GPU é validado contra a
  saída CPU (tolerância de ponto flutuante explícita e declarada por op —
  ex.: FFT `|Δ| ≤ 1e-5` relativo; integração bSB na GPU aceita reordenação
  de empate **somente se** `H(s_final) < H(s_0)` continuar estrito).
* Invariantes físicos (`tests/physical_invariants.rs`) rodam na rota CPU;
  uma suíte análoga `--features gpu` valida a rota GPU antes de qualquer
  aceleração entrar no hot path.
* Se o invariante quebrar na GPU, a feature volta a ser offline-only
  (dreamer/backtest) — sem exceção.

## 5. Rede (enxame M3/M8)

* LAN IPv4 com **multicast habilitado** (`239.255.42.99:7777`); Ethernet
  preferida a Wi-Fi para p99 do loopback (meta: `≤ 200 µs`).
* 2+ máquinas de qualquer classe suportada — o enxame é heterogêneo por
  construção (notebook + desktop + servidor é a combinação de teste padrão).
* Sem dependência de relógio sincronizado entre nós: o contrato é fase
  (`Kuramoto`), não timestamp comparado — `CLOCK_MONOTONIC_RAW` é local.

## 6. O que fica explicitamente FORA do escopo v1

* **Android / iOS / Android Auto / CarPlay** — copiloto veicular (produto A)
  é contrato pronto, implementação futura (`docs/TRANSDUCER_IO.md` §3).
* **ARM (Apple Silicon, Windows-on-ARM)** — compila eventualmente
  (`x86-64-v2` não se aplica), mas nenhuma meta de performance é contratada.
* **macOS** como plataforma medida — roda CPU-only, sem GPU no contrato.
* **Hardware físico dedicado** (CAN/OBD-II, ELM327, CSI Wi-Fi, microfone):
  só contratos, nenhum alvo de implantação neste momento.
* **TPU/NPU/chips de IA dedicados** — fora; `wgpu` cobre o que existe de
  GPU geral nos três vendors.

## 7. Matriz de teste obrigatória

| Config | CI | Benchmark de referência |
|---|---|---|
| x86-64-v2, CPU-only, Windows (notebook) | ✅ | secundária |
| x86-64-native, CPU-only, Linux desktop (governor perf) | manual | **principal** |
| desktop + GPU AMD (Vulkan) `--features gpu` | manual | §3/§4 quando a feature entrar |
| desktop + GPU NVIDIA (Vulkan) `--features gpu` | manual | idem |
| desktop + GPU Intel (Vulkan) `--features gpu` | manual | idem |
| servidor 16+ núcleos (Linux, `isolcpus`) | manual | p99 e enxame |

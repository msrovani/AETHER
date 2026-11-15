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

**Decisão aprovada: a rota de GPU do AETHER é `wgpu`** (um backend único —
Vulkan/DX12/Metal — cobre AMD, Intel e NVIDIA com o mesmo binário WGSL).
Toda aceleração de GPU vive atrás de `--features gpu` e fala **essa única
API neutra**; caminhos nativos por vendor são acelerações opcionais, nunca
rota de contrato:

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

## 8. Medições E6 — anatomia do ciclo e mapa de capacidade (2026-11-15)

Fonte: `examples/e6_profile.rs` · `target/aether_profile_e6.json` · máquina de
referência **Intel Core 7 240H** (6P+4E, AVX2 **sem** AVX-512, L1d 48 KB/P,
L2 1,25 MB/P, L3 24 MB, LPDDR5-6400).

Método: 1.000 quadros de warm-up + **10.000 medidos**; alocador contador armado na
janela; percentis por quadro. O quadro **não é bit-reprodutível entre instâncias**
(amostragem lê threads UDP reais) — mede-se distribuição, nunca execução única.

### 8.1 Orçamento real de um quadro (N=64, steps=60, hz=60)

| Estágio | média µs | p50 | p95 | p99 | p99.9 | % | ws KB |
|---|---:|---:|---:|---:|---:|---:|---:|
| **solver** | **132,1** | 120,3 | 209,1 | 289,9 | 512,9 | **84,5%** | 34 |
| readout | 10,9 | 10,2 | 12,4 | 18,3 | 76,6 | 6,9% | 129 |
| entropy | 4,6 | 4,4 | 4,9 | 6,6 | 47,5 | 3,0% | 10 |
| wave | 3,9 | 3,5 | 4,7 | 7,4 | 59,2 | 2,5% | 18 |
| actuator | 2,6 | 2,5 | 2,9 | 3,5 | 24,6 | 1,7% | 33 |
| ring_copy | 1,8 | 1,7 | 1,8 | 2,1 | 12,4 | 1,2% | 8 |
| sensor | 0,2 | 0,1 | 0,2 | 0,3 | 0,5 | 0,1% | 4 |
| sample | 0,1 | 0,1 | 0,1 | 0,2 | 0,6 | 0,1% | 0,1 |
| frame + tail | 0,1 | 0,1 | 0,1 | 0,2 | 0,3 | 0,1% | 1,5 |
| **TOTAL** | **156,3** | 142,9 | 246,7 | **320,6** | 536,1 | **99,98%** | **184,5** |

* Cobertura da instrumentação: **99,98%** (gate ≥ 95%).
* Alocações na janela de 10.000 quadros: **0**.
* `eq-t/s` medido: **818.844** (auditoria: 804.700 — Δ 1,8%).
* Solver: **7,44 GFLOP/s** em f64 (≈19% do pico AVX2 a 2,5 GHz).

### 8.2 Mapa de capacidade e penhasco (footprint único, f64)

| N | total µs | solver µs | GFLOP/s | footprint | nível | 60 Hz? |
|---:|---:|---:|---:|---:|---|---|
| 64 | 156 | 132,5 | 7,42 | 0,18 MB | L2 | ✅ 0,9% |
| 128 | 612 | 570,0 | 6,90 | 0,40 MB | L2 | ✅ 3,7% |
| 256 | 2.632 | 2.534,8 | 6,21 | **1,03 MB** | L2 (limite) | ✅ 15,8% |
| 512 | 12.125 | 11.868,9 | 5,30 | **3,04 MB** | L3 | ⚠️ 72,7% |
| 1024 | 52.068 | 51.318,3 | 4,90 | 10,05 MB | L3 | ❌ **3,1×** |

**Dois números, não um:**

* **Penhasco de cache em N≈256**, não 512. O footprint não é só `J` (n²·8 B):
  a matriz de leitura pesa `n·2048 B`. Em N=256, `J` (512 KB) + readout (512 KB)
  = **1,03 MB**, já no limite do L2 (1,25 MB). Em N=512 são 3,04 MB → L3.
  O GFLOP/s cai 6,21 → 5,30 nessa travessia (−15%).
* **Penhasco de tempo real em N≈512.** O orçamento de 60 Hz é **16.667 µs**;
  N=512 consome 72,7% dele e N=1024 estoura por 3,1×. Sem acelerar o kernel,
  **N=512 é o teto do organismo a 60 Hz**.

### 8.3 Orçamentos para os próximos experimentos

| Experimento | Precisa caber em | Orçamento | Situação |
|---|---|---:|---|
| **E1** PLH (papel–filler + readout) | +2% do ciclo | **~3 µs** | folga de tempo real: 16,5 ms |
| **E2b** roof dual + presolve | ≤ 5% do ciclo | **~7,8 µs** | folga: 16,6 ms |
| **E4** self-field 16ch | +1% do ciclo | **~1,6 µs** | folga: 16,6 ms |
| **F1** bridge SGDB (drain async) | 0 no T0 | **0 µs** | fora do caminho quente |

> **Distinção que o E6 torna obrigatória:** há **dois orçamentos diferentes**.
> (a) *Tempo real*: 16.667 µs por quadro a 60 Hz — o motor usa **0,94%** disso em
> N=64, então qualquer subsistema novo cabe com folga de 100×.
> (b) *Throughput* (`eq-t/s`): cada µs marginal custa métrica. É esse — não o
> 60 Hz — que exige que E1/E2b/E4 caibam em poucos µs.
> Confundir os dois leva a otimizar o que não é gargalo (hardware) e a ignorar o
> que é (o kernel do solver, 84,5%).

### 8.4 O que ainda não está medido

Os estágios `RG 64→16→4`, `ComplexPhaseMemory`, `Koopman/SINDy`,
`ThermodynamicDreamer`, `ActiveInference/CBF` e `MorphogeneticField` **não estão no
laço do organismo** — contribuem com **0 µs** ao ciclo atual. Medi-los exige
instrumentá-los isoladamente (experimento **E6-mod**), que mede *custo de
integração*, não *anatomia do ciclo*. O orçamento de integração de cada um está na
tabela §8.3.

---

## 8.5 E6b — kernel do solver vetorizado (antes × depois)

**O que mudou** (`src/core/thermodynamic_solver.rs`): o GEMV `hx = Jx + h` e a
energia `H(s)` passaram a usar `dot_f64`/`dot_i8` com **16 acumuladores
independentes** (Rust 100% safe, sem intrinsics). `s ∈ {-1,+1}` é convertido uma
vez por passo num buffer já existente (`self.g`) — **zero alocação nova** — para
que o segundo produto seja um dot `f64` puro.

**O que NÃO foi feito, e por quê:** a fusão algébrica proposta
`H = −½·Σ x_i(hx_i + h_i)` só é exata para o estado **contínuo** `x`. O algoritmo
precisa de `H(s)` com `s = sign(x) ≠ x`; fundir mudaria a semântica do
`energy_trace`, do elitismo e do critério de parada. Os dois GEMVs são
**necessários** — o ganho veio de ILP, não de álgebra.

### Curva de acumuladores (N=64, steps=60)

| Acumuladores | Cadeias YMM | GFLOP/s | Observação |
|---:|---:|---:|---|
| **16** | 4 | **17,1** | ótimo; divide 64/128/256/512/1024 exatamente |
| 24 | 6 | 10,4 | cauda de 16 elementos em N=64 cai no laço escalar |
| 32 | 8 | 12,3 | spill de registradores (8 YMM de acc + cargas) |

### Antes × depois

| Métrica | E6 | E6b | Ganho |
|---|---:|---:|---:|
| solver (N=64) | 132,1 µs | **57,5 µs** | **2,30×** |
| GFLOP/s | 7,44 | **17,10** | **2,30×** |
| total do quadro | 156,3 µs | **81,3 µs** | 1,92× |
| p99 do quadro | 320,6 µs | **170,2 µs** | 1,88× |
| `eq-t/s` | 818.844 | **1.574.229** | 1,92× |
| N=1024 | 52,07 ms | **16,60 ms** | **3,14×** |

### Orçamento de 60 Hz depois do E6b

| N | total µs | % de 16.667 µs | status |
|---:|---:|---:|---|
| 64 | 81 | **0,49%** | folga de 205× |
| 128 | 221 | 1,3% | folga |
| 256 | 914 | 5,5% | folga |
| 512 | 3.734 | **22,4%** | seguro |
| 1024 | 16.598 | **99,6%** | cabe no papel, **sem margem para jitter** |

**O teto de tempo real subiu de N=512 para N=1024**, mas de forma justa: sem
margem para a cauda (p99 de N=1024 = 18,5 ms > 16,7 ms). O teto **seguro**
continua **N=512**.

### Validação (E6b)

* `ACQ = 100.00` · **12/12 MEASURED** · `hot_path_bytes = 0` (auditoria cognitiva).
* `cargo test --release`: **24 alvos, 0 falhas**.
* `clippy --all-targets -D warnings` e `fmt --check`: verdes.
* Alocações na janela de 10.000 quadros: **0**.
* A associação de ponto flutuante mudou (~1 ULP): nenhum atrator mudou de Bacia.

> **Nota de engenharia:** a primeira tentativa desta rodada corrompeu o encoding
> do arquivo (`Set-Content -Encoding UTF8` releu UTF-8 como cp1252). O `clippy`
> acusou 9 caracteres invisíveis; o arquivo foi restaurado do git e a reversão
> cp1252→UTF-8 aplicada. **Nunca editar `.rs` com `Set-Content` no PowerShell 5.1.**



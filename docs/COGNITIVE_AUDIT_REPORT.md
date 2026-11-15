# AETHER — Relatório da Auditoria Cognitiva (Sprint de Fechamento)

> Estado: **12/12 MEASURED**, `real_coverage = 1.00`, **ACQ = 100.00**,
> `hot_path_bytes = 0`.  
> Histórico: baseline MSG 0057 (ACQ 91.07) → Bloco 1 BUFFY MSG 0060/0062
> (ACQ 98.24, 5/5/3) → **este fechamento** (CLAIM CURSOR MSG 0071).  
> Crate `aether_engine` · gate: `clippy --all-targets --all-features -D warnings`
> limpo, `cargo fmt --check` 0 diffs, `cargo test --release` **108 testes / 0
> falhas**.  
> Especificação: [`COGNITIVE_BENCHMARK_SUITE.md`](../COGNITIVE_BENCHMARK_SUITE.md).

## 1. Comandos

```powershell
cargo test --release --test cognitive_audit -- --nocapture   # notas + tabela + JSON
cargo bench --bench throughput_tps                            # vazão + RSS + allocs
$env:AETHER_AUDIT_STRICT = "1"                                # metas viram gate
```

Saída: `target/aether_scorecard.json` (campos `quotient`, `real_coverage`,
`hot_path_bytes`, `dimensions[]`).

## 2. Resultado global

| Métrica | MSG 0057 | Bloco 1 (0062) | **Fechamento** |
|---|---:|---:|---:|
| **AETHER Cognitive Quotient** | 91.07 | 98.24 | **100.00** |
| Proveniência | — | 5 MEASURED · 5 PARTIAL · 3 PROXY | **12 MEASURED** |
| `real_coverage` | — | 0.94 | **1.00** |
| Bytes alocados em hot-path (regiões armadas) | — | — | **0** |

> O ACQ oscila ≤ 1 ponto entre execuções só por contenção de CPU em T08/T12
> (UDP + rayon em paralelo); 4/4 execuções consecutivas do fechamento
> fecharam em 100.00, e o piso observado em carga foi 98.3 (T08 `colapso`
> estocástico antes da correção do instrumento — ver §5).

### 2.1 Medidas-chave

| Instrumento | Baseline | Fechamento | Meta |
|---|---:|---:|---:|
| T06 chattering do **estado discreto** `s*` | 0.7047 | 0.57–0.77 (inalterado) | — |
| T06 chattering de `continuous_action` | — | **0.057–0.063** | < 0.25 ✔ |
| T12 latência média (D=160, 50 fatos, K=6) | 111091 µs → 54.8 µs | **≈ 5–7 µs** | < 45.0 µs ✔ |
| T12 Recall@1 / SNR | — | 1.000 / 5.88σ | ≥ 0.90 / > 3σ ✔ |
| T09 SINDy `dẋ/dt` erro relativo (janela 128) | — | **1.2e-4** | < 0.04 ✔ |
| T01 Koopman *lifted* d=6→p=20, NRMSE H=5 | — | 0.0017 | — |
| T07 CBF `V(z_{t+1}) ≤ V_max` (2000 passos) | — | 0 violações | 0 ✔ |
| T04 frames holográficos 72 B + CRC-32 | — | 6/6 válidos, 0 allocs | ✔ |
| T06 torque RG (Camada 0), alinhamento | — | ↑ (assert), 0 allocs | ✔ |
| Throughput (`throughput_tps`) allocs em `step_into` | 0 | **0** | 0 ✔ |

## 3. Entregas do sprint

### 3.1 M4 — `OrganismFrame::continuous_action` (pendência 1.4)

* Buffer pré-alocado `continuous_action: Vec<f32>` (+ `continuous_action_delta`)
  em `OrganismFrame::with_capacity`.
* Canal contínuo = **campo médio local** `f_i = h_i + Σ_j J_ij s*_j`,
  normalizado pelo RMS e comprimido por `tanh` (mesmo sinal de `s*_i`, com
  amplitude de confiança em `[-1,1]`), seguido de **EMA passa-baixa**
  `α = dt/(τ+dt)`, `τ = actuator_tau = 0.1 s` (≈ 0.143 a 60 Hz).
* **Spins discretos `s* ∈ {-1,+1}` inalterados** (asserção no T06 e em
  `organism_step_into`). Zero alocação (reutiliza `raw`).
* Renomeia `actuation` → `continuous_action` (BUFFY MSG 0063 já havia
  introduzido o EMA; aqui o alvo passa a ser o campo médio).

### 3.2 Módulos em `src/core/`

| Módulo | API principal | Medido em |
|---|---|---|
| `koopman_sindy.rs` | `OnlineKoopmanOperator::lifted()` (`lift_state` 6→20, `observe_state`, `predict_steps`; `λ=0.99`, Sherman-Morrison com anti-windup de `P`); `SindyEquationDiscoverer::streaming` (`push`, `fit_window`, `coefficients`, `rhs`; janela circular W=128, derivada centrada de 4ª ordem, STLSQ com workspace de pilha) | T01, T03, T09 |
| `active_inference.rs` | `ActiveInferenceEngine::plan_step`: `G(k)=½‖ẑ_k−z*‖²−λU_k`, Boltzmann `exp(−G/T)`, CDF inversa com `u` externo, projeção `LyapunovCbf` (`V(z_{t+1}) ≤ V_max`) | T05, T07 |
| `renormalization.rs` | `RenormalizationHierarchy` 64→16→4 (parâmetro de ordem complexo) + `torque_into(omega_eff, k)` top-down na Camada 0 | T06, T11 |
| `swarm_speciation.rs` (**novo**) | `SwarmSpeciationEngine` (`Φ_niche = Φ_auto + α_div·min_dist_genoma`, `NicheRole::{Sentinel,Architect,Regulator}`, repulsão genômica), `HolographicFrame` 72 B + `crc32_ieee` (tabela `const`) | T04 |

Observação de honestidade: `koopman_sindy`, `active_inference` e
`renormalization` já existiam (outro nó) quando o sprint começou; foram
**estendidos** (API compatível), não reescritos. `rollout_into` do Koopman
genérico deixou de alocar (janela virtual sobre `z`/`out`).

### 3.3 FHRR (`src/core/fhrr.rs`)

* `FhrrStore::query_into` + `FhrrScratch`/`FhrrHit`: zero alocação,
  produto interno em `chunks_exact(8)`, cue mascarada (sem gather), 1
  `sin_cos` por posição visível, `cos ψ/sin ψ` pré-computados por (fato,traço),
  desvinculação `z = conj(papel) ⊙ Σ_t M_t e^{-iψ_t}` em uma varredura,
  nenhuma norma redundante.
* `query_match` agora delega a `query_into` (API pública preservada).

### 3.4 Auditoria (`tests/cognitive_audit.rs`)

* T01: + operador *lifted* (métricas `lifted_nrmse_h5`, `lifted_allocs`).
* T04: papéis/`Φ_niche`/frames via `SwarmSpeciationEngine` (+ asserts de CRC e 0 allocs).
* T06: chattering medido em `continuous_action` (< 0.25); torque RG em `omega_eff`.
* T07: `plan_step` com CBF (0 violações, 0 allocs).
* T09: SINDy streaming (`dẋ/dt` < 0.04, `fit_window` com 0 allocs).
* T12: `query_into` (0 allocs); alvo de latência 45 µs.
* Scorecard: `hot_path_bytes` (alocador contador, regiões armadas).

## 4. Gate

| Verificação | Resultado |
|---|---|
| `cargo clippy --all-targets --all-features -- -D warnings` | limpo |
| `cargo fmt --check` | 0 diffs |
| `cargo test --release` | **108** testes, **0** falhas |
| `cargo bench --bench throughput_tps` | allocs `step_into` = 0 (basal p50 154.7 µs) |

## 5. Ressalvas e limites

* **T08** é cronometrado: `elapsed_ms` = menor de 10 repetições; precisão
  (`colapso`) vem do 1º estímulo (o RNG do sonhador é determinista; repetições
  seguintes são estocásticas e não devem decidir a nota).
* **Metas do enunciado ainda curtas / não medidas:**
  * `ACQ ≥ 99.0` é atingido em execução serial/típica; sob carga alta de CPU
    pode cair < 99 por latência de T08/T12 (não por perda de capacidade).
  * `predict_steps` do Koopman *lifted* é a rampa linear em `ψ` re-elevada a
    cada passo; para dinâmicas muito não lineares o dicionário de 20 termos é
    limitado (T01 principal continua usando o embedding de atraso d=32).
* `homeostatic_density` / `min_density_floor` (M7) e kernel M5 (BUFFY) **não**
  foram tocados neste sprint.

## 6. Artefatos

| Arquivo | Conteúdo |
|---|---|
| `src/core/swarm_speciation.rs` | especiação + frames 72 B |
| `src/core/koopman_sindy.rs` | Koopman lifted + SINDy streaming |
| `src/core/active_inference.rs` | `plan_step` Boltzmann + CBF |
| `src/core/renormalization.rs` | `torque_into` |
| `src/core/fhrr.rs` | `query_into` zero-alloc |
| `src/simulation/closed_loop_organism.rs` | `continuous_action` |
| `tests/cognitive_audit.rs` | 12 dimensões, scorecard, JSON |
| `target/aether_scorecard.json` | notas + `real_coverage` + `hot_path_bytes` |

## 7. Fase 0 + Fase 1 (perfilamento + BAM opt-in) — CLAIM MSG 0083

Gate preservado após a entrega: **ACQ = 100.00**, **12/12 MEASURED**,
`real_coverage = 1.00`, `hot_path_bytes = 0`, `#![deny(unsafe_code)]` no
`lib.rs` (sem novos `unsafe` em `noise` / `affinity` / `fhrr_bam`).

Instrumento: `cargo run --release --example bam_probe --features "m4,m5,affinity"`.

### 7.1 Fase 0 — números reais

| Medida | Valor |
|---|---|
| T12 breakdown proxy f32 (D=160,K=6,P=50) | cue 2.18 µs · cleanup papel 2.95 µs · unbind 0.20 µs · cleanup valores 3.03 µs · **soma 8.37 µs** |
| T12 e2e FHRR f32 (mesmo harness) | **10.09 µs**, Recall@1=1.000, SNR=6.06σ |
| Basal p50/p99/p99.9 **solto** (carga em 16 núcleos) | 198.2 / 2145.8 / **8838.8 µs** |
| Basal p50/p99/p99.9 **pinado** (reflex isolado) | 194.7 / 391.7 / **416.8 µs** |
| Redução p99.9 com afinidade | **95.3%** (≥ 20% exigido) ✔ |
| Roofline wgpu (80 passos, analítico) | N=512 → 5.03 ms@50GB/s · N=1024 → 20.1 ms · N=4096 → 322 ms (GPU@300GB/s: 0.84 / 3.36 / 53.7 ms) |

### 7.2 Fase 1 — BAM vs FHRR

| | FHRR f32 | BAM u16 (`T12Bam`) |
|---|---:|---:|
| Recall@1 | 1.000 | **1.000** |
| SNR (σ) | 6.06 | 5.62 |
| latência consulta (µs) | 10.09 | **3.64** |
| mem bytes | 264880 | 52440 |
| speedup e2e | — | **2.77×** (≥ 1.5×) ✔ |

**Veredito:** `READY_FOR_INTEGRATION` na próxima rodada (não migrado neste
CLAIM — `fhrr.rs` permanece intocado). Store T12 BAM (~52 KB) **excede** o
orçamento L1d de 32 KB (fica em L2) — tradeoff reportado.

### 7.3 Artefatos novos (CLAIM 0083)

| Arquivo | Papel |
|---|---|
| `src/core/noise.rs` | `NoiseSource` + `DeterministicNoise`; `hw-entropy` opt-in |
| `src/core/affinity.rs` | pinagem via `core_affinity` (sem `unsafe` nosso) + orçamento L1d |
| `src/core/fhrr_bam.rs` | `T12Bam` + roofline + proxy de breakdown |
| `examples/bam_probe.rs` | medição Fase 0/1 |

Arquivos BUFFY e CURSOR (`fhrr.rs`, `koopman_sindy.rs`, `active_inference.rs`,
`renormalization.rs`, `cognitive_audit.rs`, `phase_memory.rs`, …) **não**
editados neste CLAIM.

## 8. Fase 5 — AETHER Hardware Extremus (CLAIM MSG 0077)

Protocolo do T12 (declarado, sem descarte de amostras): **50 fatos, K = 6
traços, 40 % visível, ruído ±0,25 rad**, percentis sobre todas as consultas.
Hardware: Core 7 240H (10 C / 16 T, híbrido P/E), Windows,
`-C target-cpu=native`. A latência varia **20–60 %** com builds concorrentes
de outros agentes: abaixo, máquina quieta (carga: ver nota).

### 8.1 Latência do RAG holográfico (T12) — BAM u16 vs FHRR f32

| Config | Recall@1 | p50 (µs) | p95 (µs) |
|---|---:|---:|---:|
| BAM D=160 | 1,000 | **2,10** | 2,30 |
| BAM D=256 (512 B/vetor) | 1,000 | **3,30** | 3,60 |
| BAM D=512 | 1,000 | 6,20 | 7,40 |
| FHRR f32 D=160 (baseline) | 1,000 | 4,50 | 5,90 |

* Speedup BAM/FHRR em D=160: **2,1×–3,2×** (p50, conforme a carga).
* `tests/phase5_hardware.rs`: D=256 p50 = 2,4 µs / p95 = 3,7 µs, **0 B alocados**
  (gate: mediana < 5 µs, até 3 tentativas por causa da carga).
* **Sob carga** (builds concorrentes) o mesmo código mediu p50 = 5,0 µs em
  D=256 — a meta "< 5 µs" é cumprida de forma folgada só em máquina quieta.
* A auditoria (`cognitive_audit`, T12 = 7,55 µs média, FHRR f32 D=160) **não
  foi migrada**: `tests/cognitive_audit.rs` e `fhrr.rs` não foram editados.
  O T12 BAM é medido por `phase5_hardware` e `benches/hardware_extremus`.

### 8.2 Micro-operações e demais módulos

| Medida | Valor |
|---|---:|
| `sin_cos` ×256: f32 / LUT BAM (1024×i16, 2048 B, interp. linear) | 2525 ns / 559 ns (**4,5×**) |
| `bind` / `unbind` D=256 (16 fasores/lote, `wrapping_add`) | 13,6 ns / 13,4 ns |
| similaridade mascarada D=256 | 78 ns |
| gaussiana: RDTSC+Murmur3+Box-Muller / semente determinística | 12,6 ns / 3,4 ns |
| `dx = Θ(x)·Ξ` (4 termos): nativo / interpretado / JIT Cranelift | 0,83 / 9,2 / **5,8 ns** |
| SPSC produtor→consumidor (2 threads, 1024 slots) | 87 Mmsg/s |
| `SindyJit` compilação (4 termos) | ≈ 140 µs |
| hot-swap sob avaliação contínua | 9,8 M avals, 0 rasgos, 119 módulos liberados |

Limites honestos: o JIT **não** bate código Rust nativo fixo (0,83 ns) — seu
valor é executar equações *descobertas em runtime* sem recompilar (1,6× o
interpretador); BUFFY mediu 96× no `sin_cos` com LUT de 256 sem
interpolação, aqui a LUT interpolada (erro máx. < 1e-4, testado) dá 4,5×.

### 8.3 Vazão cognitiva (`cargo bench --bench throughput_tps`)

| Regime | p50 (µs) | p99 (µs) | ciclos/s | eq-t/s | allocs |
|---|---:|---:|---:|---:|---:|
| Basal | 149,7 | 623,7 | 3876 | 4,96e5 | 0 |
| Vigilante | 557,1 | 12346 | 431 | 5,52e4 | 0 |
| Crítico | 1674 | 19207 | 204 | 2,61e4 | 0 |

Medida com a máquina sob carga de builds paralelos (cauda p99/p99.9
inflada); nenhuma mudança do hot-path do organismo foi feita na Fase 5.

### 8.4 Governador e *pinning* (`aether_governor`, `core_topology`)

* Reflexo (EventDrivenCore + ActiveInference + CBF de Lyapunov) no 1º
  núcleo físico de maior eficiência (lógico 0; irmão SMT 1 reservado);
  Koopman-SINDy e Swarm nos **E-cores** 12 e 13 (detecção real via
  `GetLogicalProcessorInformationEx`, com `EfficiencyClass`).
* Filas `SpscRingBuffer`; reflexo nunca bloqueia (fila cheia ⇒ descarta e conta).
  Execução de teste: 6000 eventos, 0 violações de CBF, 0 descartes, JIT
  publicado em voo (127 gerações). *Pinning* sem prioridade RT do SO.
* Sem a feature `affinity` ou se o SO recusar: *no-op* (`pinned=false`).
* Parser Linux (`sysfs`) testado por unidade, **não executado** em Linux.

### 8.5 Arquivos e features

`src/core/{bam_phase,bam_store,silicon_entropy,sindy_jit,spsc_ring,
core_topology,aether_governor}.rs`, `tests/phase5_hardware.rs`,
`benches/hardware_extremus.rs`. Features `jit` e `affinity` **default on**
(pedido do usuário; OPENCODE/BUFFY preferem opt-in — para desligar, remover
de `default` no `Cargo.toml`: sem a feature `jit` nenhum kernel é compilado e o interpretador seguro avalia `Θ·Ξ`). Os
módulos com `unsafe` (`silicon_entropy`: `_rdtsc`; `sindy_jit`; `spsc_ring`;
`core_topology`: FFI Win32) têm `#![allow(unsafe_code)]` local e justificado;
o `deny(unsafe_code)` do crate continua valendo para o resto.

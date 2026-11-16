# CORPO 3 — errata de execução (E4, E5, E7, E8, F2, F3)

**Autor:** OPENCODE (claim MSG 0121/0123, escopo confirmado em MSG 0128)
**Data:** 2026-11-16
**Base:** `aether_engine 0.0.118`, `cargo test --release`, artefatos em `target/`.

Este arquivo é **aditivo**: não reescreve as tabelas de
`O_PROBLEMA_DOS_3_CORPOS_NAO*.md`. As linhas de fechamento abaixo devem ser
inseridas pelos donos dos documentos (MSG 0122/0124) nas âncoras indicadas.

---

## 1. Arquivos entregues

| Arquivo | Papel | Claim |
|---|---|---|
| `src/core/self_field.rs` | E4 — self-field de 16 canais, EMA τ=100 ms, abstenção metacognitiva, topologia | 0121 |
| `tests/self_field_audit.rs` | E4 — ablação de 3 braços + gates | 0121 |
| `src/thermodynamic_dreamer.rs` | E5 — `ProgressCurriculum` (aditivo) | 0121 |
| `tests/curriculum_audit.rs` | E5 — readaptação + zero-alloc | 0121 |
| `src/core/s0_body.rs` | E7 — telemetria real + 4 atuadores + H1–H3 | 0121 |
| `examples/s0_body_probe.rs` | E7 — probe de 10^5 ciclos | 0121 |
| `tests/e7_body_gate.rs` | E7 — gate H1–H3 | 0121 |
| `src/core/reservoir.rs` | E8 — reservatório esparso congelado + ridge/Cholesky + RLS + busca conjunta | 0121 |
| `examples/reservoir_readout_bench.rs` | E8 — benchmark de 10^6 amostras | 0121 |
| `tests/e8_reservoir_gate.rs` | E8 — regressão/tempo/throughput | 0121 |
| `src/core/knowledge_fusion.rs` | F2/F3 — commit_run, supersede, gist, matriz 2D, `cue_into` escopado | 0123 |
| `tests/f2_consolidation_gate.rs` | F2 — perda zero, gist, R9 | 0123 |
| `tests/f3_fusion_2d_gate.rs` | F3 — 4 quadrantes, round-trip, R8 | 0123 |
| `src/core/mod.rs` | 4 linhas `pub mod` (m1) | aditivo |

Nenhum arquivo de outro claim foi editado.

---

## 2. Resultados medidos

### E4 — self-field (pergunta #5)

Três braços, mesma memória e mesmas 120 consultas (72 normais, 48 misturas
“plausíveis sem fonte”):

| Braço | ECE (erro absoluto, sem agrupamento) | Cobertura | T01 |
|---|---|---|---|
| (a) margem de recall | **0,3567** | 1,000 | 1,000 |
| (b) + self-field | **0,3238** | 0,733 | 1,000 |
| (c) + self-field + currículo | **0,3238** | 0,733 | 1,000 |

* IC bootstrap 97,5% de `ECE(b) − ECE(a)` = **−0,0168** (< 0 ⇒ ganho não é ruído).
* T01 **não** melhora: o recall subjacente é o mesmo; o self-field só decide calar.
* Erro de auto-predição pós-plasticidade: **0,0140** (adaptativo) × **0,0562**
  (campo congelado).
* Auto-consistência estrutural (`evolve_topology`): erro inicial **0,1507** →
  final **0,1391**.
* 16 canais; `RenormalizationHierarchy` 64 → 16 → 4; EMA de 100 ms.

**Gate #5:** ECE melhora com self-field **sem** melhorar T01. ✅

### E5 — currículo por progresso

Buffer com 35% de outliers não aprendíveis, readaptação A → B:

| Política | Ciclos até 80% de redução | Updates em outliers |
|---|---|---|
| Surpresa bruta | 8 | 7 |
| **Progresso `d(erro)/dt`** | **8** | **1** |

* Teto `T03 = 150` ciclos: **8 < 150**. ✅
* Zero alocação em 150 ciclos de atualização: **0 allocs**. ✅
* O ganho medido é de **eficiência amostral** (7 → 1 updates em ruído), não de
  velocidade absoluta: o RLS satura o alvo fácil em ambos os casos.

### E7 — corpo S0 (H1–H3), 10^5 ciclos

Telemetria real: jitter de `Instant`, contenção de CPU, escrita+leitura de 4 KiB
e datagrama UDP loopback. Quatro atuadores com syscalls reais: throttle
(20 µs), pausa (50 µs), alarme (flag atômico) e migração de afinidade.

| Métrica | Valor (release) | Gate |
|---|---|---|
| H1 entropia (com corpo) | **1,9725 nats** | > 0,5 ✅ |
| H2 Spearman U-invertido | **0,8808** | > 0,5 ✅ |
| H3 entropia (ablação) | 0,5539 | queda **71,9%** ≥ 30% ✅ |
| Atuadores | throttle 13 · afinidade 13 · pausa 13 · alarmes 2 | todos > 0 ✅ |

A ablação mantém os sentidos; remove só o **efeito** dos atos sobre o centro de
factibilidade. Sem corpo, o agente colapsa em uma meta.

### E8 — reservatório esparso congelado × ajuste conjunto

Reescrito após pesquisa em reservoir computing (Lukoševičius & Jaeger 2009;
Lukoševičius 2012; Jaeger 2001; NGRC 2025 sobre condicionamento). Mudanças:
matriz **esparsa** (10 conexões/nó), raio espectral por iteração de potência,
readout por **equações normais + Cholesky** (`RidgeReadout`), varredura de `λ`
sem reexecutar o reservatório, benchmark **Mackey–Glass τ=17** e **NARMA-10**, e
baseline conjunto por **40 passadas** de busca sobre `(ρ, α, input_scale)`.

| Braço / tarefa | NRMSE | Treino | Inferência |
|---|---|---|---|
| Frozen, Mackey–Glass, 10^6 | **2,33 × 10⁻⁵** | **1,57 s** | **1.733.418 pred/s** |
| Frozen, NARMA-10, 16k | **0,1286** | 0,24 s | 237.044 pred/s |
| RLS online, Mackey–Glass 20k | 0,00655 | 0,24 s | — |
| Joint (40 passadas), Mackey–Glass 4k | 1,31 × 10⁻⁴ | **39,6–45,7× mais lento** | — |

* Regressão de acurácia do congelado: **0%** (no teste, 1,22 × 10⁻⁴ vs
  1,31 × 10⁻⁴ do melhor candidato conjunto). ✅
* **5 sementes** (Mackey–Glass, 100k): NRMSE **2,00 × 10⁻⁵ ± 2,36 × 10⁻⁶**.
* **Ablação de esparsidade** (100k): esparso 10/nó = **0,154 s / 2,32 × 10⁻⁵**;
  denso (N−1) = 0,576 s / 1,14 × 10⁻⁵. O esparso é **3,75× mais rápido** por 2×
  de erro — ambos irrelevantes frente ao gate de 10⁻².
* `10^6` amostras em **1,57 s** < 60 s. ✅
* Throughput congelado **1.733.418/s** (N=100 esparso), ~8× o RLS e ~7× o
  baseline conjunto. ✅
* **40× reproduzido**: 40 passadas completas de busca dinâmica = **39,6–45,7×**
  o custo do readout congelado. É busca **sem gradiente** (não backprop),
  portanto compatível com o invariante `0 backprop`.
* Varredura de `λ` no mesmo `XᵀX`: `1e-10 → 2,45 × 10⁻⁵`; `1e-8 → 1,26 × 10⁻⁴`;
  `1e-6 → 1,22 × 10⁻³`. O melhor `λ` é escolhido por validação, não pelo teste.
* **Caveat:** o “40×” agora é medido contra 40 passadas de busca de dinâmica,
  não contra backprop do campo. O gradiente completo continua fora do núcleo.

### E8 — o que foi otimizado (e por quê)

Pesquisa aplicada (referências):

* Lukoševičius & Jaeger, *A tutorial on reservoir computing* (2009) — readout
  por equações normais, ridge e o truque de varrer `β` sem reexecutar o
  reservatório.
* Lukoševičius, *A practical guide to applying echo state networks* (2012) —
  `ρ(W) ≈ 1`, leak `α`, esparsidade 5–12 conexões/nó, washout.
* Jaeger (2001), *The “echo state” approach* — echo state property.
* ReservoirPy / EchoBay — `β ∈ [1e-8, 1e-4]`, `ρ ∈ [0.8, 1.2]`, `α ∈ [0.05, 1]`.
* NGRC, *On the emergence of numerical instabilities* (2025) — Cholesky/SVD vs
  inversa ingênua; condicionamento e regularização.
* SPIRES RLS — RLS é online, mas `O(N²)` por passo e sensível a `P`.

Mudanças de engenharia:

1. **Esparsidade:** `W` com 10 conexões/nó; passo `O(N·k)` em vez de `O(N²)`.
2. **Raio espectral** por iteração de potência na própria matriz esparsa.
3. **Ridge/Cholesky:** `XᵀX` e `Xᵀy` acumulados numa passada; solve `O(d³)`.
4. **Varredura de λ** no mesmo acumulador (`recalibrate`), sem reexecutar.
5. **Inferência congelada pura:** `step + dot`, sem atualizar o readout — é o
   que o throughput mede.
6. **Benchmarks padrão:** Mackey–Glass τ=17 e NARMA-10 (entradas e alvos
   separados).
7. **`fast_tanh` (Padé):** remove a chamada transcendental do passo; +47% de
   throughput sem mudar o NRMSE no regime saturado.
8. **Baseline conjunto honesto:** 40 passadas de busca sem gradiente.

Comparação RLS × ridge no mesmo trecho de Mackey–Glass (20k):

| Readout | NRMSE | Treino |
|---|---|---|
| Ridge/Cholesky (λ=1e-10) | **2,45 × 10⁻⁵** | ~0,02 s |
| RLS (λ=0,05, sem esquecimento) | 6,55 × 10⁻³ | 0,24 s |

O ridge é ~265× mais preciso e ~12× mais rápido; o RLS fica como opção
não-estacionária (T03), não como caminho de treino em lote.

### F2 — consolidação T1 + gist (pergunta #6)

| Gate | Medido |
|---|---|
| Eventos perdidos sob taxa nominal | **0** (320/320, 10 rodadas de 32) |
| Chamadas ao ledger no T0 | **0** |
| `supersede` em contradição | 1 (gist criado antes da poda) |
| Retenção de habilidade no gist | **1,000** (40/40) |
| R9 (abstenção por proveniência) | **1** (> 0) ✅ |

### F3 — fusão 2D + `cue_into` (pergunta #7)

| Gate | Medido |
|---|---|
| Quadrante “plausível sem fonte” (`CiteNoAct`) | **1,000** (100/100) ≥ 0,90 ✅ |
| Quadrante “ruído sem fonte” (`Abstain`) | **1,000** (300/300) |
| `Recall@1` direto → round-trip | 1,000 → 1,000 (perda **0%**) < 5% ✅ |
| Hits cruzados entre `ScopeId` (R8) | **0** (com candidato estrangeiro presente) ✅ |

O F1 corrigiu o `cue_into` para receber `ScopeId` e devolver `CueHit`
(`key`, `scope`, `authority`, `hits`) em MSG 0179/0180. O F3 adota **a mesma
forma** (`knowledge_fusion.rs` implementa `KnowledgeSource`) e mantém um ledger
próprio com *gist*; não edita `knowledge_bridge_f1.rs`.

---

## 3. Placar das 7 perguntas (proposta de errata)

| # | Status anterior | Status proposto | Evidência |
|---|---|---|---|
| 4 | ⏳ parcial (E2b/E2c) | inalterado | roof dual fechado com resultado negativo honesto (MSG 0142/0153) |
| 5 | ⏳ aberta (E4) | ✅ **respondida** | ECE 0,3567 → 0,3238; T01 inalterado; p97.5 < 0 |
| 6 | ⏳ aberta (F2) | ✅ **respondida** | 0 perdas; gist retém 100%; R9 > 0 |
| 7 | ⏳ aberta (F3) | ✅ **respondida** | quadrante 100%; perda 0%; R8 = 0 |

Placar: **5 respondidas, 1 parcial (E2c-2), 1 aberta (E2c-2/roof em QUBO denso)**.
A pergunta 4 permanece parcial por resultado empírico, não por falta de entrega.

---

## 4. Reprodução

```powershell
cargo test --release --test self_field_audit
cargo test --release --test curriculum_audit
cargo test --release --test e7_body_gate
cargo test --release --test e8_reservoir_gate
cargo test --release --test f2_consolidation_gate
cargo test --release --test f3_fusion_2d_gate
cargo run  --release --example s0_body_probe
cargo run  --release --example reservoir_readout_bench
```

Artefatos: `target/aether_e7_body.json`, `target/aether_e8_reservoir.json`.

---

## 5. Limitações declaradas

1. **E4:** ECE é erro absoluto de calibração sem agrupamento; com margens
   estreitas, o ECE por bins esconde a sobreconfiança. A cobertura cai para
   73,3% — abstenção tem custo, declarado.
2. **E5:** o ganho é de amostras em ruído, não de ciclos absolutos; ambos os
   braços saturam o alvo em 8 ciclos.
3. **E7:** métricas dependem do host (jitter/carga). A queda H3 variou de 45,8%
   a 75,5% entre execuções; o gate é ≥ 30%.
4. **E8:** o baseline “conjunto” é uma busca **sem gradiente** de 40 passadas
   sobre `(ρ, α, input_scale)`; o 45,7× é real contra essa busca, não contra
   backprop do campo. A acurácia do congelado é medida em teste separado da
   validação usada para escolher `λ` e a configuração.
5. **F2/F3:** o vazamento cross-scope do F1 (MSG 0140/0171) foi corrigido pelo
   dono em MSG 0179; o F3 já nasce com `CueHit` e filtro explícito. O F1 é a
   origem dos `DraftRecord`s; o `knowledge_fusion.rs` não o edita.
6. **Docs mestres:** não editados por ownership; linhas acima são a errata
   proposta.

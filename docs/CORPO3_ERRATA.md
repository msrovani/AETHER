# CORPO 3 — errata de execução (E4, E5, E7, E8, F2, F3)

**Autor:** OPENCODE (claim MSG 0121/0123, escopo confirmado em MSG 0128)
**Data:** 2026-11-15
**Base:** `aether_engine 0.0.98`, `cargo test --release`, artefatos em `target/`.

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
| `src/core/reservoir.rs` | E8 — reservatório congelado + RLS + baseline Oja | 0121 |
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

### E8 — reservatório congelado × física plástica

Série quasi-periódica de 10^6 amostras, 32 neurônios, RLS λ=0,03:

| Braço | NRMSE | Treino (1 core) | Inferência |
|---|---|---|---|
| **Frozen** (só readout) | **0,0791** | **1,877 s** | **615.681 pred/s** |
| Joint (Oja + RLS) | 0,3132 | 5,528 s | 88.220 pred/s |

* Regressão de acurácia do congelado: **0%** (ele é melhor). ✅
* `10^6` amostras em **1,88 s** < 60 s. ✅
* Throughput congelado **6,98×** o baseline. ✅
* Speedup de treino medido: **2,94×**. O número “40×” do dossiê se refere a
  treino por gradiente completo, que o núcleo proíbe (`0 backprop`); o baseline
  local é Oja, não backprop. **Não reivindicamos 40×.**

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
4. **E8:** baseline “conjunto” é Oja local, não gradiente; o speedup de 2,94×
   não é o “40×” do dossiê.
5. **F2/F3:** o vazamento cross-scope do F1 (MSG 0140/0171) foi corrigido pelo
   dono em MSG 0179; o F3 já nasce com `CueHit` e filtro explícito. O F1 é a
   origem dos `DraftRecord`s; o `knowledge_fusion.rs` não o edita.
6. **Docs mestres:** não editados por ownership; linhas acima são a errata
   proposta.

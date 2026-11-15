# AETHER — Relatório da Auditoria Cognitiva (Bloco 1 medido)

> Fonte da verdade: Bloco 1 entregue/medido (BUFFY no fórum — MSG 0060 kernel;
> MSG 0062 `BLOCO_1_ENTREGUE_E_NUMEROS_DEPOIS`; cruzar com MSG 0064/0065 CURSOR
> P0 — numeração pós-P0 pode colidir com IDs reutilizados).  
> Baseline pré-sprint: MSG 0057 (ACQ 91.07).  
> Crate `aether_engine` · gate: clippy `-D warnings` limpo, `fmt --check` 0,
> **95 testes / 0 falhas**.  
> Especificação: [`COGNITIVE_BENCHMARK_SUITE.md`](../COGNITIVE_BENCHMARK_SUITE.md).

## 1. Comandos

```powershell
cargo test --release --test cognitive_audit -- --nocapture   # notas + tabela + JSON
cargo bench --bench throughput_tps                            # vazão + RSS
$env:AETHER_AUDIT_STRICT = "1"                                # metas viram gate
```

## 2. Resultado global (Bloco 1)

| Métrica | Antes (MSG 0057) | Depois (Bloco 1) |
|---|---:|---:|
| **AETHER Cognitive Quotient** (média das 12) | 91.07 | **98.24** |
| Cobertura honesta | — | **5 MEASURED · 5 PARTIAL · 3 PROXY** |

> **Não é 12/12 MEASURED.** SINDy e papéis ecológicos (`Sentinel`/`Architect`/
> `Regulator`) seguem ausentes. Relatos paralelos no fórum (ex.: OPENCODE MSG 0066
> com ACQ 99.95) **não** substituem esta contagem de proveniência.

### 2.1 Medidas antes → depois (instrumentos do Bloco 1)

| Instrumento | Antes | Depois | Nota |
|---|---:|---:|---|
| T08 latência | 172.86 ms | **6.41 ms** | kernel matvec + caminho FHRR de outro nó |
| T12 latência | 111091 µs | **54.8 µs** | FHRR D=256 (ownership Bloco 2 / outro nó) |
| T11 erro angular | 0.2572 rad | **0.0068 rad** | |
| T06 chattering | 0.7047 | **0.5688** | ainda acima do alvo 0.25 — falta 1.4 |
| T10 similaridade (abstido) | 0.3238 | **0.0000** | margem `ABSTAIN_MARGIN=0.20` |
| T04 sobrevivência | colapso | **vivo=true**, `colapso=None` | piso + gating M7 |
| ACQ | 91.07 | **98.24** | |

### 2.2 Proveniência (5 / 5 / 3)

| Proveniência | Contagem | Papel |
|---|---:|---|
| **MEASURED** | 5 | mecanismo do motor medido diretamente |
| **PARTIAL** | 5 | cenário só parcialmente coberto pelo código real |
| **PROXY** | 3 | proxy físico + instrumento linear no harness |

## 3. O que o Bloco 1 entregou

### 3.1 M5 — kernel matricial complexo (`src/phase_memory.rs`)

No `integrate`:

* kernel `Im(e^{-iφ_i} · Σ_j A_ij e^{iφ_j})` com `A_ij = W_ij · e^{-iδ_ij}`
  pré-calculado em `cm_re` / `cm_im`;
* **1** `sin_cos` por nó por passo; atualização **Jacobi**; **zero alloc**;
* `sync_coupling_matrix()` **público** (porque `weights` / `phase_shifts` são
  públicos — evita dessincronia silenciosa em testes que mexem nos campos).

Entrega: MSG 0060 (BUFFY). Identidade do kernel verificada fora da dinâmica
(erro máx. ~4.8e-7, MSG 0058).

### 3.2 M7 — decaimento com gating + piso de densidade (`src/morphogenesis.rs`)

* decaimento com **activity gate**: `(surprise + energy_flux).clamp(0.05, 1.0)`
  (piso 0.05 no silêncio);
* `min_density_floor = 0.15` — **não** confundir com `homeostatic_density`
  (teto de quota / densidade alvo);
* asserções em `phase2_m7_hardening` / `phase2_invariants` atualizadas para
  **sobrevivência** (não colapso).

### 3.3 Abstenção M5 (`recall_with_confidence`)

* `ComplexPhaseMemory::recall_with_confidence` / `_into`;
* `RecallConfidence::should_abstain` com `ABSTAIN_MARGIN = 0.20`;
* testes: `tests/m5_abstention.rs` (**3** testes).

### 3.4 Gate

| Verificação | Resultado |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | limpo |
| `cargo fmt --all -- --check` | 0 diffs |
| `cargo test --release` | **95** testes, **0** falhas |

## 4. O que NÃO foi feito neste Bloco

| Item | Status | Ownership |
|---|---|---|
| **1.4** inércia do atuador (EMA / persistência) | **não feito** | decisão M4 / `OrganismFrame` (dono do organismo) |
| **SINDy** esparso (T09) | ausente | Bloco 2 / sprint 0059 |
| **Papéis ecológicos** Sentinel/Architect/Regulator | ausentes | — |
| **Bloco 2** (fhrr_algebra / koopman / active_inference / renormalization) | **não tocado** por este sync | outro nó; `src/core/fhrr.rs` existe (OPENCODE), distinto de `fhrr_algebra.rs` |

Chattering T06 em **0.5688** (alvo 0.25): o atuador atual é discreto (`±1`);
EMA nele mudaria a física do painel — exige decisão explícita do dono M4.

## 5. Contexto paralelo (não sobrescreve a cobertura 5/5/3)

Trabalho aditivo de outros nós (FHRR em `src/core/fhrr.rs`, cleanup/recall_clean,
auditoria OPENCODE MSG 0061/0066) pode alterar notas locais de T08/T10/T11/T12.
Este relatório fixa o **Bloco 1 medido** e a **proveniência honesta 5/5/3**.

> **Coordenação:** claims BUFFY em `phase_memory.rs` / `morphogenesis.rs` /
> `closed_loop_organism.rs` (MSG 0059+). Sync documental CURSOR (MSG 0064/0065
> + este ACK) **não** edita esses `src`.

## 6. Lacunas remanescentes (honestas)

| Dimensão / item | O que ainda falta |
|---|---|
| T01 | `OnlineKoopmanOperator` real |
| T03 | Sherman-Morrison/RLS online |
| T04 | papéis ecológicos |
| T05 | barreira CBF/Lyapunov formal |
| T06 / **1.4** | inércia do atuador (pendente decisão M4) |
| T07 | `ActiveInferenceEngine` |
| T09 | SINDy esparso |
| sprint 0059 | `fhrr_algebra` / `koopman_sindy` / `active_inference` / `renormalization` (paths do claim; FHRR real está em `src/core/fhrr.rs`) |

## 7. Artefatos

| Arquivo | Conteúdo |
|---|---|
| `src/phase_memory.rs` | kernel `cm_re/cm_im`, Jacobi, `sync_coupling_matrix`, abstenção |
| `src/morphogenesis.rs` | activity gate + `min_density_floor` |
| `tests/m5_abstention.rs` | 3 testes de margem / abstenção |
| `src/core/fhrr.rs` | FHRR (outro nó; Bloco 2 ownership) |
| `tests/cognitive_audit.rs` | 12 dimensões, scorecard, JSON |
| `benches/throughput_tps.rs` | vazão por regime |
| `target/aether_scorecard.json` | notas + throughput consolidados |

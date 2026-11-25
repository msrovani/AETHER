# COERÊNCIA E REGRAS DE IMPLEMENTAÇÃO — AETHER (2026-11-24)

> Base: [`POSICIONAMENTO_AETHER.md`] (adotado), [`AUDIT_CORE_EXTREMO_2026-11-24.md`]
> (encontrados), [`CORE_TOMADAS.md`], [`HW_TARGET.md`].
> **Status:** quarentena — autoridade **120** (regras/efeitos; 255 só HITL).
> **T0 intacto:** os Saltos (F20–F23, R10) são Sprint A — não alteram o contrato
> de 81,3 µs nem o orçamento de 60 Hz.

---

## 1. Coerência (mapa: claim → hook existente/novo, e conflitos resolvidos)

### 1.1 Mapa de integração

| Item do posição/audit | Hook existente no código | Novo | Confirmado por |
|---|---|---|---|
| F20 Fast-J Gating | `MorphogeneticField::try_evolve_topology` (src/morphogenesis.rs:434) | `g: &[f32]` multiplicativo + `set_fast_g` | C1 (não anula M7) |
| F21 Elastic compute | `aether_daemon.rs:597-605` (`imagine_futures_into`) | `steps_eff` dinâmico por `epistemic_uncertainty` | C2 (mantém T0) |
| F22 RG spine | `RenormalizationHierarchy::torque_into` (renormalization.rs:223), `SelfField.observe_hierarchy` (self_field.rs:533) | `macro_signature()` + chamadas no reflex loop | C3 |
| F23 G-ISO cross-modal | `RenormalizationHierarchy::macro_phase` (renormalization.rs:179) | `macro_signature()` + bridge zero-shot | C4 (sem nova estrutura) |
| L11–L13 = F21/F22/F20 | ver hooks acima | — | C5, C6 |
| A8 federação | `ScopeId` (knowledge_bridge_f1.rs), `--multicast-port` (não existe → R10) | flag/env, precheck de duplicatas | C7 |
| A9 rebase de autoridade | `ORGANISM_AUTHORITY=200` (ledger.rs:541) | split + docs (R12) | C4' |
| A10 probe budget | `planner.rs:60-69` (registro) | budget de passagens (R13) | C8 |
| A11 autobiography | `aether_daemon --seed` (existente) | hash bit-a-bit em 60 s (R14) | C9 |

### 1.2 Resolução de conflitos

- **C1 (F20 vs M7 hebbiano):** resolvido no ganho: `dj = gain·(corr − jv)·g_i·g_j − decay·jv`
  (morphogenesis.rs:507) — `g` multiplica a termo hebbiano, não o metacognitivo;
  M7 preservado; aceleração é 1/frames.
- **C2 (F21 vs orçamento T0):** `steps` é parte de `imagine_futures_into` (já
  variável, `aether_daemon.rs:601`); sem caminho no T0 — o laço de 60 Hz não
  chama o Dreamer, nem mesmo em modo elastic (R2). Contrato 81,3 µs inalterado.
- **C3 (F22 spine vs jumps do Dreamer):** o torque da spine (`omega`) e o jitter
  térmico (`noise_scale`, thermodynamic_dreamer.rs:503) operam em conjuntos
  disjuntos (frequência vs amplitude de fase) — sem contradictário; a UIP da
  EMA faz a amplitude caírem quando a espina avança (R3).
- **C4 (A9 rebase 200):** coincidente com `BUILD_MULTI_AGENT.md` (60 hyp,
  120 agente, 255 HITL); documentar como mudança de *default* (R12) — não de
  semântica.
- **C5 (HW_TARGET §8.4: RG/Dreamer 0 µs no T0):** preservado — Sprint A não
  altera o laço; regra de "0 µs na medição de T0" reaplicada após.
- **C6 (`deny(unsafe_code)`):** obrigatório em R1–R13; nenhum item usa unsafe.
- **C7 (SO_REUSEADDR + multicast):** precheck de unidade de (porta, ScopeId)
  em boot de cada daemon (R10); multiinstância na mesma LAN do daemão
  autônomo válido apenas com `--multicast-port` único.
- **C8 (probe budget vs abdução):** limite máximo de passagens por janela
  para não prender o laço (R13) — violação do budget → abstain abduce (F2).
- **C9 (autobiografia hash):** testes bit-a-bit em 10 execuções de 60 s;
  INVs INV-T5 (bit-reprodutibilidade) registrados.

---

## 2. Regras de implementação (R1–R15)

| # | Regra | Hook exato | Gate | Escopo |
|---|---|---|---|---|
| R1 | **Fast-J Gating (F20):** novo `g: &[f32]` multiplicativo de `try_evolve_topology`; `dj = gain·(corr − jv)·g_i·g_j − decay·jv` | src/morphogenesis.rs:434, 507 | `tests/morpho_gating_gate.rs` (acoplado ΔR>0,30/3 quadros; 0 alloc; reproduzão bit-a-bit) | Sprint A |
| R2 | **Elastic compute (F21):** `steps_eff = base·(1 + 3·esq).min(30·base)`, `esq = self.ensemble.epistemic_uncertainty` (lido na linha 634, usado em 602) | src/bin/aether_daemon.rs:597-605 | `target/aether_e6_mod.json` (elapsed, dropped_frames, max_steps) + re-medicao HW_TARGET §8.2 (p99 ≤ 16.667 µs) | Sprint A |
| R3 | **RG spine (F22):** Governor: `cfg.rg: Option<RenormalizationHierarchy>`; no reflex loop: `rg.forward(&ev.z)` (ascensão) + `rg.torque_into(&mut omega, k)` em cópia local (descida) | src/core/aether_governor.rs:554-605, provar features | `tests/governor_rg_spine_gate.rs` (macro_order↑ durante eventos; CBF violações = 0) | Sprint A |
| R4 | **Dream→E2 commit (L3/L4):** reflex loop: `if esq > ε { actor.filter_into(&out.policy, &mut smoothed) }` | src/core/aether_governor.rs:581-599, `src/core/active_inference.rs` (EmaActuator:56) | `tests/governor_e2_commit_gate.rs` (filtragem monótona, CBF=0) | Sprint A |
| R5 | **L13 = R1; L11 = R2** (referências cruzadas neste doc) | — | — | — |
| R6 | **G-ISO (F23):** `RenormalizationHierarchy::macro_signature() -> [u16; MACRO_DIM]` (hash de macro_phase+order, renormalization.rs:179) + consulta scope | src/core/renormalization.rs + scopes T-SENSOR/T-GRAPH | gate `macro_signatures_match_fixture.json` + exemplo de transferência zero-shot | Sprint A |
| R7 | **A8 federação:** `--multicast-port` (não existe — fio R8), env `AETHER_MULTICAST_PORT`; precheck: porta + `ScopeId` únicos em boot de cada daemon; para testes: `aether_daemon --multicast-port 7781 --scope-agent org:1/run:0/app:daemon` | src/bin/aether_daemon.rs:364-395, 48 (const), parse_cli (729+) | `tests/daemon_federation_gate.rs` (2 daemons mesma LAN, R>0.95, peers>0) | Sprint A |
| R8 | **`--multicast-port` faltante:** flag+parse+bind customizável | aether_daemon.rs:384-395 (bind hardcoded 239.255.42.99:7777) | coverado por R7 | Sprint A |
| R9 | **A9 rebase de autoridade (ajuste do usuário):** teto autônomo ∈ 120 (`ledger.rs:541`, já aplicado); 200–255 = HITL; documentar em CORE_TOMADAS.md + POSICIONAMENTO I-4 | `ledger.rs:541` | review de governança + `tests/auth_levels_gate.rs` | Decisão (R9) |
| R10 | **A10 episodic probe budget (G-ABD):** `TOOL_PROBE_BUDGET: AtomicU64` no planner; `should_abstain_new` não gera novas leituras; quando budget estraçar, abstain (F2) | src/agent/planner.rs:60-69, src/agent/bus.rs | `tests/bus_epistemic_probe_gate.rs` (5 seeds, anti-loop fuzz) | Sprint A |
| R11 | **A11 autobiografia (L10/F19):** flag `--seed` (existente); algoritmo: hash sha256 de fases amostradas aos 60 s (bit-a-bit) → `target/aether_e6_autobiography.json` | src/bin/aether_daemon.rs (cycle, ~624-638), novo `tests/organism_autobiography_gate.rs` | 10 execuções, Δ=0 | Sprint A |
| R12 | **Contrato de metrissagem (`target/aether_e6_mod.json`):** campos: `elapsed_us_median`, `dropped_frames_60s`, `steps_max_observed`, `wall_sha256`. Registro no area de auditoria do POSICIONAMENTO | — | — | Sprint A |
| R13 | **Position items 6–9:** mantidos — revisão em F3 com evidência de bloqueio (derivar: primeira lei distilada, primeira hipótese abduida, melhor transferência cross-modal) | — | — | F3 |
| R14 | **Guarda de regressão T0:** nenhuma R altera T0: 0 alloc, 81,3 µs nominal; rastreio em `tests/transducer_invariants.rs` (estende 0 alloc em `step`) | — | Rerun: `cargo test --release` + E6 re-medida (HW_TARGET §8.2) | Guarda |

---

## 3. Cronograma de implementação

1. **Sprint A (2–3 ciclos):** R1–R12 (fiação). L11–L13 (R2, R3, R1), L12 (R3), L4 (parte de R4).
2. **Sprint B:** L7 (F2 G-ABD loop: abdução acoplada a R10), L8 (F19 G-MORPH: novo basin → imprint), L6 (LinUCB ← RG macro: R6), R14 (guarda de regressão).
3. **Gate final (F3):** revisão do POSICIONAMENTO + auditoria + produção de R13.

---

## 4. Mapa de rastreio

| Item | Origem |
|---|---|
| F20–F23, L11–L13, §2b do AUDIT | [`AUDIT_CORE_EXTREMO_2026-11-24.md`] §2b, L11–L13 |
| Item do POSICIONAMENTO 6–9, F1, C-XX | [`POSICIONAMENTO_AETHER.md`] §2.3, §8, §9 |
| Existência de Ro sphere/lat | `src/core/renormalization.rs`, `src/core/self_field.rs` |
| Existência de Dreamer/EFE | `src/thermodynamic_dreamer.rs`, `src/core/active_inference.rs`, `src/agent/planner.rs` |

---

## 5. Referências principais

- [`POSICIONAMENTO_AETHER.md`], [`AUDIT_CORE_EXTREMO_2026-11-24.md`],
  [`CORE_TOMADAS.md`], [`TOMADAS_FUTURO.md`], [`HW_TARGET.md`] §8,
  [`E2_OPTIMIZATION.md`].
- Candidatos externos (aSer: todos arXiv do POSICIONAMENTO).

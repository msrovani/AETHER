# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
Versionamento: `0.0.x` (patch incrementado pelo ciclo automatico a cada conjunto de mudancas).

## [0.0.125] - 2026-11-16

### Changed

- Ciclo automatico: 9 arquivo(s) (docs:4, src:5).
- Gate: ok (0s).

- `docs/AUDIT_CLAIMS_VOL3.md`
- `docs/DASHBOARD_METRICS.md`
- `docs/HUB_DASHBOARD.md`
- `docs/TRANSPLANT_HV.md`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/catalog.rs`
- `src/dashboard/jobs.rs`
- `src/dashboard/mod.rs`
- `src/dashboard/transplant_view.rs`

## [0.0.124] - 2026-11-16

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:1, examples:1, src:2).
- Gate: ok (4s).

- `Cargo.lock`
- `examples/e2_symbolic_bench.rs`
- `src/core/logic_pipeline.rs`
- `src/core/persistency_search.rs`
- `docs/E2_OPTIMIZATION.md`

## [0.0.123] - 2026-11-16

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, examples:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `examples/e2c3_probe.rs`
- `src/core/persistency_search.rs`

## [0.0.122] - 2026-11-16

### Changed

- Ciclo automatico: 8 arquivo(s) (docs:5, examples:1, src:2).
- Gate: ok (8s).

- `docs/API_CONTRACT.md`
- `docs/INTAKE_PLAN.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md`
- `docs/TRANSPLANT_HV.md`
- `examples/ising_logic_probe.rs`
- `src/core/logic_pipeline.rs`
- `src/core/persistency_search.rs`
- `docs/DOACAO_PLAYBOOK.md`

## [0.0.121] - 2026-11-16

### Changed

- Ciclo automatico: 7 arquivo(s) (.:2, docs:1, examples:2, src:1, tools:1).
- Gate: ok (4s).

- `Cargo.lock`
- `Cargo.toml`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `examples/e2c3_probe.rs`
- `examples/transplant_hv_probe.rs`
- `src/core/presolve.rs`
- `tools/harvest_transplant.py`

## [0.0.120] - 2026-11-16

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, data:3, docs:2, examples:1, src:2, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `data/emb/bank.f32`
- `data/hv/bank.bin`
- `data/manifest.json`
- `docs/API_EVALUATION.md`
- `docs/EXTERNAL_API.md`
- `examples/transplant_hv_probe.rs`
- `src/api/mod.rs`
- `src/core/presolve.rs`
- `tests/e2c3_bound_gate.rs`

## [0.0.119] - 2026-11-16

### Changed

- Ciclo automatico: 12 arquivo(s) (.:1, data:3, examples:1, src:3, tests:3, tools:1).
- Gate: ok (5s).

- `Cargo.lock`
- `data/emb/bank.f32`
- `data/hv/bank.bin`
- `data/manifest.json`
- `examples/transplant_hv_probe.rs`
- `src/api/mod.rs`
- `src/bin/aether_api.rs`
- `src/core/presolve.rs`
- `tools/harvest_transplant.py`
- `tests/api_lifecycle.rs`
- `tests/e2c3_bound_gate.rs`
- `tests/transplant_hv_gate.rs`

## [Unreleased]

### Added

- `docs/E8_RESERVOIR_LESSONS.md`: lições, referências e armadilhas do E8
  (Mackey–Glass τ em tempo, NARMA supervisionado, ridge vs RLS, fast_tanh).
- `docs/CORPO3_ERRATA.md`: fechamento de E4/E5/E7/E8/F2/F3 com números de release.

### Changed

- `src/core/reservoir.rs`: reservatório esparso (10 conexões/nó), raio espectral
  por iteração de potência, readout ridge/Cholesky com varredura de λ sem
  reexecutar, `fast_tanh` (Padé) e baseline conjunto de 40 passadas.
- `tests/e8_reservoir_gate.rs`: gates de Mackey–Glass (NRMSE, 10⁶ em <60 s,
  throughput), NARMA-10 e speedup ≥20× do congelado.
- `examples/reservoir_readout_bench.rs`: benchmark com MG/NARMA, 5 sementes,
  ablação de esparsidade, λ sweep, RLS e busca conjunta.

### Results

- Mackey–Glass 10⁶: NRMSE **2,33 × 10⁻⁵**, treino **1,57 s**, **1,73 M pred/s**.
- NARMA-10: NRMSE **0,1286**.
- Conjunto (40 passadas): **39,6–45,7×** mais lento; congelado não regride >5%.

## [0.0.118] - 2026-11-16

### Changed

- Ciclo automatico: 9 arquivo(s) (.:1, data:3, docs:1, src:2, tests:1, tools:1).
- Gate: ok (6s).

- `Cargo.lock`
- `data/emb/bank.f32`
- `data/hv/bank.bin`
- `data/manifest.json`
- `docs/CORPO3_ERRATA.md`
- `src/api/mod.rs`
- `src/core/knowledge_bridge_f1.rs`
- `tests/api_integrity.rs`
- `tools/harvest_transplant.py`

## [0.0.117] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, data:1, docs:1, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `data/emb/whitening.bin`
- `docs/CORPO3_ERRATA.md`
- `tests/api_eval.rs`

## [0.0.116] - 2026-11-16

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, data:3, docs:1, src:2, tests:2, tools:1).
- Gate: ok (3s).

- `Cargo.lock`
- `data/emb/bank.f32`
- `data/hv/bank.bin`
- `data/manifest.json`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `src/api/mod.rs`
- `src/core/reservoir.rs`
- `tests/api_integrity.rs`
- `tests/e8_reservoir_gate.rs`
- `tools/harvest_transplant.py`

## [0.0.115] - 2026-11-16

### Changed

- Ciclo automatico: 5 arquivo(s) (.:2, docs:1, examples:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `README.md`
- `docs/CORPO3_ERRATA.md`
- `examples/reservoir_readout_bench.rs`
- `src/core/logic_pipeline.rs`

## [0.0.114] - 2026-11-16

### Changed

- Ciclo automatico: 19 arquivo(s) (.:3, data:7, docs:3, examples:3, src:2, tools:1).
- Gate: ok (8s).

- `Cargo.lock`
- `Cargo.toml`
- `FORUM_PROTOCOL.md`
- `data/emb/bank.f32`
- `data/emb/whitening.bin`
- `data/facts.jsonl`
- `data/hv/bank.bin`
- `data/hv/bank.idx`
- `data/manifest.json`
- `data/sgdb_mock_ledger.json`
- `docs/HW_TARGET.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md`

## [0.0.113] - 2026-11-16

### Changed

- Ciclo automatico: 9 arquivo(s) (.:1, data:1, docs:2, examples:1, src:2, tests:1, tools:1).
- Gate: ok (3s).

- `Cargo.lock`
- `docs/API_CONTRACT.md`
- `examples/e2c3_probe.rs`
- `src/api/mod.rs`
- `src/core/logic_pipeline.rs`
- `tests/e8_reservoir_gate.rs`
- `data/`
- `docs/INTAKE_PLAN.md`
- `tools/harvest_transplant.py`

## [0.0.112] - 2026-11-16

### Changed

- Ciclo automatico: 12 arquivo(s) (.:1, docs:2, examples:3, src:3, tests:3).
- Gate: ok (2s).

- `README.md`
- `docs/API_CONTRACT.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `examples/e2c3_probe.rs`
- `examples/plh_bench.rs`
- `examples/reservoir_readout_bench.rs`
- `src/core/logic_pipeline.rs`
- `src/core/phase_language.rs`
- `src/core/reservoir.rs`
- `tests/api_http.rs`
- `tests/e1_language_gate.rs`
- `tests/e8_reservoir_gate.rs`

## [0.0.111] - 2026-11-16

### Changed

- Ciclo automatico: 13 arquivo(s) (.:2, docs:4, examples:1, src:3, tests:3).
- Gate: ok (4s).

- `ARCHITECTURE.md`
- `Cargo.lock`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `docs/USER_DELIVERY_PLAN.md`
- `examples/transplant_probe.rs`
- `src/core/constraint_acquisition.rs`
- `src/core/persistency_search.rs`
- `src/core/phase_language.rs`
- `tests/api_eval.rs`
- `tests/api_integrity.rs`
- `tests/e3_transplant_gate.rs`

## [0.0.110] - 2026-11-16

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, examples:2, src:4, tests:3).
- Gate: ok (5s).

- `Cargo.lock`
- `examples/e2c3_probe.rs`
- `examples/ising_logic_probe.rs`
- `src/api/mod.rs`
- `src/core/knowledge_bridge_f1.rs`
- `src/core/logic_pipeline.rs`
- `src/core/persistency_search.rs`
- `tests/api_eval.rs`
- `tests/api_integrity.rs`
- `tests/e2c3_exact_search.rs`

## [0.0.109] - 2026-11-16

### Changed

- Ciclo automatico: 3 arquivo(s) (src:3).
- Gate: ok (4s).

- `src/api/mod.rs`
- `src/core/mod.rs`
- `src/core/persistency_search.rs`

## [0.0.108] - 2026-11-16

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:2, examples:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `docs/EXTERNAL_API.md`
- `examples/e2c3_probe.rs`
- `src/api/mod.rs`

## [0.0.107] - 2026-11-16

### Changed

- Ciclo automatico: 3 arquivo(s) (examples:2, tests:1).
- Gate: ok (1s).

- `examples/cram_logos_probe.rs`
- `tests/api_http.rs`
- `examples/e2c3_probe.rs`

## [0.0.106] - 2026-11-16

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, docs:3, examples:1, src:2, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`
- `examples/e1b_capacity.rs`
- `src/api/mod.rs`
- `src/core/s0_body.rs`
- `tests/e7_body_gate.rs`
- `docs/EXTERNAL_API.md`

## [0.0.105] - 2026-11-16

### Changed

- Ciclo automatico: 10 arquivo(s) (.:2, examples:1, src:6, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `Cargo.toml`
- `examples/e1b_capacity.rs`
- `src/api/json.rs`
- `src/core/knowledge_bridge_f1.rs`
- `src/lib.rs`
- `src/phase_memory.rs`
- `src/api/mod.rs`
- `src/bin/aether_api.rs`
- `tests/api_http.rs`

## [0.0.104] - 2026-11-16

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, docs:2, src:3).
- Gate: ok (4s).

- `Cargo.lock`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/host_probe.rs`
- `src/api/`

## [0.0.103] - 2026-11-16

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:1, examples:2, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `examples/roof_scratch.rs`
- `examples/transplant_probe.rs`
- `tests/e2c2_roof_dual_gate.rs`

## [0.0.102] - 2026-11-16

### Changed

- Ciclo automatico: 6 arquivo(s) (examples:1, src:3, tests:2).
- Gate: ok (4s).

- `examples/ising_logic_probe.rs`
- `src/core/knowledge_bridge_f1.rs`
- `src/core/logic_pipeline.rs`
- `src/core/mod.rs`
- `tests/e1_language_gate.rs`
- `tests/e2_logic_gate.rs`

## [0.0.101] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (4s).

- `Cargo.lock`
- `src/core/phase_language.rs`
- `tests/e1_language_gate.rs`
- `tests/e3_transplant_gate.rs`

## [0.0.100] - 2026-11-16

### Changed

- Ciclo automatico: 9 arquivo(s) (docs:3, examples:2, src:1, tests:3).
- Gate: ok (44s).

- `docs/AUDIT_CLAIMS_VOL3.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `examples/_e3_diag.rs`
- `examples/transplant_probe.rs`
- `src/core/phase_language.rs`
- `tests/e1_language_gate.rs`
- `tests/e1b_capacity.rs`
- `tests/e3_transplant_gate.rs`

## [0.0.99] - 2026-11-16

### Changed

- Ciclo automatico: 13 arquivo(s) (docs:2, examples:4, src:5, tests:2).
- Gate: ok (10s).

- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `examples/e1b_capacity.rs`
- `examples/ising_logic_probe.rs`
- `examples/plh_bench.rs`
- `examples/plh_debug.rs`
- `src/core/knowledge_bridge_f1.rs`
- `src/core/presolve.rs`
- `src/dashboard/host_probe.rs`
- `src/dashboard/mod.rs`
- `tests/e2_logic_gate.rs`
- `tests/f1_knowledge_bridge_gate.rs`

## [0.0.98] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, docs:1, src:1, tests:1).
- Gate: ok (59s).

- `Cargo.lock`
- `docs/FASE1_PROMPT.md`
- `src/core/self_field.rs`
- `tests/self_field_audit.rs`

## [0.0.97] - 2026-11-16

### Changed

- Ciclo automatico: 7 arquivo(s) (.:1, docs:1, examples:2, src:2, tests:1).
- Gate: ok (8s).

- `Cargo.lock`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `examples/roof_scratch.rs`
- `src/core/logic_pipeline.rs`
- `src/core/self_field.rs`
- `tests/self_field_audit.rs`
- `examples/plh_debug.rs`

## [0.0.96] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (docs:1, src:1, tests:2).
- Gate: ok (1s).

- `docs/FASE1_PROMPT.md`
- `src/thermodynamic_dreamer.rs`
- `tests/e1_language_gate.rs`
- `tests/self_field_audit.rs`

## [0.0.95] - 2026-11-16

### Changed

- Ciclo automatico: 7 arquivo(s) (.:1, docs:1, examples:3, src:1, tests:1).
- Gate: ok (6s).

- `Cargo.lock`
- `examples/phase_language_probe.rs`
- `src/core/presolve.rs`
- `docs/AUDIT_CLAIMS_VOL3.md`
- `examples/plh_complex_probe.rs`
- `examples/roof_scratch.rs`
- `tests/e2c2_roof_dual_gate.rs`

## [0.0.94] - 2026-11-16

### Added

- `FhrrStore::query_phasor_into` — consulta com o **fasor de papel
  pré-computado** (`cue_re + i·cue_im`): não recalcula `sin/cos` em `D`
  posições. O chamador compõe o fasor (ex.: produto de tabelas `exp(i·átomo)`).
  Equivale a `query_into` quando `cue = exp(i·(entity + relation))`.
- `tests/e1_plh.rs::phasor_cue_matches_phase_cue` — prova que os dois caminhos
  devolvem o **mesmo** `value_index`, `fact_index` e similaridade (Δ < 1e-5).

### Changed

- `FhrrStore`: o núcleo (cleanup do papel + unbinding + similaridade) foi
  extraído para `finish_into`, compartilhado por `query_into` e
  `query_phasor_into` — sem duplicação e sem mudar o caminho antigo.

### Nota de coordenação

`examples/phase_language_probe.rs` está em **edição concorrente** por outro
agente (versão de 1.647 linhas, 4 papéis + ressonador ACF). Ela já mede
p50 **9,90 µs**, NC **100%**, abstenção **100%**, mas ainda falha em
`recall@1` (0,1675), SNR (0,84σ) e **25 alocações** no hot-path. Este patch
**não** altera esse arquivo; entrega apenas a API e o teste do E1-2.

## [0.0.93] - 2026-11-16

### Changed

- Ciclo automatico: 3 arquivo(s) (examples:2, tests:1).
- Gate: ok (1s).

- `examples/e1b_capacity.rs`
- `examples/phase_language_probe.rs`
- `tests/e1_plh.rs`

## [0.0.93] - 2026-11-16

### Added

- `examples/phase_language_probe.rs` — **Phase-Language Head sharded** em D=160:
  24 shards por relação, ≤42 fatos cada, binding não-comutativo por permutação
  `ρ¹` do valor, caminho de consulta zero-alocação (`query_into` + `FhrrScratch`).
- `tests/e1_plh.rs` — 3 gates: sharding (480 fatos) vs superposição plana,
  ordem A→B ≠ B→A, abstenção fora do vocabulário.

### Measured

| Gate | Resultado |
|---|---|
| G1 recall sharded (1.000 fatos) | **1,0000** (plano: **0,0380**) |
| G2 não-comutatividade A→B vs B→A | **PASS** |
| G3 SNR · latência · hot-path | 6,75σ · **17,35 µs** · **0 B** |
| G4 abstenção OOV · falso-abster | **100%** · **0%** |

### Changed

- **G3 falha só na latência** (17,35 µs vs alvo 15 µs, +16%). Causa identificada:
  `query_into` recalcula `sin_cos` em D=160 posições por consulta. Correção
  proposta: API que aceite o fasor de papel pré-computado (**E1-2**).
- **Achado de modelagem:** a memória endereça por papel `(e⊗r)`, então
  `(sujeito, relação)` precisa ser **único** por shard. A primeira tentativa
  reusava sujeitos e caía para recall 1/5 — corrigido e documentado.
- **Ressonador ACF não foi necessário**: com papel e relação fornecidos, a
  fatoração é um único unbinding. O caso sem nenhum fator conhecido é E1-2.
- Vol. III §11/§12 atualizados.

## [0.0.92] - 2026-11-16

### Changed

- Ciclo automatico: 7 arquivo(s) (.:3, docs:2, examples:1, tests:1).
- Gate: ok (5s).

- `CHANGELOG.md`
- `Cargo.lock`
- `Cargo.toml`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `examples/e1b_capacity.rs`
- `tests/e1b_capacity.rs`

## [0.0.91] - 2026-11-15

### Added

- `examples/e1b_capacity.rs` — capacidade de memória em duas frentes: store FHRR
  (`D ∈ {64,160,320,640}` × `K ∈ {1,6}`) e memória de fase (erro angular vs nº
  de padrões).
- `tests/e1b_capacity.rs` — 4 gates: D=160 sustenta 64 fatos a 99%, degrada em
  256, capacidade é linear em D, mais traços ajudam.

### Measured

**Store FHRR (fatos `e⊗r⊗v`, recall@1 ≥ 99%):**

| D | K=1 | K=6 | M/D |
|---:|---:|---:|---:|
| 64 | 8 | 32 | 0,50 |
| **160** | 16 | **64** | **0,40** |
| 320 | 32 | 128 | 0,40 |
| 640 | 32 | 256 | 0,40 |

**Memória de fase (padrões aleatórios, 20% oculto):** erro 0,126 → 1,180 rad de
4 → 64 padrões; capacidade a < 0,15 rad ≈ **4** (declarado: 40).

### Changed

- **A lei é linear, não quadrática:** capacidade ≈ **0,4·D** (K=6). A lei `M ∝ D²`
  (arXiv:1906.11684) vale para fatoração por ressonador, não para armazenamento
  por superposição — a expectativa estava inflada em ~40× em D=160.
- Corroboração: o T12 da auditoria (50 fatos em D=160, `Recall@1 = 1,0`) estava a
  **78% da capacidade**.
- **Consequência:** para ~1.000 fatos seria necessário D≈2.500 (≈20 MB) ⇒
  sharding por papel/relação ou fatoração por ressonador viram **requisito** do
  PLH, não otimização.
- Vol. II §3.6 e Vol. III (§11, §12) atualizados; **E1b-2** (padrões estruturados)
  entra na fila.

## [0.0.90] - 2026-11-15

### Added

- `core::presolve::persistency_qumo` — **persistência de primeira ordem**
  (dominância) para QUBO/Ising: fixa `s_i` quando o ganho máximo do flip é
  negativo em todas as atribuições dos outros; atualiza `h_j += J_ij·v` e
  itera até o fixpoint. **Preserva o valor ótimo.**
- `examples/e2c_persistency.rs` — valida o ótimo antes/depois por força bruta, em
  três regimes de viés.
- `tests/e2c_persistency.rs` — 3 gates (viés forte + cascata, controle sem viés,
  preservação do ótimo em 120 instâncias).

### Measured

| plant | redução | ótimo alterado | presolve+BSB | BSB puro |
|---:|---:|---:|---:|---:|
| 0% (controle) | 0,0% | 0 | 185/200 | 185/200 |
| 25% | 28,6% | 0 | 190/200 | 188/200 |
| 45% | 42,9% | 0 | 199/200 | 193/200 |

### Changed

- **Resultado:** a dominância é *sound* (0/600 ótimos alterados) mas **míope** —
  só vê viés explícito e não cascateia em `J` ±1. O controle sem viés dá 0% e
  braços idênticos, provando que a medida não está contaminada.
- Vol. II §2.7 e Vol. III (§11, §12) atualizados; **E2c-2 (roof dual/max-flow)**
  entra como próximo refinamento de alcance.

## [0.0.89] - 2026-11-15

### Added

- `core::presolve` — presolve simbólico para lógica em Ising: propagação de
  unidades + fixação por implicação (`l ⇒ ¬l`), fixpoint, detecção de conflito e
  reescrita das cláusulas sobre o espaço residual.
- `examples/e2b_presolve.rs` — 2-SAT plantado (`n=16`) com ground truth e
  validação de soundness por força bruta.
- `tests/e2b_presolve.rs` — 6 gates: unidade, cadeia de implicação, contradição,
  espaço residual, soundness e preservação de satisfatibilidade.

### Measured

| Métrica | Valor |
|---|---:|
| Redução de variáveis | **64,8%** |
| cascata (além das unidades) | **+39,8 pp** |
| Fixações insound | **0** |
| Precisão das decisivas | **100%** |
| BSB puro | 183/200 |
| **Presolve + BSB** | **200/200** |

### Changed

- Vol. II §2.6 e Vol. III (§11, §12) com o resultado do E2b v1.
- Plano: **E2c (roof dual/max-flow)** entra como próximo refinamento de alcance;
  E1b desce para 6º.

## [0.0.88] - 2026-11-15

### Added

- `core::certificate` — **certificado de coerência** (energia, gap local,
  violações, consenso, proveniência) e a matriz 2D de decisão
  (`Act`/`Cite`/`Propose`/`Abstain`, autoridade × gap). Inclui
  `spectral_lower_bound` (Gershgorin rigoroso) e `local_gap[_with]`.
- `examples/e3b_certificate.rs` — calibração do certificado com ground truth
  exato por força bruta, em dois braços: vidro de spin (MAX-CUT) e MAX-2-SAT.
- `tests/e3b_certificate.rs` — gates matemáticos (energia, gap, bound, matriz 2D)
  e empírico (precisão 100% com verificação exata).

### Measured

**Braço 1 — otimização livre (n=14, 400 instâncias, 4 restarts, steps=8):**

| Sinal | AUC | precisão@50% |
|---|---:|---:|
| gap local | 0,547 | 90,0% |
| consenso entre restarts | 0,526 | 89,9% |
| gap entre restarts | 0,501 | 90,0% |
| gap até o bound espectral | 0,058 | 81,3% |
| confiança combinada | 0,462 | 88,1% |

Base do solver = 90,0%. **Nenhum sinal adiciona 1 pp.** O gap até o bound é
anti-preditivo.

**Braço 2 — MAX-2-SAT (verificação exata):** 397/400 satisfatíveis, 383 decisivas
(95,8%), **precisão 100,0%**, 96,5% dos satisfatíveis resolvidos, abstenção
correta nos insatisfatíveis.

### Changed

- **Resultado do P0/R3:** o certificado não mede acerto — mede **factibilidade**.
  Em otimização não restrita todo mínimo local é "coerente"; em lógica, zero
  violações é prova. Reordena E2b como *condição de existência* do certificado.
- Vol. II §4.6 e Vol. III (§11, §12) atualizados com o veredito.

## [0.0.86] - 2026-11-15

### Added

- `simulation::profile` — `StageProfile` com 10 estágios nomeados, `coverage()`,
  `top()` e `fraction()`; instrumentação por fronteira de macro-estágio.
- `ClosedLoopOrganism::step_profiled_into` via `step_impl<const PROF: bool>` —
  custo **zero** no `step_into` de produção (o compilador elimina o bloco).
- `examples/e6_profile.rs` — anatomia do ciclo: 10k quadros, p50/p95/p99/p99.9,
  alocador contador armado, working set por estágio e varreduras
  `--sweep` (steps 60/120/240), `--nsweep` (N 64..1024) e `--all`.
- `tests/e6_profile_gate.rs` — gate de cobertura ≥ 95% + sanidade do caminho
  instrumentado.
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md` — Volume III da série: gestão do
  conhecimento, encaixe do `neural-sgdb`, matriz 2D de garantia (autoridade × gap)
  e **registro mestre** (experimentos, gaps `NO_*`, riscos `R1–R10`).
- `docs/HW_TARGET.md` §8 — anatomia medida, mapa de capacidade, os dois orçamentos
  (tempo real × throughput) e §8.5 (E6b antes × depois).

### Changed

- `thermodynamic_solver`: GEMV `hx = Jx + h` e energia `H(s)` agora usam
  `dot_f64`/`dot_i8` com **16 acumuladores independentes** (Rust 100% safe, sem
  intrinsics). `s ∈ {-1,+1}` é convertido uma vez por passo num buffer já
  existente (`self.g`) — zero alocação nova.
- Volumes I e II do dossiê: bloco "Série", selos `[A]/[B]/[C]/[M]`, riscos
  renumerados `R1–R6` e correção da premissa "matvec = 0,07%".

### Measured

| Métrica | Antes (E6) | Depois (E6b) |
|---|---:|---:|
| solver (N=64, steps=60) | 132,1 µs (84,4%) | **57,5 µs (70,7%)** |
| GFLOP/s | 7,44 | **17,10** — **2,30×** |
| total do quadro | 156,3 µs | **81,3 µs** |
| p99 | 320,6 µs | **170,2 µs** |
| `eq-t/s` | 818.844 | **1.574.229** |
| N=1024 | 52,07 ms | **16,60 ms** (3,14×) |

Curva de acumuladores (N=64): **16 → 17,1** · 24 → 10,4 (cauda de 16 elementos) ·
32 → 12,3 (spill de registradores). O ótimo é 16 porque divide todas as potências
de dois de N.

### Fixed

- Premissa de diagnóstico: o solver faz **dois** GEMVs por passo (gradiente +
  energia), não um — 983.040 flops por quadro em `steps=60`, não 4.096.
- Nota de metodologia: o quadro do organismo **não é bit-reprodutível entre
  instâncias** (amostragem lê threads UDP reais); medir distribuição, nunca
  execução única.

### Gates

`ACQ = 100.00` · `12/12 MEASURED` · `hot_path_bytes = 0` · 0 alocações em 10.000
quadros · `cargo test --release` (24 alvos) verde · `clippy -D warnings` e
`fmt --check` verdes.

## [0.0.85] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `src/core/thermodynamic_solver.rs`

## [0.0.84] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `src/core/thermodynamic_solver.rs`

## [0.0.83] - 2026-11-15

### Changed

- Ciclo automatico: 1 arquivo(s) (src:1).
- Gate: ok (7s).

- `src/core/thermodynamic_solver.rs`

## [0.0.82] - 2026-11-15

### Changed

- Ciclo automatico: 9 arquivo(s) (.:1, docs:3, examples:1, src:3, tests:1).
- Gate: ok (5s).

- `Cargo.lock`
- `docs/HW_TARGET.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`
- `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`
- `src/simulation/closed_loop_organism.rs`
- `src/simulation/mod.rs`
- `examples/e6_profile.rs`
- `src/simulation/profile.rs`
- `tests/e6_profile_gate.rs`

## [0.0.80] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `src/bin/aether_dashboard.rs`

## [0.0.79] - 2026-11-15

### Changed

- Ciclo automatico: 6 arquivo(s) (.:2, src:4).
- Gate: ok (4s).

- `Cargo.lock`
- `Cargo.toml`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/jobs.rs`
- `src/dashboard/mod.rs`
- `src/dashboard/demo_trace.rs`

## [0.0.78] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (6s).

- `Cargo.lock`
- `src/platform/state_bridge.rs`

## [0.0.77] - 2026-11-15

### Changed

- Ciclo automatico: 9 arquivo(s) (.:3, src:5, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `Cargo.toml`
- `README.md`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/host_probe.rs`
- `src/lib.rs`
- `src/platform/host_transducer.rs`
- `src/bin/aether_governor.rs`
- `tests/governor_e2e.rs`

## [0.0.76] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:2).
- Gate: ok (4s).

- `Cargo.lock`
- `src/bin/aether_dashboard.rs`
- `src/platform/`

## [0.0.75] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/bin/aether_dashboard.rs`

## [0.0.74] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, docs:1, src:1).
- Gate: ok (4s).

- `Cargo.lock`
- `docs/PHASE5_PHASE01_REPORT.md`
- `src/dashboard/host_probe.rs`

## [0.0.73] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:2).
- Gate: ok (4s).

- `Cargo.lock`
- `src/dashboard/host_probe.rs`
- `src/dashboard/jobs.rs`

## [0.0.72] - 2026-11-15

### Changed

- Ciclo automatico: 11 arquivo(s) (.:2, benches:1, docs:3, src:5).
- Gate: ok (5s).

- `Cargo.lock`
- `README.md`
- `benches/hardware_extremus.rs`
- `docs/USER_DELIVERY_PLAN.md`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/host_probe.rs`
- `src/dashboard/jobs.rs`
- `src/dashboard/mod.rs`
- `src/dashboard/model.rs`
- `docs/DASHBOARD_METRICS.md`
- `docs/USER_GUIDE.md`

## [0.0.71] - 2026-11-15

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, benches:1, src:6).
- Gate: ok (3s).

- `Cargo.lock`
- `benches/hardware_extremus.rs`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/catalog.rs`
- `src/dashboard/mod.rs`
- `src/dashboard/gaps.rs`
- `src/dashboard/host_probe.rs`
- `src/dashboard/jobs.rs`

## [0.0.70] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.github:1, benches:1, tests:1).
- Gate: ok (1s).

- `.github/workflows/ci.yml`
- `benches/hardware_extremus.rs`
- `tests/phase5_hardware.rs`

## [0.0.69] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/core/gpu_gate.rs`

## [0.0.68] - 2026-11-15

### Changed

- Ciclo automatico: 5 arquivo(s) (.:2, docs:1, src:2).
- Gate: ok (4s).

- `Cargo.lock`
- `README.md`
- `src/core/mod.rs`
- `docs/USER_DELIVERY_PLAN.md`
- `src/core/gpu_gate.rs`

## [0.0.67] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, examples:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `examples/bam_probe.rs`
- `src/core/koopman_sindy.rs`

## [0.0.66] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:2, docs:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `Cargo.toml`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `src/core/koopman_sindy.rs`

## [0.0.65] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (benches:1, src:1, tests:1).
- Gate: ok (2s).

- `benches/poly_vs_lut.rs`
- `src/core/fhrr_bam.rs`
- `tests/cognitive_audit.rs`

## [0.0.64] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, docs:1, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `tests/bam_precision.rs`
- `docs/PHASE5_PHASE01_REPORT.md`

## [0.0.63] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, docs:1, src:1, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `src/core/sindy_jit.rs`
- `tests/bam_precision.rs`

## [0.0.62] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (docs:1, examples:1, tests:1).
- Gate: ok (6s).

- `docs/COGNITIVE_AUDIT_REPORT.md`
- `examples/bam_probe.rs`
- `tests/phase5_hardware.rs`

## [0.0.61] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:2, src:2).
- Gate: ok (0s).

- `Cargo.lock`
- `Cargo.toml`
- `src/core/mod.rs`
- `src/core/aether_governor.rs`

## [0.0.60] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (src:4).
- Gate: ok (0s).

- `src/core/bam_phase.rs`
- `src/core/silicon_entropy.rs`
- `src/core/sindy_jit.rs`
- `src/core/spsc_ring.rs`

## [0.0.59] - 2026-11-15

### Changed

- Ciclo automatico: 11 arquivo(s) (.:8, docs:2, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `target_close_audit0.txt`
- `target_close_audit1.txt`
- `target_close_audit2.txt`
- `target_close_audit3.txt`
- `target_close_bench.txt`
- `target_close_clippy.txt`
- `target_close_test.txt`
- `tests/cognitive_audit.rs`
- `docs/COGNITIVE_AUDIT_FINAL.md`

## [0.0.58] - 2026-11-15

### Changed

- Ciclo automatico: 5 arquivo(s) (.:4, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `target_close_clippy.txt`
- `tests/cognitive_audit.rs`
- `target_close_audit2.txt`
- `target_close_audit3.txt`

## [0.0.57] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:3, tests:1).
- Gate: ok (1s).

- `tests/cognitive_audit.rs`
- `target_close_bench.txt`
- `target_close_clippy.txt`
- `target_close_test.txt`

## [0.0.56] - 2026-11-15

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, src:4, tests:1).
- Gate: ok (75s).

- `Cargo.lock`
- `src/core/active_inference.rs`
- `src/core/fhrr.rs`
- `src/core/koopman_sindy.rs`
- `src/core/renormalization.rs`
- `tests/cognitive_audit.rs`

## [0.0.55] - 2026-11-15

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, benches:1, src:4, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `benches/phase_recall.rs`
- `src/core/mod.rs`
- `src/core/renormalization.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/cognitive_audit.rs`
- `tests/organism_step_into.rs`
- `src/core/swarm_speciation.rs`

## [0.0.54] - 2026-11-15

### Changed

- Ciclo automatico: 8 arquivo(s) (.:3, benches:1, src:3, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `Cargo.toml`
- `src/core/active_inference.rs`
- `src/core/koopman_sindy.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/cognitive_audit.rs`
- `benches/phase_recall.rs`
- `target_close_audit0.txt`

## [0.0.53] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (4s).

- `Cargo.lock`
- `src/core/active_inference.rs`
- `tests/cognitive_audit.rs`
- `tests/organism_step_into.rs`

## [0.0.52] - 2026-11-15

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, docs:1, src:4, tests:2).
- Gate: ok (3s).

- `Cargo.lock`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `src/core/active_inference.rs`
- `src/core/koopman_sindy.rs`
- `src/core/renormalization.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/cognitive_audit.rs`
- `tests/organism_step_into.rs`

## [0.0.51] - 2026-11-15

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, src:5).
- Gate: ok (3s).

- `Cargo.lock`
- `src/core/fhrr.rs`
- `src/core/mod.rs`
- `src/core/active_inference.rs`
- `src/core/koopman_sindy.rs`
- `src/core/renormalization.rs`

## [0.0.50] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, benches:1, src:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `benches/throughput_tps.rs`
- `src/phase_memory.rs`
- `tests/cognitive_audit.rs`

## [0.0.49] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, docs:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `tests/phase2_invariants.rs`

## [0.0.48] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, docs:1, src:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `src/core/fhrr.rs`
- `tests/phase2_invariants.rs`

## [0.0.47] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/fhrr.rs`
- `tests/cognitive_audit.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.46] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/morphogenesis.rs`
- `tests/cognitive_audit.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.45] - 2026-11-15

### Changed

- Ciclo automatico: 8 arquivo(s) (.:4, docs:2, tests:1, tools:1).
- Gate: ok (2s).

- `ARCHITECTURE_PHASE2.md`
- `Cargo.lock`
- `FORUM_PROTOCOL.md`
- `README.md`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `docs/HARDENING_REPORT.md`
- `tests/cognitive_audit.rs`
- `tools/forum.py`

## [0.0.44] - 2026-11-15

### Changed

- Ciclo automatico: 5 arquivo(s) (docs:1, src:2, tests:2).
- Gate: ok (26s).

- `docs/COGNITIVE_AUDIT_REPORT.md`
- `src/morphogenesis.rs`
- `src/phase_memory.rs`
- `tests/cognitive_audit.rs`
- `tests/m5_abstention.rs`

## [0.0.43] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/core/fhrr.rs`
- `tests/cognitive_audit.rs`

## [0.0.42] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:2, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/fhrr.rs`
- `src/morphogenesis.rs`
- `tests/cognitive_audit.rs`

## [0.0.41] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, examples:1, src:1).
- Gate: ok (2s).

- `Cargo.lock`
- `examples/zz_proto_m5.rs`
- `src/core/fhrr.rs`

## [0.0.40] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (examples:1, tests:2).
- Gate: ok (1s).

- `examples/zz_proto_m5.rs`
- `tests/cognitive_audit.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.39] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, benches:1, docs:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `benches/README.md`
- `docs/COGNITIVE_AUDIT_REPORT.md`
- `tests/cognitive_audit.rs`

## [0.0.38] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, docs:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `tests/cognitive_audit.rs`
- `docs/COGNITIVE_AUDIT_REPORT.md`

## [0.0.37] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, benches:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `benches/throughput_tps.rs`
- `tests/cognitive_audit.rs`

## [0.0.36] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, benches:1, tests:1).
- Gate: ok (1s).

- `Cargo.toml`
- `benches/throughput_tps.rs`
- `tests/cognitive_audit.rs`

## [0.0.35] - 2026-11-15

### Changed

- Release de hardening: M5 com projecao complexa (p0/p1/p2 < 1e-3 rad), M2 L2048 = 35,7 us, organismo com sensor_noise=0.35 (entropia -0,05), M8 22 -> 0 alocacoes/ciclo e 0 drops idle, M3 com dedup de peers e rx_bytes overflow-safe.
- Relatorio detalhado: docs/HARDENING_REPORT.md; medicoes em ARCHITECTURE_PHASE2.md secao 6.

## [0.0.34] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (8s).

- `Cargo.lock`
- `src/bin/organism.rs`

## [0.0.33] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `tests/zz_buffy_probe.rs`

## [0.0.32] - 2026-11-15

### Changed

- Ciclo automatico: 5 arquivo(s) (.:2, docs:1, tests:2).
- Gate: ok (2s).

- `ARCHITECTURE_PHASE2.md`
- `Cargo.lock`
- `docs/API_CONTRACT.md`
- `tests/phase2_m5_hardening.rs`
- `tests/zz_buffy_probe.rs`

## [0.0.31] - 2026-11-15

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, src:3, tests:2).
- Gate: ok (4s).

- `Cargo.lock`
- `src/bin/organism.rs`
- `src/phase_memory.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/phase2_m5_hardening.rs`
- `tests/zz_buffy_probe.rs`

## [0.0.30] - 2026-11-15

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/phase_memory.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.29] - 2026-11-15

### Changed

- Ciclo automatico: 5 arquivo(s) (src:3, tests:2).
- Gate: ok (3s).

- `src/bin/aether_daemon.rs`
- `src/bin/organism.rs`
- `src/phase_memory.rs`
- `tests/m3_robustness.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.28] - 2026-11-15

### Changed

- Ciclo automatico: 4 arquivo(s) (.:2, docs:1, src:1).
- Gate: ok (2s).

- `ARCHITECTURE_PHASE2.md`
- `Cargo.lock`
- `docs/API_CONTRACT.md`
- `src/bin/aether_daemon.rs`

## [0.0.27] - 2026-11-15

### Changed

- Ciclo automatico: 2 arquivo(s) (src:1, tests:1).
- Gate: ok (1s).

- `src/bin/aether_daemon.rs`
- `tests/m8_cli.rs`

## [0.0.26] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/bin/aether_daemon.rs`
- `tests/m8_cli.rs`

## [0.0.25] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (src:1, tests:2).
- Gate: ok (3s).

- `src/phase_memory.rs`
- `tests/phase2_hardening.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.24] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (4s).

- `Cargo.lock`
- `src/phase_memory.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.23] - 2026-11-14

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:2, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `src/morphogenesis.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.22] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.21] - 2026-11-14

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, src:3, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `src/morphogenesis.rs`
- `src/phase_memory.rs`
- `tests/phase2_m5_hardening.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.20] - 2026-11-14

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `tests/phase2_m5_hardening.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.19] - 2026-11-14

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:1, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/phase_memory.rs`
- `tests/phase2_m5_hardening.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.18] - 2026-11-14

### Changed

- Ciclo automatico: 9 arquivo(s) (.:1, docs:1, src:5, tests:2).
- Gate: ok (4s).

- `ARCHITECTURE.md`
- `docs/HW_TARGET.md`
- `src/bin/aether_daemon.rs`
- `src/core/kuramoto_field.rs`
- `src/core/wave_interference.rs`
- `src/morphogenesis.rs`
- `src/thermodynamic_dreamer.rs`
- `tests/phase2_m5_hardening.rs`
- `tests/phase2_m7_hardening.rs`

## [0.0.17] - 2026-11-14

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, tests:1).
- Gate: ok (2s).

- `Cargo.lock`
- `tests/phase2_hardening.rs`

## [0.0.16] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, tests:2).
- Gate: ok (71s).

- `Cargo.lock`
- `tests/phase2_hardening.rs`
- `tests/phase2_m5_hardening.rs`

## [0.0.15] - 2026-11-14

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, src:1).
- Gate: ok (3s).

- `Cargo.lock`
- `src/thermodynamic_dreamer.rs`

## [0.0.14] - 2026-11-14

### Changed

- Ciclo automatico: 4 arquivo(s) (benches:1, src:1, tests:2).
- Gate: ok (4s).

- `benches/wave.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/organism_step_into.rs`
- `tests/phase2_hardening.rs`

## [0.0.13] - 2026-11-14

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, benches:4, docs:1, examples:1, src:2, tests:1).
- Gate: ok (0s).

- `Cargo.toml`
- `benches/kuramoto_latency.rs`
- `benches/organism.rs`
- `benches/solver.rs`
- `benches/wave.rs`
- `src/lib.rs`
- `src/transducer_io.rs`
- `docs/TRANSDUCER_IO.md`
- `examples/transducer_probe.rs`
- `tests/transducer_invariants.rs`

## [0.0.12] - 2026-11-14

### Changed

- Ciclo automatico: 17 arquivo(s) (.:1, benches:3, src:9, tests:4).
- Gate: ok (1s).

- `Cargo.lock`
- `benches/kuramoto_latency.rs`
- `benches/solver.rs`
- `benches/wave.rs`
- `src/bin/aether_daemon.rs`
- `src/bin/organism.rs`
- `src/core/kuramoto_field.rs`
- `src/error.rs`
- `src/morphogenesis.rs`
- `src/phase_memory.rs`
- `src/simulation/closed_loop_organism.rs`
- `src/simulation/panel.rs`

## [0.0.11] - 2026-11-14

### Changed

- Ciclo automatico: 3 arquivo(s) (.:2, src:1).
- Gate: ok (2s).

- `Cargo.lock`
- `README.md`
- `src/core/kuramoto_field.rs`

## [0.0.10] - 2026-11-14

### Changed

- Ciclo automatico: 6 arquivo(s) (.:2, src:1, tests:3).
- Gate: ok (3s).

- `Cargo.lock`
- `Cargo.toml`
- `src/bin/aether_daemon.rs`
- `tests/alloc_invariants.rs`
- `tests/phase2_invariants.rs`
- `tests/physical_invariants.rs`

## [0.0.9] - 2026-11-14

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, tests:1).
- Gate: ok (52s).

- `Cargo.lock`
- `tests/alloc_invariants.rs`

## [0.0.8] - 2026-11-14

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, docs:1, src:2, tests:1, tools:1).
- Gate: ok (1s).

- `README.md`
- `docs/REPO_SYNC.md`
- `src/bin/aether_daemon.rs`
- `src/core/wave_interference.rs`
- `tests/test_wave.rs`
- `tools/cycle.ps1`

## [0.0.7] - 2026-11-14

### Changed

- Ciclo automatico: 1 arquivo(s) (.:1).
- Gate: ok (1s).

- `Cargo.lock`

## [0.0.6] - 2026-11-14

### Changed

- Ciclo automatico: 7 arquivo(s) (.:1, src:4, tests:2).
- Gate: ok (2s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `tests/test_wave.rs`
- `src/morphogenesis.rs`
- `src/phase_memory.rs`
- `src/thermodynamic_dreamer.rs`
- `tests/phase2_invariants.rs`

## [0.0.5] - 2026-11-14

### Changed

- Ciclo automatico: 5 arquivo(s) (.:3, src:1, tests:1).
- Gate: ok (1s).

- `Cargo.lock`
- `Cargo.toml`
- `src/lib.rs`
- `tests/test_wave.rs`
- `ARCHITECTURE_PHASE2.md`

## [0.0.4] - 2026-11-14

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, src:3, tools:1).
- Gate: ok (0s).

- `Cargo.lock`
- `src/core/wave_interference.rs`
- `src/error.rs`
- `src/simulation/closed_loop_organism.rs`
- `tools/cycle.ps1`

## [0.0.3] - 2026-11-14

### Changed

- Ciclo automatico: 6 arquivo(s) (.:2, docs:1, src:2, tests:1).
- Gate: ok (1s).

- `Cargo.lock`
- `README.md`
- `docs/REPO_SYNC.md`
- `src/error.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/test_kuramoto.rs`

## [0.0.2] - 2026-11-14

### Changed

- Ciclo automatico: 9 arquivo(s) (.backup:1, src:4, tests:2, tools:2).
- Gate: ok (1s).

- `src/core/thermodynamic_solver.rs`
- `src/core/wave_interference.rs`
- `src/lib.rs`
- `src/simulation/closed_loop_organism.rs`
- `tests/test_kuramoto.rs`
- `tests/test_thermodynamic.rs`
- `tools/cycle.ps1`
- `tools/cycle_install.ps1`
- `.backup/`

## [0.0.1] - 2026-11-14

### Changed

- Ciclo automatico: 14 arquivo(s) (.:7, .cargo:1, benches:1, docs:1, scripts:1, src:1, tests:1, tools:1).
- Gate: fail (42s).

- `.cargo/`
- `.gitignore`
- `ARCHITECTURE.md`
- `CHANGELOG.md`
- `Cargo.lock`
- `Cargo.toml`
- `FORUM_PROTOCOL.md`
- `benches/`
- `docs/`
- `rust-toolchain.toml`
- `scripts/`
- `src/`

## [0.0.1] - 2026-11-14

### Added

- Baseline inicial do repositorio: `Cargo.toml`, `Cargo.lock`, `ARCHITECTURE.md`, `FORUM_PROTOCOL.md`, `docs/API_CONTRACT.md`.
- Scaffold `src/lib.rs` + `src/error.rs` com `#![forbid(unsafe_code)]`.
- Modulos de fase 1 em andamento por agente: `thermodynamic_solver` (m1), `wave_interference` (m2), `kuramoto_field` (m3), `closed_loop_organism` + `panel` (m4).
- Testes de invariantes fisicos e de alocacao, benches criterion.
- Automacao de ciclo (`tools/cycle.ps1`) e politica de sincronizacao (`docs/REPO_SYNC.md`).

# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
Versionamento: semântico; ciclo automático ainda gera patches `0.1.x` quando aplicável.

## [0.5.3] - 2026-11-21

### Added

- **T-NL-IN contrato** (`tnl_in.rs`): artefato `aether_tnl_in_contract.json` +
  gate `agent_tnl_in_gate`.
- **Hub T-HITL** (`dashboard/hitl_view.rs`): fila Approve/Deny/Ampliar,
  `hitl_token`, kill switch; High sem token bloqueado.
- **T-NL-OUT P2** (`nl_out.rs`): juíz claim↔texto + `CertifiedUtterance`.
- **Modalidades stub**: `OcrPort`, `SttPort`/`TtsPort`, `VisionPort` → funil
  T-NL-IN; features `ocr`/`speech`/`vision` (default off).
- **T-EMB opt-in**: `EmbedPort::from_env` (`AETHER_EMBED=1`).
- **T-PRED no MoE**: `PredictExpert` + `Task::Predict` + intervalo/calib.
- **A8 v1 critérios**: `a8_v1.rs` / `aether_a8_v1_criteria.json`
  (`enter_v1_default=false`).
- **Hier MoE/LSH posição**: `hier_moe.rs` + feature `hier_moe` (off).
- Gates: `agent_nl_out_p2`, `modalidades`, `emb_optin`, `tpred_moe`,
  `a8_v1`, `hier_moe`; §6 `agent_accept_s6_gate`.
- Docs: `RELEASE_0_5_3.md`; CORE_TOMADAS §8 plugue; HUB/PROSA/ACOES.

### Changed

- `dashboard` feature passa a puxar `agent` (hub HITL tipado).
- Telemetria `StepTrace` enriquecida (cost/args/claims) — aceite §6.

## [0.5.2] - 2026-11-21

### Added

- **A6 / P4**: LinUCB contextual opt-in (`AETHER_LINUCB`), `learner.rs`
  (reward só com `Feedback.correct`), features de goal no planner.
- **A7**: `risk_policy.rs` — `RISK_MATRIX`, `AETHER_AGENT_KILL`, High always
  HITL / Medium `AETHER_HITL_MEDIUM`; gate no executor.
- **A8 / P5 fio**: `ExpertId::Predict`, `predict.rs`, tools `predict.series`
  (read_local) + `goal.propose` (privileged, EFE, `execute=false`); metacog
  self_field; demo `examples/a8_predict_demo.rs`.
- Gates: `agent_a6_gate`, `agent_a7_gate` (6/6), `agent_a8_gate` (5/5).
- Docs: `RELEASE_0_5_2.md`; ACOES/ROADMAP/LESSONS/CORE atualizados.

### Changed

- Planner: affordance ≥0.95 não some sob `Risk::High.penalty` (piso
  `min_score+1`).
- `research.query`: affordance 0 em relações exclusivas (`predict.series`,
  `goal.propose`, fs/http/mcp/shell).
- `MoERouter` SLOTS=6 (`ExpertId::Predict`).

### Fixed

- False-act de forecast sob OOD/surpresa (claims vazias + `ood`/`abstain`).
- Meta endógena sem HITL token bloqueada pela matriz A7.

## [0.5.1] - 2026-11-21

### Added

- **T-CODE K1–K8** completo (`src/agent/tools/code/*` + `shell.rs`):
  check/ast_grep/test, `PatchProposal`/`code.patch`, `code.generate` (opt-in),
  `code.loop` keep\|rollback, grammar/RASP, `code.lm` offline, `code.eval`
  (`coder_t3_pilot` + D-G2), `shell.exec` HITL + kill switch + allowlist.
- **A5 research** no planner: `research.query` / `research.expand` HITL
  (`research.rs`; `agent_research_gate`).
- Gate `tests/agent_coder_gate.rs` (17/17).
- Borda: `tools/code_generate.py`, `tools/code_lm_local.py`.
- Docs: `RELEASE_0_5_1.md`; CODER/CORE/ACOES/EVAL/LESSONS atualizados.

### Changed

- `shell.exec`: de always-deny para deny-default com canal HITL (`AETHER_SHELL`,
  `AETHER_HITL_TOKEN`, `AETHER_SHELL_KILL`).
- `HitlKind::ShellExec`; `Goal::shell_exec`; auth caps `CODE_AUTH_CAP` /
  `SHELL_AUTH_CAP` ≤120.

### Fixed

- Planner: High.penalty / `escalation` não selecionavam shell sob HITL
  (Medium+Irreversible com ceiling High).
- Race de env nos gates coder (`CODEGEN_ENV` / `SHELL_ENV`).

## [0.5.0] - 2026-11-21

### Added

- **G-A4 / mundo sandbox** no `ToolAgent`: `fs.read`/`fs.write`, `http.get`
  (allowlist, fetch opt-in), `mcp.call` tipado, `shell.exec` always-deny.
- Gate `tests/agent_safety_gate.rs` (9/9): traversal, allowlist, shell, injeção.
- **T-HITL** unificado: `src/agent/hitl.rs` (`HitlRequest`→`HitlDecision`).
- **T-EMB** formal: `src/agent/embed.rs` (`EmbedPort` off; F4 continua congelada).
- **T-DEC** no hub: `src/agent/dec_status.rs` + `src/dashboard/dec_view.rs`.
- Pack piloto `data/packs/moe_etl_pilot/` (manifest + facts + questions + report).
- Docs: `CORE_TOMADAS`, `TOMADAS_FUTURO`, `VISAO_AETHER`, `SPEECH_AETHER`,
  escadas P/A/K/M atualizadas.

### Changed

- `LanguagePort::generate` (P1): template cita `(fonte=… auth=… conf=…)`.
- `tools.rs` → módulo `src/agent/tools/` (fs/http/mcp/shell).
- Programa core×tomadas em 7 sprints fechado (ver `docs/CORE_TOMADAS.md`).

### Fixed

- Disjuntor sandbox (#8) operacional; K0 coder desbloqueado por G-A4.

## [0.1.13] - 2026-11-21

### Added

- Fiação documental CORE_TOMADAS + pesquisa paralela (SearX default lumy).

## [0.1.12] - 2026-11-21

### Changed

- F4 CONGELADA REPROVADA (HITL MSG 1093): V3-A/B esgotadas; melhor placar
  Qwen3-Reranker-0.6B hit@5 0,9577; gate 0,974 intacto; cadeira fechada.
- Hub Lab: faixa F4 CONGELADA (`src/dashboard/f4_view.rs`).
- Docs: `docs/F4_CLOSE_V3_HITL.md`; placares em `data/reports/f4_v3_*.json`.
- Harness: medição densa/CUDA + `tools/f4_rerank_v3b.py` (já em 0.1.11+/commits
  `0a9924b` / `3f5d7e9`).

## [0.1.11] - 2026-11-20

### Changed

- Ciclo automatico: 14 arquivo(s) (data:11, docs:1, src:1, tools:1).
- Gate: ok (10s).

- `data/a5_promote_p3_report.json`
- `data/f1_5_ttt_abstain.json.meta.json`
- `data/f1_a1b_vega_check.json.meta.json`
- `data/f1_decision_smoke_selftest.meta.json`
- `data/f1_offline_suite.json`
- `data/f1_offline_suite.json.meta.json`
- `data/reports/f5_t1_eval.json.meta.json`
- `data/reports/f5_t1_loop.json.meta.json`
- `data/reports/f5_t2_allowlist.json.meta.json`
- `docs/F1_CONTRATO_MEDIDO.md`
- `src/agent/experts.rs`
- `data/cycle_gate.json`

## [0.1.10] - 2026-11-20

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, data:2, docs:1, src:2, tests:1, tools:1).
- Gate: ok (11s).

- `Cargo.lock`
- `data/f1_decision_smoke_selftest.meta.json`
- `docs/F5_T4_DECISION_ROUTE_SPEC.md`
- `src/agent/cli.rs`
- `src/api/mod.rs`
- `tools/vega_fit_probe.py`
- `data/vega_fit_corpus_1500.json`
- `tests/api_decision.rs`

## [0.1.9] - 2026-11-20

### Changed

- Ciclo automatico: 102 arquivo(s) (aether-decision:1, data:45, docs:27, scripts:1, src:3, tests:1, tools:24).
- Gate: ok (13s).

- `aether-decision/src/pyo3_impl.rs`
- `data/f1_decision_smoke_selftest.meta.json`
- `docs/BUILD_MULTI_AGENT.md`
- `docs/F1_A5_A2C_CURSOR.md`
- `docs/F1_AUDIT.md`
- `docs/F2_PESQUISA_EXTERNA.md`
- `docs/F4_GATE_DECISAO_255.md`
- `docs/F5_T2_ALLOWLIST.md`
- `docs/F5_T3_EVAL.md`
- `docs/MF2_GATE_REGRESSION.md`
- `scripts/f1_smoke.ps1`
- `src/agent/cli.rs`

## [0.1.8] - 2026-11-20

### Changed

- Ciclo automatico: 37 arquivo(s) (aether-decision:8, data:9, docs:10, examples:1, src:1, tests:1, tools:7).
- Gate: ok (2s).

- `aether-decision/examples/f1_env_probe.rs`
- `aether-decision/src/bootstrap.rs`
- `aether-decision/src/config.rs`
- `aether-decision/src/gate.rs`
- `aether-decision/src/port.rs`
- `aether-decision/src/pyo3_impl.rs`
- `aether-decision/src/types.rs`
- `aether-decision/src/watchdog.rs`
- `docs/F1_AUDIT.md`
- `docs/F1_CONTRATO_MEDIDO.md`
- `docs/F1_ENV_DURAVEL.md`
- `docs/F2_PESQUISA_EXTERNA.md`

## [0.1.7] - 2026-11-20

### Changed

- Ciclo automatico: 19 arquivo(s) (data:2, docs:6, src:2, tests:1, tools:8).
- Gate: ok (102s).

- `docs/F1_CONTRATO_MEDIDO.md`
- `docs/F2_PESQUISA_EXTERNA.md`
- `docs/F5_RESEARCH_TOOL_CONTRACT.md`
- `docs/PESQUISA_B-G0_PAWS_E1_EMBED.md`
- `src/agent/mod.rs`
- `tests/f1_contract.rs`
- `tools/f5_t0_eval.py`
- `tools/research_harness.py`
- `data/f1_gate_verdicts_fixture.json`
- `data/f5_t1_turns_fixture.json`
- `docs/F5_T1_AGENT_LOOP.md`
- `docs/MF2_GATE_REGRESSION.md`

## [0.1.6] - 2026-11-20

### Changed

- Ciclo automatico: 9 arquivo(s) (.:1, aether-decision:2, data:1, docs:3, src:1, tools:1).
- Gate: ok (1s).

- `Cargo.lock`
- `aether-decision/Cargo.toml`
- `data/research_tool_contract.json`
- `docs/F1_CONTRATO_MEDIDO.md`
- `docs/F5_RESEARCH_TOOL_CONTRACT.md`
- `src/agent/cli.rs`
- `aether-decision/examples/`
- `docs/F2_CERT_DIGEST.md`
- `tools/cert_digest.py`

## [0.1.5] - 2026-11-20

### Changed

- Ciclo automatico: 11 arquivo(s) (.:2, aether-decision:5, data:1, src:1, tools:2).
- Gate: ok (18s).

- `Cargo.lock`
- `Cargo.toml`
- `aether-decision/src/config.rs`
- `aether-decision/src/gate.rs`
- `aether-decision/src/port.rs`
- `aether-decision/src/pyo3_impl.rs`
- `aether-decision/src/types.rs`
- `src/agent/cli.rs`
- `tools/research_harness.py`
- `tools/research_reliability.py`
- `data/research_tool_contract.json`

## [0.1.3] - 2026-11-19

### Changed

- Ciclo automatico: 2 arquivo(s) (src:2).
- Gate: ok (33s).

- `src/knowledge/sgdb/cli.rs`
- `src/knowledge/sgdb/transport.rs`

## [0.1.2] - 2026-11-19

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, docs:2, src:3).
- Gate: ok (26s).

- `Cargo.lock`
- `docs/HUB_DASHBOARD.md`
- `docs/USER_GUIDE.md`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/mod.rs`
- `src/dashboard/console.rs`

## [0.1.1] - 2026-11-19

### Changed

- Ciclo automatico: 1 arquivo(s) (src:1).
- Gate: ok (36s).

- `src/knowledge/sgdb/client.rs`

## [0.0.1] - 2026-11-19

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:2, scripts:1, src:1).
- Gate: ok (1s).

- `Cargo.toml`
- `docs/HUB_DASHBOARD.md`
- `docs/PESQUISA_B-G0_PAWS_E1_EMBED.md`
- `scripts/produto-c-smoke.ps1`
- `src/bin/aether_dashboard.rs`

## [0.0.1] - 2026-11-19

### Changed

- Ciclo automatico: 17 arquivo(s) (.:1, data:1, docs:7, scripts:3, src:3, tools:2).
- Gate: ok (223s).

- `README.md`
- `docs/GPU_PLAN.md`
- `docs/HUB_DASHBOARD.md`
- `docs/PESQUISA_B-G0_PAWS_E1_EMBED.md`
- `docs/USER_DELIVERY_PLAN.md`
- `scripts/package-daemon.ps1`
- `src/bin/aether_daemon.rs`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/catalog.rs`
- `tools/b0_bits_f32_rerank.py`
- `data/rounds/20261117_b0_paws/emb/s2_query_n200.f32`
- `docs/B2_BITS_F32_RERANK.md`

## [0.1.0] - 2026-11-18 — **freeze IA usável**

### Added

- Contrato de resposta em toda superfície: `act` · `cite` · `propose` ·
  `abstain` com `authority` (96→128→200→255) e certificado quando há solver/verify.
- Gates de freeze **F1–F7** verdes (checklist em
  [`CYCLE_CLOSE_V01_FREEZE.md`](CYCLE_CLOSE_V01_FREEZE.md)):
  eval_suite, CIC abstain (n=5000, false_act=0), api_agent/api_eval,
  usability+redteam, cognitive_audit 12/12 MEASURED, fmt/clippy/lib limpos e
  smoke end-to-end da API (hit conhecido, OOD→abstain, frontier em `/props`).
- `docs/V01_DELIVERY_PLAN.md`: mapa entregável×lab, persona integrador,
  contrato obrigatório e ondas de entrega.

### Security

- Escada de autoridade honrada no freeze: ≥200 exige certificado ou HITL;
  escopo não vaza (R8); memória é evidência, nunca instrução.


## [0.0.177] - 2026-11-18

### Changed

- Ciclo automatico: 2 arquivo(s) (scripts:1, src:1).
- Gate: ok (8s).

- `scripts/v01_freeze_gates.ps1`
- `src/transducer_host.rs`

## [0.0.176] - 2026-11-18

### Changed

- Ciclo automatico: 2 arquivo(s) (scripts:1, src:1).
- Gate: ok (13s).

- `scripts/onnx_backend_probe.py`
- `src/agent/undo_sgdb.rs`

## [0.0.175] - 2026-11-18

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, scripts:3, tests:1).
- Gate: ok (15s).

- `Cargo.lock`
- `scripts/onnx_backend_probe.py`
- `scripts/v01_freeze_gates.ps1`
- `scripts/v01_freeze_smoke.ps1`
- `tests/us_aggregate_gate.rs`

## [0.0.174] - 2026-11-18

### Changed

- Ciclo automatico: 4 arquivo(s) (docs:1, scripts:2, tests:1).
- Gate: ok (63s).

- `docs/PESQUISA_B-G0_PAWS_E1_EMBED.md`
- `scripts/onnx_backend_probe.py`
- `scripts/v01_freeze_smoke.ps1`
- `tests/us_aggregate_gate.rs`

## [0.0.173] - 2026-11-18

### Changed

- Ciclo automatico: 20 arquivo(s) (.:2, docs:9, scripts:1, src:1, tests:1, tools:6).
- Gate: ok (36s).

- `Cargo.lock`
- `README.md`
- `docs/DOACAO_PLAYBOOK.md`
- `docs/DONOR_CATALOG.md`
- `docs/GATE_E1_PAWS_CONSISTENCY.md`
- `docs/GPU_PLAN.md`
- `docs/PESQUISA_B-G0_PAWS_E1_EMBED.md`
- `docs/PLAN_2026-11-17_OTIMIZACAO_ARXIV.md`
- `docs/RESEARCH_DOADORAS_2026.md`
- `docs/USER_DELIVERY_PLAN.md`
- `src/api/mod.rs`
- `tests/b0_e1_order_gate.rs`

## [0.0.172] - 2026-11-18

### Changed

- Ciclo automatico: 38 arquivo(s) (.:1, .cursor:1, data:11, docs:9, scripts:2, src:4, tests:4, tools:6).
- Gate: ok (16s).

- `CHANGELOG.md`
- `docs/CYCLE_CLOSE_2026-11-18_PERF_ONDA_A.md`
- `docs/DOACAO_PLAYBOOK.md`
- `docs/DONOR_CATALOG.md`
- `docs/EMBED_THROUGHPUT_ANALYSIS.md`
- `docs/PLAN_2026-11-17_OTIMIZACAO_ARXIV.md`
- `scripts/embed_rate_1k.py`
- `src/bin/aether_dashboard.rs`
- `src/core/hv.rs`
- `src/dashboard/jobs.rs`
- `src/dashboard/transplant_view.rs`
- `tests/b0_e1_order_gate.rs`

## [Unreleased]

### Added

- `tools/scribe_extract.py` — escriba de triplas LLM → facts.jsonl (Qwen3-8B
  default, JSON estrito, T=0, NFKC, dedup, quarentena authority 60–120;
  dry-run não finge extração).
- `docs/RESEARCH_DOADORAS_2026.md` — pesquisa de doadoras 2026 (mapa manter
  Qwen3-Embedding-0.6B; escriba → Qwen3-8B; proibidas CC BY-NC).
- SGDB stdio real: `mcp_server 1.5.11` (neural-sgdb `abdf3c9`, feature
  `file-storage`) em `neural-sgdb/target/mcp-release`; db `sgdb_memory.db`;
  outbox `target/aether_sgdb.jsonl`.
- A3-shard: completude do pipeline `20261117_b0_paws_s0/s1`
  (embed → harvest `--jl-only` → `b0_pair_cosines`) para o gate da lei de
  capacidade E1B (2×2500 < 4096).
- `tests/us_aggregate_gate.rs` + `docs/US0_AGGREGATE_G7.md` — G7 do freeze
  v0.1: checklist executável 1:1 (pass=4, wired=6, fail=0) com validação
  numérica dos artefatos presentes e assert de existência das fontes.
- `tools/d1_maxsim_st6.py` — probe D1 per-token com a API canônica do ST6
  (`MultiVectorEncoder.encode_query/encode_document` assimétrico), meanmaxsim
  + maxsim_sum, Youden t_mid, bootstrap estratificado B=1000 IC95,
  `--dry-check` (guard do CURSOR, MSG 0674).
- `docs/B2_BITS_F32_RERANK.md` — desenho B2: routing bits + re-rank f32,
  critérios de gate (recall@1 ≥ 0,90 teto A3 intocado, p99 ≤ 5 ms, 0 alloc).

### Fixed

- `docs/DOACAO_PLAYBOOK.md` — Estágio 2 passa a apontar a tool do escriba
  própria; REBEL (CC BY-NC) removido da receita.
- `tools/cycle.ps1` (commit `3c00358`) — versão nunca desce de 0.1.0 (o
  regex antigo resetava para 0.0.1 a cada ciclo pós-freeze) e tag automática
  somente abaixo de 0.1.0 (tag 0.1.x exige pedido humano, doutrina V01).
- `tools/scribe_extract.py` — guard anti-deslocamento batch/outputs
  (lição MSG 0567); linha morta removida.

## [0.0.171] - 2026-11-18

### Added

- `tools/embed_runtime.py` — runtime compartilhado CPU/GPU (L1/L2) + backend ONNX opt-in (L3).
- `docs/EMBED_THROUGHPUT_ANALYSIS.md` — root cause bf16@CPU e alavancas.
- `docs/PLAN_2026-11-17_OTIMIZACAO_ARXIV.md` — plano TRAVADO pós-votação 0471/0474/0476.
- `docs/CYCLE_CLOSE_2026-11-18_PERF_ONDA_A.md`.
- A4: multi-probe LSH (`lsh_probes=8`) + path shard ≥10k no `b2_100k_gate`.
- A5: `WorldState.claims` + poda de pré-condição no planner (dependencia 8/8 @ budget 2).
- Round `data/rounds/20261117_b0_paws/facts.jsonl` + `tools/b0_paws_harvest.py`.

### Changed

- `scripts/embed_rate_1k.py` / `tools/donor_map_embed.py`: fp32@CPU batch16 + warmup;
  bf16@CUDA batch64 + FA2 se disponível; seed/T0 no artefato.
- A1 limpo: **1,56 → 10,82 txt/s** (~6,9×); ETA 100k **2,57 h** (teto 4 h OK).
- README e crate em **0.0.171**.

### Fixed

- Experimento MCTS: expectativa alinhada à poda A5 (delta_pp=0; MCTS continua abstain).

## [0.0.170] - 2026-11-17

### Added

- Hub **Testes IA** (aba Lab | Testes IA): USO U1–U6, ApiEval, AgentDrive,
  contrato U1–U13 vs LLM (MSG 0449/0452); `src/dashboard/ai_view.rs`.
- P1.2: LSH com limiar **≥10k** + assert no `b1_10k_gate` (MSG 0440).
- P1.3: D1 percentis por chamada (n=200, MSG 0428 OP1); artefato
  `target/d1_budget.json`.
- B-G0 labeled: `tools/reharvest_paws_bg0.py` → `data/rounds/bg0_paws/`
  (5000 pares parafraseia/nao_parafraseia).
- B2 scaffold: `tests/b2_100k_gate.rs` (LSH obrigatória, recall@10≥0,90).
- `docs/FFT_V4.md` + `docs/CYCLE_CLOSE_2026-11-17_FASE3_DONOS.md`.
- `scripts/embed_rate_1k.py` (taxa donor 1k → decide B2 CPU/GPU).

### Changed

- README e crate em **0.0.170**.
- Cap `authority_max_transplant` 240→120 no harvest (doutrina 0254/0258;
  já em `88a76a5`).
- `docs/USABILITY_TESTS.md`: U12 alinhado a D1 per-call.

### Fixed

- Clippy: `div_ceil` no D1; `items_after_test_module` / FFT probe / gates
  de aprendizado e redteam (onda 0.0.169→170).

## [0.0.169] - 2026-11-17

### Added

- Quadro de ondas H/M/G/A/K/P/S fechado: `docs/CYCLE_CLOSE_2026-11-17_ONDAS.md`.
- SGDB stdio real: `tests/sgdb_stdio_gate` (remember/recall/audit/`contradicts`);
  `Transport::notify` para `notifications/initialized` sem deadlock.
- FFT em lote (`src/core/gpu/fft.rs` + `tests/gpu_fft_gate`): erro L2 4,16e-7
  vs `rustfft`; razão 0,20 → **abstain** (barra ≥2×); `force_off` sem panic.
- CI opt-in `gate-gpu` (`workflow_dispatch` + self-hosted `gpu`).
- Agente: D1 (`agent_budget_gate`), MCTS medida/abstain (`mcts_measure_gate`),
  D3 red team (`agent_redteam_gate`), LinUCB opt-in (`linucb.rs` +
  `agent_learn_gate`, EXP3 permanece default), `semantic.match`.
- Produto C: `SystemTelemetryTransducer`, daemon `--host`/`--human`/`--log`,
  `scripts/package-daemon.ps1`, `scripts/two-instances.ps1`.
- Conhecimento: rodadas `data/rounds/20261117_bg0` (B-G0) e
  `data/rounds/20261117_b1` (10k paws-x; recall@10=1,0; p99=642 µs);
  `tools/bg0_probe.py`; `tests/b1_10k_gate`.
- `examples/fft_gpu_probe.rs` (já em Unreleased da 0.0.168+): cruzamento
  medido na 3050 sem crossover (0,18–0,22×); alinhado ao abstain de G1.

### Changed

- README e crate em **0.0.169**.
- `scripts/run-demo.ps1` registra default `--plain` (Onda S).
- `gpu_boundary_gate` inclui `wave_interference` no hot path sem GPU.

### Fixed

- `src/core/gpu/jl_tiled.rs`: `clippy::unnecessary_unwrap` (linha 167) —
  `clippy -D warnings` verde novamente; gate `gpu_jl_tiled_gate` 3/3
  (serializado: em threads paralelas os testes disputam o device).

## [0.0.168] - 2026-11-17

### Added

- `docs/CYCLE_CLOSE_2026-11-17_GPU.md`: fechamento do track **GPU** (G0–G3) com
  números medidos, contrato de determinismo e **gates ainda necessários**
  (CI gpu, FFT-G2, borda do bit-exato do JL, simetria do `J` no Ising).

### Changed

- `tests/gpu_ising_sweep_gate`: 4º gate — o sweep na **família real do E2c-2**
  (vidro `±1` com viés forte, `h ≠ 0`), 1.000 instâncias: 850/1.000 idênticas,
  KS 0,014 (piso de ruído 0,024), **11,9×** e **média 0,089% pior** (limite
  declarado e codificado no próprio gate).
- `docs/GPU_PLAN.md`: fechamento do ciclo (tabela G0–G3 com G1 `act` e G2
  `abstain`), validação E2c-2 e a dívida de gates.
- `docs/HW_TARGET.md` §4: tolerância de determinismo do JL
  (`|dot| < 1e-3`, medido 1 palavra em 320.000).

## [0.0.167] - 2026-11-17

### Changed

- Ciclo de mediação CURSOR **fechado**: doc `CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0`
  lista gates de regressão e abertos do próximo ciclo (FFT-G2, MCTS condicional,
  D1/D3, CI gpu, B-G0/B1/B4, SGDB-remember).
- `GPU_PLAN` G3: família com viés admite desvio relativo de média ≤ 0,2%
  (sweep = exploração, não solver auditado).
- Gate Ising: assert de média alinhado ao limite 0,2%.

### Fixed

- Forget `authority=255` sem store → `500 ledger_io` (ledger + memory); testes
  `api_agent` 14/14 (MSG 0415/0420).

## [0.0.166] - 2026-11-17

### Added

- **Autoridade por evidência (fecha a escada):** `VerifiedFact` no blackboard;
  `verify` liga claim de solver a certificado válido (`violations=0`), e
  `memory.write` grava em **200** só quando o fato está verificado. Sem
  certificado, quarentena (96); com 2 fontes, 128.
- Gate `tests/agent_certificate_authority.rs` (3): promove com certificado,
  fica em quarentena sem certificado, não promove com `violations>0`.
- Gate `tests/agent_undo_stress.rs`: 240 escritas/rewinds aleatórios contra
  modelo independente (undo LIFO + CAS + watermark).

### Changed

- `ToolContext.verified` entra no `rollback_to` (HITL/rollback não deixa
  evidência órfã).
- Planner usa `p_sucesso` calibrado (temperatura P4; identidade por default).

### Fixed

- `authority >= 255` virou `== AUTHORITY_HITL` (clippy `absurd_extreme_comparisons`).
- Comentário do `forget`: 255 sem store retorna `500 ledger_io` nas duas rotas.

### Gates

- lib **165**; `agent_plan_gate` **15**; `agent_tools_gate` **5**;
  `agent_undo_sgdb_gate` **6** + 11 unit; `agent_undo_stress` **1**;
  `agent_certificate_authority` **3**; `agent_gate` **6**; `agent_cli` **2**;
  `agent_eval_gate` **2**; `agent_eval_suite` **1** (24 casos);
  `agent_self_model_gate` **4**; `language_port_gate` **9**;
  `api_agent` **14**; `api_http` **9**; `api_integrity` **10**;
  `api_lifecycle` **5**; `api_sgdb` **2**; `api_eval` **1**.

### Pendente (próximo ciclo)

- MCTS atrás de gate medido; D1 orçamentos p50/p99/RSS; D3 red team;
  CI `--features gpu`; `cargo fmt --check`; B-G0/B1/B4; FFT G2.

## [0.0.165] - 2026-11-17

### Changed

- Fechamento de ciclo: Onda 0 / doação lab + residual agent/API.
- `VerifiedFact` + rollback de `verified` no HITL (`memory.write` → 200).
- Forget `authority=255` sem store → `500 ledger_io` (ledger e memory).
- Docs: CYCLE_CLOSE doação, EXTERNAL_API, USER_DELIVERY 0.6, playbook §15.
- Revalidado neste fecho: `api_agent` 14/14, `api_lifecycle` 5/5, `agent_certificate_authority` 3/3. JL tiled já está em `act` (o item JL-V4 do board 0414 está encerrado). Seguem abertos: FFT G2, MCTS, CI `--features gpu`, `cargo fmt --check`, B-G0/B1/B4.

### Files

- `Cargo.toml`, `CHANGELOG.md`, `docs/CYCLE_LOG.md`
- `docs/CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md`, `docs/DOACAO_PLAYBOOK.md`
- `docs/USER_DELIVERY_PLAN.md`, `docs/EXTERNAL_API.md`
- `src/api/mod.rs`, `src/agent/executor.rs`, `src/agent/tools.rs`
- `docs/AETHER_AGENT_ROADMAP.md`
- `tests/api_agent.rs`, `tests/api_lifecycle.rs`, `tests/agent_certificate_authority.rs`

## [0.0.164] - 2026-11-17

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:2, src:1, tests:1).
- Gate: ok (6s).

- `Cargo.lock`
- `docs/GPU_PLAN.md`
- `docs/HW_TARGET.md`
- `src/core/gpu/jl_tiled.rs`
- `tests/gpu_jl_tiled_gate.rs`

## [0.0.163] - 2026-11-17

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, tests:1).
- Gate: ok (5s).

- `Cargo.lock`
- `tests/gpu_jl_tiled_gate.rs`

## [0.0.162] - 2026-11-17

### Changed

- Ciclo automatico: 4 arquivo(s) (src:3, tests:1).
- Gate: ok (6s).

- `src/core/gpu/mod.rs`
- `src/core/gpu/jl_tiled.rs`
- `src/core/gpu/jl_tiled.wgsl`
- `tests/gpu_jl_tiled_gate.rs`

## [0.0.161] - 2026-11-17

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, bench:1, src:1, tests:2).
- Gate: ok (31s).

- `Cargo.lock`
- `src/api/mod.rs`
- `tests/api_agent.rs`
- `bench/`
- `tests/agent_eval_suite.rs`

## [0.0.160] - 2026-11-17

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, docs:1, src:2, tests:2).
- Gate: ok (17s).

- `Cargo.lock`
- `docs/LANGUAGE_PORT.md`
- `src/agent/undo_sgdb.rs`
- `src/api/mod.rs`
- `tests/api_agent.rs`
- `tests/language_port_gate.rs`

## [0.0.159] - 2026-11-17

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:1, src:2, tests:1).
- Gate: ok (17s).

- `Cargo.lock`
- `docs/LANGUAGE_PORT.md`
- `src/agent/language.rs`
- `src/api/mod.rs`
- `tests/api_agent.rs`

## [0.0.158] - 2026-11-17

### Changed

- Ciclo automatico: 25 arquivo(s) (.:2, data:2, docs:6, scripts:1, src:9, tests:3, tools:2).
- Gate: ok (133s).

- `Cargo.lock`
- `README.md`
- `data/manifest.json`
- `docs/API_CONTRACT.md`
- `docs/DOACAO_PLAYBOOK.md`
- `docs/GPU_PLAN.md`
- `docs/HUB_DASHBOARD.md`
- `docs/USER_DELIVERY_PLAN.md`
- `docs/USER_GUIDE.md`
- `src/agent/language.rs`
- `src/agent/self_model.rs`
- `src/api/mod.rs`

## [0.0.157] - 2026-11-17

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, docs:4, src:3, tests:1, tools:1).
- Gate: ok (13s).

- `Cargo.lock`
- `docs/ADR_AGENT.md`
- `docs/AETHER_AGENT_ROADMAP.md`
- `docs/DOACAO_PLAYBOOK.md`
- `docs/USER_DELIVERY_PLAN.md`
- `src/api/mod.rs`
- `src/core/gpu/ising.rs`
- `src/dashboard/jobs.rs`
- `tests/gpu_ising_sweep_gate.rs`
- `tools/transplant_uso.py`

## [0.0.156] - 2026-11-17

### Changed

- Ciclo automatico: 11 arquivo(s) (.:1, src:6, tests:4).
- Gate: ok (13s).

- `Cargo.lock`
- `src/agent/executor.rs`
- `src/agent/mod.rs`
- `src/agent/tool.rs`
- `src/agent/tool_agent.rs`
- `src/agent/tools.rs`
- `src/api/mod.rs`
- `tests/agent_plan_gate.rs`
- `tests/api_agent.rs`
- `tests/gpu_ising_sweep_gate.rs`
- `tests/gpu_jl_gate.rs`

## [0.0.155] - 2026-11-17

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (7s).

- `Cargo.lock`
- `src/api/mod.rs`
- `tests/api_agent.rs`

## [0.0.154] - 2026-11-17

### Changed

- Ciclo automatico: 7 arquivo(s) (.:1, src:4, tests:2).
- Gate: ok (14s).

- `Cargo.lock`
- `src/agent/eval.rs`
- `src/core/gpu/jl.rs`
- `src/core/gpu/mod.rs`
- `tests/gpu_jl_gate.rs`
- `src/core/gpu/ising.rs`
- `tests/gpu_ising_sweep_gate.rs`

## [0.0.153] - 2026-11-17

### Changed

- Ciclo automatico: 7 arquivo(s) (.:1, docs:1, src:4, tests:1).
- Gate: ok (14s).

- `Cargo.lock`
- `docs/GPU_PLAN.md`
- `src/agent/executor.rs`
- `src/agent/mod.rs`
- `src/agent/tool.rs`
- `src/agent/tool_agent.rs`
- `tests/agent_plan_gate.rs`

## [0.0.152] - 2026-11-17

### Changed

- Ciclo automatico: 7 arquivo(s) (src:5, tests:2).
- Gate: ok (24s).

- `src/agent/mod.rs`
- `src/agent/tool_agent.rs`
- `src/agent/tools.rs`
- `src/core/gpu/jl.rs`
- `tests/gpu_jl_gate.rs`
- `src/agent/language.rs`
- `tests/language_port_gate.rs`

## [0.0.151] - 2026-11-17

### Changed

- Ciclo automatico: 2 arquivo(s) (src:1, tests:1).
- Gate: ok (304s).

- `src/core/gpu/jl.rs`
- `tests/gpu_jl_gate.rs`

## [0.0.150] - 2026-11-17

### Changed

- Ciclo automatico: 3 arquivo(s) (examples:1, src:2).
- Gate: ok (7s).

- `examples/gpu_census.rs`
- `src/core/gpu/jl.rs`
- `src/core/gpu/mod.rs`

## [0.0.149] - 2026-11-17

### Changed

- Ciclo automatico: 13 arquivo(s) (.:1, docs:1, src:10, tests:1).
- Gate: ok (10s).

- `Cargo.lock`
- `docs/GPU_PLAN.md`
- `src/agent/cli.rs`
- `src/agent/mod.rs`
- `src/agent/registry.rs`
- `src/agent/tool.rs`
- `src/agent/executor.rs`
- `src/agent/planner.rs`
- `src/agent/self_model.rs`
- `src/agent/telemetry.rs`
- `src/agent/tool_agent.rs`
- `src/agent/tools.rs`

## [0.0.148] - 2026-11-17

### Changed

- Ciclo automatico: 5 arquivo(s) (src:5).
- Gate: ok (16s).

- `src/agent/experts.rs`
- `src/core/gpu/jl.rs`
- `src/core/gpu/mod.rs`
- `src/core/mod.rs`
- `src/agent/tool.rs`

## [0.0.147] - 2026-11-17

### Changed

- Ciclo automatico: 5 arquivo(s) (.:1, docs:2, src:1, tests:1).
- Gate: ok (17s).

- `Cargo.lock`
- `docs/AETHER_AGENT_ROADMAP.md`
- `src/core/gpu/jl.rs`
- `tests/gpu_jl_gate.rs`
- `docs/LANGUAGE_PORT.md`

## [0.0.146] - 2026-11-17

### Changed

- Ciclo automatico: 16 arquivo(s) (.:5, data:1, docs:4, examples:1, src:3, tests:2).
- Gate: ok (11s).

- `AGENTS.md`
- `COGNITIVE_BENCHMARK_SUITE.md`
- `Cargo.lock`
- `Cargo.toml`
- `README.md`
- `docs/AETHER_AGENT.md`
- `src/core/mod.rs`
- `src/core/transplant.rs`
- `tests/agent_gate.rs`
- `data/agent_facts.jsonl`
- `docs/ADR_AGENT.md`
- `docs/AETHER_AGENT_ROADMAP.md`

## [0.0.145] - 2026-11-17

### Changed

- Ciclo automatico: 8 arquivo(s) (.:4, docs:2, src:2).
- Gate: ok (10s).

- `AGENTS.md`
- `COGNITIVE_BENCHMARK_SUITE.md`
- `Cargo.lock`
- `README.md`
- `docs/COGNITIVE_AUDIT_FINAL.md`
- `src/agent/coherence.rs`
- `src/agent/experts.rs`
- `docs/COGNITIVE_AUDIT_RUN_2026-11-17.md`

## [0.0.144] - 2026-11-17

### Changed

- Ciclo automatico: 16 arquivo(s) (.:4, docs:1, examples:1, src:7, tests:3).
- Gate: ok (10s).

- `AGENTS.md`
- `Cargo.lock`
- `Cargo.toml`
- `README.md`
- `examples/f2_f3_live.rs`
- `src/agent/coherence.rs`
- `src/agent/experts.rs`
- `src/agent/mod.rs`
- `src/agent/router.rs`
- `src/api/mod.rs`
- `tests/agent_gate.rs`
- `docs/AETHER_AGENT.md`

## [0.0.143] - 2026-11-17

### Changed

- Ciclo automatico: 8 arquivo(s) (.:2, examples:1, src:4, tests:1).
- Gate: ok (8s).

- `Cargo.lock`
- `Cargo.toml`
- `src/agent/coherence.rs`
- `src/agent/experts.rs`
- `src/agent/router.rs`
- `src/lib.rs`
- `examples/f2_f3_live.rs`
- `tests/agent_gate.rs`

## [0.0.142] - 2026-11-17

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, src:1, tests:1).
- Gate: ok (7s).

- `Cargo.lock`
- `tests/f23_adversarial_gate.rs`
- `src/agent/`

## [0.0.141] - 2026-11-17

### Changed

- Ciclo automatico: 2 arquivo(s) (.:1, tests:1).
- Gate: ok (7s).

- `Cargo.lock`
- `tests/f23_adversarial_gate.rs`

## [0.0.140] - 2026-11-17

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, docs:1, src:1).
- Gate: ok (6s).

- `Cargo.lock`
- `docs/E2_OPTIMIZATION.md`
- `src/core/logic_pipeline.rs`

## [0.0.139] - 2026-11-17

### Changed

- Ciclo automatico: 3 arquivo(s) (.:1, examples:1, src:1).
- Gate: ok (10s).

- `Cargo.lock`
- `examples/e2_pipeline_digest.rs`
- `src/core/logic_pipeline.rs`

## [0.0.138] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, docs:1, examples:1, src:1).
- Gate: ok (8s).

- `Cargo.lock`
- `docs/E2_OPTIMIZATION.md`
- `examples/e2_symbolic_bench.rs`
- `src/core/logic_pipeline.rs`

## [0.0.137] - 2026-11-16

### Changed

- Ciclo automatico: 9 arquivo(s) (.:3, docs:1, examples:1, src:3, tests:1).
- Gate: ok (7s).

- `Cargo.lock`
- `README.md`
- `docs/SGDB_INTEGRATION.md`
- `examples/e2_symbolic_bench.rs`
- `src/api/mod.rs`
- `src/core/logic_pipeline.rs`
- `src/knowledge/sgdb/cli.rs`
- `AGENTS.md`
- `tests/api_sgdb.rs`

## [0.0.136] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:2, src:2).
- Gate: ok (7s).

- `Cargo.lock`
- `Cargo.toml`
- `src/api/mod.rs`
- `src/bin/aether_api.rs`

## [0.0.135] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (docs:2, src:2).
- Gate: ok (7s).

- `docs/E2_PLAN.md`
- `docs/SGDB_INTEGRATION.md`
- `src/core/logic_pipeline.rs`
- `src/knowledge/sgdb/cli.rs`

## [0.0.134] - 2026-11-16

### Changed

- Ciclo automatico: 10 arquivo(s) (.:1, docs:1, examples:1, src:6, tests:1).
- Gate: ok (11s).

- `README.md`
- `docs/SGDB_INTEGRATION.md`
- `examples/e2_pipeline_digest.rs`
- `src/bin/aether_daemon.rs`
- `src/knowledge/sgdb/cli.rs`
- `src/knowledge/sgdb/client.rs`
- `src/knowledge/sgdb/ledger.rs`
- `src/knowledge/sgdb/mod.rs`
- `src/knowledge/sgdb/transport.rs`
- `tests/f5_sgdb_cli.rs`

## [0.0.133] - 2026-11-16

### Changed

- Ciclo automatico: 8 arquivo(s) (.:1, src:6, tests:1).
- Gate: ok (7s).

- `Cargo.lock`
- `src/core/logic_pipeline.rs`
- `src/knowledge/sgdb/client.rs`
- `src/knowledge/sgdb/ledger.rs`
- `src/knowledge/sgdb/mod.rs`
- `src/knowledge/sgdb/transport.rs`
- `src/knowledge/sgdb/worker.rs`
- `tests/f4_sgdb_gate.rs`

## [0.0.132] - 2026-11-16

### Changed

- Ciclo automatico: 6 arquivo(s) (.:1, examples:1, src:4).
- Gate: ok (12s).

- `Cargo.lock`
- `src/core/logic_pipeline.rs`
- `src/knowledge/sgdb/client.rs`
- `src/knowledge/sgdb/mod.rs`
- `src/knowledge/sgdb/transport.rs`
- `examples/e2_pipeline_digest.rs`

## [0.0.131] - 2026-11-16

### Changed

- Ciclo automatico: 4 arquivo(s) (.:1, src:3).
- Gate: ok (7s).

- `Cargo.lock`
- `src/bin/aether_daemon.rs`
- `src/knowledge/sgdb/mod.rs`
- `src/knowledge/sgdb/transport.rs`

## [0.0.130] - 2026-11-16

### Changed

- Ciclo automatico: 24 arquivo(s) (.:2, docs:1, examples:3, src:12, tests:6).
- Gate: ok (20s).

- `Cargo.lock`
- `README.md`
- `examples/cram_logos_probe.rs`
- `examples/sgdb_roundtrip.rs`
- `examples/transplant_hv_probe.rs`
- `src/api/mod.rs`
- `src/bin/aether_api.rs`
- `src/bin/aether_daemon.rs`
- `src/bin/aether_dashboard.rs`
- `src/dashboard/mod.rs`
- `src/json.rs`
- `src/knowledge/sgdb/client.rs`

## [0.0.129] - 2026-11-16

### Changed

- Ciclo automatico: 8 arquivo(s) (.:2, examples:1, src:4, tests:1).
- Gate: ok (22s).

- `Cargo.lock`
- `Cargo.toml`
- `src/bin/aether_daemon.rs`
- `src/knowledge/sgdb/ledger.rs`
- `src/knowledge/sgdb/transport.rs`
- `src/knowledge/sgdb/worker.rs`
- `tests/f4_sgdb_gate.rs`
- `examples/sgdb_roundtrip.rs`

## [0.0.128] - 2026-11-16

### Changed

- Ciclo automatico: 8 arquivo(s) (.:2, src:5, tests:1).
- Gate: ok (3s).

- `Cargo.lock`
- `Cargo.toml`
- `src/knowledge/mod.rs`
- `src/knowledge/sgdb/ledger.rs`
- `src/knowledge/sgdb/transport.rs`
- `src/knowledge/sgdb/mod.rs`
- `src/knowledge/sgdb/worker.rs`
- `tests/f4_sgdb_gate.rs`

## [0.0.127] - 2026-11-16

### Changed

- Ciclo automatico: 3 arquivo(s) (src:3).
- Gate: ok (3s).

- `src/api/mod.rs`
- `src/lib.rs`
- `src/knowledge/sgdb/`

## [0.0.126] - 2026-11-16

### Added

- `RoofDualWorkspace::bound_into`: lower bound `C + ν` do roof dual, com
  `C = c0 + Σ min(0,l_i) − Σ|q_ij|`; `pass_into`/`solve_into` intactos.
- `tests/e2c3_bound_gate.rs`: gates analíticos `n=1`/`n=2` + 300 instâncias
  densas (`bound ≤ ótimo`).
- `examples/e2c3_probe.rs`: modos `--scale`, `--deep`, `--flow`; artefato
  `target/aether_e2c3_scale.json`.

### Changed

- `ExactSearch` virou B&B: poda por `const_fixo + bound_resíduo ≥ incumbente`,
  incumbente guloso + descida coordenada, ramificação na primeira livre e
  cascata para fixar.
- Q4: `n=48` com 52.790 nós (vs 222.270 sem bound); `n=64` fecha 3/3 em ~4,2M
  nós (~2 min/instância).
- `docs/AUDIT_CLAIMS_VOL3.md`: §8.7 (E2c-3C) e §9 (lições).

### Fixed

- `pass_into` devolve bits 0/1; o estado usa 0=livre/±1. A conversão evita
  recursão infinita ao aplicar fixações.

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

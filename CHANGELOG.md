# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
Versionamento: `0.0.x` (patch incrementado pelo ciclo automatico a cada conjunto de mudancas).

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

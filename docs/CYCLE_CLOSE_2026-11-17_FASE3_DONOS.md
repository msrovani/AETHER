# Ciclo 0.0.170 — P1.2/P1.3 + Fase 3 donos (2026-11-17)

Host: ROCKY. Forum: MSG 0440/0428 → 0454–0463.

## Entregas CURSOR

| Item | Veredito | Evidência |
|---|---|---|
| **P1.2 LSH ≥10k** | **act** | `HvBank::load` limiar inclusivo; B1 `lsh_tables=4`, recall@10=1.0, p99≈4 µs |
| **P1.3 D1 percentis** | **act** | 200 chamadas individuais; p50≈600 ns, p99≈700 ns; teto p99=5 µs; `div_ceil` |
| **B-G0 re-harvest** | **act** (facts) | `data/rounds/bg0_paws/` 5000 pares (2234/2766); overlap B1 4948/5000 |
| **B2 gate scaffold** | **act** (skip) | `tests/b2_100k_gate.rs` — LSH obrigatória, recall≥0,90; sem bank 100k |
| **FFT V4** | **abstain** (critério) | `docs/FFT_V4.md` — modelo 0459 aceito; merge só com razão ≥2× |
| **Embed 1k** | **abstain** | `scripts/embed_rate_1k.py` pronto; ST ausente nesta sessão |
| **MCTS experimento** | **cite** | OPENCODE (MSG 0461/79801e0); CURSOR não toca `mcts_measure_gate` |

## Divisão (MSG 0461 ACK 0463)

* OPENCODE: `tests/agent_mcts_experiment.rs`, gate B-G0 quando embed/JL landar, protótipo FFT V4 fora do repo.
* CURSOR: harvest B-G0 labels, gate B2, D1/LSH, decisão V4, taxa embed quando ST existir.

## Doutrina

*LLM gera; AETHER cita, certifica ou abstém.* Cap transplant 120 (`88a76a5`).

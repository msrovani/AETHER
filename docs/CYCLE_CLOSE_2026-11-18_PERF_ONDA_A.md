# Ciclo 0.0.171 — Onda A + runtime embed (2026-11-18)

**Forum:** 0470–0500 · Plano `docs/PLAN_2026-11-17_OTIMIZACAO_ARXIV.md` · Análise `docs/EMBED_THROUGHPUT_ANALYSIS.md`

## Aprendizado (memória operacional)

1. **bf16 no CPU é anti-otimização** — default do Qwen3-Embedding no ST; medido ~6–9× mais lento que fp32.
2. **Warmup obrigatório** na taxa A1 — 1º lote fria alocação e distorce ETA.
3. **Multi-probe LSH** (8 vizinhos Hamming-1) + shards **≤4096** (lei E1B 0,4×D, OPENCODE 0520). Piso recall 0,90 só dentro da capacidade; 100k = 25×4k.
4. **A5 poda por claims** no `WorldState` zera o delta MCTS na família dependencia (budget 2 = 8/8).
5. **FFT V4** fechado com ABSTAIN honesto (OPENCODE 0481): CPU rustfft mantém o laço 60 Hz.
6. **CIC (P8)** entregue OPENCODE: false_act=0 + Wilson IC; tetos 0,49/0,20 ainda heurísticos até troca pelos donos.

## Entregas CURSOR

| Item | Resultado |
|---|---|
| A0/L1 runtime | `tools/embed_runtime.py` — CPU fp32/B16 · GPU bf16/B64+FA2 · `--backend onnx` opt-in |
| A1 re-medida | **10,82 txt/s** · ETA 100k **2,57 h** (era 1,56 / 17,8 h) |
| A2 path | seed=42 · T=0 · CUDA ABSTAIN neste host |
| A4 | multiprobe + lei E1B + gate shards ≤4k (`b2_shards/4k`) |
| A5 | `WorldState.claims` + sync no `run_goal` |

## OPENCODE (em paralelo)

- B1 CIC landado · C1 FFT ABSTAIN · A3 gate+sidecar · B-G0 embed L1 em curso  
- Adotou runtime (0495)

## Anti

Destilar LLM · baixar teto PAWS · bf16 no CPU · claim alheio sem ACK.

## Addendum B2 / E1B (MSG 0520→0524)

- `e1b_atom_capacity` / `shard_plan_e1b` em `hv.rs` (4096 @ 10240 → 25 shards / 100k)
- LSH auto a partir da capacidade (não só ≥10k)
- Gate: piso 0,90 **só** dentro da capacidade; over-cap reporta
- Shard medido: `data/rounds/b2_shards/4k/` recall@10=1.000 · p99≈11 µs (release)

**Frase:** LLM gera; AETHER cita, certifica ou abstém.

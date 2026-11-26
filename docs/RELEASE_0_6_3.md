# Release 0.6.3 — A5 mede as duas direções e o gate gera o round rev

**Data:** 2026-11-26
**Âncora:** após 0.6.2 (B2 duas direções, A5 T-HIER, Sprints A/B/C).
**Natureza:** código + gate + evidência medida + docs. Fecha o limite honesto que
0.6.2 declarou: o lado reverso (0,9170) agora é medido pelo gate Rust.

## O que muda

| Tema | Item | Entrega |
|---|---|---|
| **A5 rev no gate** | `src/agent/hier_moe.rs`, `tests/agent_hier_moe_gate.rs` | `HierMoeStatus.fwd: Option<DirectionMeasure>` + `rev`; `measured(over_capacity, k, fwd)`; predicado GO único em `DirectionMeasure::lsh_ok`, com `both_directions_ok = lsh_ok() && rev.is_some_and(..)` |
| **Round rev** | `tools/a5_rev_round.py` | `emb/bank_rev.f32` → `hv/bank_rev.bin` com o `P` do manifest (gemv linha a linha), espelhado em `hv/bank.bin` para o `Round::load`, + autoverificação bit a bit do fwd; round em `target/a5_rev_round/` com `provenance.json` |
| **Gate sem skip** | `tools/cycle_gate.py` | check `A5_rev_round` antes de `agent_hier_moe_gate`: árvore limpa regenera o round e falha vermelho em vez de o A5 pular a direção reversa em silêncio |

## Evidência

- `data/aether_hier_moe_status.json` (+`.meta.json`): release, n=5000, config
  8×8×8 — **fwd** recall 0,7708 = 0,7708 do plano, p99 **335 µs < 462 µs**;
  **rev** recall **0,9170** = 0,9170 do plano, p99 **370 µs < 477 µs**;
  `lsh_ok=true`, `both_directions_ok=true`, `may_enable=false`,
  `default_path=false`.
- `data/cycle_gate.json`: **12/12** verde na árvore desta release, incluindo
  `A5_rev_round` e `cargo_test_agent_hier_moe_gate`.
- Repetição sob carga manteve a ordem nas duas direções (o p99 absoluto sobe, a
  razão LSH/plano não inverte).

## Compatibilidade

- **Nada no caminho default.** O A5 continua atrás de `--features hier_moe` +
  `AETHER_HIER_MOE=1`; ligar o default é decisão separada.
- Nenhuma dependência nova; `tools/a5_rev_round.py` usa o numpy do
  `.vega/venv` já existente.
- `src/knowledge/sgdb/` e `src/core/persistency_search.rs` intocados.
- Tetos mantidos: auth ≤ 120, A3 recall ≥ 0,90, F4 hit@5 ≥ 0,974.

## Limites honestos

- O GO compara LSH contra o **MoE plano no mesmo bank**, não contra um índice
  exato externo; o teto f32 do B2 (0,9170) é a referência do lado reverso.
- O round rev é regenerado a cada `cycle_gate` (custo Python) e vive em
  `target/` (gitignored); a evidência versionada é o status promovido a `data/`.
- `clippy -D warnings` segue com falha pré-existente em
  `src/agent/tool_agent.rs:455` (`needless_option_as_deref`).

# Release 0.6.2 — B2 (duas direções), A5 T-HIER e Sprints A/B/C

**Data:** 2026-11-25
**Âncora:** após 0.6.1 (A4 T-CLIP).
**Natureza:** código + gates + evidência medida + docs (fecha B2 e A5 de
`docs/PLANO_A1_D4.md`; Sprints A/B/C vêm de `docs/REGRAS_IMPL_SAT_2026-11-24.md`).

## O que muda

| Tema | Item | Entrega |
|---|---|---|
| **B2 pós-FAIL** | `tools/b2_oversample_adc.py` | oversample k≤1024 e ADC empatam o teto f32 em **0,755** (n=200): o gap não era rerank |
| **B2 duas direções** | `tools/b2_two_directions.py` | indexar `s2\|rel\|s1` leva o recall@1 de 0,7708 (fwd) a **0,9170** (rev, n=5000) — piso 0,90 cumprido |
| **A5 T-HIER** | `src/agent/hier_moe.rs` | LSH multi-probe + re-rank f32 top-32: **0,7680 vs 0,7680** de recall e **p99 174 µs vs 455 µs** (release, n=500, 8×10×16) |
| **Sprint A** | `src/agent/epistemic_budget.rs`, `src/core/aether_governor.rs`, `src/morphogenesis.rs`, `src/core/renormalization.rs`, `src/core/self_field.rs`, `src/bin/aether_daemon.rs` | fast-J por onda (S1), `steps_eff` elástico (S2), RG spine (S3), Dream → E2 commit (S4), `macro_signature()` G-ISO (S5), federação 2 nós (S6), rebase de autoridade (S7), probe budget G-ABD/A10 (S8) |
| **Sprint B** | `src/agent/linucb.rs`, `src/knowledge/sgdb/ledger.rs`, `src/morphogenesis.rs`, `src/core/self_field.rs` | SB-1 sequencial pinned, SB-1b lock do daemon, SB-2 `last_koopman_j`, SB-3 RG macro no LinUCB, SB-4 SINDy→Law, SB-5 bacias G-MORPH, SB-6 estabilidade J |
| **Sprint C** | `src/core/thermodynamic_solver.rs`, `src/core/wave_interference.rs`, `src/bin/aether_daemon.rs`, `src/knowledge/sgdb/worker.rs` | SC-1 dedup de energia, SC-2 reuso do espectro na entropia, SC-3 zero-alloc confirmado, SC-4 `busy_work` no boot + outbox em batches |

Gates novos: `agent_hier_moe_gate` (A5), `governor_rg_spine_gate`,
`governor_e2_commit_gate`, `macro_signature_gate`, `morpho_gating_gate`,
`daemon_federation_gate`, `bus_epistemic_probe_gate`, `governor_koopman_j_gate`,
`linucb_rg_signature_gate`, `g_law_distill_gate`, `g_morph_basin_gate`,
`self_field_introspection_gate`.

## Evidência

- `data/b2_oversample_adc.json` (+`.meta.json`): `verdict=FAIL`, 0,755.
- `data/b2_two_directions.json` (+`.meta.json`): fwd 0,7708 · rev **0,9170** ·
  teto f32 0,9170; banco reverso em
  `data/rounds/20261117_b0_paws/emb/bank_rev.f32` (sha256 `11773c9c…`).
- `data/aether_hier_moe_status.json` (+`.meta.json`): `lsh_ok=true`.
- `data/cycle_gate.json`: 11/11 verde na árvore desta release.

## Compatibilidade

- **Nada no caminho default.** O A5 continua atrás de `--features hier_moe` +
  `AETHER_HIER_MOE=1`; `default_path:false` e `lsh_ok` no artefato.
- Nenhuma dependência nova; `cargo check --all-targets --offline` limpo.
- `src/knowledge/sgdb/` e `src/core/persistency_search.rs` intocados
  (as mudanças do Sprint B/C em `ledger.rs` e `worker.rs` são aditivas).
- Tetos mantidos: auth ≤ 120, A3 recall ≥ 0,90, F4 hit@5 ≥ 0,974.

## Limites honestos

- O A5 mediu o bank **fwd** (0,7680), que é justamente o lado abaixo de 0,90 no
  B2. O lado que fecha B2 (reverso, 0,9170) ainda não tem `bank_rev.bin` no
  formato consumido pelo gate Rust — ligar o A5 no default continua proibido sem
  essa medição.
- Os Sprints A/B/C entram com gates de teste (vermelho→verde), não com medição
  de campo; os números de performance medidos nesta release são os de B2 e A5.
- `clippy -D warnings` segue com falha pré-existente em
  `src/agent/tool_agent.rs:455` (`needless_option_as_deref`).

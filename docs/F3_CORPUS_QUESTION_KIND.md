# F3 — Corpus 500/tipo + `question_kind`

Data: 2026-11-20 · CLAIM: **MSG 0795** · Deliberação: **MSG 0790** (CURSOR ativo) ·
Plano: `docs/VEGA_F3_PLANO_CONSOLIDADO.md`.

## Escopo

| Entrega | Path |
|---|---|
| Grower | `tools/vega_corpus_grow.py` |
| Corpus F3 | `data/vega_flywheel_v1.json` (ou `--out`) |
| Starter (base) | `data/vega_calibration_starter.json` (124) |

## Política de autoridade

| Proveniência | Authority |
|---|---|
| starter | 120 |
| forum_mine | 100 |
| augment (paráfrase) | **60** (quarentena) |
| HITL sanciona subconjunto | **255** |

Nunca promover augment a 255 sem HITL (MSG 0790).

## `question_kind`

`factual` | `irreducible_open` | `procedural` | `conflict_arb` | `ood`

Alinha ao Eixo F + PEDIDO_CURSOR MSG 0793/0794: ambiguidade ≠ `unknown` para
pesquisa; `irreducible_open` permanece open 60.

## Validar

```bash
python tools/vega_corpus_grow.py inventory
python tools/vega_corpus_grow.py selftest
python tools/vega_corpus_grow.py grow --target 500 --out data/vega_flywheel_v1.json
```

## Fora deste claim

- Fit CV do head `passou` no vegaml (env F1 / OPENCODE+BUFFY)
- Editar `aether-decision` / `cli.rs` / F4 embed

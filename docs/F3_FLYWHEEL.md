# F3 — Flywheel de corpus e `question_kind`

Data: 2026-11-20 · Dono: **CURSOR** (MSG 0790 / claim 0795) · Política:
rótulo automático em quarentena 60–120; **HITL** sanciona subconjunto a 255.

## Meta

| Cabeça | Starter | Alvo |
|---|---|---|
| `passou` (boolean) | 68 | 500 |
| `winner` (choice) | 25 | 500 |
| `route` (choice) | 31 | 500 |

Mais rótulo transversal `question_kind`:
`factual | irreducible_open | procedural | conflict_arb | ood`.

## Ferramenta

```bash
python tools/vega_corpus_grow.py inventory
python tools/vega_corpus_grow.py grow --target 500 --out data/vega_flywheel_v1.json
python tools/vega_corpus_grow.py selftest
```

## Proveniência / autoridade

| provenance | authority | nota |
|---|---|---|
| `starter` | 120 | corpus humano (`vega_calibration_starter.json`) |
| `forum_mine` | 100 | linhas do FORUM com padrões de gate |
| `augment` | 60 | paráfrase determinística (seed=42) — nunca 255 |

## Aceite F3 (parcial desta entrega)

- Contagens ≥500 por cabeça no flywheel
- Todo exemplo com `question_kind`
- Nenhum authority >120 / nenhum 255
- Starter **não** sobrescrito (arquivo novo)

Aceite pleno do plano (head `passou` vence engine em CV) depende do fit
vega — fora desta fatia de dados; OPENCODE/BUFFY no F1.

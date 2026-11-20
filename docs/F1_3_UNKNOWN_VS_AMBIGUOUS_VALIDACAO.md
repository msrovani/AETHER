# F1_3 — validação unknown vs ambiguous (pós-BUFFY 0909)

Data: 2026-11-20 · CURSOR (MSG 0915) · Rede: false

## Pedido

BUFFY MSG 0909: margem fina agora é `ambiguous`; o gate só abre pesquisa em
`unknown` (lacuna/unbound) — confirmar que o harness CURSOR continua correto.

## Provas (todas EXIT 0)

| Tool | Resultado |
|---|---|
| `tools/mf2_gate_regression.py` | ok; smoke `passou` → ambiguous; só unknown abre F2 |
| `tools/research_harness.py selftest` | ok; n_checks=14; mf2=PASS |
| `tools/f2_ttt_conformal.py` | ok; head TTT sem conjunto ≠ unknown |
| `tools/f1_abstain_map.py selftest` | ok; unbound→unknown; set>1→ambiguous |
| `tools/f1_gate_promote.py selftest` | ok; `passou_not_unknown=true` |

## Contrato intacto

- `ambiguous` / margem fina → HITL, **não** abre F2
- `unknown` só com unbound → abre F2
- head TTT (`cv_accuracy` sem `conformal_set`) **não** promove a unknown

Autoridade agente: 120.

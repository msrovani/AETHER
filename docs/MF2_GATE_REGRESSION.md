# MF2 — regressão harness × vereditos F1-3

Referência: MSG 0802 / 0806 / 0808. Posse CURSOR. Não edita `aether_decision`,
`cli.rs`, SGDB nem F4.

## Contrato

| Condição | `abstain_reason` | Pesquisa F2 |
|---|---|---|
| `unbound=true` | `unknown` | abre (única porta) |
| `prediction_set.len()>1` (sem unbound) | `ambiguous` | **não** |
| abstain sem classe / margem fina | `ambiguous` | **não** |
| `conflict` / `engine_unavailable` | — | harness recusa (exit 2) |

## Como rodar

```powershell
python tools/mf2_gate_regression.py
python tools/mf2_gate_regression.py --smoke target/f1_decision_smoke.json
python tools/research_harness.py selftest   # inclui MF2
python tools/f5_t0_eval.py                  # T0 + MF2
```

Artefato de gate esperado: `target/f1_decision_smoke.json` com
`passou.abstain_reason=ambiguous` e `conformal_set_present=true`.

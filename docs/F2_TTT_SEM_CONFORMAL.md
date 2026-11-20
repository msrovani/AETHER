# F2 — head TTT não conta com conjunto conformal

Data: 2026-11-20 · Claim: **MSG 0859** · Pedido: **MSG 0853** (BUFFY) · Gestor: OPENCODE.

O head TTT responde com `probs`, `confidence`, `cv_accuracy`, `choice` e
`readout`. Não traz `abstain`, `unbound` nem `prediction_set`. Ausência de
`conformal_set` nesse caminho é o contrato, não lacuna de conhecimento.

## Regra

| Sinal | Pesquisa F2 |
|---|---|
| `abstain_reason=unknown` e não é head sem conjunto | abre (única porta) |
| `cv_accuracy` presente e `conformal_set` ausente | **não abre**, mesmo se alguém gravar `unknown` |
| `ambiguous` / margem fina | não abre (HITL) |

O harness continua recusando tudo que não é `unknown`. Este módulo impede o
atalho “sem conjunto → unknown” no head.

```powershell
python tools/f2_ttt_conformal.py selftest
```

Não edita `aether-decision/`, `src/agent/cli.rs`, SGDB nem F4. A MSG 0858
cobre o harness; este arquivo é o verificador separado.

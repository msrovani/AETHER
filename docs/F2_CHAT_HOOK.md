# F2 — gancho do chat (MSG 1022)

O fixture `tools/f2_wire_port.py` (MSG 1017) prova o cabo offline. O chat só
chama pesquisa quando o `DecisionPort` imprime `motivo=unknown` num turno
fora do léxico.

O stub default abstem com `engine_unavailable` e **não** abre pesquisa.
Texto sem caso da régua grava `sem_caso` com autoridade 60. Não cai em
`terra_lua`.

```powershell
python tools/f2_chat_hook.py selftest
```

Não edita `aether-decision/`, o dataset do F4 nem a tranca de GPU.

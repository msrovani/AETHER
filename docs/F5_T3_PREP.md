# F5 T3 PREP — hit@k sem rede

CLAIM: **MSG 0869** · Gestor: **MSG 0866** · Freeze: **MSG 0862**.

## Escopo

Preparação **offline** da suite T3:

1. Validar freeze `data/embed_eval_set.jsonl` (722 docs / 142 queries)
2. Medir **hit@5 lexical (BM25)** como piso
3. Gate MiniLM (`hit@5 ≥ 0.974` e ≥ lexical) fica com **F4 / AGENT_A**

Rede default **OFF**. Não baixa modelo.

## Artefatos

| Path | Papel |
|---|---|
| `data/f5_t3_contract.json` | contrato k=5 + SHA gestor |
| `tools/f5_t3_eval.py` | selftest + run |
| `target/f5_t3_eval.json` | relatório |

```powershell
python tools/f5_t3_eval.py selftest
python tools/f5_t3_eval.py run
```

## Também no quadro 0866

`FIX_FATIA3` snake_case: `chat_consulta_o_port_em_todo_turno` (já aplicado).

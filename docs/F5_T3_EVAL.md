# F5 T3 — prep hit@k (offline)

Data: 2026-11-20 · CLAIM: **MSG 0871** · Deliberação gestor: **MSG 0866**.

## Escopo (PREP)

| Item | Valor |
|---|---|
| Tool | `tools/f5_t3_eval.py` |
| Rede | **OFF** (sem HTTP, sem download) |
| MiniLM | **não** — run completo é AGENT-A / F4 |
| Gate | `hit5 ≥ 0.974` e ≥ `lexical_bm25` em `embed_minilm` + `hybrid_rrf60` |

Prep valida a **matemática do contrato** e rankings materializados. Full pass
só depois do artefato MiniLM no dataset congelado (`data/embed_eval_set.jsonl`,
hash MSG 0862).

## Uso

```bash
python tools/f5_t3_eval.py selftest
python tools/f5_t3_eval.py prepare
python tools/f5_t3_eval.py eval data/f5_t3_rankings_agent_a.json
```

`prepare` confere freeze MSG 0862 (722/142, sha `f868…`) sem inventar hit@k.
`eval` (MSG 0951/0957, aditivo): aplica gate hit@5 ≥ 0.974 e ≥ lexical sobre
rankings materializados; recusa SHA ≠ freeze; sem rede. Selftest cobre o
caminho com rankings sintéticos.
Contrato: `data/f5_t3_contract.json`.

## Relação com F4

1. AGENT-A congela dataset + roda MiniLM (`docs/F4_EMBED_EVAL.md`).
2. Rankings por sistema entram num JSON no schema do fixture.
3. `f5_t3_eval.py eval` aplica o gate sem rede.

Baseline lexical pode vir de `tools/embed_eval.py --bm25-only` (outro CLAIM).

## Fora de escopo

- `aether-decision/`, `src/agent/cli.rs`, download HF, F4 medição ao vivo.

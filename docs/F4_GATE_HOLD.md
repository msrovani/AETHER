# F4 — trava executável do gate

Data: 2026-11-20 · Claim: **MSG 0953** · Decisão: **MSG 0947** (opção B, autoridade 255).

O teto continua `hit@5 ≥ 0.974`. O híbrido medido `0.7465` supera o BM25
`0.7042` e fica abaixo do teto. Esta geração está **reprovada**. O dataset
`data/embed_eval_set.jsonl` não é reescrito.

```powershell
python tools/f4_gate_hold.py selftest
```

O hash conferido é o do freeze (MSG 0862/0875), 64 hex. Não edita o crate,
`cli.rs`, o eval T0 nem o exemplo de smoke do gestor.

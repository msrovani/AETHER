# F5 T1 — unknown → quarentena no laço agent (offline)

Data: 2026-11-20 · Dono: **CURSOR** (MSG 0803 / 0812) · **Não edita**
`src/agent/cli.rs` (posse OPENCODE): o laço é um harness que espelha a política
pós-`DecisionPort`.

## Contrato

```
DecisionPort tipado
  unknown     → research_harness (rede OFF) → JSONL quarentena authority≤120
  ambiguous   → escalate HITL (sem pesquisa)
  conflict    → escalate HITL
  age         → sem pesquisa
```

Alinhado a F1_3 / MF2: só `unknown` (unbound) abre pesquisa.

## Validar

```bash
python tools/f5_t1_agent_loop.py selftest
```

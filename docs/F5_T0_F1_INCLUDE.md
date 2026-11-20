# F5 T0 — inclusão dos gates F1 offline (MSG 0948)

Data: 2026-11-20 · Dono: **CURSOR** · Gestor: OPENCODE  
Escopo: `tools/f5_t0_eval.py` · **Não toca** crate / F4 dataset

## O quê

O tier T0 passa a rodar, além de F2/F2.5/T1–T3:

| check | tool |
|---|---|
| `f1_offline_suite` | A2 / A2c / A3 / A4 / F1_5 / A5 |
| `vega_path_audit` | contrato `.vega/` (MSG 0892) |
| `qa_suite_probe` | classificador flake vs real (MSG 0879) |

## Como

```bash
python tools/f5_t0_eval.py
```

Rede = false. Wire do crate continua com BUFFY/OPENCODE.

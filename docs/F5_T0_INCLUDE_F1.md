# F5 T0 — inclui suite F1 offline + A5 (MSG 0948 / 0978)

`python tools/f5_t0_eval.py` roda também:

- `tools/f1_offline_suite.py selftest` (A2/A2c/A3/A4/F1_5/A5 promote)
- `tools/qa_suite_probe.py selftest` (classificador flake)
- `tools/a5_evidencia_audit.py selftest` (MSG 0978 / deliberação 0967)

Rede: sempre off. F4 desta geração = **REPROVADO** (MSG 0947 opção B);
T3 full continua na fila até nova sanção 255.

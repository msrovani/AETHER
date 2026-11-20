# VEGA_PATH_AUDIT — paths do motor (MSG 0892)

Data: 2026-11-20 · Dono: **CURSOR** · Gestor: OPENCODE  
Pedido: *usar os novos paths e nunca criar venv dentro de `target/`*.

## Contrato

| papel | path |
|---|---|
| venv | `.vega/venv` |
| models | `.vega/models` |
| lock | `data/vega_env.lock.json` |
| python | `.vega/venv/Scripts/python.exe` |

## Auditoria CURSOR (`tools/` + `scripts/`)

```bash
python tools/vega_path_audit.py selftest
python tools/vega_path_audit.py
# → target/vega_path_audit.json
```

- Defaults de `tools/vega_attach.py`, `vega_probe.py`, `vega_fit_probe.py`,
  `vega_calibrate.py` e `scripts/f1_contract_probe.py` apontam para `.vega/`.
- `tools/vega_env_setup.ps1` ainda *lê* `target/vega` como **fonte de migração**
  (permitido) — destino é `.vega/`.

## Resíduos fora do escopo CURSOR (não editados)

Crate `aether-decision` e exemplos ainda citam `target/vega` em comentários /
defaults legados (`config.rs`, `pyo3_impl.rs`, `examples/f1_smoke.rs`, …).
Dono: OPENCODE / posse do crate. CURSOR não toca.

`docs/F1_CONTRATO_MEDIDO.md` — claim DOC_SYNC de outra sessão CURSOR (MSG 0902).

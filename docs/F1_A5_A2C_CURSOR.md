# F1 A-5 + A-2c — artefato durável e lock de GPU (CURSOR)

Data: 2026-11-20 · Dono: **CURSOR** · Ref: `docs/F1_AUDIT.md` · Fórum claim
MSG 0851 · **Não toca** `aether-decision/src` nem `cli.rs` (fatia 3 alheia).

## A-5 — smoke fora de `target/`

```bash
python tools/f1_gate_promote.py --src target/f1_decision_smoke.json
# → data/f1_decision_smoke.json + data/f1_decision_smoke.meta.json (sha256)
python tools/f1_gate_promote.py selftest
```

Recusa destino dentro de `target/`. Checks: `passou` ≠ `unknown`, conformal presente.

## A-2c — lock de engine GPU

**Canónico (MSG 1014):** `.vega/engine.lock` — ver `docs/A2C_ENGINE_LOCK.md`.

```bash
python tools/engine_lock.py acquire CURSOR 1800
python tools/engine_lock.py status
python tools/engine_lock.py release CURSOR
python tools/engine_lock.py selftest
```

Legado (target/): `tools/gpu_engine_lock.py` — não usar para corridas novas.

Protocolo fórum: antes de carregar VegaPort na GPU, `acquire`; ao terminar,
`release` + MSG STATUS se lease longo.

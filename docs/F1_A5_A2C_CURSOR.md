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

```bash
python tools/gpu_engine_lock.py acquire --holder CURSOR --lease-s 1800
python tools/gpu_engine_lock.py status
python tools/gpu_engine_lock.py release --holder CURSOR
python tools/gpu_engine_lock.py selftest
```

Segundo holder recebe `gpu_busy` → ação sugerida `abstain_engine_unavailable`.
Lock file default: `target/gpu_engine.lock.json` (efêmero) ou `AETHER_GPU_LOCK`.

Protocolo fórum: antes de carregar VegaPort na GPU, `acquire`; ao terminar,
`release` + MSG STATUS se lease longo.

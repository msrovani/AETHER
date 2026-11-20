# A-2c — auditoria de paths da tranca (MSG 1026)

Data: 2026-11-20 · CURSOR · Ref: MSG **1014** PEDIDO_A_2C · Gestor: OPENCODE  
Escopo: scan offline · **não** rouba claim 1020 (suite) · **não** toca GPU/engine

## Canónico

| Item | Path |
|---|---|
| Tool | `tools/engine_lock.py` |
| Lock file | **`.vega/engine.lock`** (fora de `target/`) |
| Legado | `tools/gpu_engine_lock.py` → `target/gpu_engine.lock.json` |

## Como validar

```bash
python tools/a2c_lock_audit.py
python tools/a2c_lock_audit.py selftest
python tools/engine_lock.py status
```

Se `status.locked=true` com holder alheio (ex.: BUFFY), o `engine_lock.py selftest`
**deve** falhar no acquire — isso prova a tranca. O audit marca
`selftest_blocked_by_holder=true` e ainda assim passa no path canónico.

## Resíduos esperados

Referências a `gpu_engine_lock` / `target/gpu_engine.lock` em docs/tools antigos
ficam listadas em `data/a2c_lock_audit.json` (`legacy_hits`). Migração = preferir
`engine_lock.py`; não apagar o legado sem ACK do dono.

## Fora de escopo

- `acquire` na GPU enquanto BUFFY/OPENCODE segura a tranca
- Editar `cli.rs` / `aether-decision` / dataset F4

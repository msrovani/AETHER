# A-2c — selftest isolado (MSG 1028 / 1014)

`python tools/engine_lock.py selftest` usa `.vega/engine.lock.selftest`
(via `AETHER_ENGINE_LOCK`), **sem** disputar `.vega/engine.lock` de produção.

Assim a suite offline passa mesmo com BUFFY/OPENCODE segurando a GPU.

```bash
python tools/engine_lock.py selftest   # EXIT 0 com prod_locked=true ok
python tools/f1_offline_suite.py selftest
```

Produção continua em `.vega/engine.lock`. Rede: off. Auth: 120.

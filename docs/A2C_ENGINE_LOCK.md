# A-2c — tranca de GPU canónica (MSG 1014)

Data: 2026-11-20 · Claim CURSOR: **MSG 1020** · Gestor: OPENCODE  
Ferramenta: `tools/engine_lock.py` · Path: **`.vega/engine.lock`** (fora de `target/`)

## Por quê

Pico isolado ~4393 MiB (2 readers). Dois engines na mesma 3050 → OOM.
`cargo clean` apaga `target/` — lock em `target/` morre no meio da corrida.

## Uso

```bash
python tools/engine_lock.py status
python tools/engine_lock.py acquire CURSOR 1800
python tools/engine_lock.py wait CURSOR 1800
python tools/engine_lock.py release CURSOR
python tools/engine_lock.py selftest
```

Convenção: **antes** de carregar o engine (chat `--vega`, f1_smoke, F1_PROD),
`acquire`. Com tranca ocupada: espere ou faça outra coisa — nunca corrija junto.

## Adoção CURSOR

- `tools/f1_offline_suite.py` roda `A2c_engine_lock` (não mais só `gpu_engine_lock` em target).
- `tools/f2_wire_port.py` não toca GPU; documenta respeito à tranca.
- Legado: `tools/gpu_engine_lock.py` permanece para compat; canónico = este.

## Fora de escopo

Editar crate / ligar watchdog no port (BUFFY A-2/A-3/A-4).

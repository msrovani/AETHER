# A5 P1 — promote gate reports (MSG 0990)

Fatia **distinta** do P0 (MSG 0986: embed/bm25/mf2).

```bash
python tools/a5_promote_p1.py
python tools/a5_promote_p1.py selftest
```

| Origem | Destino |
|---|---|
| `target/f1_vram_watchdog.json` | `data/f1_vram_watchdog.json` + `.meta.json` |
| `target/f1_gil_gate.json` | `data/f1_gil_gate.json` + `.meta.json` |
| `target/qa_suite_probe.json` | `data/qa_suite_probe.json` + `.meta.json` |
| `f4_gate_hold.py selftest` | `data/f4_gate_hold.json` + `.meta.json` |

Não move dataset F4, crate, nem artefatos do P0. Rede: off. Auth: 120.

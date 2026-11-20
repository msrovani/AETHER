# F5 T2 — allowlist de rede (stub)

Data: 2026-11-20 · CLAIM: **MSG 0823** · Deliberação: **MSG 0803 / 0806**.

## Escopo deste stub

Preparação **offline** da política de rede para `research.query`:

| Regra | Valor |
|---|---|
| Default | **deny** |
| `allow_network_default` | `false` |
| Hosts | lista vazia até HITL sancionar |
| HTTP | **não implementado** neste stub |

T2 *live* (fetch real) fica na fila até allowlist + HITL + implementação.

## Artefatos

| Path | Papel |
|---|---|
| `data/research_network_allowlist.json` | política deny-default |
| `tools/f5_t2_allowlist.py` | `check` + `selftest` |
| `target/f5_t2_allowlist.json` | relatório do selftest |

```bash
python tools/f5_t2_allowlist.py selftest
python tools/f5_t2_allowlist.py check --host example.com
```

## Fluxo

```
allow_network=false → offline fixtures (T1)
allow_network=true + HITL!=SIM → deny
allow_network=true + host∉lista → deny
allow_network=true + host∈lista → stub: ainda sem HTTP (t2_live_fetch_not_implemented)
```

## Fora de escopo

- Ligar rede / baixar páginas
- Editar `cli.rs`, `aether-decision`, SGDB, F4

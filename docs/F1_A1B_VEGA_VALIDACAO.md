# F1 A-1b — validação readonly `--vega` (CURSOR)

Data: 2026-11-20 · Claim: MSG **0935** · Gestor: OPENCODE  
Escopo: **leitura** de `src/agent/cli.rs` + doc · **sem patch** no crate

## Veredito

**PASS** — contrato A-1b presente no produto (`open_chat_port` + flag `--vega`).

| sinal | evidência |
|---|---|
| help `--vega` | `cli.rs` ~L80 |
| parse argv | `"--vega" => args.vega = true` ~L188 |
| default StubPort | `open_chat_port(false)` → StubPort + msg Eixo C ~L325–330 |
| opt-in VegaPort | `VegaPort::new` sob feature `decision-pyo3` ~L335–342 |
| degradacao | falha → StubPort + aviso `vega_indisponivel` ~L350–352 |
| doc | `docs/F1_A1B_VEGA_OPTIN.md` |

## Comando

```bash
python tools/f1_a1b_vega_check.py selftest
python tools/f1_a1b_vega_check.py check
```

## Fora de escopo

- Editar `cli.rs` / `aether-decision`
- Rodar smoke GPU / download
- F4 / HITL

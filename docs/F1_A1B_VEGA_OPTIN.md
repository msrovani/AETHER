# F1 A-1b — VegaPort opt-in no chat

Data: 2026-11-20 · CURSOR (MSG 0874 / 0877) · Escopo: `src/agent/cli.rs`

## Problema

Após a fatia 3 o port fala em **todo** turno, mas o produto ainda faz
`StubPort` literal — o AND gate / head `route` reais só rodavam no example.

## Contrato

| Modo | Comportamento |
|---|---|
| default | `StubPort` + linha `decisor: port=stub (default Eixo C…)` |
| `--vega` + build `decision-pyo3` + env ok | `VegaPort` (`EngineConfig::from_env`) |
| `--vega` sem feature / falha de load | `StubPort` + aviso explícito (não inventa) |

Eixo C intacto: **nenhuma** chamada externa no default.

## Validar

```powershell
cargo test --features agent --test f1_contract
# opt-in real (host com venv):
cargo run --release --features agent,decision-pyo3 --bin aether-agent -- chat --vega "qual e a capital do Brasil"
```

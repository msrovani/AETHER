# Release 0.5.0 — core × tomadas (G-A4)

**Data:** 2026-11-21  
**Âncora:** programa de 7 sprints (ajuste do core após escadas de produto).

## O que esta versão é

Marco em que o **agent age no mundo com disjuntor verde** (G-A4) e as tomadas
próximas ficam inventariadas e testadas — sem tokenizer/pixels/PCM no T0.

| Sprint | Entrega | Gate |
|---|---|---|
| 1 | fs / http.get / mcp.call / shell deny | `agent_safety_gate` 9/9 |
| 2 | pipeline mundo + risco | plan/tools gates verdes |
| 3 | T-HITL (`hitl.rs`) | unit 3/3 |
| 4 | P1 template com cite | language + C1 gates |
| 5 | pack `moe_etl_pilot` | ask in-domain act / OOD abstain |
| 6 | T-EMB formal (off) | unit embed; F4 congelada |
| 7 | T-DEC no hub | `dec_view` + inventário |

## Non-goals mantidos

- Sem shell default, sem POST HTTP, sem cliente MCP genérico.
- Sem reabrir F4 até HITL sancionar novo gate (≥0.974).
- Sem STT/TTS/VIS no default.

## Como verificar

```bash
cargo test --features agent --test agent_safety_gate
cargo test --features agent --test agent_plan_gate --test agent_tools_gate
cargo run --bin aether-agent -- ask --facts data/packs/moe_etl_pilot/facts.jsonl --subject Vec --relation metodo --json
```

## Docs

[`CORE_TOMADAS.md`](CORE_TOMADAS.md) · [`TOMADAS_FUTURO.md`](TOMADAS_FUTURO.md) ·
[`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) · [`CHANGELOG.md`](../CHANGELOG.md)

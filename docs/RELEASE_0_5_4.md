# Release 0.5.4 — Barramento trio (NL-IN · HITL · NL-OUT)

**Data:** 2026-11-21  
**Âncora:** após 0.5.3 (fila stubs); productiza o **fio comum** no Console/API/hub.

## O que esta versão é

Economia + infraestrutura única: **um fio, um disjuntor, filtros comuns, um DB
(n-sgdb)** para todas as tomadas. O chat deixa o ledger de fase lossy e passa a
usar `run_trio_turn` (parse → solve → `CertifiedUtterance` → learn ≤120 → HITL).

| Camada | Entrega |
|---|---|
| Fio | `bus.rs` — Goal/Claim/Action/Hitl/CertifiedUtterance |
| Disjuntor | banda CORE (OOV, risk, auth, kill) — para todos |
| Filtros | A7 + cap 120 + P2 + T-DEC AND opt-in (`t_dec` no `x_aether`) |
| DB | n-sgdb: learn ≤120; promote 255 só hub HITL |
| HITL | outbox `aether_hitl_queue.jsonl` → `hitl_view` vivo |
| Chat | API `/v1/chat/completions` no fio trio; abstain = “não sei” |

## Non-goals mantidos

- Sem tokenizer/LLM gerador; gemma/Vega não geram prosa.
- OCR/TAB/PRED/EVAL/P2P/VIS/speech motores — depois, no **mesmo** barramento.
- A8 `enter_v1_default=false`; Hier MoE off.
- 0 auto-255.

## Como verificar

```bash
cargo test --features agent --test agent_trio_bus_gate
cargo test --features agent --test agent_learn_hitl_gate
cargo test --features agent --lib bus::
cargo check --features api,agent
cargo check --features dashboard,agent --bin aether-dashboard
```

Artefatos: `target/aether_nl_out_p2.json`, `target/aether_learn_hitl.json`,
outbox `target/aether_hitl_queue.jsonl`.

## Memória

- Trio bus: `md/L3/mcp/1795298354555000` (auth 255)
- Release note espelhada no SGDB sob `aether/fact`

## Docs

[`CORE_TOMADAS.md`](CORE_TOMADAS.md) §2.5 · [`HUB_DASHBOARD.md`](HUB_DASHBOARD.md) ·
[`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) · [`CHANGELOG.md`](../CHANGELOG.md)

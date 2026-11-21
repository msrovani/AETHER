# Release 0.5.3 — Fila T-NL-IN (F0–F8)

**Data:** 2026-11-21  
**Âncora:** após 0.5.2 (A6–A8 fio); fecha a fila sequencial “pronto para T-NL-IN”.

## O que esta versão é

Marco em que o **funil de intenção externa** fica estável: bordas só entram por
**T-NL-IN**; hub HITL cobre High; prosa P2 certifica; modalidades e Hier MoE
ficam **stubs/opt-in** sem tocar o T0.

| Fase | Entrega | Gate / artefato |
|---|---|---|
| F0 | Contrato `LanguagePort::parse` / `GoalSchema` | `agent_tnl_in_gate` → `aether_tnl_in_contract.json` |
| F1 | Hub `hitl_view` (Approve/Deny/Ampliar + token + kill) | unit `hitl_view` |
| F2 | T-NL-OUT P2 + `CertifiedUtterance` | `agent_nl_out_p2_gate` |
| F3–5 | OCR / STT / VIS / TTS stubs → T-NL-IN | `agent_modalidades_gate` |
| F4 | `EmbedPort::from_env` (`AETHER_EMBED=1`); F4 congelada | `agent_emb_optin_gate` |
| F6 | `PredictExpert` no MoE + `Task::Predict` + intervalo | `agent_tpred_moe_gate` |
| F7 | Critérios A8 `enter_v1` | `agent_a8_v1_gate` → `aether_a8_v1_criteria.json` |
| F8 | Hier MoE/LSH posição (off; não bloqueia NL-IN) | `agent_hier_moe_gate` |

Também embute o fechamento §6 de aceite (`agent_accept_s6_gate`: act+trace,
research persist ≤120) que acompanhou o fio A8.

## Non-goals mantidos

- Tokenizer / LLM / PCM / pixels no T0.
- A8 **`enter_v1_default=false`** até sanção HITL + `AETHER_A8_V1=1`.
- Hierarchical MoE / LSH **fora do default** (pós T-EMB + B1).
- Features `ocr` / `speech` / `vision` / `hier_moe` **off**.
- F4 congelada; auth externas ≤120; 255 só HITL.

## Como verificar

```bash
cargo test --features agent --test agent_tnl_in_gate
cargo test --features agent --test agent_nl_out_p2_gate
cargo test --features agent --test agent_modalidades_gate
cargo test --features agent --test agent_emb_optin_gate
cargo test --features agent --test agent_tpred_moe_gate
cargo test --features agent --test agent_a8_v1_gate
cargo test --features agent --test agent_hier_moe_gate
cargo check --features dashboard,agent --bin aether-dashboard
```

## Lições (resumo)

- Ingresso único: STT/OCR/VIS → texto → T-NL-IN (nunca planner direto).
- TTS só consome `CertifiedUtterance`; abstain → beep “não sei”.
- Hier MoE não é pré-requisito do funil NL-IN.

## Docs

[`CORE_TOMADAS.md`](CORE_TOMADAS.md) §8 · [`HUB_DASHBOARD.md`](HUB_DASHBOARD.md) ·
[`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) · [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) ·
[`CHANGELOG.md`](../CHANGELOG.md)

## Memória

- `md/L3/mcp/1795297008054000` — fila F0–F8 (auth 255)
- Release note espelhada no SGDB sob `aether/fact`

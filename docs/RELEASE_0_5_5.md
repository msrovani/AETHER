# Release 0.5.5 — T-DEC dual-bank + Gemma sentinela ∧ escriba

**Data:** 2026-11-21  
**Âncora:** após 0.5.4 (barramento trio); economia de meios no decisor e na prosa.

## O que esta versão é

1. **Upgrade T-DEC dual-bank** — trocar readers (primário / sentinela) com
   catálogo, HW, smoke T1/T2/C1/AND, HITL download+cutover; produto só lê
   `active`; candidate nunca no bus.
2. **Gemma = sentinela ∧ escriba** — um artefato, dois plugues (T-DEC +
   T-NL-OUT); opt-in `AETHER_SCRIBE=gemma`; default chat ainda template;
   juíz P2 + P6 AND em `act`.

| Camada | Entrega |
|---|---|
| Catálogo | `data/dec_reader_catalog.json` (`roles` sentinel+scribe) |
| Dual-bank | `dec_upgrade.rs` + job `DecUpgrade` + tools smoke/run |
| Lock active | `scribe_path` = sentinel; EngineConfig só active |
| Escriba | `gemma_scribe.rs` — nunca `vegaml.decide` |
| Hub | lâmpadas T-DEC, Up/Revert, chip 1 artefato · 2 papéis |
| Docs | `T_DEC_UPGRADE.md`, `GEMMA_SENTINEL_SCRIBE.md`, USER_GUIDE §7c |

## Non-goals mantidos

- Sem auto-cutover / auto-255.
- Candidate no Console/API.
- Aceite amarrado ao note 6 GB de lab (orçamento = host de produto).
- Segundo download “só prosa”.

## Como verificar

```bash
cargo test --features agent --test agent_dec_upgrade_gate
cargo test --features agent --test agent_gemma_scribe_gate
cargo check --features dashboard,agent --bin aether-dashboard
.\tools\aceite_0_5_5.ps1          # dry + HITL roundtrip
.\tools\aceite_0_5_5.ps1 -Live    # + T2/C1 gemma (T1 precisa vega-u-0.8b local)
```

Roteiro: [`ACEITE_0_5_5.md`](ACEITE_0_5_5.md).

## Memória

Fatos `md/L3/mcp/1795299745239000` (dual-bank), `1795303791820000` (scribe),
release 0.5.5 (este ciclo).

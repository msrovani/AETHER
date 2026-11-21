# Aceite 0.5.5 — T-DEC dual-bank + Gemma sentinela ∧ escriba

**Ritual:** gates → host_probe (VRAM) → dry dual-bank → live smoke (se modelos) →
HITL cutover+revert → escriba anchored → relatório.

Orquestrador: [`tools/aceite_0_5_5.ps1`](../tools/aceite_0_5_5.ps1).

## Critérios

| # | Critério | Passa se |
|---|---|---|
| G1 | Gates Rust | `agent_dec_upgrade_gate` + `agent_gemma_scribe_gate` verdes |
| G2 | Hub compila | `cargo check --features dashboard,agent --bin aether-dashboard` |
| H1 | Host report | `target/aether_host_report.json` com `vram_*` se NVIDIA |
| D1 | Dual-bank dry | prepare → smoke verde → fase `hitl_cutover` (active intacto) |
| D2 | Cutover HITL | `approve-cutover` promove; `scribe_path` amarrado; `revert` restaura |
| L1 | Live T2/C1 | gemma-prose abstém OOD; sem erro confiante C1 (se pesos locais) |
| L2 | Live T1 | vega-u-0.8b age no claro (**requer** `.vega/models/vega-u-0.8b`) |
| S1 | Escriba | `AETHER_SCRIBE=gemma` + generate anchored passa P2; default template |

## Non-goals (não bloqueiam aceite lab)

- Auto-cutover / auto-255
- Aceite amarrado só ao note 6 GB (orçamento = host de produto; 3050 = lab)
- Download HF neste ritual (`--network` opt-in à parte)
- Live T1 se primary ausente → **parcial amarelo**, não inventar verde

## Comandos manuais

```powershell
.\tools\aceite_0_5_5.ps1
# ou com live:
.\tools\aceite_0_5_5.ps1 -Live
```

HITL explícito (exemplo Rust):

```powershell
cargo run --example aceite_055_hitl --features agent -- prepare-sentinel
cargo run --example aceite_055_hitl --features agent -- approve-cutover
cargo run --example aceite_055_hitl --features agent -- ensure-scribe
cargo run --example aceite_055_hitl --features agent -- revert
```

## Artefatos

- `data/reports/aceite_0_5_5.json`
- `target/aether_dec_upgrade_smoke.json`
- `target/aether_host_report.json`
- `target/aether_dec_upgrade.json`

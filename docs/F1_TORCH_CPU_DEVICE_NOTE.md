# F1 — nota: `torch.cpu.device` × vegaml (MSG 1025)

Data: 2026-11-20 · CURSOR · Gestor: OPENCODE  
Ref: RELATORIO **1025** F1_CHAT_SMOKE_VEGA · autoridade desta nota: **60** (hipótese)

## Achado

No smoke de produto (`--vega`), o port absteve com
`engine_indisponivel` por **AttributeError** envolvendo `torch.cpu.device`
(vegaml **0.16.5**). O core respondeu corretamente (Brasília / act) —
**degradação honesta (Eixo E)**.

Veredito OPENCODE: `SMOKE_PASS_COM_DEGRADACAO_HONESTA`.

## Classificação (offline)

```bash
python tools/f1_torch_device_check.py selftest
python tools/f1_torch_device_check.py check --error "AttributeError: module 'torch.cpu' has no attribute 'device'"
```

- Match `AttributeError` + `cpu.device` → `degrade_engine_unavailable` (não OOM)
- Não toca GPU / `.vega` / crate
- Fix de runtime = BUFFY/OPENCODE (pin torch / vegaml); HITL para 255

## Fora de escopo

- Editar `pyo3_impl.rs`, `cli.rs`, baixar wheels, adquirir GPU lock

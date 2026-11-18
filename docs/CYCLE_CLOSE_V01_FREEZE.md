# Ciclo — Freeze v0.1 (IA usável)

**Data:** 2026-11-18 · **Plano:** [`V01_DELIVERY_PLAN.md`](V01_DELIVERY_PLAN.md)  
**Crate atual:** ver `Cargo.toml` (tag `v0.1.0` **ainda não** — exige pedido explícito)  
**Fórum:** MSG 0614 claim · MSG 0622 relatório plano

## Veredito

AETHER v0.1 está definido e instrumentado como **coprocessador de evidência**:
API/CLI com `act|cite|propose|abstain`, hub só observa. Frase de fronteira em
`/props.aether.frontier`.

## Gates (artefatos em `target/v01_freeze/`)

| ID | Resultado | Nota |
|---|---|---|
| F1 `agent_eval_suite` | **PASS** | false_act=0 · n=24 |
| F2 `cic_abstain_gate` | **PASS** | false_act=0 |
| F3 `api_agent` / `api_eval` | **PASS** | logs em `target/v01_freeze/F3*.log` |
| F4 `usability_gate` | **PASS** | 6/6 |
| F4 `agent_redteam_gate` | **PASS** | 4/4 |
| F5 `cognitive_audit` | **PASS** | ACQ 100 · 12/12 MEASURED |
| F6 fmt + lib | **PASS** | lib 172 testes; clippy: rodar `v01_freeze_gates.ps1` se precisar revalidar |
| F7 smoke | **PASS** | `target/v01_smoke.json` · frontier OK · known hit · OOD abstain |

## Entregas desta onda

- [`docs/V01_DELIVERY_PLAN.md`](V01_DELIVERY_PLAN.md)
- Crosslinks: README + [`USER_DELIVERY_PLAN.md`](USER_DELIVERY_PLAN.md)
- Scripts: `v01_freeze_gates.ps1`, `v01_freeze_smoke.ps1` → `v01_smoke.ps1`
- `/props`: campos `frontier` + `actions` ([`src/api/mod.rs`](../src/api/mod.rs))

## Tag 0.1.0 — bloqueio consciente

Bump `Cargo.toml` → `0.1.0` + tag git **somente** com pedido humano explícito
(commit/push/tag). Checklist: F1–F7 verdes no host + este ciclo fechado.

## Anti

Não vender como chatbot · não baixar teto AUC · não misturar ACQ com MMLU.

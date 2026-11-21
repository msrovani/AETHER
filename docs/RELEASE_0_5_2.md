# Release 0.5.2 — A6 LinUCB + A7 risco + A8 predict

**Data:** 2026-11-21  
**Âncora:** após 0.5.1 (T-CODE K1–K8 + A5); fecha a escada A6–A8 (fio mínimo).

## O que esta versão é

Marco em que o agent **aprende com feedback verificado (A6)**, **calibra HITL
por matriz de risco (A7)** e **antecipa séries com abstain honesto (A8)** —
sem Hierarchical MoE e **sem** ligar A8 no v1 default.

| Degrau | Entrega | Gate |
|---|---|---|
| A6 / P4 | LinUCB opt-in (`AETHER_LINUCB`); `learner.rs` reward só com `Feedback.correct` | `agent_a6_gate` + `agent_learn_gate` |
| A7 | `RISK_MATRIX` + `AETHER_AGENT_KILL`; High always HITL; Medium opcional | `agent_a7_gate` 6/6 |
| A8 / P5 fio | `predict.series` (read_local) + metacog self_field; `goal.propose` privileged HITL | `agent_a8_gate` 5/5 |

## Non-goals mantidos

- A8 **`enter_v1_default=false`** — valor medido no artefato; revisão humana.
- Hierarchical MoE / LSH (P5 pesquisa) **fora**.
- Shell off por default; auth coder/shell/predict ≤120; 255 só HITL.
- F4 congelada.

## Como verificar

```bash
cargo test --features agent --test agent_a6_gate
cargo test --features agent --test agent_a7_gate
cargo test --features agent --test agent_a8_gate
cargo run --release --features agent --example a8_predict_demo
```

Artefatos: `target/aether_risk_matrix.json` (`aether_risk_matrix/a7`),
`target/aether_a8_gate.json` (`aether_a8_gate/a8`).

## Lições (resumo)

Ver [`LESSONS_AETHER_AGENT.md`](LESSONS_AETHER_AGENT.md) §2: High+affordance
explícita precisa de piso no planner; research não compete com relações
exclusivas; OOD/metacog antes de inventar yhat; propor ≠ executar.

## Docs

[`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) · [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) ·
[`CORE_TOMADAS.md`](CORE_TOMADAS.md) · [`CHANGELOG.md`](../CHANGELOG.md)

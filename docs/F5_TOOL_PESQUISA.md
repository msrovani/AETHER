# F5 — Contrato da tool `research.external` (draft)

Data: 2026-11-20 · Dono draft: **CURSOR** (claim fórum) · Implementação
real da tool no `aether-agent` = fase F5 conjunta (OPENCODE coordena).

## Por que existe

F2 entrega a tomada **offline** no harness. F5 promove o mesmo contrato a
tool própria do agente, com `tier` + `independence` por fonte, e ganchos
de eval T0–T3.

Machine contract: `data/research_tool_contract.json`.

## Gatilho

Só `AbstainReason::Unknown`. Qualquer outro motivo → a tool **recusa**
(espelha `tools/research_harness.py`).

## Fluxo

```
DecisionPort abstain unknown
  → digest certificado ≤320 tokens
  → research.external (default allow_network=false)
  → verdict + authority∈[60,120] + sources independentes
  → remember quarentena (nunca 255)
  → (HITL) ampliar? → re-roda → marca resolvido_por_ampliacao | open_definitivo
```

## Dependências

| Fase | Entrega | Status |
|---|---|---|
| F2 / F2.5 | harness + tier + cases | feito (CURSOR) |
| F1 | DecisionPort no chat | OPENCODE |
| F4 | embed_server MiniLM | AGENT-A |
| F5 | tool no registry + API | conjunto |

## Aceite do draft

- JSON de contrato parseável
- `non_goals` incluem: sem 255, sem ampliar auto, sem rede default
- T0 aponta para `research_cases` + selftest já verdes

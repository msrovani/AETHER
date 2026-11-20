# F5 T4 — rota de decisão na API (spec do gestor)

Data: 2026-11-20 · Gestor: **OPENCODE** · Plano: `docs/VEGA_F3_PLANO_CONSOLIDADO.md`
(F5: *"API do híbrido + tool própria de pesquisa + eval tiers T0–T3"*)

## O que falta

O plano fecha o ciclo com uma **API do híbrido**. Hoje a API tem
`/v1/aether/agent`, `/v1/aether/cue`, `/v1/aether/solve`, `/v1/chat/completions`
e as rotas de memória — mas **não existe rota de decisão**: o `DecisionPort`
(F1, o AND gate) só é alcançável pelo binário `aether-agent` (com `--vega`).

Consequência: um app externo (o `aether-api` é a borda HTTP para apps de IA)
**não consegue pedir a decisão** que o chat pede. O híbrido está fechado no
CLI.

## A rota

```
POST /v1/aether/decision
```

**Entrada** — o contrato do crate (`aether_decision::types::DecisionRequest`):

```json
{
  "state": "digest de certificado (≤320 tokens) ou payload do pedido",
  "questions": {
    "passou": { "type": "boolean", "instructions": "..." },
    "route":  { "type": "choice", "instructions": "...", "criteria": {...} }
  }
}
```

**Saída** — `DecisionResponse` (Eixo B do plano, o schema já congelado):

```json
{
  "answers": [
    { "question": "passou", "choice": "False", "probs": {...},
      "abstain": true, "abstain_reason": "ambiguous", "readout": "and_gate",
      "confidence": 0.67, "conformal_set": [false] }
  ],
  "conformal_set": { "passou": [false] },
  "meta": { "latency_ms": 240.2, "engine": "vega-u-0.8b AND gemma-prose",
            "degraded": false, "vram_used_mb": 4393.0 }
}
```

## Regras (as do plano, não novas)

1. **O port é evidência, não veredito.** A rota devolve o veredito do gate;
   quem decide continua sendo o core. A rota não inventa resposta.
2. **Degradação honesta.** Sem engine (ou com a GPU ocupada), a rota devolve
   `abstain: true` com `abstain_reason: "engine_unavailable"` — nunca um
   veredito inventado, nunca 500 silencioso.
3. **Zero chamada externa por default.** Sem a feature `decision-pyo3` (ou sem
   as variáveis do engine), a rota usa o `StubPort`: degrada e diz. É o mesmo
   default do CLI (Eixo C).
4. **Allowlist e budget** como as outras rotas do agente (T2).
5. **O `state` é um digest, não o payload bruto** (Eixo B): a rota recusa
   `state` acima do orçamento e diz o motivo.

## Aceite

| item | como se prova |
|---|---|
| o schema da resposta é o do crate | teste de integração: serializa `DecisionResponse` e compara com o JSON da rota |
| degradação honesta sem engine | chamada sem `--features decision-pyo3` ⇒ `engine_unavailable`, HTTP 200, `degraded: true` |
| a rota usa as perguntas calibradas | o pedido do teste traz `passou`/`route` do corpus |
| não há atalho para o Python | `rg` na rota: nada de `python`/`vegaml` — só a trait |

## Onde cai

- `src/api/` — a rota e o handler (dono: quem claimar a fase);
- `tests/api_decision.rs` — o gate de integração (novo arquivo, sem tocar os
  testes existentes);
- `docs/F5_T4_DECISION_ROUTE.md` — o resultado.

**Não toca**: `src/knowledge/sgdb/`, `aether-decision/src/`, o dataset
congelado do F4.

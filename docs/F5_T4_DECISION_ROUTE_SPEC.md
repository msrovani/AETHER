# F5 T4 — rota de decisão na API

Data: 2026-11-20 · OPENCODE CodeMode · missão reivindicada em MSG 1058
(proposta pelo gestor em MSG 1055).

## Contrato

`POST /v1/aether/decision`

Entrada = `DecisionRequest` do crate `aether-decision` (schema congelado,
`decision/1`):

```json
{
  "state": "digest de certificado (<= 320 tokens)",
  "questions": {
    "passou": {"type": "boolean", "instructions": "..."},
    "route": {"type": "choice", "instructions": "...", "criteria": {}}
  }
}
```

Saída = `DecisionResponse`:

```json
{
  "answers": [
    {"question": "passou", "abstain": true,
     "abstain_reason": "engine_unavailable"}
  ],
  "conformal_set": null,
  "meta": {"latency_ms": 0.0, "engine": "stub", "degraded": true,
           "vram_used_mb": null}
}
```

## Invariantes

| Regra | Como é garantida |
|---|---|
| Port é evidência, não veredito | A rota só devolve o que `DecisionPort` produziu; o core AETHER continua decidindo |
| Degradação honesta | Default `StubPort` → `abstain_reason = engine_unavailable`, `degraded = true`; nunca inventa `choice` |
| Zero chamada externa por default | Nenhum import de Python, nenhum socket; o engine real entra pelo mesmo port quando o daemon opta |
| Eixo B (cano) | `state` limitado a 320 tokens (~1280 bytes) antes do port; `questions` deve ser objeto não vazio |
| Schema estável | Serialização feita pelo próprio `serde` do crate — a resposta é o mesmo tipo do chat |
| Sem atalho para o Python | A rota não conhece `pyo3_impl`; trocar implementação não toca a API |

Erros: `400` para corpo inválido, `state` acima do orçamento ou `questions`
vazio; `200` para abstenção (abstenção é resposta, não erro HTTP).

## DoD

- Rota em `src/api/mod.rs` (`needs_auth`, dispatch e `decision_route`).
- Teste de integração `tests/api_decision.rs` (3 casos: abstenção tipada,
  `state` grande, `questions` vazio).
- Este documento.

Fora de escopo (por posse): `src/knowledge/sgdb/`, `aether-decision/src/`,
`src/agent/cli.rs` e o dataset congelado do F4.

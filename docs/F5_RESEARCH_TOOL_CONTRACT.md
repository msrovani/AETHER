# F5 — Contrato da tool própria de pesquisa (draft)

Data: 2026-11-20 · Dono draft: **CURSOR** · Plano:
`docs/VEGA_F3_PLANO_CONSOLIDADO.md` F5 · Depende de F2/F2.5 (DONE) e F1 wired.

> Alias / espelho: `docs/F5_TOOL_PESQUISA.md` (MSG 0777/0779). Machine contract:
> `data/research_tool_contract.json`.

## Posicionamento

| Fase | Quem busca | Onde mora |
|---|---|---|
| F2 | fixtures offline / harness IDE (opcional) | `tools/research_harness.py` |
| F5 | tool `research.query` no `aether-agent` | registry + executor (não este draft) |

Este doc é **contrato**, não implementação no agent (F1/OPENCODE ainda fia
`DecisionPort`; tool F5 entra depois).

## Schema de entrada

```json
{
  "tool": "research.query",
  "args": {
    "question": "string",
    "abstain_reason": "unknown",
    "state_digest": "string ≤320 tokens",
    "allow_network": false,
    "ampliar": null
  }
}
```

Regras:
- `abstain_reason` ≠ `unknown` → recusa (mesma doutrina F2)
- `ampliar` só se objeto `{ "hitl_ack": "SIM", "actions": [...] }`
- `allow_network=true` exige allowlist + HITL ack (`data/research_network_allowlist.json`, stub T2)

## Schema de saída

```json
{
  "verdict": "settled|weak|open",
  "authority": 60,
  "sources": [
    {
      "tier": "primary|secondary|aggregator|seo_llm_farm",
      "origin": "string",
      "independence_cluster": 0,
      "url": "string|null",
      "excerpt": "string"
    }
  ],
  "n_independent": 1,
  "numeric": null,
  "semantic": {"label": "agree|conflict|insufficient"},
  "quarantine": true,
  "hitl_required_for_255": true,
  "ampliacao_mark": null
}
```

Invariantes: `authority ≤ 120`; nunca 255; cópias → 1 origem (Jaccard /
`origin` em `data/source_tier.json`).

## Eval tiers (contínuos na F5)

| Tier | O que mede |
|---|---|
| T0 | selftest fixtures (`tools/f5_t0_eval.py`) |
| T1 | unknown→quarentena em laço agent — **DONE** `tools/f5_t1_agent_loop.py` / `docs/F5_T1_AGENT_LOOP.md` |
| T2 | híbrido com rede allowlist — stub deny-by-default `tools/f5_t2_allowlist.py` (MSG 0823); HITL ainda não sancionou hosts |
| T3 | suite externa / regressão hit@k — **PREP** `tools/f5_t3_eval.py` (MSG 0866): freeze + BM25 lexical offline; MiniLM = F4 AGENT_A |

## Fora de escopo deste draft

- Código em `src/agent/*` / `cli.rs`
- Crate `aether-decision`
- Push / rede default on

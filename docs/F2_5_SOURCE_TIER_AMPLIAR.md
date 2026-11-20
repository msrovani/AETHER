# F2.5 — Tier de fonte + comando `ampliar` (HITL)

Data: 2026-11-20 · CLAIM: **MSG 0768** (CURSOR) · REF: MSG 0761 · Plano:
`docs/VEGA_F3_PLANO_CONSOLIDADO.md` Eixo F (camadas 0–3).

## Entrega

| Artefato | Papel |
|---|---|
| `data/source_tier.json` | Tabela de tiers + caps + regras de independência/veredito/`ampliar` |
| `data/research_cases.json` | 3 casos de régua HITL (Terra–Lua, Rust vs C, 3 blogs) |
| `tools/research_reliability.py` | Avaliador + CLI `ampliar` (nunca automático) |
| Este doc | Spec do comando e critérios de aceite |

Não toca: `aether-decision/`, `Cargo.toml` workspace, `src/agent/cli.rs`,
`src/knowledge/sgdb/`.

## Camada 0 — tier

Ordem: `primary` > `secondary` > `aggregator` > `seo_llm_farm`.

```
autoridade_final = min(teto_do_tier, score_de_convergencia) − penalidade_independencia
autoridade_final ≤ 120 sempre   # quarentena; 255 só HITL
```

Fonte ruim convergida **não sobe** (ex.: 3 blogs SEO idênticos → 1 origem → cap 60).

## Camada 3 — `ampliar` (HITL only)

### Contrato CLI

```bash
python tools/research_reliability.py ampliar \
  --case <id> \
  --hitl-ack SIM \
  [--actions mais_fontes mais_tiers sinonimos_PT_EN janela_datas]
```

| Regra | Comportamento |
|---|---|
| Sem `--hitl-ack SIM` | recusa (exit 2); sistema **nunca** amplia sozinho |
| Com ACK | re-avalia com ações; grava **delta** `{verdict, authority, n_independent, mark}` |
| Teto do tier | **nunca** sobe por ampliar |
| Conflito persiste | marca `open_definitivo` (sinal F3 `question_kind`) |
| Convergiu após | marca `resolvido_por_ampliacao` se veredito settled/weak |

### Schema do delta (saída JSON)

```json
{
  "command": "ampliar",
  "hitl_ack": "SIM",
  "delta": {
    "verdict": ["open", "settled"],
    "authority": [60, 120],
    "n_independent": [1, 2],
    "mark": "resolvido_por_ampliacao"
  },
  "never_raises_tier_cap": true
}
```

## Aceite F2.5 (MSG 0759 + plano)

| Caso | Esperado |
|---|---|
| Distância Terra–Lua | settled, authority **120**, σ/μ < 5% |
| Rust vs C | open **60**, sem classe vencedora (`irreducible_open`) |
| 3 blogs idênticos | **1** origem independente → open **60** |
| `ampliar` sem HITL | recusa |
| `ampliar` + conflito | `open_definitivo` |

Validação:

```bash
python tools/research_reliability.py selftest
```

# F2 / F2.5 — Pesquisa externa e confiabilidade (Eixo F)

Data: 2026-11-20 · Dono: **CURSOR** (CLAIM F2.5 = **MSG 0768**; F2 = MSG 0762) · Contrato:
`docs/VEGA_F3_PLANO_CONSOLIDADO.md` + `docs/BUILD_MULTI_AGENT.md` · Spec F2.5:
`docs/F2_5_SOURCE_TIER_AMPLIAR.md`.

## Escopo

| Item | Entrega |
|---|---|
| F2 | taxonomia alinhada a `AbstainReason` (OPENCODE `aether-decision`); tomada de pesquisa só em `unknown`; tripla evidência→convergência→veredito; quarentena 60–120 |
| F2.5 | `data/source_tier.json`; comando HITL `ampliar`; cópias contam como 1 |

Não toca: `aether-decision/`, `Cargo.toml` workspace, `src/agent/cli.rs`, `src/knowledge/sgdb/`.

## Porta de entrada

Só `AbstainReason::Unknown` admite pesquisa (`admits_research()`). Demais motivos:

- `ambiguous` / `conflict` → HITL
- `engine_unavailable` → degradação, sem inventar

**F1_3 / MF2 (MSG 0802):** `unknown` só quando `unbound=true`; `prediction_set` com
mais de 1 candidato → `ambiguous` (não dispara harness). Regressão:
`python tools/mf2_gate_regression.py` (usa `target/f1_decision_smoke.json`).

**F1-5 (MSG 0853, pedido do BUFFY):** pergunta respondida pelo head `ttt`
(`readout=ttt` ou `cv_accuracy` sem `conformal_set`) **não** tem
`prediction_set`. A F2 não exige conjunto conformal nesse caminho e não abre
pesquisa por ausência de set. `admits_research_answer` no harness cobre isso.

Na F2 a tomada usa **websearch do harness**. Tool própria no `aether-agent` é F5.

## Pipeline

```
unknown → buscar fontes → tier (camada 0) → independência → convergência → veredito
         → (opcional) ampliar [HITL] → re-roda → delta
         → remember autoridade ≤120 (nunca 255)
```

Implementação executável:
- `tools/research_reliability.py` — Eixo F (tier/convergência/ampliar)
- `tools/research_harness.py` — tomada F2: só `unknown` → evaluate → quarentena ≤120
- `tools/f2_wire_port.py` — **cabo** MSG 1014: DecisionPort/`fora_lexico` → digest ≤320 → harness → quarentena (sem editar `cli.rs`)

Casos de régua: `data/research_cases.json`. Fixture do cabo: `data/f2_wire_turns.json`.

```bash
python tools/research_harness.py selftest
python tools/research_harness.py --reason unknown --case terra_lua \
  --write-quarantine target/research_quarantine.jsonl
python tools/f2_wire_port.py selftest
```

Rede default **OFF** (socket fora do inventário). `--allow-network` = reserva F5.
WebSearch do harness Cursor pode validar fixtures (ex.: Terra–Lua ≈384400 km) mas
não entra no caminho feliz offline.

### Vetor vivo (`tools/research_live.py`) — cascata

Opt-in: `AETHER_RESEARCH_NETWORK=1`.

| Env | Default | Efeito |
|---|---|---|
| `AETHER_RESEARCH_PROVIDER` | `cascade` | `ddg` \| `searx` \| `wiki` \| `cascade` |
| `AETHER_SEARX_URL` | `https://search.lumy.live` | override / local; `0`\|`off`\|`-` desliga SearX |
| `AETHER_RESEARCH_PARALLEL` | `1` | `0` = first-hit sequencial; `1` = threads I/O + fusão |

Cascata: **DDG lite ∥ SearXNG ∥ Wikipedia/Wikidata API**. Default dispara
em paralelo (I/O) e **funde** fontes; SearX usa instância pública JSON-ok
(`search.lumy.live`) se o env estiver vazio — pública e frágil; preferir
SearXNG local quando possível. `PARALLEL=0` restaura first-hit. ≥2
providers com evidência → autoridade fraca **80** (`multi_provider_confirm`);
settled numérico continua ≤120. Evidência em quarentena 60–120.

**Gate de produtividade** (`tools/research_intent.py`):
`research` (fato mundano) | `memory` (interno AETHER) | `deny` (lixo/gerativo/controle).
Sem fontes após cascata → **nega conhecimento** (não limbo `fontes=0`).

**Persistência:** pesquisa com fontes grava `aether-sgdb put` (auth ≤120,
entidade `aether/ops`). O turno seguinte cita via `MemoryCitations` — confirmar
com `python tools/research_recall_confirm.py`.

```bash
$env:AETHER_RESEARCH_NETWORK=1
python tools/research_live.py selftest
python tools/f2_chat_hook.py selftest
python tools/research_recall_confirm.py
python tools/research_live.py "capital da Franca"
python tools/f2_chat_hook.py once --text "quem escreveu Dom Casmurro?"
```

## Ampliar (HITL only)

- Sistema **nunca** amplia sozinho.
- Não sobe teto do tier; só melhora convergência / cobertura.
- Persistiu conflito → `open_definitivo` (sinal para `question_kind` na F3).
- Convergiu após ampliar → settled com marca `resolvido_por_ampliacao`.

## Aceite

- Terra–Lua → settled 120
- Rust vs C → open 60, sem classe vencedora
- 3 blogs copiados → 1 origem → open/60 ou descarte

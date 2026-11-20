# F4 — carta de decisão HITL (gate)

Data: 2026-11-20 · CURSOR MSG **0937** · Dono F4: **AGENT-A** · Gestor: OPENCODE  
Fontes: fórum MSG **0926** (AGENT-A) + review MSG **0930** / `docs/F4_GATE_REVIEW_0926.md`  
Autoridade deste doc: **120** — **não decide**; só empacota opções.

## Números congelados (já conferidos)

| métrica | valor | vs gate 0.974 |
|---|---|---|
| BM25 hit@5 | 0.7042 | FAIL |
| embed MiniLM-ML 384d | 0.5141 | FAIL |
| híbrido RRF60 | 0.7465 | FAIL |
| híbrido > BM25? | sim (0.7465 > 0.7042) | — |
| SHA set | `f8682442…16bf0` | íntegro |

Dataset: `data/embed_eval_set.jsonl` — **não reescrever** para passar.

## Opção A — Redefinir o gate (granularidade)

**Proposta AGENT-A:** muitas misses do híbrido acertam o **arquivo** certo e
perdem para irmã da mesma página (hit@5 por arquivo ≈ 0.8873 vs seção 0.7394 —
hipótese AGENT-A; review 0930 **não** rederivou ranks).

HITL escolhe um critério novo, por exemplo:

1. hit@5 **por arquivo** (ouro multi-seção do mesmo path conta), **ou**
2. ouro com **múltiplas seções** aceitas por query,

e republica o número-alvo (não precisa ser 0.974). Dataset pode permanecer;
muda só a **métrica de aceite**.

## Opção B — Manter 0.974 seção e declarar REPROVADO

Gate atual (seção, 0.974) **não passa** com estes números. HITL declara
**F4 REPROVADO** para esta geração (modelo + set + métrica). Próxima geração
exige plano novo (outro embedder / corpus / ouro) — ainda sem retune silencioso
do set congelado.

## O que ninguém deve fazer sem HITL

- Reescrever `data/embed_eval_set.jsonl` para “passar”
- Baixar outro modelo sem sanção
- Baixar o teto 0.974 no script sem autoridade 255
- Tratar autoridade ≤120 como aceite

## Pedido ao HITL

Marcar **A** ou **B** (ou variante escrita) no fórum. AGENT-A executa; CURSOR
só documenta.

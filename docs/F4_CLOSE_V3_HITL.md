# F4 — fechamento V3 (HITL, 2026-11-21)

**Decisão:** opção 2 do chat — **F4 CONGELADA como REPROVADA**.
Cadeira de memória semântica **fechada**. Gate `hit@5 ≥ 0.974` **inalterado**.
Set v2 `data/embed_eval_set_v2.jsonl` SHA-256
`c422a1a00fc18c9335b437f507f8bf250f8df2a1689f3ef2f2259c7fad0f3397` **intacto**.

**Autoridade:** 255 (HITL no chat + fórum MSG 1093). Hub: MSG 1094.
Fato nsgdb: `md/L3/mcp/1795232987735000` (authority 255).

## Placar medido (mesmo set, sem tuning pós-número)

| Fase | Sistema | Embed / denso | Híbrido ou rerank | Veredito |
|---|---|---:|---:|---|
| G8 / v2 | MiniLM multilíngue 384d | 0,6761 | híbrido 0,9225 | REPROVADO |
| V3-A | `BAAI/bge-m3` denso 1024d | **0,9366** | híbrido 0,9296 | REPROVADO |
| V3-A | `Qwen/Qwen3-Embedding-0.6B` | 0,8451 | híbrido 0,9296 | REPROVADO |
| V3-B | BGE denso + `bge-reranker-v2-m3` (pool 50) | 0,9366 | rerank **0,9437** | REPROVADO |
| V3-B | BGE denso + `Qwen3-Reranker-0.6B` (pool 50) | 0,9366 | rerank **0,9577** | REPROVADO |

Melhor número da fila V3: **0,9577** (136/142). Gate pede 139. Faltam **3**.

Artefatos versionados (cópia dos placares):

- `data/reports/f4_v3_bge_m3_v2.json`
- `data/reports/f4_v3_qwen3_06b_v2.json`
- `data/reports/f4_v3_bge_rerank_v2.json`
- `data/reports/f4_v3_qwen_rerank_06b.json`
- `data/embed_eval_last.json` — placar oficial v2 MiniLM (não sobrescrito)

Harness: `tools/embed_server.py`, `tools/embed_eval.py`, `tools/embed_measure*.ps1/py`,
`tools/f4_rerank_v3b.py`. Hub: `src/dashboard/f4_view.rs` (faixa CONGELADA).

## Lições (aprendizado operacional)

1. **Ouro por arquivo** (v2) subiu o híbrido MiniLM de 0,7465 → 0,9225; o déficit
   restante não se resolve com mais cirurgia no gold.
2. **BGE-M3 denso** foi o melhor mapa zero-shot (0,9366); o RRF60 **piorou** o
   denso no semântico — híbrido não é sempre melhor.
3. **Rerank ajuda pouco neste set:** +0,0071 (BGE) e +0,0211 (Qwen) sobre 0,9366;
   troca erros, não fecha o gate.
4. **Eval ≠ produto:** 722 embeds + 7 100 pares de rerank são custo de **gate**,
   não latência de pergunta. Resposta quente Vega ~0,1–0,6 s (já medida).
5. **Próximo modelo exige sanção nova** (scoring por arquivo, modelo maior, ou
   outra ideia). V3-A/B esgotadas. Sem baixar teto.

## Fora de escopo deste fechamento

G7 (adapter), baixar 0,974, reescrever ouro, esparso/ColBERT do BGE, Embedding 4B/8B.

## Fórum

MSG 1086 (G8), 1088 (sanção V3), 1089–1092 (medições V3-B), 1093 (congelar),
1094 (hub fecha capítulo).

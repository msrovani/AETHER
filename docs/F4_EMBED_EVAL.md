# F4 — embed server + eval de recall (hit@5)

> Missão **F4** do plano F2→F3 (`docs/VEGA_F3_PLANO_CONSOLIDADO.md`) · claim
> **MSG 0780** · aprovado com ressalva **D3** na **MSG 0772** · dataset
> congelado na **MSG 0861** · dono: **AGENT-A**.

## 1. O que existe aqui

| Arquivo | Papel |
|---|---|
| `tools/embed_server.py` | Embedder semântico local (Eixo C, tomada 3). HTTP stdlib, contrato `/embed` + `/embed_batch` do trait `Embedder` do neural-sgdb |
| `tools/embed_eval.py` | Freeze do dataset + métricas (BM25, cosine, RRF k=60) + selftest |
| `data/embed_eval_queries.jsonl` | 142 consultas escritas à mão, com `gold` (append-only, dono F4) |
| `data/embed_eval_set.jsonl` | **Dataset congelado**: meta + corpus + consultas + hash |
| `target/embed_eval.json` | Artefato da medição (não versionado) |

## 2. Contrato do server

```text
GET  /health                           -> {"ok","model","dim","normalized","offline"}
POST /embed       {"text": "..."}      -> {"embedding": [...]}
POST /embed_batch {"texts": [...]}     -> {"embeddings": [[...], ...]}
```

- modelo default `sentence-transformers/all-MiniLM-L6-v2` (384-dim, normalizado);
- **offline por default** (`HF_HUB_OFFLINE=1`): nenhuma chamada externa em runtime;
- aquisição única do modelo = `AETHER_EMBED_ALLOW_DOWNLOAD=1` (setup explícito,
  feito em 2026-11-20 com autorização do HITL);
- exceção do motor vira `500 {"error": ...}` — nunca derruba o servidor.

```powershell
.venv-py/Scripts/python.exe tools/embed_server.py --port 8788
```

## 3. Protocolo do gate (ressalva D3 — sem tuning depois de medir)

1. `python tools/embed_eval.py --freeze` monta o corpus e **valida todo `gold`
   contra o corpus**; imprime o `sha256` do arquivo;
2. o hash é publicado no fórum **antes** de qualquer medição
   (MSG 0861: `94491702…cba8a8`);
3. `python tools/embed_eval.py --run --sha256 <hash>` **recusa rodar** se o
   dataset mudou, e só então mede;
4. número ruim vira número ruim — o dataset não é reescrito para passar.

## 4. Corpus (722 docs)

| Fonte | Docs | Nota |
|---|---|---|
| `docs/*.md` por seção `## ` | 679 | seção com <40 chars é âncora, não doc |
| `data/agent_facts.jsonl` + `data/facts.jsonl` | 41 | dedupe por canônico (`_1.._19` são fixture repetida) |
| nsgdb (`.nsgdb/memory.db`, leitura via MCP stdio) | 2 | 5 memórias no banco; 2 recuperáveis por consulta lexical |

As **consultas** cobrem quatro origens: fatos estruturados, seções de docs,
memórias nsgdb, e três dificuldades declaradas no campo `kind`:

- `lexical` — a consulta repete o vocabulário do documento;
- `semantic` — paráfrase: palavras diferentes, mesmo conteúdo;
- `hard` — irmãs da mesma seção, negações e vizinhos compartilham vocabulário.

## 5. Sistemas medidos

| Sistema | Ranqueamento |
|---|---|
| `lexical_bm25` | BM25 (k1=1.2, b=0.75) — baseline forte, não baseline de pária |
| `embed_minilm` | cosseno sobre vetores 384-dim via `/embed_batch` |
| `hybrid_rrf60` | RRF k=60 sobre os dois ranqueamentos (mesmo k do nsgdb) |

Métricas: `hit5` (algum gold no top-5) e `recall5` (fracção de golds no top-5),
no total e por `kind`.

**Gate (plano F2→F3, F4):** `hit5 ≥ 0.974` em `embed_minilm` **e**
`hybrid_rrf60`, além de não ficar abaixo do `lexical_bm25`.

## 6. Como rodar

```powershell
python tools/embed_eval.py --selftest            # matemática das métricas, sem rede
python tools/embed_eval.py --freeze              # (re)congela e imprime o hash
.venv-py/Scripts/python.exe tools/embed_server.py --port 8788
python tools/embed_eval.py --run --sha256 94491702d9d14c20762f95f6de9e5890f438a2efb9680de9250f28f8e0cba8a8
```

## 7. Fora de escopo

- F3 (corpus 124→500/tipo + `question_kind`) — na fila do AGENT-A (MSG 0772);
- F5 (API do híbrido + tool de pesquisa) — contrato draft do CURSOR (MSG 0779);
- `src/knowledge/sgdb/` — intocado (posse + estabilidade).

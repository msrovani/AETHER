# F4 — embed server + eval de recall (hit@5)

> **Estado 2026-11-21:** F4 **CONGELADA REPROVADA** (HITL MSG 1093). Gate
> `0.974` intacto. Fechamento V3 e placares: [`F4_CLOSE_V3_HITL.md`](F4_CLOSE_V3_HITL.md).
>
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

- modelo default `BAAI/bge-m3` (1024-dim, normalizado, cap 512 tokens) —
  encaixe HITL 2026-11-22 (V3-A: hit@5 0.9366 vs MiniLM 0.6761 no set v2);
  MiniLM 384 permanece via `--model` (era legada do nsgdb);
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

## 7. Resultado medido (2026-11-20) — GATE REPROVADO

Dataset congelado `sha256 f868244256ee…416bf0` (validado na MSG 0862),
722 docs / 142 consultas. Duas medições, mesma query:

| Sistema | Medição 1 (EN `all-MiniLM-L6-v2`) | Medição 2 (MLT `paraphrase-multilingual…`) |
|---|---|---|
| `lexical_bm25` | 0.7042 | 0.7042 |
| `embed_minilm` | 0.4085 | **0.5141** |
| `hybrid_rrf60` | ~0.59 | **0.7465** |
| gate ≥ 0.974 | FAIL | **FAIL** |

Híbrido por tipo (medição 2): `hard` 0.78 · `lexical` 0.84 · `semantic` 0.67.
O híbrido **bate** o BM25 (0.7465 > 0.7042); o embed puro, não.

**Diagnóstico honesto** (proposto na MSG 0926):

1. Medição 1 era inválida como julgamento do modelo — modelo EN contra
   corpus PT (cosseno PT→ouro 0.217 vs EN→ouro 0.420) + bug do tokenizador
   (`kuramoto_field` ≠ `kuramoto` no BM25). Ambos corrigidos; dataset intacto.
2. O gap que sobrou **não é de modelo**: 21 das 37 misses do híbrido acertam
   o **arquivo** certo e perdem para a irmã da mesma página —
   `hit@5` por **arquivo** = **0.8873** vs por **seção** = 0.7394.
   O gate 0.974 foi proposto sobre outro corpus/planta de consultas; com ouro
   por seção e consultas parafraseadas à mão, ele mede granularidade, não
   qualidade do retriever.
3. Disclosure: o chat HITL aprovou o download do `all-MiniLM-L6-v2` (MSG 0802
   autoriza "MiniLM 384d"); a variante multilingue é da mesma família e
   dimensão, mas não foi citada — pedido de ratificação no mesmo post.

Decisão (HITL): redefinir o gate em nível de arquivo / ouro múltiplo, ou
manter 0.974 e declarar F4 reprovado nesta geração. **O dataset congelado não
é reescrito para passar.**

## 8. Fora de escopo

- F3 (corpus 124→500/tipo + `question_kind`) — na fila do AGENT-A (MSG 0772);
- F5 (API do híbrido + tool de pesquisa) — contrato draft do CURSOR (MSG 0779);
- `src/knowledge/sgdb/` — intocado (posse + estabilidade).

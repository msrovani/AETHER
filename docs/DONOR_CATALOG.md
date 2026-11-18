# Catálogo de doadoras — mapa, escriba, extrator

> Objetivo: AETHER como **IA completa destokenizada**, paralela às LLMs.
> Doadoras **nunca** entram no hot path T0 (146 µs). Sidecar offline → HV + fatos.
> Evidência de ciclo: MSG **0519–0521**, `target/b0_parafra_gate.json`, playbook §2.

Atualizado: **2026-11-18** · mediado CURSOR (pesquisa HF + fórum A3).

---

## 0. Doutrina (não negociar)

| Regra | Conteúdo |
|-------|----------|
| Doação ≠ pesos | Geometria (mapa) + triplas (escriba/extrator); núcleo = bits + keys |
| Auth | Geração ETL **60–120** (quarentena); teto duro ≤**240**; **255** = HITL |
| Dim mapa | Operacional **768** (MRL / truncate); HV **10240** bits |
| Capacidade | Lei E1B ≈ **0,4 × D** → **4096** átomos/shard (`HvBank::e1b_capacity`) |
| B-G0 / B-G0′ | Cosine AUC≈0,49; E1 ordem scaffold **PASS** AUC≈0,61 (`b0_e1_order_gate`) |
| Non-goal | Tokenizer / pesos / BAM / JIT no CLI de produto ou no T0 |

Arquitetura alvo:

```text
texto ──► [borda NL: parser | LanguagePort | LLM opcional]
              │
              ▼  fatos tipados (quarantine 60–120)
         neural-sgdb / ledger
              │
mapa HF ──► emb.f32 ──► JL+sign ──► HvBank shards ≤4k
escriba/extrator ──► facts.jsonl ──► FHRR + remember
              │
              ▼
         núcleo: cue HV / SAT / QUBO / certificado  (sem token)
```

---

## 1. Ranking — mapa (geometria)

| # | Modelo | Licença | Dim / MRL | Papel | Notas |
|---|--------|---------|-----------|-------|-------|
| **1** | [`Qwen/Qwen3-Embedding-0.6B`](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) | Apache-2.0 | 1024 nativo; MRL **768** | **Produto** | Canônico no playbook; GPU bf16 ~10–23 txt/s (MSG 0509/0515) |
| 2 | `Qwen/Qwen3-Embedding-4B` / `8B` | Apache-2.0 | MRL até 2560/4096 | Qualidade | Só se B-G0* retrieval exigir; mais VRAM |
| 3 | [`nomic-ai/modernbert-embed-base`](https://huggingface.co/nomic-ai/modernbert-embed-base) | Apache-2.0 | 768→256 | Alt. EN | Prefixo `search_query:` / `search_document:` |
| 4 | [`Snowflake/snowflake-arctic-embed-m-v1.5`](https://huggingface.co/Snowflake/snowflake-arctic-embed-m-v1.5) | Apache-2.0 | 768 MRL | Alt. compacta | Bom para compressão int8 |
| 5 | `BAAI/bge-m3` | MIT | 1024 | Multilingue | Não MRL nativo Qwen; validar truncate 768 |
| — | `tardellirs/colibri-embed-ptbr` | Gemma | — | Lab PT-BR | Só se produto permitir Gemma |
| **Proibido no crate** | qualquer peso | — | — | — | Sidecar `tools/donor_map_embed.py` apenas |

\*Trocar mapa **não** resolve PAWS lexical-overlap sozinho (MSG 0520). Use mapa para
**retrieval**; use **E1 phase language** para discriminação de paráfrase.

---

## 2. Ranking — escriba / extrator (triplas)

| # | Modelo | Licença | Papel | Quando |
|---|--------|---------|-------|--------|
| **1** | [`fastino/gliner2.5-base-v1`](https://github.com/fastino-ai/GLiNER2) (+ JointIE) | Apache-2.0 | **Extrator schema** | Preferido: CPU-first, grafo tipado, sem “chat” |
| 2 | `fastino/gliner2.5-multi-v1` / `small-v1` (74M) | Apache-2.0 | Extrator leve | Lab / edge |
| **3** | [`Qwen/Qwen2.5-7B-Instruct`](https://huggingface.co/Qwen/Qwen2.5-7B-Instruct) | Apache-2.0 | Escriba batch | Corpus aberto / relações livres; VRAM ~14–16 GB |
| 4 | [`mohar07/qwen3-0.6b-kg-triplets`](https://huggingface.co/mohar07/qwen3-0.6b-kg-triplets) | MIT | Escriba barato | Ontologia **20** relações; Entity F1 baixo — quarentena |
| 5 | Qwen2.5-14B / 32B Instruct | Apache-2.0 | Escriba pesado | Só produção com GPU dedicada |
| **Rerank** | [`Qwen/Qwen3-Reranker-0.6B`](https://huggingface.co/Qwen/Qwen3-Reranker-0.6B) | Apache-2.0 | ETL/eval | Opcional no harvest; **não** no 60 Hz |
| **Evitar** | Qwen2.5-**3B** / **72B** | Research / Qwen | — | Licença restrita |
| **Proibido** | Babelscape/REBEL (código) | CC BY-SA-**NC** | — | Non-commercial |
| **Proibido** | `victorlxh/ICKG-v3.2` | NC | — | Playbook §2 |

Pipeline escriba recomendado:

1. **GLiNER2.5 JointIE** → triplas schema-conformant → `facts.jsonl` (auth 60–120).
2. Opcional: **Qwen2.5-7B** só para relações fora do schema / normalização.
3. `harvest` estágio 5–6 → FHRR + mock/real SGDB; **nunca** auth 255 sem HITL.

---

## 3. Corpora doáveis (conhecimento)

| Fonte | Licença típica | Uso |
|-------|----------------|-----|
| Wikidata (entities) | **CC0** | Triplas canônicas → GLiNER/escriba ([dumps](https://dumps.wikimedia.org/wikidatawiki/entities/)) |
| Wikipedia (`ptwiki`/`enwiki`) | CC BY-SA 4.0 | Passagens → mapa; sharealike no derivado |
| Simple English Wiki | CC BY-SA 4.0 | Lab leve / smoke B4 |
| PAWS-X | pesquisa | **Gate B-G0** (não corpus produto) — `data/rounds/20261117_b0_paws` |
| arXiv abstracts | várias | Domínio técnico (filtrar licença) |

---

## 4. Estado medido (A3 / B-G0) — 2026-11-18

Artefato: `target/b0_parafra_gate.json`

| Métrica | Valor | Leitura |
|---------|-------|---------|
| AUC cos(s1,s2) donor | **0,4945** | ≈ acaso — mapa cego à ordem |
| recall@1 stack HV @5k | **0,7676** | Degradação **prevista** (over capacity) |
| Capacidade E1B | **4096** | Shard ≤4k obrigatório (B2) |

**B-G0′ (2026-11-18):** scaffold E1+PAWS PASS (AUC_ord=0,6054 vs bag=0,5102). Subir com LanguagePort/roles melhores; teto cosine A3 **não** relaxado; T0 intocado.

---

## 5. Roadmap de doação → IA completa

| Fase | Entrega | Gate |
|------|---------|------|
| Lab (feito) | dry-run + hub + sidecar `--jl-only` | 8 gates HV / U1–U6 |
| A3 (feito) | B-G0 honesto FAIL + lei capacidade | MSG 0519–0521 |
| **Agora** | B2 shards ≤4k + LSH união | recall@1≥0,90 *dentro* capacidade |
| B-G0′ | E1 ordem + PAWS | **scaffold PASS** AUC_ord=0,6054 Δbag=+0,095 (`target/b0_e1_order_gate.json`); subir com LanguagePort |
| Escriba-0 | GLiNER2.5 → facts.jsonl | schema F1 + auth≤240 |
| B4 | rodada produção corpus+licença | 8 gates oficiais + revision HF |
| SGDB | remember / audit / contradicts reais | URL/stdio estável |
| Borda NL | LanguagePort parser (já) + LLM opcional | OOV / CIC; núcleo sem token |

---

## 6. Comandos (sidecar)

```powershell
# Mapa (CUDA se houver)
python tools/donor_map_embed.py --out data/rounds/<rodada> --donor Qwen/Qwen3-Embedding-0.6B --dim 768 --device cuda --seed 42
python tools/harvest_transplant.py --jl-only --out data/rounds/<rodada> --no-whiten

# B-G0 (já rodado)
# tools/b0_paws_harvest.py → donor_map_embed → b0_pair_cosines → cargo test --features m1 --test b0_parafra_gate
```

Refs: [`DOACAO_PLAYBOOK.md`](DOACAO_PLAYBOOK.md) · [`CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md`](CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md) · [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) · [`PLAN_2026-11-17_OTIMIZACAO_ARXIV.md`](PLAN_2026-11-17_OTIMIZACAO_ARXIV.md).

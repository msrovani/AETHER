# Pesquisa: doadoras de conhecimento para o AETHER (estado da arte 2026)

Data: **2026-11-18** · Fonte: pesquisa web (subagente OPENCODE) · Status: **quarentena**
(autoridade 120 no SGDB — evidência web/LLM, nunca 255 sem HITL)
Doutrina: *LLM gera; AETHER cita, certifica ou abstém.* Nada entra no crate sem gate.

---

## 1. Papel "mapa" (embedder → geometria HV)

| Modelo | Licença | Dim | MRL | PT-BR | Veredito |
|---|---|---|---|---|---|
| `Qwen/Qwen3-Embedding-0.6B` (**atual**) | Apache-2.0 | 1024 (trunc 768) | **oficial** | #2/242 MTEB PT (71,63), zero-shot | **manter** — risk-adjusted melhor |
| `microsoft/harrier-oss-v1-0.6b` | MIT | 1024 | **não documentado** | 69,0 vs 64,3 (MMTEB v2, mesma placa) | **piloto condicionado**: +5 pts, mas truncar a 768 sem garantia contratual → exigir gate próprio (B-G0 + E3) antes de trocar |
| `google/embeddinggemma-2` | Apache-2.0 | **768 nativa** | 768/512/256/128 | 61,4 (menor qualidade) | encaixe perfeito no pipeline, qualidade menor; multimodal |
| `nvidia/Nemotron-3-Embed-1B-BF16` | OpenMDW-1.1 | 2048 | não | RTEB 72,4 (melhor retrieval do porte) | alternativa; licença menos familiar; sem MRL |
| `BAAI/bge-m3` | MIT | 1024 | não | envelhecido | só se sparse/multivetor interessar |

**Proibidas (non-commercial CC BY-NC 4.0):** jina-embeddings v3/v5, Linq-Embed-Mistral,
SFR-Embedding-2, REBEL. **Gemma ToU (não permissiva):** EmbeddingGemma-300M v1,
Gemma 1–3/3n, PaliGemma — "Gemma" não é marca de licença Apache; verificar por
checkpoint (Gemma 4 e EmbeddingGemma 2 **são** Apache-2.0).
Outros a evitar: Noema 1.5 (licença custom), LFM Open License (teto US$10M).

**Regra de troca de mapa:** nova rodada de transplante com gate E3 completo
(Spearman ≥0,95, recall@10 ≥0,90, conflitos ≥95%) — nunca trocar sem re-rodar
B-G0/E3; proveniência `manifest.donor` + `revision` + `sha256_bank`.

---

## 2. Papel "escriba" (extração de triplas → facts.jsonl)

| Modelo | Licença | Por quê | Risco |
|---|---|---|---|
| `Qwen/Qwen3-8B` (**recomendado**) | Apache-2.0 | IFStruct **79,75** (melhor <10B) — JSON estrito | RAM CPU: rodar em GGUF q4_k_m (~5 GB) fora do crate |
| `google/gemma-4-E4B` | Apache-2.0 | IFEval 96,7 / IFStruct 76,65 — melhor seguidor de instruções pequeno | multimodal desnecessário; RAM efetiva maior |
| `Qwen/Qwen2.5-7B-Instruct` (baseline antigo) | Apache-2.0 | maduro | superseded por Qwen3-8B |
| `fastino/gliner2.5-*` / `gliner-relex-large-v1.0` | Apache-2.0 (205–500M) | NER+relações+JSON zero-shot **em CPU** | sem raciocínio; cobertura PT não confirmada |
| `mohar07/qwen3-0.6b-kg-triplets` | MIT | receita (schema-injection + validação) | ontologia fechada 20 relações, inglês-only; reprodução contestada |

**Literatura (KG-LLM @ LREC, text2kgbench):** o fator dominante é **schema
fechado + ontologia enumerada no prompt**, mais que tamanho do modelo.
Implementação: `tools/scribe_extract.py` (criado neste ciclo) — temperatura 0,
JSON estrito com descarte auditado, NFKC, dedup `s|r|o`, authority 60–120,
proveniência `scribe:<modelo>@<revision>|src=<fonte>`.

**Escriba de regras (CNF/FOL):** não existe modelo dedicado no mercado em 2026.
Padrão: LLM pequeno + **verificador simbólico no host** (ARc/ARGOS/ILM —
8B traduz NL→formal com ICL, mas instável). Coerente com "o core não decide".

---

## 3. Ligações com o achado negativo do B-G0 (mesmo dia)

`target/b0_parafra_gate.json`: AUC **0,4945** (n=4999 pares paws-x) no espaço
do donor — o embedder não separa paráfrase adversarial (overlap léxico idêntico,
ordem diferente). Consequências:

1. Não é defeito do stack JL/HV (recall@10 = 1,0 no B1; o recall@1 0,7676 do
   A3 é excesso de capacidade — shards em curso).
2. **Não trocar de mapa esperando resolver PAWS** — é limite da família
   embedding em pares adversariais; discriminação exige outro sinal
   (cleanup/Hopfield graduado — P1 onda B — ou raciocínio explícito).
3. Teto não relaxado (anti-objetivo do plano 0476).

---

## 4. Fontes

- MTEB leaderboard: Qwen3-Embedding-0.6B (PT #2/242) · harrier-oss-v1-0.6b
- Model cards HF: microsoft/harrier-oss-v1-0.6b · google/embeddinggemma-2 ·
  nvidia/Nemotron-3-Embed-1B-BF16 · mohar07/qwen3-0.6b-kg-triplets
- IFStruct (Liquid AI): Qwen3-8B 79,75 · Gemma 4 E4B 76,65
- GLiNER2/GLiNER2.5 (fastino, Apache): github.com/fastino-ai/GLiNER2 · arXiv 2507.18546
- Regras/lógica: Survey Rule Learning LLMs (Data Intelligence 2026) ·
  ARc autoformalização (arXiv 2511.09008) · ARGOS (ICLR 2026)
- Licenças: Gemma ToU rev. 1/abr/2026 · Gemma 4 = Apache-2.0 · comparativos Qwen/Llama/Mistral

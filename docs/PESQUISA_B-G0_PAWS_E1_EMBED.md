# Pesquisa externa — B-G0 / PAWS / E1 / embed (2026-11-18)

Questão no fórum: AUC paráfrase — **ERRATA MSG 0567**: o 0,4945 era bug de
parser (labels deslocados). Alinhado = **~0,6909**. E1 ordem AUC 0,6054 fica
**abaixo** do donor puro.

## Literatura estabelecida
| Fonte | Achado | Link |
|---|---|---|
| PAWS (Zhang et al., NAACL 2019) | Dataset **desenhado** para alta sobreposição BOW + ordem diferente; BOW/cosine falha | https://arxiv.org/abs/1904.01130 |
| ColBERT (Khattab & Zaharia, 2020) | Late interaction token-level MaxSim — não colapsa em bag | https://arxiv.org/abs/2004.12832 · https://github.com/stanford-futuredata/ColBERT |
| BGE-M3 | Dense + sparse + multi-vector num só modelo | https://arxiv.org/abs/2402.03216 · https://github.com/FlagOpen/FlagEmbedding |
| HyDE / ARAGOG | Hypothetical doc embedding melhora precisão de retrieval | https://arxiv.org/abs/2212.10496 · https://arxiv.org/abs/2404.01037 |

## Em estudo / neuro-simbólico
| Fonte | Achado |
|---|---|
| arXiv:2401.06808 | VSA role–filler: `s = Σ r⊗f` |
| AID/D3 TPR 2024 | Decomposição role–filler aprendível |

## Runtimes (L3/L4 reboot)
- TEI: https://github.com/huggingface/text-embeddings-inference (Candle CUDA / ONNX CPU)
- Sentence-Transformers backends: `onnx` / `openvino`
- OpenVINO INT8 laptop (ex. BGE-M3 ~245 samp/s batch16 Arc): comunidade Meteor Lake

## Números da literatura (corroboram MSG 0520/0532)

| Modelo / setup | Dataset | Métrica | Valor |
|---|---|---|---|
| BOW | PAWS-QQP (após treino QQP+PAWS) | AUC-PR | **27,3** |
| BERT | PAWS-QQP (mesmo) | AUC-PR | **83,1** |
| BOW | PAWS-X (média langs) | AUC-PR | **48,4** |
| BERT | PAWS-X (média) | AUC-PR | **88,2** |
| LLMs zero/few-shot | PAWS-X (PARAPHRASUS 2024) | erro | perto do acaso; BERT fine-tune vence |

Nosso mapa cosine alinhado AUC **~0,69** (MSG 0567 / commit `f60f374`). O
0,4945 era artefato de parse — **não** usar como motivo de redesenho. E1 ordem
**0,6054** está ~0,09 **abaixo** do donor; a régua de mecanismo novo sobe para
~0,73 (não 0,51).

## Implicação AETHER
Não baixar teto AUC. Premissa “mapa no acaso” **retirada**. B4/NySHD ainda
não injeta ordem sintática (MSG 0539/0561), mas o motivo muda: gap até o donor
e até cross-encoder, não “cosine=acaso”. Caminho: shards ≤cap + (opt-in)
multivetor/cross no gate; E1 precisa **passar** o donor, não só o bag.

## Consequência de design (0539 → errata 0567/0577 → 0579)
- **B4 NysHD** = recuperação (C-G0 recall). Motivo “similaridade=acaso” **retirado** (era parser).
- **Label:** E1 n=5000 AUC **0,5983** < piso 0,60 e < donor 0,6909 — achado negativo; composição só vale se **ganhar** do donor.
- **Spike D1:** ColBERT-XM MaxSim + IC95 (OPENCODE 0581); régua 0,69 / 0,73; piso abstain 0,60 intacto.

## Errata 2026-11-18 (MSG 0567/0577)

- AUC 0,4945 era **bug de parser** (labels deslocados); donor real **AUC 0,6909**.
- E1 n=5000: **0,5983** (abaixo do piso 0,60) — achado negativo honesto (MSG 0579).
- Lei E1B shards imune ao bug (recall).
- Proximo probe: D1 ColBERT-XM MaxSim + IC95 (claim OPENCODE 0581).

## Guarda zip (MSG 0595/0599, tick 6)

Classe do bug 0567: `zip` silencioso truncando/deslocando. Lit:

| Fonte | Achado |
|---|---|
| [PEP 618](https://peps.python.org/pep-0618/) | `zip(..., strict=True)` → `ValueError` se lengths divergem (Py≥3.10) |
| Defensive / offensive Python | preferir crash a resultado errado em batch alinhado |

Aplicado: `scribe_extract` (fail-batch), `b0_pair_cosines`, `b0_maxsim_colbert` (fail-fast).

## R4 GPU pós-reboot (MSG 0605)

OPENCODE mediu RTX 3050: **19,33 txt/s** (bf16 batch64 n=1000) ≈ **1,79×** vs CPU A1; ETA 100k ≈ 1,44 h. GATE_A1 limpo. D1 MaxSim ainda sem artefato.

## ColBERT-XM API (MSG 0608 / ALERTA 0617)

- Paper COLING 2025: IR multilingual (mMARCO MRR@10), **não** AUC PAWS.
- ST **v6**: `MultiVectorEncoder` obrigatório; `encode_query` / `encode_document` são **assimétricos** (prefixos, caps, skiplist).
- MaxSim clássico = **soma** dos máx por token da query; `meanmaxsim` = média (bounded, melhor p/ AUC entre pares).
- Risco D1: `SentenceTransformer.encode` simétrico pode invalidar o probe — gravar `api` + `shape` + `similarity_fn` no `d1_maxsim.json`.

## R5 ONNX / TEI (MSG 0611)

- TEI 1.8: Qwen3 0.6B/4B/8B em CPU + FlashQwen3 CUDA.
- Proxy ONNX INT8 ~**2,07×** vs ONNX fp (30k docs, comunidade).
- IPEX + P-cores: latência ~30 ms (i9-13900HK) — útil p/ R3/R5, não substitui D1.
- Medir no host: ST onnx/openvino vs R4 CUDA 19,33 e A1 CPU 10,82 (mesmo n/batch/seed).
- Smoke R5 (MSG 0619): **ABSTAIN** exit 3 — ST onnx exige `optimum`; ORT 1.32 presente.
- Errata venv (MSG 0632): `pip install optimum` rebaixou transformers e quebrou ST 6.3; restaurado. Conflito residual optimum↔transformers≥5.
- Caminho (MSG 0639/0652): export ONNX manual sem optimum; **last_token** pooling (não masked_mean) → cosine médio **1,0** (gate 0,99 PASS). Taxa n128 ainda pendente em janela limpa.

## A3 / lei E1B (MSG 0624)

`b0_shard_gate` **PASS**: mono 5k recall **0,7676** vs shards s0/s1 **0,8760 / 0,8728** (Δ ~+11 pp).  
Lei E1B (≤0,4×D) **confirmada**; piso empírico ~0,85. Teto A3 0,90 ainda exige rerank float (B2). Gap: assimetria consulta vs fato concat + JL 1-bit.

## R7 re-embed shards (MSG 0642)

R7 **fechado**: 5k fatos / 2 shards ×2500, CUDA bf16 seed42 T0, donor Qwen3-0.6B.  
Recall s0/s1 **0,8760 / 0,8728** (piso 0,85). SHA recalculado bate manifesto.

## ColBERT-XM + ST6 (MSG 0643)

Confirmado no card HF e lista ST pretrained: `MultiVectorEncoder("antoinelouis/colbert-xm")` (853M, 128d, NanoBEIR 0,5915). API canônica ≠ `SentenceTransformer.encode`.

## B2 bits → float rerank (MSG 0635)

| Fonte | Achado |
|---|---|
| Qdrant BQ + oversample 3× + rescore | recall@100 ~0,98–0,9966 (OpenAI emb) |
| Azure BQ + rerank | NDCG quase recupera perda do binário |
| EMVB / PLAID | bit/centroid prefilter → late interaction |

Alinha `tools/b0_bits_f32_rerank.py`: top-k Hamming → rescore f32; gate sugerido recall@1 ≥ 0,90 ou ABSTAIN.

## D1 status (MSG 0667/0668 → 0673+)

- VERIFY 0667: sem `d1_maxsim.json` / sem processo.
- OPENCODE **assumiu** D1 via `tools/d1_maxsim_st6.py` (ST6 canônico), serial após janela R5/R3.
- Youden `t_mid` (MSG 0671): útil como limiar exploratório; IC do cutpoint e sens/spec no mesmo corpus são otimistas sem holdout. Régua = AUC meanmaxsim + IC95.
- CURSOR (0673): `--dry-check` n=5000 balance 2500/2500 OK; claim D1 **não** tocado.
- Host: PIDs `embed_rate` sumiram — **janela CPU/GPU livre** para OPENCODE rodar o probe e gravar `target/d1_maxsim_st6.json`.

## R5 ONNX probe host (artefato `target/pos_reboot/onnx_backend_probe.json`)

| Backend | txt/s (n=128 batch16 CPU) | Cosine vs torch |
|---|---|---|
| torch fp32 | **1,57** | — |
| ONNX CPU EP | **0,01** | 1,0 (gate 0,99 PASS) |

`promote_onnx=false` — fidelidade OK, taxa ~**157×** pior que torch neste export. Não promove; R5 continua ABSTAIN em throughput.

## Identidade ≠ geometria (MSG 0649, arXiv:2609.28290)

Em PAWS-X overlap-matched: bi-encoders densos AUC **0,55–0,70** (pico ~0,70); MaxSim/late fusion **não** saem dessa banda. Probe em forward **conjunto** chega **0,90–0,96**.  
Implicação D1: falhar em passar 0,6909 com MaxSim pode ser **achado negativo honesto** — não baixar teto; cross-encoder/joint fica opt-in.


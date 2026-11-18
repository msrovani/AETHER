# Pesquisa externa — B-G0 / PAWS / E1 / embed (2026-11-18)

Questão no fórum: AUC paráfrase ~0,4945 (acaso) com mapa cosine; E1 ordem AUC 0,6054.

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

Nosso mapa cosine AUC **0,4945** ≈ acaso é o comportamento esperado de bag/ordem-cego em PAWS — **não** é regressão do Qwen3. E1 ordem **0,6054** sobe na direção certa, ainda longe de encoder com estrutura.

## Implicação AETHER
Não baixar teto AUC. Mapa cego à ordem **não** fecha B-G0 em PAWS (previsto pela literatura). Caminho alinhado: E1 roles + (opt-in) late-interaction / multi-vector / pair-encoder, shards ≤cap.

## Consequência de design (OPENCODE 0539 · CURSOR ACK 0544)
- **B4 NysHD** = recuperação (C-G0 recall), **não** label PAWS — similaridade herdaria AUC≈acaso.
- **Label** = E1 + fato `parafraseia` + gate `e1_paws_consistency` (OPENCODE).
- **Spike opt-in:** BGE-M3 multivetor primeiro (ACK 0542), depois ColBERT; critério AUC≥0,60 sem baixar teto; T0 intacto.

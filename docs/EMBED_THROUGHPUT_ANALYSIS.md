# Análise — taxa embed limpa (Qwen3-Embedding-0.6B)

**Baseline A1:** `target/embed_rate_1k.json` — **1,56 txt/s** · CPU · batch=32 · dtype **default=bf16** · ETA 100k ≈ **17,8 h**  
**Probe:** `target/embed_dtype_probe.json` (n=128, facts B1, seed host, 2026-11-18)  
**Doutrina:** sidecar Python fora do crate; seed=42 · T=0 · manifest SHA; gate ETA>4h → GPU.

## 1. Diagnóstico do gargalo

| Fato | Evidência |
|---|---|
| Modelo é LLM-encoder (~0,6B), não MiniLM | Qwen3-Embedding; last-token pool + MRL 768 |
| A1 carregou **bf16 no CPU** | warning ST: *“loaded in torch.bfloat16 on CPU… significantly slower than float32”* |
| Textos curtos | chars mean≈120 · p95≈173 → ~30–45 tok; batch padding ainda cobra o pior do lote |
| Sem CUDA neste host CURSOR | `torch.cuda.is_available()=False` |
| Threads | `torch.get_num_threads()=10` |

**Causa raiz #1 (medida):** bf16 em CPU é emulado e **~6× mais lento** que fp32 no mesmo host.

### Probe dtype × batch (n=128, CPU)

| dtype | batch | txt/s | ETA 100k (h) | vs A1 1,56 |
|---|---:|---:|---:|---:|
| **bf16** | 32 | **2,04** | 13,59 | ~1,3× (n menor) |
| fp32 | 8 | 13,30 | 2,09 | ~8,5× |
| **fp32** | **16** | **14,47** | **1,92** | **~9,3×** |
| fp32 | 32 | 12,5 | 2,2 | ~8× |
| fp32 | 64 | 11,8 | 2,4 | — |
| fp32 | 128 | 11,1 | 2,5 | — |

Conclusão imediata: **só trocar para `torch_dtype=float32` no CPU** coloca ETA 100k **abaixo do teto 4 h** (~2 h) **sem GPU**. Batch ótimo neste host ≈ **16** (não 32).

## 2. Alavancas (ROI decrescente)

### L1 — CPU fp32 + batch ótimo (AGORA, sem hardware novo)
- `model_kwargs={"torch_dtype": torch.float32}` quando `device=cpu`
- Sweep batch {8,16,32,64} no artefato A1
- `torch.set_num_threads(N_fisicos)`; evitar oversubscribe com embed paralelo
- **Ganho esperado:** ~8–10× · ETA 100k ≈ **2 h** · 5k B-G0 ≈ **6 min**
- **Risco:** nenhum na doutrina (seed/T0 intactos); cosine ≈ idêntico ao bf16 para estes textos curtos
- **Lit/docs:** [ST efficiency](https://github.com/huggingface/sentence-transformers/blob/main/docs/sentence_transformer/usage/efficiency.rst) — *avoid bf16/fp16 on CPU*

### L2 — GPU 3050 + fp16/bf16 + FA2 (OPENCODE sequência 0487)
- Torch CUDA no venv; `--device cuda`; dtype **fp16 ou bf16** (no GPU aceleram)
- `attn_implementation="flash_attention_2"` se kernels ok
- Batch 32–128 até VRAM (3050 8 GB: folga larga para 0,6B)
- **Referência externa ONNX FP16:** T4 ≈ **450 txt/s** @B32 · A10 ≈ **865 txt/s** @B32 ([sinequa ONNX](https://huggingface.co/sinequa/Qwen3-Embedding-0.6B-ONNX))
- **Ganho esperado vs A1:** **50–400×** (dezenas–centenas txt/s) · 100k em minutos
- **Gate A2:** razão GPU/CPU ≥2× **ou** abstain + shards

### L3 — Backend ONNX / OpenVINO (CPU) ou ONNX-GPU
- `SentenceTransformer(..., backend="onnx")` + `pip install sentence-transformers[onnx]`
- ST v3.2+: **~2–3×** vs torch em vários modelos CPU
- Export oficial em discussão HF PR/27; TEI CPU com revision ONNX
- **Ganho esperado:** +2–3× **em cima do** L1 · risco: validar cosine vs torch fp32 (gate B-G0)

### L4 — Servidor de embeddings (TEI / Infinity / Arctic)
- [TEI](https://huggingface.co/docs/inference-endpoints/engines/tei): dynamic batching por tokens, FlashAttention, Candle
- [Arctic Inference](https://arxiv.org/abs/2507.11830): até 2,4× TEI em seq curtas (GPU)
- Sidecar HTTP local; AETHER continua sem pesos no crate
- **Quando:** rounds ≥50k recorrentes ou multi-agente competindo pelo donor

### L5 — TensorRT / TRT-LLM (só se L2 saturar)
- [cvs-health/qwen-embedding-trt](https://github.com/cvs-health/qwen-embedding-trt): BF16 obrigatório (FP16 estoura RMSNorm)
- Cuidado: reports de zeros com FP16 TRT ([NVIDIA forum](https://forums.developer.nvidia.com/t/tensorrt-produce-all-zero-output-for-qwen3-embedding-0-6b/337047))
- **Adiar** até A2 GPU medida; complexidade alta vs ROI no 0,6B

### Anti-alavancas (não fazer)
- Destilar / trocar donor sem re-rodar B-G0 e proveniência
- bf16/fp16 no **CPU** “para ir mais rápido” (faz o oposto)
- Subir `max_seq_length` / padding agressivo
- Múltiplos processos embed no mesmo host sem pin de threads (volta ao 0,19 txt/s contido)

## 3. Extrapolação de ETA (ingênua, pós-L1)

| Cenário | txt/s | 5k B-G0 | 10k shard | 100k |
|---|---:|---:|---:|---:|
| A1 bf16 CPU (hoje) | 1,56 | ~53 min | ~1,8 h | **17,8 h** |
| L1 fp32 B16 CPU | ~14,5 | **~6 min** | **~11 min** | **~1,9 h** |
| L2 GPU (ordem T4 ONNX) | ~400 | ~12 s | ~25 s | **~4 min** |

## 4. Plano de implementação sugerido

1. **Patch L1** em `scripts/embed_rate_1k.py` + `tools/donor_map_embed.py`:  
   - CPU → `torch_dtype=float32`; CUDA → `bfloat16` (ou fp16 medido)  
   - default batch CPU=16, GPU=32  
   - regravar A1 limpo `target/embed_rate_1k.json`  
2. **OPENCODE** conclui probe 3050 (0487) com mesmo seed/T0 → `target/embed_a2_gpu.json`  
3. **Opcional L3:** spike ONNX 1k vs L1; aceitar só se cosine médio ≥0,99 vs fp32  
4. Shards 10k/50k continuam path A4 até GPU estável  

## 5. Aceite

- [x] L1/L2 no código (`tools/embed_runtime.py` + scripts) — 2026-11-18  
- [x] A1 re-medido **10,82 txt/s** CPU fp32 B16 + warmup (`target/embed_rate_1k.json`)  
- [x] ETA_100k_h = **2,57** < 4 (CPU ok sem GPU neste host)  
- [ ] A2 GPU ≥2× CPU na 3050 (path pronto; CUDA ausente no host CURSOR)  
- [x] Manifest seed=42 · temperature=0  
- [ ] B-G0 não relaxa tetos  

### Defaults aplicados

| Device | dtype | batch | extras |
|---|---|---:|---|
| CPU | fp32 | 16 | `torch.set_num_threads(n_físicos)` |
| CUDA | bf16 | 64 | FA2 se disponível · cudnn.benchmark |  

**Frase:** LLM gera; AETHER cita, certifica ou abstém — o donor é sidecar medido, não fé.

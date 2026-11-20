# Otimização F2 — vega-U 0.8B + porto gemma-prose (assento AND gate)

Data: 2026-11-20 · AND gate APROVADO por HITL. Este doc lista só práticas com
evidência aceita (repo oficial, docs do PyTorch, papers/publicações e debates da
comunidade) — teorias sem prática ficam de fora. Baseline medido neste host
(3050 6GB, torch 2.14.1+cu126): **0.8B p50 111–186ms · gemma-prose p50 100.5ms ·
VRAM 1.6–2.9GB · acurácia nos rótulos ~0.50–0.52 · ECE pós-calibração 0.074–0.089**.

## 1. VELOCIDADE (o gargalo medido)

**1.1 Instalar os kernels que faltam — a prática nº 1, com evidência no log.**
O transformers avisa em toda carga: `causal_conv1d` e `flash-linear-attention`
ausentes ⇒ "falling back to its reference PyTorch implementation... much slower".
Os Qwen3.5 usam **Gated DeltaNet** (ICLR 2025, arXiv:2412.06464) e a biblioteca
oficial `fla-org/flash-linear-attention` é Triton-based.
- **triton-windows**: fork oficial (triton-lang/triton-windows assumiu do
  woct0rdho/triton-windows; thread no HN 41849718). Versão EMPARELHADA ao minor
  do torch: `pip install "triton-windows==3.8.0.*"` para torch 2.14. Exige
  VS BuildTools (C compiler) para os kernels JIT.
- **flash-linear-attention**: `pip install flash-linear-attention[cuda]` (repo
  oficial). `causal_conv1d` (1.6.x): wheels da comunidade para Windows existem
  (relatos com 1.6.1); se não houver wheel cp314/win, instalar sem ele — o conv
  é a fração menor do custo.
- Ganho esperado: honesto, NÃO os 33ms do paper (esses exigem os kernels CUDA
  completos em Linux); alvo realista **60–120ms por decisão** neste host.
- Medição: re-probe antes/depois (mesmo protocolo, n=30).

**1.2 torch.compile (inductor) no forward do reader.**
Prática aceita do PyTorch 2.x; no Windows, inductor CUDA exige Triton
(triton-windows resolve); docs oficiais cobrem Windows CPU/XPU (tutorial
`inductor_windows`). Aplicar ao forward do reader com warmup fora da medição;
**manter só se ganho ≥20%** medido (compile tem custo de warmup e pode regredir
em grafos dinâmicos — medir, não acreditar).

**1.3 O que NÃO fazer agora (registrado como evidência):**
- **Quantizar os readers atuais (NF4/bnb)**: prática aceita (QLoRA arXiv:2305.14314;
  bnb NF4 é o padrão p/ inferência 4-bit; vLLM tem plugin) mas **sem benefício
  aqui** — os readers já cabem (1.6–2.9GB de 6GB); quantização só faria sentido
  num attach futuro do Qwen3.5-4B (9.3GB bf16 → ~2.4GB NF4). Marcar como
  EXPERIMENTAL, não adotado.
- **FlashQLA (QwenLM)**: kernels fundidos oficiais para GDN — build from source,
  Linux-first. Caminho de ouro no Linux; fora do escopo Windows.
- **ONNX do engine**: o engine é 57MB — nunca foi o gargalo; útil só para
  deploy CPU-only sem torch (vegaml[onnx]).

## 2. RESOLUÇÃO (acurácia/abstenção)

**2.1 Crescer o corpus rotulado para ~500/tipo (docs do próprio vegaml: "500
usable, 1000–2000 settle").** Estamos em 68/56 (boolean/choice). Cada decisão
das cadeiras com desfecho vira rótulo (loop já definido). Efeito medido
aqui: ECE 0.2073→0.0739 com 124; e a calibração recusou-se a ajustar com <30 —
o pacote é honesto sobre amostragem.

**2.2 Heads TTT por pergunta (mode="auto") — a prática documentada para C3.**
O README adverte exatamente o nosso caso C3: *"if what distinguishes your
examples sits in a question's instructions... pass the question per example,
or every example gets an identical prompt and the head is fitted on noise"*.
Com ≥30 exemplos por pergunta, `v.fit(examples, questions)` treina heads por
pergunta com cross-validation e o `auto` só os usa **se vencerem o engine**
(nothing beats chance nunca é servido). Tentativa concreta: head `route` com
pergunta por exemplo → re-probe C3. `fit` custa segundos em CPU (features
cacheadas) — barato e reversível (`save/load_calibration`).

**2.3 Fine-tune do adapter (caminho oficial do repo, rank-32 + gate).**
`notebooks/vega_finetune.py`: extract (features cacheadas, GPU) → train
(minutos/época em features cacheadas) → eval, com **gate negatives** de corpora
não-relacionados (para o adapter aprender quando NÃO disparar — "an adapter that
always fires is not routable") + refit térmico/conformal depois. Fase 2 do
roadmap do assento, só depois de 2.1/2.2.

## 3. UTILIDADE DAS RESPOSTAS

- **Emitir o conjunto conformal + roteamento com motivo por resposta** na saída
  do AND gate (`prediction_set`, `route auto/review`, `readout`, `abstain`,
  `unbound`): a saída vira evidência auditável, não um rótulo seco.
- **mode="auto"** (default ≥0.8.0): heads onde venceram, engine no resto —
  resposta mais útil sem risco de head ruído (o próprio pacote descarta head
  no nível do acaso).
- **vegaml[mcp]**: `vegaml-mcp-server` expõe `decide()` como tool com gate —
  integração nativa com o nosso padrão (mesma arquitetura do neural-sgdb).
- **vegaml[structured]**: pydantic na folha de perguntas e na resposta validada
  — contrato tipado para a fiação no `aether-agent`.
- **vegaml-serve (`vegaml[serve]`)**: HTTP `POST /v1/decide` local com a MESMA
  resposta de `decide()` — desacopla o processo Rust do processo Python.

## 4. RECURSOS (VRAM/tempo)

- **AND gate em um processo, ambos readers na GPU**: pico estimado 0.8B (1.6GB)
  + gemma-1B (~2.9GB observado no attach; decisão ~2GB) ≈ **4–4.5GB de 6GB** —
  cabe; alternar modelos no MESMO processo evita duplicar cache de tokenizer.
- **Read-once prefix caching** (built-in): N perguntas sobre um estado custam
  ~1 leitura — as cadeiras mandam 2–3 perguntas por estado; manter o estado
  compacto (digest de certificado, ≤320 tokens de orçamento).
- **bf16 no CUDA** (capability-based policy do pacote) — já ativo.
- **CPU fallback**: funciona (298.8ms medido, calibração idêntica); GPU é sempre
  preferida no load automático.

## 5. ORDEM DE EXECUÇÃO (cada passo medido antes do próximo)

| Passo | Ação | Critério de aceite |
|---|---|---|
| P1 | triton-windows 3.8.0 + FLA[cuda] (+ causal_conv1d se wheel existir) | p50 cai; re-probe n=30 |
| P2 | torch.compile no forward | ≥20% vs P1, senão descarta |
| P3 | fit de heads por pergunta (per-example question p/ `route`) | C3 separa OU head descartado honestamente |
| P4 | corpus → ~500/tipo (loop das cadeiras) | ECE ↓; T2/C1 melhoram |
| P5 | fine-tune adapter (fase 2) | acurácia nos rótulos ↑ sem perder abstain |
| P6 | (futuro, Linux) FlashQLA/FLA CUDA completo | 33ms do paper |

## 6. Fontes

- fla-org/flash-linear-attention (repo oficial; Gated DeltaNet ICLR'25
  arXiv:2412.06464; DeltaNet NeurIPS'24 arXiv:2406.06484; GDN2 arXiv:2605.22791;
  Kimi Linear arXiv:2510.26692)
- triton-lang/triton-windows (fork oficial assumido; HN 41849718)
- PyTorch docs: inductor Windows (CPU/XPU unstable; CUDA exige Triton)
- QLoRA/NF4: Dettmers arXiv:2305.14314; HF docs bnb; vLLM bnb plugin
- FlashQLA (blog Qwen): kernels fundidos para GDN
- vegaml README/docs/vega-u.md (fit/auto/ttt/comportamento de descarte de head;
  fine-tune notebook; read-once caching; mcp/serve/structured extras)

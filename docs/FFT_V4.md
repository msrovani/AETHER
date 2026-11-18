# FFT V4 — modelo de tráfego e critério de entrada (MSG 0459)

## Diagnóstico (medido no probe)

| Grandeza | Valor |
|---|---|
| Host / n | ROCKY · L=8192 |
| B=2048 CPU | 64,5 ms (`rustfft`) |
| B=2048 GPU (kernel atual) | 379,5 ms |
| Tráfego do kernel atual | 13 passes × R/W ≈ 134 MB → **3,5 GB** |
| Banda efetiva | **9,2 GB/s** |
| Gargalo | padrão de acesso / stride — não a GPU crua |

Para razão ≥2× a GPU precisa ≤32 ms em B=2048 → orçamento de tráfego ≈ **1 GB**.

## Caminho V4

1. Loop de estágios **dentro do shader** (1 dispatch).
2. Dados residentes por linha em workgroup local.
3. Linha 8192 complexos = 64 KB > limite típico 32 KB → sub-FFT 4096 ou 2 workgroups/linha.
4. Tráfego alvo ≈ 400 MB → predição **10–25 ms** → razão **2,6–6,5×** em B≥1024.

## Critério de entrada (dono CURSOR)

* Protótipo **fora do repo** (padrão do probe) sob ACK CURSOR → OPENCODE (MSG 0462).
* AB no **mesmo processo** que `examples/fft_gpu_probe.rs` / `target/fft_gpu_probe.json`.
* **Não** alterar números do gate sem novo probe.
* Se razão ≥2× em B=1024 e B=2048 → G1 vira ACT para B grande offline; merge em `src/core/gpu/fft.rs` pelo CURSOR.
* Se falhar → **G2 ABSTAIN final** (doc já prevê).
* Invariante: **CPU rustfft** permanece a rota do laço 60 Hz.

## Decisão desta onda

Aceito o modelo de tráfego da MSG 0459. Protótipo V4 = OPENCODE fora do repo; CURSOR só faz merge após número ≥2×. Sem protótipo medido neste ciclo, o estado permanece **ABSTAIN** (não ACT).

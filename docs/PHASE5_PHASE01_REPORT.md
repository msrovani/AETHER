# AETHER — Fase 0/1: perfilamento e hardware opt-in

> Execução: 2026-11-15 · `aether_engine` v0.0.62 · Intel Core 7 240H
> (10C/16T, Windows) · `cargo test --release`, clippy all-features, fmt 0.
> Autorização: usuário (Fase 0 + Fase 1, `deny(unsafe_code)` mantido).

## 1. Fase 0 — números reais

### 1.1 Breakdown do T12 (D=160, K=6, 50 fatos, 40% de oclusão)

Cópia de perfil f32 (mesma aritmética do `FhrrStore`), 20 consultas × 200 repetições:

| Estágio | Tempo |
|---|---:|
| Pista (2×`sin_cos` + role cue) | **8.03 µs** |
| Cleanup do papel (50 × visíveis) | 2.88 µs |
| Unbind (K × D) | 0.21 µs |
| Cleanup de valores (50 × D) | 2.84 µs |
| **Soma** | **13.95 µs** |
| `FhrrStore` completo medido | 16.37 µs |

A pista é **58%** do custo: é exatamente o imposto `sin_cos` que o BAM elimina.

### 1.2 Latência Basal e afinidade (Windows, carga concorrente)

Organismo Basal (60 passos), 2.5 s por modo; carga em 16 núcleos lógicos no
modo solto e em 15 no modo fixado (reflex = core físico 9 isolado):

| Modo | p50 | p99 | p99.9 |
|---|---:|---:|---:|
| solto | 190.3 µs | 395.8 µs | **14 854.9 µs** |
| fixado | 194.4 µs | 383.2 µs | **401.7 µs** |
| redução p99.9 | — | — | **97.3%** |

O p50 fica ~2% pior (perda de migração oportunista), mas a cauda p99.9 cai
**37×**. Resultado: afinidade é útil como modo **opt-in**, não default em CI.

### 1.3 Roofline de memória para wgpu (80 passos, referência)

| N | arestas | MB (12 B/aresta) | ms CPU 50 GB/s | ms GPU 300 GB/s |
|---:|---:|---:|---:|---:|
| 512 | 262 144 | 251.7 | 5.03 | 0.84 |
| 1024 | 1 048 576 | 1 006.6 | 20.13 | 3.36 |
| 4096 | 16 777 216 | 16 106.1 | 322.12 | 53.69 |

N=4096 já move 16.1 GB por 80 passos; **< 200 µs não é suportado** pela
roofline. GPU só compensa com streaming/persistent kernels e N ≥ 512.

## 2. Fase 1 — implementação opt-in

### 2.1 `src/core/noise.rs`

- `trait NoiseSource` object-safe, `fill_into`/`fill_gaussian_into` sem alocação.
- `ChaChaNoise` determinístico (default; mesma semente ⇒ mesma sequência).
- `HardwareNoise` atrás de `--features hw-entropy`, usando o `SiliconEntropy`
  (TSC/Instant) já existente; nunca default.
- Testes: determinismo, gaussianas finitas, faixa uniforme.

### 2.2 `src/core/affinity.rs`

- `L1Budget` (conta bytes, `fits()` contra 32 KB), `AffinityReport`,
  `pin_reflex_core`, `pin_to_core`, `core_ids`, `core_plan`.
- Pinning via `aether_governor::pin_current_thread` (API segura; sem `unsafe`
  neste arquivo). Sem a feature, no-op com `pinned=false`.

### 2.3 `src/core/fhrr_bam.rs` + `examples/bam_probe.rs`

Protótipo BAM u16 reutilizando `BamHolographicStore` (LUT Q15, cleanup e
unbinding vetorizáveis) com API em radianos:

| Storage | Recall@1 | SNR | Latência | Memória | Veredito |
|---|---:|---:|---:|---:|---|
| FHRR f32 | 1.000 | 6.06 | 16.37 µs | 264 880 B | — |
| **BAM u16** | **1.000** | 5.62 | **2.04 µs** | **52 440 B** | **READY (8.02×)** |

Critério de aceite: Recall@1=1.0 **e** ganho ≥1.5× → **8.02×**, pronto para
integração na próxima rodada (sem tocar em `fhrr.rs`, sob claim da CURSOR).

**L1d:** 52 440 B > 32 KB (fica em L2). Projeção compacta D=96/K=4:
30 736 B (roles 9 600 + valores 19 200 + traços 1 536 + ψ 400) → cabe em L1d.

## 3. Gate

| Item | Resultado |
|---|---|
| `cargo fmt --all --check` | **0** |
| `cargo clippy --all-targets --all-features -D warnings` | **0** |
| `cargo test --release` | **20 blocos, 0 falhas** |
| Auditoria cognitiva | **12/12, ACQ 100.00, real_coverage 1.00** |
| `hot_path_bytes` / alocações T02 | **0 B / 0** |
| Throughput Basal | 148.8 µs, 4 944 cycles/s, **632 797 eq-t/s**, 0 allocs |
| RSS / heap | 14.36 MB / 0.48 MB |

## 4. Decisão para a Fase 2

1. **BAM:** integrar no caminho T12 na próxima rodada (arquivo `fhrr.rs` está
   sob claim da CURSOR; requer handoff). Ganho 8×, Recall 1.0, memória 5× menor.
2. **Afinidade:** manter opt-in; o p99.9 melhora 97.3% sob carga, mas o p50
   perde ~2%. Não virar default de CI.
3. **Entropia:** default determinístico; `hw-entropy` só em experimentos.
4. **wgpu:** só depois de um backend com fallback CPU e benchmark em N≥512;
   a roofline não sustenta “<200 µs” em N=4096.
5. **SINDy/JIT:** avaliador especializado primeiro; JIT continua fora do
   default (custo/`unsafe` sem gargalo comprovado).
6. **eBPF/AF_XDP/ultrassom:** tracks separados (Linux/root e demo acústico).

## 5. Coordenação

`src/core/fhrr_bam.rs` sofreu edição concorrente da CURSOR durante a sessão.
A versão dela foi preservada; apenas corrigi imports/Debug e o borrow
mecânico, e o `examples/bam_probe.rs` usa a API pública resultante. Nenhum
arquivo de `phase_memory`/`morphogenesis`/organismo foi tocado.

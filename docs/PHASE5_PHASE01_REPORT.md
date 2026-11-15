# AETHER — Fases 0/1/2 + encerramento da Fase 5

> Execução: 2026-11-15 · `aether_engine` v0.0.71 · Intel Core 7 240H
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

---

# Fase 2 — execução da sequência estipulada

> Execução: 2026-11-15 · `aether_engine` v0.0.68 · mesmo host.
> Claim da CURSOR liberado (MSG 0087, `LIBERO_CLAIM=sim`), então a integração
> no T12 deixou de ser bloqueada.

## 6. BAM integrado ao T12 da auditoria

`tests/cognitive_audit.rs::test_12_holographic_rag` passou a usar `T12Bam`
(u16 + LUT Q15) no lugar de `FhrrStore::query_into`:

| Storage no T12 | Recall@1 | SNR | Latência | Memória | Alocações |
|---|---:|---:|---:|---:|---:|
| FHRR f32 (`query_into`, otimizado pela CURSOR) | 1.000 | 5.96 | 7.55–8.0 µs | 264 880 B | 0 |
| **BAM u16 (`T12Bam`)** | **1.000** | **5.96** | **2.4–3.0 µs** | **52 440 B** | **0** |

Ganho e2e: **2.5–3.1×** contra o f32 já otimizado e **~7.5×** contra o
`query_match` original (16.0 µs). Memória 5× menor; `hot_path_allocs = 0`.

O exemplo `bam_probe` mantém a comparação lado a lado e o breakdown
(pista 8.26 µs → 0 no BAM, que não usa `sin_cos`):
`cargo run --release --example bam_probe --features m4,m5,affinity`.

## 7. Defaults opt-in (JIT / afinidade / entropia)

`Cargo.toml` passou a ter `default = ["m1".."m8"]`:

| Feature | Estado | Motivo |
|---|---|---|
| `jit` | **opt-in** | 5.24 ns vs 0.82 ns nativo; não paga o custo/`unsafe` |
| `affinity` | **opt-in** | p99.9 −97.3%, mas p50 +2% e no-op em CI |
| `hw-entropy` | **opt-in** | auditoria precisa ser reprodutível (ChaCha default) |

## 8. SINDy: avaliador especializado antes de JIT — resultado negativo

Implementado `SindyEquationDiscoverer::rhs_active` (termos ativos por equação,
potências pré-computadas, `TERM_EXP`) com teste de equivalência
`rhs ≡ rhs_active` (erro < 1e-12). Medição (2 M avaliações):

| Avaliador | ns/avaliação |
|---|---:|
| nativo fixo (referência da Fase 5) | 0.82 |
| `rhs` completo (10 termos, laço direto) | **2.42** |
| JIT Cranelift | 5.24 |
| `rhs_active` genérico (termos ativos) | **10.33** |

**Conclusão:** a indireção por tabela quebra a vetorização; o laço fixo de 10
termos é o caminho mais rápido. `rhs` permanece default, `rhs_active` fica
como API documentada (sem uso interno) e **JIT continua fora do default**.

## 9. wgpu: fechado para o laço reflexivo, com portão mensurável

Novo módulo `src/core/gpu_gate.rs` (sem dependências, sem `unsafe`) decide por
*roofline* se um workload justifica backend GPU:

- `Workload { n, steps, target_us }` → `bytes()`, `cpu_us()`, `gpu_us()`;
- `decide()` → `BelowMinN | CpuFaster | TooSlowForTarget | WorthIt`;
- `roofline_table(80)` reproduz exatamente a tabela da Fase 0.

Veredito medido: para `N ∈ {512, 1024, 4096}` e 80 passos, **todos** caem em
`TooSlowForTarget` para alvo de 200 µs (0.84 / 3.36 / 53.69 ms de GPU). O
único caso `WorthIt` é streaming longo (≥ ~2 GB de tráfego, ex.: 200 mil
passos), onde o ganho projetado é ~6× (50 → 300 GB/s).

**Decisão:** Fase 2 fecha sem backend `wgpu` no laço reflexivo — não é
justificado pelos números. O portão fica como critério de admissão para uma
frente de *streaming/batch*, que exigirá fallback CPU obrigatório e
`N ≥ 512`.

## 10. Tracks separados (não executáveis neste host)

| Track | Pré-requisito | Estado |
|---|---|---|
| eBPF / AF_XDP | Linux + root, `aya`/`libbpf` | não iniciado (host Windows) |
| Ultrassom (demo acústico) | hardware de áudio + DSP | não iniciado (fora do núcleo) |

Ambos ficam fora do gate de ACQ; exigem bancada própria e não bloqueiam
12/12.

## 11. Gate final da Fase 2

| Item | Resultado |
|---|---|
| `cargo fmt --all --check` | **0** |
| `cargo clippy --all-targets --all-features -D warnings` | **0** |
| `cargo test --release --no-fail-fast` | **21 blocos, exit 0, 0 falhas** |
| Auditoria cognitiva | **12/12 MEASURED, ACQ 100.00, real_coverage 1.00** |
| `hot_path_bytes` | **0 B** |
| T12 (BAM) | Recall 1.0, SNR 5.96, **2.4 µs**, 0 allocs |
| Throughput Basal | **143–147 µs** p50 · p99.9 418–437 µs · 6 329–6 440 cycles/s · **810–824 k eq-t/s** · 0 allocs |
| Throughput Vigilante / Crítico | 260–262 / 490–504 µs p50 · 0 allocs |
| RSS / heap | 5.6–14.3 MB / 0.48 MB |
| `#![deny(unsafe_code)]` | mantido nos arquivos novos |

---

# Encerramento da Fase 5 — `benches/hardware_extremus.rs` e CI

> Medições: `cargo bench --bench hardware_extremus` (default e
> `--features jit,affinity`). Bench já registrado em `Cargo.toml`
> (`harness = false`, `required-features = ["m1", "m5"]`).

## 12. Benchmark A — SINDy RHS e break-even do JIT

Equação de teste com **4 termos ativos** (`x' = y`; `y' = x − x³ − 0,3y`),
2 M avaliações por avaliador:

| Avaliador | ns/avaliação |
|---|---:|
| nativo especializado (referência) | **0,80–0,82** |
| interpretado (`eval_interpreted`) | **8,85–9,10** |
| JIT Cranelift (`integ.eval` com `--features jit`) | **4,98–5,46** |
| fallback com `jit` off | 8,01 (= interpretado) |

Compilação Cranelift medida (republicações com Ξ levemente diferente):

| Métrica | Valor medido |
|---|---:|
| 1ª compilação (cold start, ISA incluída) | **173–418 µs** |
| recompilação em regime | 173–418 µs (mesma ordem) |
| ganho JIT vs interpretado | 3,64–3,88 ns/avaliação |
| **`break_even_evals`** | **45 461–107 854 avaliações** |
| tempo equivalente | **0,2–0,6 ms** de avaliação contínua |

O `~622 µs` da MSG 0087 é a ordem de grandeza de um *cold start* mais frio
(ISA + primeira alocação de módulo); com ele o break-even seria
`622 µs / 3,88 ns ≈ 160 mil avaliações ≈ 0,8 ms` — mesma conclusão: o JIT só
amortiza em execuções longas, e mesmo assim perde para o avaliador nativo
especializado (0,82 ns). Por isso **`jit` segue opt-in**.

## 13. Benchmark B — p50/p99/p99.9 do ciclo reflexivo com e sem pinning

`EventDrivenCore::process`, 4 M eventos por modo (~1,2 s), melhor de 2
passadas, carga em 16 núcleos lógicos (solta) e em 14 (fixada, excluindo o
núcleo 9 e seu irmão SMT):

| Modo | p50 | p99 | p99.9 |
|---|---:|---:|---:|
| solto | 0,30–0,50 µs | 0,40–0,60 µs | **0,50–0,80 µs** |
| fixado (core físico 9) | **0,30 µs** | **0,40 µs** | **0,40 µs** |
| redução p99.9 | — | — | **20%** |

Sem `--features affinity` o pinning é no-op (`pin_ok=false`) e a redução é
**0%** — é exatamente o contrato do caminho opt-in.

No ciclo reflexivo isolado a cauda é sub-microssegundo e dominada por
interrupções/DPC; o ganho de pinning aparece em escala maior no **ciclo Basal
completo** (Fase 0): p99.9 **14 854,9 µs → 401,7 µs (−97,3%)** com p50 apenas
~2% pior. Conclusão mantida: afinidade é opt-in, valiosa sob carga real.

## 14. Benchmark C — BAM u16 vs FHRR f32 (D=160, 256, 512)

50 fatos, K=6, 40% de oclusão, ±0,25 rad, n=2 000:

| Storage | Recall@1 | p50 | p95 | p99 |
|---|---:|---:|---:|---:|
| BAM D=160 | 1,000 | **2,00–2,20 µs** | 2,10–3,20 | 2,30–3,70 |
| BAM D=256 | 1,000 | **3,10–3,20 µs** | 3,30–3,90 | 4,10–5,10 |
| BAM D=512 | 1,000 | 5,90–6,00 µs | 7,00–7,20 | 9,70–9,90 |
| FHRR f32 D=160 | 1,000 | 7,50–7,90 µs | 9,00–11,80 | 13,00–16,20 |
| FHRR f32 D=256 | 1,000 | 12,50 µs | 14,00–15,40 | 18,10–24,60 |

Ganho p50: **×3,4–4,0** em D=160 e D=256; memória 52 440 B vs 264 880 B.
Micro-operações: `sin_cos` LUT **×16** mais rápido que `f32` (0,48 vs 7,7 µs
por 256 fases); bind 16–18 ns; unbind 17–18 ns; similaridade 75–79 ns.

## 15. Blindagem contra regressão silenciosa

1. **Testes do JIT** (`tests/phase5_hardware.rs`): com a feature ligada,
   `#[cfg(feature = "jit")] assert!(jit_compiled_in() && compiles > 0)` em
   `sindy_descobre_duffing_e_jit_publica_sem_interromper` e em
   `jit_hot_swap_sob_avaliacao_continua_nunca_rasga_estado`. Cair no
   interpretador com `--all-features` agora **falha o teste**.
2. **CI em duas matrizes obrigatórias** (`.github/workflows/ci.yml`, Linux +
   Windows em ambas):
   - **matriz 1** (build padrão, `#![deny(unsafe_code)]`):
     `cargo fmt --all --check` · `cargo clippy --all-targets -- -D warnings` ·
     `cargo test --release`;
   - **matriz 2** (`--all-features`):
     `cargo clippy --all-targets --all-features -- -D warnings` ·
     `cargo test --release --all-features`.
   Mantidos os jobs de feature isolada (m1..m8) e organismo headless.
3. **fmt do repositório**: `cargo fmt --all` foi aplicado; `src/bin/
   aether_dashboard.rs` e `src/dashboard/*` estavam fora do formato e
   quebravam a matriz 1 (aviso para a CURSOR).

## 16. Gate final da Fase 5

| Item | Resultado |
|---|---|
| `cargo fmt --all --check` | **0** |
| `clippy --all-targets` (default) | **0** |
| `clippy --all-targets --all-features` | **0** |
| `cargo test --release` | **21 blocos, exit 0** |
| `cargo test --release --all-features` | **22 blocos, exit 0** (inclui `phase5_hardware` com 10 testes e os guards de JIT) |
| `cargo bench --bench hardware_extremus` | A/B/C medidos acima; JSON em `target/aether_bench_hardware.json` |
| Auditoria cognitiva | **12/12, ACQ 100.00, real_coverage 1.00, hot_path_bytes 0** |
| Throughput Basal (pós-gate) | 147,2 µs p50 · p99.9 423,4 µs · **810 100 eq-t/s** · 0 allocs |

**Fase 5 encerrada sem pendências**: bench A/B/C medido e versionado, JIT e
afinidade blindados por CI + asserts, e nenhum claim aberto.

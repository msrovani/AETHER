# AETHER — Auditoria Cognitiva Final (12/12 medidos)

> Execução: 2026-11-15 · crate `aether_engine` v0.0.57 · Intel Core 7 240H.
> Comando: `cargo test --release --test cognitive_audit -- --nocapture`.
> Este arquivo é o fechamento do sprint; o relatório parcial do Bloco 1
> (`docs/COGNITIVE_AUDIT_REPORT.md`, CURSOR/BUFFY) descreve um estado anterior.

## 1. Resultado global

| Métrica | Valor |
|---|---:|
| **AETHER Cognitive Quotient** | **100.00** |
| **real_coverage** (12/12 `MEASURED`) | **1.00** |
| **hot_path_bytes** (regiões armadas) | **0** |
| Throughput Basal | **804.7k eq-t/s** |
| RSS pico / heap pico | **14.34 MB / 0.48 MB** |

## 2. Dimensões (todas `MEASURED`, nota 100)

| ID | Mecanismo real | Medida |
|---|---|---|
| T01 | `OnlineKoopmanOperator` (RLS/Sherman-Morrison) + embedding de atraso | R² 1.0000, NRMSE 0.0008; lifted NRMSE 0.0017, 0 allocs |
| T02 | wave + solver em 10k ciclos hostis | 0 alocações, 0 NaN/Inf, ρ 2.0 |
| T03 | RLS com esquecimento após ruptura A→B | erro 2.866 → 0.000155 (**99.99%**) em 150 ciclos |
| T04 | poda por quota + `SwarmSpeciationEngine` (Φ_niche, papéis, frames 72 B CRC-32) | 3 papéis, Φ 1.31, esparsidade 0.086→0.581, fiação −54.1% |
| T05 | CBF de Lyapunov (projeção ortogonal) + Kuramoto + M5 | 0 violações, Vmax 1.0, R>0.92 em 3.1 ciclos |
| T06 | RG 64→16→4 com torque top-down + EMA τ=100 ms + 4 nós UDP | RG align 0.983, torque 0.967→1.000, chattering contínuo 0.0598 |
| T07 | `ActiveInferenceEngine` (G = custo − λU) + Bayes | U_B 0.05 em 5 passos, 19 visitas, esforço −94.4% |
| T08 | FFT + FHRR + homeostase + sonhador | composto 100%, **1.00 ms** (4 amostras/40 passos) |
| T09 | SINDy/STLSQ + bSB MAX-CUT | erro 0.0004; razão 1.0000 em 20 grafos N=64 |
| T10 | FHRR + gate warrant-based; U de Langevin | chave ausente: sim 0.0, abstenção; U ruído 0.966 |
| T11 | RG macro persistente + cleanup memory | erro 0.0068 rad; drift RG 0.0016 rad; 5/5 |
| T12 | FHRR D=160, K=6, cleanup O(D) | Recall@1 1.0, SNR 5.88σ, **13.5 µs**, 0 allocs |

## 3. Throughput (`cargo bench --bench throughput_tps`)

| Regime | p50 µs | p99 µs | cycles/s | SOP/s | **eq-t/s** | allocs |
|---|---:|---:|---:|---:|---:|---:|
| Basal | 146.5 | 311.7 | 6.286 | 3.16e9 | **804.7k** | 0 |
| Vigilante | 268.0 | 419.3 | 3.615 | 3.60e9 | **462.7k** | 0 |
| Crítico | 508.7 | 791.2 | 1.878 | 3.71e9 | **240.4k** | 0 |

Heap pico 0.48 MB · RSS 14.34 MB · **0 alocações** no `step_into`.
Suíte completa: **19 alvos, 0 falhas**; clippy release `-D warnings` verde.

## 4. Módulos implementados

| Arquivo | Conteúdo | Referências |
|---|---|---|
| `src/core/fhrr.rs` | FHRR bind/unbind Hadamard, K traços com fase por fato, cleanup, gate seletivo | Plate 2003; arXiv:2111.06077; arXiv:2001.11797 |
| `src/core/koopman_sindy.rs` | Koopman online (RLS/Sherman-Morrison + forgetting), delay embedding, SINDy STLSQ e streaming | Koopman 1931; Brunton et al. PNAS 2016; Ljung |
| `src/core/active_inference.rs` | `ActiveInferenceEngine` (G), CBF de Lyapunov, EMA, `classify_swarm` | Friston 2010; Ames et al. 2019; arXiv:2203.00211 |
| `src/core/renormalization.rs` | hierarquia Micro/Meso/Macro 64→16→4, torque top-down, alinhamento | Wilson 1971; Kadanoff 1966 |
| `src/core/swarm_speciation.rs` | especiação e papéis ecológicos (CURSOR) | — |
| `src/simulation/closed_loop_organism.rs` | atuador contínuo com inércia EMA (BUFFY) | — |

## 5. Coordenação (fórum)

* MSG 0059/0060/0062 — BUFFY: kernel matvec, `activity_gate`, piso M7, atuador.
* MSG 0071 — CURSOR reivindica o sprint dos três módulos e do `cognitive_audit.rs`.
* MSG 0072 — OPENCODE faz handoff e aponta o build vermelho (`stlsq_ws`).
* MSG 0073 — fechamento: 12/12, ACQ 100, correções mecânicas no teste e no merge
  do bench; clippy release verde.

## 6. Reprodução

```powershell
cargo test --release --test cognitive_audit -- --nocapture
cargo clippy --all-targets --release -- -D warnings
cargo bench --bench throughput_tps
```

`target/aether_scorecard.json` consolida as 12 notas, `real_coverage`, o
`hot_path_bytes` e o bloco de throughput.

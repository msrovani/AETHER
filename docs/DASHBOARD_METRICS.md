# Métricas do dashboard AETHER (canônico)

Fonte de verdade textual alinhada a `src/dashboard/catalog.rs`. UI e docs devem citar a mesma narrativa.

| Métrica | Significado | Unidade | Bom | Ruim / atenção |
|---|---|---|---|---|
| **ACQ** | Média T01–T12 — inteligência operacional do motor | 0–100 | ≥ 99 | < 90: suite incompleta ou regressão |
| **real_coverage** | Fração de dimensões MEASURED (não PROXY) | 0–1 | 1.00 | < 1: auditoria parcial |
| **hot_path_bytes** | Bytes alocados no ciclo cognitivo armado | B | 0 | > 0: jitter / imprevisibilidade |
| **# dimensões** | Cards T01…T12 no scorecard | count | 12 | < 12: `PARTIAL_AUDIT` |
| **T01** | Inferência contínua (Koopman/RLS) | score 0–100 | alto R² / baixo NRMSE | previsão fraca |
| **T02** | Confiabilidade sob ruído | score | 0 NaN / 0 alloc em stress | instabilidade numérica |
| **T03** | Aprendizado online pós-ruptura | score | reaprende sem gradiente | trava no regime antigo |
| **T04** | Autoevolução estrutural | score | topologia viva | colapso / silêncio |
| **T05** | Autocorreção (CBF / resync) | score | estabiliza após choque | runaway |
| **T06** | Coerência multi-escala + EMA | score | R alinhado, sem chatter | tremor no atuador |
| **T07** | Busca epistêmica (U) | score | exploração dirigida | passeio aleatório |
| **T08** | Resposta multimodal | score | pipeline composto preciso | elo quebrado |
| **T09** | Matemática descoberta (SINDy/bSB) | score | leis esparsas úteis | descoberta fraca |
| **T10** | Resistência a alucinações | score | confiança→0 sem warrant | afirma sem chave |
| **T11** | Imunidade a desvio de intenção | score | meta estável sob ruído | drift |
| **T12** | RAG holográfico (BAM/FHRR) | score + µs | Recall@1 alto, latência baixa | esquece / lento |
| **p50** | Latência mediana do ciclo | µs | fluidez (~1/cycles) | — |
| **p99.9** | Cauda quase extrema | µs | máquina quieta ~centenas µs | > 1000 µs Basal → contenção SO |
| **cycles/s** | Quadros cognitivos / s | 1/s | Basal ~ meta 60 Hz | cauda domina |
| **eq-t/s** | Tokens equivalentes ≈ cycles/s × 128 | 1/s | comparativo (não tokens reais) | — |
| **SOP/s** | Ops sinápticas/espectrais / s | 1/s | densidade de trabalho | — |
| **TOP/s** | SOP/s ÷ 1e12 | TOP/s | comparar regimes | — |
| **hot_path_allocations** | Allocs no bench | count | 0 | > 0 |

## Gaps heurísticos (`Gap.code`)

| Código | Severidade típica | Remediação curta |
|---|---|---|
| `SCORECARD_MISSING` | aviso/info | Rodar auditoria / benchmark |
| `PARTIAL_AUDIT` | aviso | Auditoria completa (12 dims) |
| `TAIL_LATENCY_HIGH` | aviso | Máquina quieta + affinity |
| `L1D_PRESSURE` | info | Esperado (BAM ~52 KB > L1d 32 KB) |
| `AFFINITY_OFF` | info | `--features affinity` se quiser pin |
| `TOOLCHAIN_PATH` | crítico/aviso | PATH MinGW / `scripts/cargo.ps1` |
| `NO_LAN_PEERS` | info | Segundo nó na LAN para R(LAN) |

## Arquivos gerados

| Arquivo | Origem |
|---|---|
| `target/aether_scorecard.json` | `cognitive_audit` (+ merge throughput) |
| `target/aether_bench_throughput.json` | `throughput_tps` |
| `target/aether_host_report.json` | botão **Sondar hardware** |

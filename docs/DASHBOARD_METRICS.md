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

## Memória semântica (F4)

Lido de `data/embed_eval_last.json`. O hub não recalcula o gate.

| Medidor | Campo | Leitura atual (v2, MiniLM 384d) |
|---|---|---|
| **BM25** | `systems.lexical_bm25.hit5` | 0,8662 |
| **Embed** | `systems.embed_minilm.hit5` | 0,6761 |
| **Híbrido** | `systems.hybrid_rrf60.hit5` | 0,9225 · REPROVADO |
| **Gate** | `gate.hit5_min` | 0,974 inalterado |

A linha v3 cita a sanção MSG 1088 e os artefatos `target/embed_eval_*.json`. Medições (mesmo SHA, gate 0,974): BGE-M3 denso 0,9366; Qwen3-Emb 0,8451; BGE-rerank 0,9437; Qwen3-Reranker-0.6B **0,9577** (melhor). **HITL 2026-11-21 (MSG 1093): F4 CONGELADA REPROVADA** — V3 esgotada; cadeira fechada; sem baixar teto nem reescrever ouro.

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
| `data/manifest.json` | ETL doação (origem) |
| `target/aether_transplant_hv.json` | probe INTAKE (gates ρ/R@10/lat) |
| `target/aether_e3.json` | E3 geometria / analogia |
| `target/aether_transplant_uso.json` | usabilidade transplante U1–U6 |
| `target/aether_api_eval.json` | usabilidade API (exact/ruído/paráfrase/caso/1char/prefixo/escopo/chat) |
| `target/aether_agent_drive.json` | test drive ask Brasil/Atlântida (cite vs abstain) |
| `data/rounds/20261117_b1/manifest.json` | rodada B1 preferida no loader do hub |

## Doação / Transplante (painel Lab)

| Medidor | Significado | Bom |
|---|---|---|
| **Doador** | Mapa HF (ex. Qwen3-0.6B) + revision | Apache-2.0 |
| **Átomos · fatos** | Contagem no lote ETL | = labels no ledger |
| **dim** | `dim_in` → `dim_hv` (768→10240) | AETHV1 |
| **Conflitos** | detectados / plantados | ≥ 95% |
| **HvBank / T0 / Auth** | destino: bits T1/T2; organismo intacto; cap 240 | T0 intacto |
| **Probe / ρ / R@10 / lat** | gates INTAKE | pass · ρ≥0,95 · R@10≥0,90 · ≤1,5 ms |
| **E3 ρ / Analogia** | geometria JL + analogia FHRR | ρ≥0,95 · R@1 alto |
| **Chip USO** | resume pass_all (grade na aba Testes IA) | PASS |

## Testes IA (aba)

| Bloco | Artefato / job | Resultado esperado |
|---|---|---|
| Transplante USO U1–U6 | `UsoRegen` → `aether_transplant_uso.json` | 6/6 PASS |
| API usabilidade | `ApiEval` → `aether_api_eval.json` | exact+noise hit 100%; paráfrase/caso/1char/prefixo/escopo abstêm 100% |
| AgentDrive | `AgentDrive` → `aether_agent_drive.json` | Brasil ≠ abstain; Atlântida = abstain |
| Contrato U1–U13 | hover Detalhe (MSG 0449) | AETHER cita/certifica/abstém — não alucina |

North star (MSG 0449): false-act=0; abstain OOD ≥90%; 0 efeito sem HITL; respostas citáveis quando age.

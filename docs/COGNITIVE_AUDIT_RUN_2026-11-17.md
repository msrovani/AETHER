# AETHER — Reexecução da Auditoria Cognitiva (2026-11-17)

> Registro de execução da suíte que valida o motor como **IA equivalente** pelas
> 13 dimensões de [`COGNITIVE_BENCHMARK_SUITE.md`](../COGNITIVE_BENCHMARK_SUITE.md):
> 12 testes cognitivos (`tests/cognitive_audit.rs`) + vazão
> (`benches/throughput_tps.rs`).
>
> **Resultado: ACQ 100.00 · 12/12 `MEASURED` · `real_coverage = 1.00` ·
> `hot_path_bytes = 0`.** Suíte completa: **52 binários de teste / 318 testes /
> 0 falhas**.
> Strict mode: **12/12**, exit 0.
>
> Execução: 2026-11-17 01:30–01:32 (-03:00) · crate `aether_engine` v0.0.143 na
> auditoria e v0.0.144 no bench (o ciclo automático incrementou a versão durante
> a sessão) · Intel Core 7 240H (10C/16T) · Windows 11 Pro 10.0.26300 ·
> rustc 1.100.0. Este é um **registro de execução**; o fechamento histórico do
> sprint continua em [`COGNITIVE_AUDIT_FINAL.md`](COGNITIVE_AUDIT_FINAL.md).

## 1. Comandos

```powershell
# 1) auditoria cognitiva (12 dimensões + scorecard)
cargo test --release --test cognitive_audit -- --nocapture --test-threads=1

# 2) vazão (mescla o bloco "throughput" no scorecard)
cargo bench --bench throughput_tps

# 3) suite completa (gates E/F/M + invariantes + borda)
cargo test --release

# 4) gate rígido: metas de capacidade viram falha de CI
$env:AETHER_AUDIT_STRICT = "1"
cargo test --release --test cognitive_audit -- --test-threads=1
```

**Ordem importa:** `write_scorecard` reescreve `target/aether_scorecard.json` e
remove o bloco `throughput`; o bench é quem o mescla de volta. Rode o audit
primeiro e o bench depois se quiser o JSON consolidado.

## 2. Resultado global

| Métrica | Valor |
|---|---:|
| **AETHER Cognitive Quotient** | **100.00** |
| Quociente ponderado por confiança | **100.00** |
| `real_coverage` (12/12 `MEASURED`) | **1.00** |
| `hot_path_bytes` (regiões armadas) | **0** |
| Strict mode (metas como gate) | **false** na execução canônica; **true** passou 12/12 |
| Heap pico / RSS pico | **0,72 MB / 14,21 MB** |
| Alocações no hot-path (`step_into`) | **0** |

## 3. Dimensões cognitivas

Todas `MEASURED` (nenhuma `PROXY`/`PARTIAL`), nota 100.00. Métricas lidas de
`target/aether_scorecard.json`.

| ID | Dimensão | Critério da spec | Medido nesta execução |
|---|---|---|---|
| T01 | Inferência contínua (VdP, H=15) | R² ≥ 0.95, NRMSE < 0.05 | R²=1.0000, NRMSE=0.000824; lifted H=5 NRMSE=0.0017, 0 allocs |
| T02 | Confiabilidade (10k ciclos, SNR 5 dB, 30% perda) | 0 panic/NaN/Inf, 0 alocação | 10.000 ciclos, 0 allocs, 0 nonfinite, ρ(Gershgorin)=2.0 |
| T03 | Aprendizado online sem backprop | erro cai ≥ 80% em 150 ciclos | 2.8663 → 0.000155 (**99.99%**) em 150 ciclos |
| T04 | Autoevolução estrutural (6 nós × 800 ciclos) | Φ sobe, esparsidade cresce, ≥ 2 papéis | Φ_niche=1.31; custo 907→416 (−54,1%); esparsidade 0.086→0.581; 3 papéis |
| T05 | Autocorreção sob falha crítica | CBF sem violação; R > 0.92 em < 40 ciclos | 0 violações; R 0.952 → mínimo 0.806 → **0.968 em 9,2 ciclos** |
| T06 | Coerência multi-escala e enxame | R_align > 0.85; chattering baixo | R_align=0.901; RG_align=0.983; chattering contínuo=0.058 |
| T07 | Exploração epistêmica | U da zona B < 0.20 | U_B=0.05 em 5 passos; 19 visitas; esforço −94,4%; 0 violações CBF |
| T08 | Respostas complexas multi-modais | precisão > 90% em < 1,5 ms | composto=1.000; **0,319 ms** |
| T09 | Matemática (SINDy + MAX-CUT) | coef. < 0.04; razão ≥ 0.96 | erro=0.000357; MAX-CUT=1.0000 em 20 grafos N=64 |
| T10 | Resistência a alucinações | U > 0.75, sim < 0.15, ação neutra | U(ruído)=0.966; sim(chave ausente)=0.000; U(contradição)=0.938 |
| T11 | Imunidade a thought drift (2000 ciclos) | desvio < 0.20 rad; 5/5 memórias | drift RG=0.0016 rad; recall=0.0068 rad; cleanup 5/5 |
| T12 | RAG holográfico (50 fatos, 40% oclusão) | Recall@1 ≥ 0.90; SNR > 3σ; < 50 µs | Recall@1=1.000; SNR=5.96σ; **1,90 µs**; 0 allocs |

## 4. Vazão (`throughput_tps`, 5 s/regime)

| Regime | p50 | p95 | p99 | p99.9 | cycles/s | SOP/s | eq-t/s | allocs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Basal (60) | **128,3 µs** | 165,7 | 283,3 | 590,0 | 8.320 | 4,183e9 | **1,065e6** | 0 |
| Vigilante (120) | **140,8 µs** | 251,8 | 295,4 | 698,5 | 6.033 | 5,999e9 | **7,722e5** | 0 |
| Crítico (240) | **250,8 µs** | 469,1 | 580,1 | 973,6 | 3.327 | 6,578e9 | **4,258e5** | 0 |

`eq-t/s = cycles/s × 128` é métrica **comparativa** (1 ciclo ≡ 128 tokens
equivalentes), não tokens reais. Latência e RSS dependem da máquina e da carga;
esta tabela é a referência desta execução, não um piso universal.

## 5. Suite completa e strict mode

```text
cargo test --release                  -> 52 binários, 318 testes, 0 falhas (exit 0)
AETHER_AUDIT_STRICT=1 (cognitive)     -> 12/12, 0 falhas (exit 0)
```

A suíte completa cobre, além das 12 dimensões, os gates E1–E8, F1–F5, M3/M5/M8,
invariantes físicos/termodinâmicos, zero-alocação, borda HTTP/API e CLI. São 52
binários de teste (lib + integração + doctests); o número de testes cresce com o
repositório — **318** é o total observado nesta execução.

## 6. Artefatos

| Caminho | Conteúdo |
|---|---|
| `target/aether_scorecard.json` | 12 notas, `quotient`, `real_coverage`, `hot_path_bytes` + bloco `throughput` |
| `target/aether_bench_throughput.json` | vazão bruta (p50/p95/p99/p99.9, cycles/s, SOP/s, eq-t/s, RSS) |
| `target/cognitive_audit_2026-11-17.log` | tabela completa do audit (`--nocapture`) |
| `target/throughput_2026-11-17.log` | tabela do bench |
| `target/full_test_run.log` | resumo dos 52 binários da suíte completa |

`target/` é ignorado pelo git: os logs são evidência local reproduzível, não
artefatos versionados.

## 7. Limites e ressalvas

1. **Auditar ≠ aprovar.** Por padrão a suíte registra a nota e emite
   `[AUDIT][WARN]` quando uma meta de capacidade não é atingida; só invariantes
   de segurança (NaN/Inf, alocação no hot-path, forma) são `assert!` sempre.
   `AETHER_AUDIT_STRICT=1` transforma as metas em gate — e passou 12/12.
2. **Ordem audit → bench.** O audit sobrescreve o scorecard e descarta o bloco
   `throughput`; rode o bench depois para consolidar.
3. **Oscilação entre execuções.** T05/T06/T12 usam threads, UDP e amostragem;
   os valores brutos variam (ex.: T05 recuperou em 9,2 ciclos aqui e 13,1 na
   primeira execução do mesmo dia), mas as notas e os critérios se mantêm.
4. **O que este resultado cobre.** Ele mostra que os mecanismos físicos do motor
   existem e atingem os critérios medidos neste hardware — inferência, memória
   holográfica, abstenção, autocorreção, aprendizado online, matemática e vazão.
   **Não** é uma prova de equivalência geral a um LLM/AGI: as dimensões são
   traduções físicas específicas (RAG → FHRR, alucinação → falso atrator,
   throughput → `eq-t/s`), não uma bateria de linguagem natural.
5. **Sem SGDB nesta execução.** Não havia `AETHER_SGDB_URL`/`--stdio`
   configurado; o ritual de memória rodou em `--mock` com 0 hits. Nenhuma `key`
   do `neural-sgdb` é citada neste relatório.

## 8. Reprodução mínima

```powershell
cargo test --release --test cognitive_audit -- --nocapture --test-threads=1
cargo bench --bench throughput_tps
cargo test --release
```

Saída esperada: tabela das 12 dimensões + `AETHER Cognitive Quotient` no
terminal, `target/aether_scorecard.json` atualizado e o bloco `throughput`
mesclado após o bench.

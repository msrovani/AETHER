# AETHER — Relatório da Auditoria Cognitiva (medido)

> Execução: 2026-11-15 · crate `aether_engine` v0.0.37 · Intel Core 7 240H ·
> release (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`).
> Especificação: [`COGNITIVE_BENCHMARK_SUITE.md`](../COGNITIVE_BENCHMARK_SUITE.md).

## 1. Comandos

```powershell
# notas + tabela + target/aether_scorecard.json
cargo test --release --test cognitive_audit -- --nocapture

# latência/vazão + target/aether_bench_throughput.json (mesclado no scorecard)
cargo bench --bench throughput_tps

# opcional: transforma as metas aspiracionais em gate (falha o CI)
$env:AETHER_AUDIT_STRICT = "1"
cargo test --release --test cognitive_audit -- --nocapture
```

## 2. Resultado global

| Métrica | Valor |
|---|---|
| **AETHER Cognitive Quotient** (média das 12 dimensões) | **83.45** |
| Quotient ponderado por confiança da medição | 86.90 |
| Cobertura (1.0 = tudo medido diretamente) | 0.49 |

### 2.1 Notas por dimensão

| ID | Dimensão | Nota | Proveniência |
|---|---|---:|---|
| T01 | Inferência contínua (VdP + portadora, H=15) | 100.00 | PROXY |
| T02 | Confiabilidade — 10.000 ciclos hostis | **100.00** | **MEASURED** |
| T03 | Aprendizado online sem backprop (A→B) | 100.00 | PROXY |
| T04 | Autoevolução estrutural (6 nós × 800 ciclos) | **0.00** | PARTIAL |
| T05 | Autocorreção sob falha crítica | 100.00 | PARTIAL |
| T06 | Coerência de enxame (4 nós) e chattering | 100.00 | PARTIAL |
| T07 | Exploração epistêmica (U do sonhador) | 100.00 | PROXY |
| T08 | Respostas complexas multi-modais | 80.06 | PARTIAL |
| T09 | MAX-CUT bSB (SINDy ausente) | 100.00 | PARTIAL |
| T10 | Resistência a alucinações | 71.38 | PROXY |
| T11 | Imunidade a desvio de pensamento | 74.99 | PARTIAL |
| T12 | RAG holográfico (50 fatos) | 75.01 | PARTIAL |

`MEASURED` = mecanismo do motor; `PROXY` = mecanismo físico mais próximo +
instrumento do harness; `PARTIAL` = parte do cenário não existe no crate.

## 3. O que a especificação pede × o que o crate tem

O crate implementa **8 módulos** (Ising/bSB, onda espectral, Kuramoto UDP,
organismo, memória de fase, sonhador termodinâmico, morfogênese, transdutor).
Não existe, em `src/` nem nos contratos:

| Subsistema citado na especificação | Situação |
|---|---|
| `OnlineKoopmanOperator` | ausente — T01 usa readout ridge do harness sobre features do `WaveInterferenceBlock` |
| FHRR binding/unbinding | ausente — T08/T12 usam recuperação associativa de fase do M5 |
| SINDy | ausente — T09 mede só a parte B (MAX-CUT real) |
| `ActiveInferenceEngine` / `λ_epistemic` | ausente — T07 mede separação de incerteza do sonhador |
| Grupo de Renormalização (60/15/3.75 Hz) | ausente — T06 mede R de 4 nós UDP |
| Barreira CBF / Lyapunov `V(z) ≤ Vmax` | ausente — T05 mede ressincronização física |
| Papéis `Sentinel`/`Architect`/`Regulator` | ausentes — T04 contabiliza 0 |
| Diretório `aether_core/` | não existe; o crate é `aether_engine` na raiz |

Nenhum desses subsistemas foi simulado com números inventados. Onde a
capacidade não existe, a dimensão é marcada e a nota cobre apenas o que foi
medido.

## 4. Achados que exigem atenção

1. **T04 — colapso topológico.** A morfogênese poda *todas* as sinapses no
   ciclo **64** (esparsidade 0.083 → 1.000, ρ = 0). Poda sem piso de
   sobrevivência destrói a função: a nota é **0** por projeto do critério
   (`alive = sparsity < 0.999 && ρ > 1e-3`). Um bloco de 50 ciclos de silêncio
   já é suficiente para matar a rede com `k_decay = 1.2`.
2. **T10 — falso atrator parcial.** Com chave inexistente (fases aleatórias +
   40% de oclusão), a similaridade máxima com os 5 padrões gravados foi
   **0.324**, acima do alvo 0.15. A incerteza de Langevin para ruído branco é
   alta (0.966), mas a memória de fase ainda "encaixa" parte da cena.
3. **T11 — esquecimento parcial.** Após 2.000 ciclos de distração, o pior erro
   de recall foi **0.257 rad** (alvo < 0.15). A fase macroscópica resistiu
   (desvio 0.002 rad), mas uma das 5 memórias degradou.
4. **T12 — latência.** `Recall@1 = 1.000` e SNR = 12.4σ, porém a consulta
   custa **184 ms** em N=200 (O(N²·steps) com `sin` em libm), não 50 µs. O
   alvo da especificação está ~3.600× abaixo do fisicamente possível nesta
   implementação.
5. **T08 — custo do estímulo composto.** Precisão composta 100%, tempo
   **520 ms** (medido em paralelo com outros testes; 178 ms quando executado
   sozinho) contra o alvo de 1.5 ms. O gargalo são 4 recalls de 300 passos.
6. **T05/T06** — a física de fase é sólida: R volta a >0.92 em **4.5 ciclos**
   após uma perturbação severa (ξ=120), e a memória recupera 100% das fases
   corrompidas. O que não existe é a barreira formal de segurança.

## 5. Throughput medido (`cargo bench --bench throughput_tps`)

Quadro completo do organismo (3 nós UDP + onda 8×128 + colapso 64-dim),
5 s por regime, `step_into` com buffers pré-alocados:

| Regime | Passos | p50 µs | p95 µs | p99 µs | p99.9 µs | cycles/s | SOP/s | **eq-t/s** | allocs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Basal | 60 | 147.5 | 402.4 | 698.6 | 1201.5 | 4.925 | 2.48e9 | **630.419** | 0 |
| Vigilante | 120 | 540.8 | 1260.4 | 1609.8 | 2850.4 | 1.528 | 1.52e9 | **195.642** | 0 |
| Crítico | 240 | 1892.0 | 2831.1 | 3409.4 | 4519.2 | 545 | 1.08e9 | **69.757** | 0 |

- **Heap pico:** 0.48 MB · **RSS pico do processo:** **5.52 MB**.
- **Alocações no hot-path (`step_into`): 0** em todos os regimes.
- `eq-t/s = cycles/s × 128` (definição da especificação).

O regime Basal entrega ~630 mil `eq-t/s` com 5.5 MB de RSS; o Crítico troca
vazão por profundidade de relaxamento.

## 6. Metodologia

- **Alocação zero:** `CountingAllocator` global com contagem *thread-local*
  (os testes rodam em paralelo). O laço de T02 e o `step_into` do bench são
  medidos com o contador armado.
- **Latência:** `Instant` por ciclo; percentis sobre todas as amostras de 5 s;
  `eq-t/s` derivado dos ciclos efetivamente concluídos.
- **RSS:** `K32GetProcessMemoryInfo` → `PeakWorkingSetSize` (fallback:
  `VmHWM` no Linux).
- **Notas:** `100 · clamp(medido/alvo)`; metas da especificação são
  aspiracionais. Invariantes de segurança (NaN/Inf, forma, alocação zero,
  simetria de J) são `assert!` sempre.
- **Timing em testes:** T08/T12 medem *wall-clock* com os 12 testes em
  paralelo; para números estritos use `cargo test --release --test
  cognitive_audit -- --nocapture --test-threads=1` (o bench é sempre
  single-thread).

## 7. Próximos passos para fechar as lacunas

1. **M9 (Koopman online):** operador linear atualizado por Sherman-Morrison
   sobre o embedding de atraso; T01 deixa de ser PROXY.
2. **M10 (FHRR):** binding/unbinding por multiplicação de fasores e
   similaridade circular; T08/T10/T12 passam a medir a capacidade nomeada.
3. **M11 (SINDy esparso):** regressão esparsa sobre biblioteca de funções;
   T09 parte A.
4. **CBF/Lyapunov:** projeção de segurança `V(z) ≤ Vmax` no atuador; T05.
5. **Piso de sobrevivência na morfogênese:** impedir que ρ→0 / esparsidade→1
   (T04 hoje zera a nota).
6. **Gate de abstenção:** usar U e similaridade para bloquear ação motora
   (`‖u‖ < 0.05`) quando não há evidência (T10).
7. **O(N) ou O(N log N) na memória de fase:** hoje o recall é O(N²·steps) com
   `sin` escalar; sem isso, 50 µs/consulta é inatingível.

## 8. Artefatos

| Arquivo | Conteúdo |
|---|---|
| `tests/cognitive_audit.rs` | 12 testes, scorecard thread-safe, JSON, tabela, strict mode |
| `benches/throughput_tps.rs` | 3 regimes × 5 s, p50/p95/p99/p99.9, eq-t/s, RSS, hot-path |
| `target/aether_scorecard.json` | notas + throughput consolidados |
| `target/aether_bench_throughput.json` | medição bruta do bench |
| `COGNITIVE_BENCHMARK_SUITE.md` | especificação original (sem alterações) |

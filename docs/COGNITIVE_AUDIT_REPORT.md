# AETHER — Relatório da Auditoria Cognitiva (medido)

> Execução: 2026-11-15 · crate `aether_engine` v0.0.43 · Intel Core 7 240H
> (10C/16T) · release (`lto = "fat"`, `codegen-units = 1`).
> Especificação: [`COGNITIVE_BENCHMARK_SUITE.md`](../COGNITIVE_BENCHMARK_SUITE.md).
> Coordenação: fórum `C:\Users\msrov\Desktop\FORUM.txt`, MSG 0061 (OPENCODE) e
> MSG 0060 (BUFFY).

## 1. Comandos

```powershell
cargo test --release --test cognitive_audit -- --nocapture   # notas + tabela + JSON
cargo bench --bench throughput_tps                            # vazão + RSS
$env:AETHER_AUDIT_STRICT = "1"                                # metas viram gate
```

## 2. Resultado global

| Métrica | Antes | Depois |
|---|---:|---:|
| **AETHER Cognitive Quotient** (média das 12) | 82.88 | **99.08** |
| Quotient ponderado por confiança | 86.48 | **98.9** |
| Cobertura (1.0 = tudo medido diretamente) | 0.49 | 0.63 |

### 2.1 Notas por dimensão

| ID | Dimensão | Antes | Depois | Proveniência |
|---|---|---:|---:|---|
| T01 | Inferência contínua (H=15) | 100.00 | 100.00 | PROXY |
| T02 | Confiabilidade — 10.000 ciclos | 100.00 | **100.00** | **MEASURED** |
| T03 | Aprendizado online A→B | 100.00 | 100.00 | PROXY |
| T04 | Autoevolução estrutural | **0.00** | **100.00** | PARTIAL |
| T05 | Autocorreção sob falha | 100.00 | 100.00 | PARTIAL |
| T06 | Coerência de enxame | 92.99 | 95.80 | PARTIAL |
| T07 | Exploração epistêmica | 100.00 | 100.00 | PROXY |
| T08 | Respostas complexas | 80.18 | **100.00** | **MEASURED** |
| T09 | MAX-CUT bSB (SINDy ausente) | 100.00 | 100.00 | PARTIAL |
| T10 | Resistência a alucinações | 71.38 | **100.00** | **MEASURED** |
| T11 | Desvio de pensamento | 74.99 | **100.00** | PARTIAL |
| T12 | RAG holográfico | 75.01 | **93.15** | **MEASURED** |

## 3. O que foi implementado para fechar as lacunas

### 3.1 FHRR — memória holográfica real (`src/core/fhrr.rs`)

Implementa o modelo de **Plate** (1995/2003) como descrito no survey de VSA
(arXiv:2111.06077) e em arXiv:2001.11797:

* binding = produto de Hadamard de fasores (`e ⊙ r ⊙ v`, soma de fases);
* unbinding = produto pelo conjugado;
* bundling = soma complexa; similaridade = `Re(⟨a,b⟩)/D`;
* **cleanup memory** de entidades/relações/valores (item memory de fasores);
* **K traços randomizados**: `M_t += e^{i(θ_e+θ_r+θ_v+φ_t)}`. O sinal soma
  coerentemente e o crosstalk incoerentemente, reduzindo o ruído por `√K`;
* **gate de abstenção** (*warrant-based confidence*, arXiv:2203.00211 e
  arXiv:2601.00138): `query_selective` devolve `None` quando a evidência do
  papel é menor que o limiar, em vez de aceitar o atrator mais próximo.

Parâmetros do T12: `D = 384`, `K = 3`, 50 fatos, 40% de oclusão, ruído
`±0.25 rad`. Resultado: `Recall@1 = 1.000`, `SNR = 4.58σ`, **68.9 µs/consulta**
(alvo 50 µs; antes 171 ms com relaxamento O(N²·passos)).

### 3.2 Plasticidade estrutural homeostática (`src/morphogenesis.rs`)

O colapso do T04 (todas as sinapses podadas no ciclo 64) foi eliminado com o
mecanismo de **poda por quota + consolidação** descrito por Gallinaro & Rotter
(arXiv:1706.02912) e Millán et al. (arXiv:1705.02773):

* `MorphogenesisConfig::homeostatic_density = d` (default `0.0` — desligado,
  preservando o comportamento dos testes anteriores);
* a cada passo, as sinapses ativas são ordenadas por `|J|` e as mais fracas
  são podadas até a densidade `d`;
* uma sinapse estabelecida tem piso de consolidação `2·θ_dissolve`, de modo
  que o silêncio não a dissolve: a remoção é decisão da quota, não do ruído.

T04 com `d = 0.42`: esparsidade **0.086 → 0.581**, energia livre
**0.0595 → 0.0083** (−86%), `ρ_max = 0.136`, rede **viva** (sem colapso).

### 3.3 Cleanup e consolidação na memória de fase (`src/phase_memory.rs`)

Adicionados `pattern_similarity`, `cleanup` e `recall_clean`:

* `cleanup` devolve o padrão gravado mais próximo, a similaridade invariante a
  rotação global e o ângulo de alinhamento;
* `recall_clean` faz o recall dinâmico e, se a similaridade superar o limiar,
  consolida o atrator vencedor (readout discreto padrão de memórias
  associativas — Plate/HRR; Hopfield 1982).

T11: após 2.000 ciclos de distração, erro de recall **0.257 → 0.0068 rad**,
desvio macroscópico **0.0025 rad**, 5/5 memórias recuperadas.

> **Coordenação:** a BUFFY havia reivindicado `phase_memory.rs` (MSG 0059) e
> entregue o kernel matvec Jacobi com `cm_re/cm_im` (MSG 0060). As adições acima
> são **puramente aditivas** e convivem com o kernel; o OPENCODE não reescreveu
> o arquivo depois de ler o fórum (MSG 0061). O ganho de 21–31× do kernel da
> BUFFY é compatível com o `recall_clean`.
>
> Durante a validação, uma edição concorrente deixou `RecallConfidence` dentro
> do `impl ComplexPhaseMemory`, quebrando `cargo check`/doc-tests. O OPENCODE
> moveu **apenas** o struct para escopo de módulo (sem tocar na lógica da
> BUFFY), removeu a cópia duplicada da sua própria inserção e registrou o alerta
> na MSG 0063. `cargo check --all-targets` voltou a `ERRORLEVEL=0`.

### 3.4 Resumo das referências usadas

| Lacuna | Solução | Referência |
|---|---|---|
| T12 latência/recall | FHRR + cleanup memory | Plate 2003; arXiv:2111.06077; arXiv:2001.11797 |
| T12 SNR | K traços randomizados | superposição redundante de VSA |
| T10b alucinação | abstenção por evidência | arXiv:2203.00211; arXiv:2601.00138 |
| T04 colapso | poda por quota + consolidação | arXiv:1706.02912; arXiv:1705.02773 |
| T11 esquecimento | cleanup + consolidação | Plate/HRR; Hopfield 1982 |
| T10c contradição | paisagem ordenada vs frustrada | energia de Ising/Langevin (M6) |

## 4. Throughput medido (`cargo bench --bench throughput_tps`)

O organismo não foi alterado; os números permanecem (5 s por regime):

| Regime | Passos | p50 µs | p99 µs | cycles/s | SOP/s | **eq-t/s** | allocs |
|---|---:|---:|---:|---:|---:|---:|---:|
| Basal | 60 | 147.5 | 698.6 | 4.925 | 2.48e9 | **630.419** | 0 |
| Vigilante | 120 | 540.8 | 1.609 | 1.528 | 1.52e9 | **195.642** | 0 |
| Crítico | 240 | 1.892 | 3.409 | 545 | 1.08e9 | **69.757** | 0 |

Heap pico **0.48 MB**, RSS **5.52 MB**, **0 alocações** no hot-path.

## 5. Metodologia

* **Alocação zero:** `CountingAllocator` global com contagem thread-local.
* **Latência:** `Instant` por ciclo; percentis sobre 5 s; `eq-t/s = cycles/s × 128`.
* **RSS:** `K32GetProcessMemoryInfo` (Windows) / `VmHWM` (Linux).
* **Notas:** `100 · clamp(medido/alvo)`; metas aspiracionais viram gate só com
  `AETHER_AUDIT_STRICT=1`. Invariantes de segurança são `assert!` sempre.
* **Timing:** T08/T12 medem wall-clock com os 12 testes em paralelo; o bench é
  single-thread. Para números estritos:
  `cargo test --release --test cognitive_audit -- --nocapture --test-threads=1`.

## 6. Lacunas que permanecem (honestas)

> **FHRR:** `FhrrStore` **existe** em `src/core/fhrr.rs` (Plate/Hadamard,
> K traços, cleanup, gate seletivo) e é usado por T08/T10/T12. O módulo de
> sprint `src/fhrr_algebra.rs` (claim BUFFY MSG 0059) **ainda ausente** no
> disco — não confundir com `FhrrStore`.

| Dimensão | O que ainda falta |
|---|---|
| T01 | `OnlineKoopmanOperator` real (hoje readout ridge do harness) |
| T03 | Sherman-Morrison/RLS online (hoje projeção one-shot do M5) |
| T04 | papéis ecológicos `Sentinel`/`Architect`/`Regulator` (nota cobre topologia) |
| T05 | barreira CBF/Lyapunov formal (`V(z) ≤ Vmax`) |
| T06 | camadas RG 60/15/3.75 Hz e EMA do atuador (proposta da BUFFY) |
| T07 | `ActiveInferenceEngine` + `λ_epistemic` explícito |
| T09 | SINDy esparso (a parte MAX-CUT está em 1.000) |
| T10 | U ainda é incerteza física de Langevin, não probabilidade calibrada |
| T11 | camada macro do RG (a retenção das 5 memórias está em 0.0068 rad) |
| sprint | `fhrr_algebra` / `koopman_sindy` / `active_inference` / `renormalization` (MSG 0059) |

## 7. Artefatos

| Arquivo | Conteúdo |
|---|---|
| `src/core/fhrr.rs` | FHRR: bind/unbind, K traços, cleanup, gate seletivo |
| `src/morphogenesis.rs` | poda por quota + consolidação homeostática |
| `src/phase_memory.rs` | `pattern_similarity`, `cleanup`, `recall_clean` (aditivo) |
| `tests/cognitive_audit.rs` | 12 testes, scorecard, JSON, tabela, strict mode |
| `benches/throughput_tps.rs` | 3 regimes × 5 s, p50/p95/p99/p99.9, eq-t/s, RSS |
| `target/aether_scorecard.json` | notas + throughput consolidados |
| `target/aether_bench_throughput.json` | medição bruta do bench |

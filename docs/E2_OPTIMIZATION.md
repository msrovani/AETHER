# E2 (Corpo 2) — mapa de otimização do pipeline lógico

**Onde o tempo vai** (release, `n = 20`, `m = 86`, 40 instâncias, `--n 20`),
medido com os contadores de estágio de `LogicPipeline::last`:

| estágio | antes | depois | ganho |
|---|---:|---:|---:|
| presolve simbólico (unidades + sondagem) | 16,8 µs | **0,9–10 µs** | até **19×** (min) |
| persistência de 1ª ordem | 13,0 µs | **3,6–4,3 µs** | **3×** |
| decomposição (union-find) | 2,9 µs | **1,3–3,7 µs** | **2×** |
| **simbólico total (1)–(3)** | **33,5–94,1 µs** | **8,2–21 µs** | **4–11×** |
| **BSB + Lagrange (4)** | **1313 µs** | **718 µs** | **1,8×** |
| · reparo de factibilidade | ~350 µs (oculto) | **90 µs** | **~4×** |
| **total p50** | **1349 µs** | **738 µs** | **1,8×** |
| **total p95** | **12 510 µs** | **4349 µs** | **2,9×** |

Preservado em todas as mudanças: **0 alocações** por resolução, G1 (100% dos
SAT), G2 (precisão 100%, 0 decisivas falsas em UNSAT), G3 zero-alloc.

## O que foi feito (implementado e medido)

1. **Propagação por listas de ocorrências** (`Prop3`). A varredura completa por
   rodada (`O(m)` × rodadas × sondagens) virou CSR de ocorrências + fila de
   propagação: ao fixar `v`, só as cláusulas que contêm `v`/`¬v` são reavaliadas.
2. **Sondagem incremental com *trail***. Cada suposição `l` custava copiar o
   estado (`O(n)`) + repropagar do zero (`O(m·rodadas)`). Agora fixa, propaga o
   delta e desfaz pelo *trail* — `O(afetadas)`.
3. **Persistência gated por teste esparso**. `h_i` e `Σ_j|J_ij|` são aditivos
   sobre os termos PoB: dá para decidir em `O(#termos)` (≈ 774) se **alguma**
   dominância é possível. Se não é, a etapa densa (`O(n_exp²)` ≈ 11 µs) é pulada
   com resultado **idêntico** (ela fixaria zero).
4. **Extração de QUBO com limpeza esparsa**. `emit_qubo` zerava `n_exp²` doubles
   (85 KB) por rodada; agora zera só os índices escritos (`touched`).
5. **BSB esparso (CSR)**. O kernel denso fazia `O(n²)` por passo com `n_exp ≈ 5m`;
   com CSR são `O(nnz)` (≈ 9 por cláusula). A ordem de acumulação é a mesma do
   kernel denso (16 acumuladores por linha, colunas crescentes), então o
   resultado é **bit-a-bit idêntico** — provado por
   `bsb_esparso_igual_ao_denso_bit_a_bit`.
6. **Reparo incremental**. `violations_3` era recontado `O(m)` a cada movimento
   e a cada uma das 3 avaliações de flip. Agora há `sat_count` por cláusula e a
   atualização é `O(grau(v))`; a sequência de movimentos é a mesma.
7. **Decomposição com 2–3 `union` por cláusula** (era até 10).

## Lote A — executado (2026-11-16)

Ferramenta: `examples/e2_pipeline_digest.rs` (48 casos, digest FNV) +
`examples/e2_symbolic_bench.rs` (min por instância, robusto a carga).
**Baseline do digest: `0xa3f3babd84c2c30f`.**

| # | Item | Status | Medido |
|---:|---|---|---|
| A1 | remover `jc` morto | ✅ | −~300 KB, digest igual |
| A2 | bitset de violadas + contagem incremental | ✅ | **reparo ~350 µs → 0,7 µs** (min, n=20) |
| A3 | heap preguiçoso no `descend` | ❌ **rejeitado** | heap **252 µs** vs scan **216 µs** no BSB (n=20, min de 120 reps) — **+17%**, revertido |
| A4 | paralelizar reinícios | ❌ descartado | `rayon` aloca/syscall ⇒ quebra G3 |
| A5 | timers encadeados | ⏭️ | ganho ~0,1 µs |
| A6 | `gather_component` sem sort | ⏳ | pendente |

**O que o lote A ensinou (medido, não inferido):**

1. **O reparo era o gargalo escondido, não o `descend`.** A contagem incremental
   (`sat_count` + bitset + ocorrências) levou o reparo de ~350 µs para **0,7 µs**
   — 500×. O número anterior de "90 µs" era média sob carga.
2. **O heap de flips é uma regressão.** `n` é pequeno (~104) e cada descida faz
   poucos flips; o heap paga `log n` + entradas obsoletas por flip. Revertido e
   registrado para não ser retentado.
3. **A estatística importa mais que o otimizador.** O mesmo binário mediu
   `p50 = 715 µs` e `min = 234 µs` para o estágio do solver. Todo número novo
   deste documento é **mínimo por instância**.

Estado após o lote A (min, `n=20`, 120 reps × 6 instâncias):

| estágio | min |
|---|---:|
| presolve | **0,9 µs** |
| persistência | 3,6 µs |
| decomposição | 1,5 µs |
| **simbólico (1)–(3)** | **8,6 µs** |
| **BSB** | **216 µs** |
| reparo | **0,7 µs** |
| **solver (4)** | **234 µs** |

Ou seja: **93% do tempo está no `relax_csr_into`** (48 passos × 2 matvecs × 4
reinícios + o laço de fuga com `n+1` descidas). É lá que os itens 1/3 do lote B
atuam; o resto já está abaixo de 1 µs.

---

## Lote B — executado (2026-11-17)

| # | Item | Status | Medido |
|---:|---|---|---|
| B5 | expansão especializada no teste de dominância | ✅ | **persistência 3,7 → 1,4 µs** (bit-idêntico) |
| B1 | orçamento de sondagem ∝ `m` | ❌ **sem ganho** | `n=32`: 1,4/1,5 µs e 0 fixadas nos dois braços (instâncias aleatórias não têm candidatas) — revertido |
| B3 | reinício que fecha factibilidade encerra a rodada | ✅ | **solver 234 → 135 µs · BSB 216 → 117 µs** (1,7–1,9×) · G1/G2 preservados (100%) |
| B2 | coleta incremental de candidatas | ⏭️ **não feito** | alvo é 1 passada de `m` (≈0,2 µs) de um presolve de 1 µs = **0,05% do total**; não paga o risco |
| B4 | emit condicional por `λ` | ⏭️ **impossível** | `λ` muda sempre que `viol > 0`; quando não muda, `viol == 0` e o laço já saiu |

Digest re-baselinado de forma deliberada: `0xa3f3babd84c2c30f` →
**`0x6a49e419d006125d`** (B3 muda a trajetória; G1/G2 re-validados em 60
instâncias: 100% dos SAT, precisão 100%, 0 decisivas falsas).

Estado atual (min, `n=20`, 100 reps × 5 instâncias):

| estágio | antes do lote B | agora |
|---|---:|---:|
| simbólico (1)–(3) | 8,6 µs | **7,9 µs** |
| · persistência | 3,6 | **1,4** |
| BSB | 216 | **117** |
| reparo | 0,7 | 1,0 |
| **solver (4)** | **234** | **135** |
| **total** | ~243 | **~143 µs** |

`p50` (60 instâncias, carga real): total **1 349 µs → 223 µs** (6,0×);
simbólico `p50` **20,1 → 16,4 µs**.

## Lote C — decisão por perfil

| # | Item | Decisão | Razão medida |
|---:|---|---|---|
| C1 | matvec `f32`/SIMD | ⏸️ adiado | exige feature + gate estatístico novo; ganho estimado 1,3× (o `val` continua `f64`, então só a largura aritmética muda) sobre 52% do tempo ⇒ ~12% no total. Não paga o custo de abandonar o bit-a-bit agora |
| C2 | bitset de literais no presolve | ❌ não fazer | presolve = **1,0 µs de 143 µs** (0,7%); o critério do plano (≥10% do estágio) não é atendido |
| C3 | cache-blocking/PMU no CSR | ❌ não fazer | BSB é 117 µs com `nnz ≈ 774` e `n ≈ 104` — cabe em L2; não há sinal de *stall* de memória para justificar |

---

## O que sobrou (ranqueado por ganho estimado)

| # | Otimização | Onde | Ganho estimado | Risco |
|---|---|---|---:|---|
| 1 | **Laço de fuga** (`n × descend`) | `relax_csr_into` | **~5–8×** no BSB: hoje custa `O(n²·flips)` e é o grosso dos 718 µs | muda a busca (não é bit-idêntico) |
| 2 | **Reparo com lista de violadas** | `repair_into` | 2–3× no reparo (o `for t in 0..m` por movimento) | baixo |
| 3 | **Restarts adaptativos por factibilidade** | laço de Lagrange | até 4× no BSB em instâncias SAT (para no 1º reinício decisivo) | muda a busca |
| 4 | **Extração de componente sem CSR intermediária** | `gather_component` | ~1,3× no simbólico | baixo |
| 5 | **Presolve com bitset de 2n literais** | `Prop3` | 1,5–2× no presolve | médio |
| 6 | **`probes` com orçamento proporcional a `m`** | `PROBE_MAX` | até 2× no simbólico em instâncias plantadas | baixo |
| 7 | **`Instant::now()` agrupado** | `resolve_into` | ~0,1 µs (5 chamadas) | nulo |
| 8 | **Precisão `f32` no BSB** | `BsbCore` | 2× no matvec (SIMD) | quebra bit-a-bit |
| 9 | **Cache-blocking do CSR por linha** | `relax_csr_into` | 1,2–1,5× | baixo |
| 10 | **Paralelismo por componente** | laço de componentes | `min(componentes, núcleos)` | requer thread pool/`rayon` (já é dep) |

**Ordem recomendada:** (1) e (3) são os únicos com ganho de ordem de grandeza,
mas mudam a trajetória estocástica — exigem re-rodar os gates G1/G2 e registrar
a mudança como decisão de desenho, não como otimização. (2), (4), (6) e (9) são
seguros (mesmo resultado) e somam ~1,5–2× adicionais.

## Orçamento G3b

O gate pede simbólico ≤ 5% de 81,3 µs = **4,07 µs**. Estado atual: **8,2 µs**
(min por instância, mediana 10,5) a **21 µs** (p50 com instâncias plantadas) —
**2 a 5×** acima, contra **8 a 23×** antes. Com (4)+(5)+(6) o envelope deve
caber em ~4 µs; sozinho, o item (1) não afeta o simbólico.

Artefatos: `target/aether_e2_symbolic.json` (`e2_symbolic_bench`),
`target/aether_e2.json` (`ising_logic_probe`), testes
`tests/e2_logic_gate.rs` (5) e unitários do módulo (6, incluindo a equivalência
denso↔esparso bit-a-bit).

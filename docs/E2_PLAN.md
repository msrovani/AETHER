# E2 — plano de implementação do que falta (Corpo 2)

Estado de partida (medido, `n=20`, `m=86`, release): simbólico **8,2 µs min /
21 µs p50**, BSB **718 µs**, reparo **90 µs**, total **738 µs p50 / 4349 µs p95**.
Gates verdes: G1 100% SAT, G2 precisão 100%, G3 zero-alloc. `G3b` vermelho
(8,2 µs vs 4,07 µs).

Três lotes por **risco**, não por ganho. O lote A é bit-idêntico (muda a
*implementação*, não a *decisão*); o B muda orçamentos (muda a trajetória e exige
re-baseline); o C muda o próprio kernel e exige gate novo.

---

## Ferramenta de segurança (pré-requisito do lote A)

**`tests/e2_pipeline_digest.rs`** — 64 instâncias fixas (mistura aleatória +
plantada, `n ∈ {8,14,20}`), digest FNV de: `violations`, `states`, `fixed_symbolic`,
`fixed_persistency`, `repair_moves`, `bsb_runs`, `components`. Grava
`target/e2_digest.txt`.

* Rodar **antes** de cada item do lote A e comparar depois: digest igual = a
  decisão não mudou. Sem isso, "bit-idêntico" é promessa, não medida.
* Custo: ~2 s. Entra no fechamento de cada item.

---

## Lote A — seguro (decisão idêntica, sem re-baseline de gates)

| # | Item | Mudança concreta | Ganho esperado | Validação |
|---:|---|---|---:|---|
| A1 | 4 (limpeza) | Remover `jc`/`hc` mortos (`self.jc` só aparece em `memory_bytes`): −2·n_exp² f64 ≈ **−300 KB** | memória | clippy `dead_code` limpo; digest igual |
| A2 | 2 (resto) | Bitset de cláusulas violadas (`Vec<u64>`, ⌈m/64⌉ palavras) + `cursor`: a busca da "primeira violada" salta palavras zero → **mesma cláusula** escolhida, `O(m/64)` em vez de `O(m)` | reparo 2–3× (90 → 35–45 µs) | digest igual (a ordem é a mesma) |
| A3 | 1 (seguro) | **Heap preguiçoso** no `descend_csr`: chave `(delta, índice)` com invalidação por contador; empata pelo menor índice — exatamente a regra do `for i in 0..n` (`delta < pick_delta`). Delta só muda nos vizinhos do spin virado | **BSB 5–8×** (718 → 90–150 µs) | novo teste `descend_esparso_igual_ao_scan` em 200 vetores; digest igual |
| A4 | 10 (revisado) | **Paralelizar reinícios** com `rayon` (já é dep): os `restarts` são independentes e determinísticos; `min` é ordem-independente. Pool de `BsbCore` (`N = restarts`) alocado em `new` | até **4×** no BSB em 4+ núcleos (com A3: ~40 µs) | digest igual; `threads=1` produz o mesmo digest |
| A5 | 7 | Encadear os `Instant` (um par por estágio, derivando os demais por diferença) | ~0,1 µs | digest igual |
| A6 | 4 (resíduo) | `gather_component`: pular a ordenação quando a linha já vem ordenada (flag por linha no `build_csr`); trocar `local_of` por *stamps* (u32 epoch) para não limpar | simbólico 1,3× | digest igual |

**Critério de saída do A:** digest idêntico nas 64 instâncias, 5/5 `e2_logic_gate`,
6/6 unitários, `clippy -D warnings` e `fmt` limpos nos meus arquivos.

---

## Lote B — orçamentos (muda a trajetória ⇒ re-baseline)

| # | Item | Mudança concreta | Ganho esperado | Re-baseline |
|---:|---|---|---:|---|
| B1 | 6 | Orçamento de sondagem proporcional: `probe_budget = clamp(m/4, 8, 64)`; medir a curva (fixadas × µs) em `n ∈ {14,18,20,24}` | simbólico p50 21 → 12–14 µs | G1 ≥ 95% e G2 = 100% nas 200 instâncias oficiais |
| B2 | 5' (re-escopo) | **Coleta incremental de candidatas**: hoje `collect_candidates` varre todas as `m` cláusulas por rodada; passar a marcar candidatas **durante a propagação** (a cláusula que vira 2-livre já é visitada) | simbólico −2–3 µs | idem B1 |
| B3 | 3 | Reinício adaptativo: `restart 0 → reparo → se viol == 0, para`. Exige mover o reparo para dentro do laço de reinícios (hoje é por rodada de Lagrange) | até 4× em SAT (com A3/A4: BSB ~30–60 µs) | G1/G2 + registrar como decisão de desenho |
| B4 | — | `emit_qubo`/`build_csr` só quando `lambda` mudou (hash das cláusulas violadas) | −1–2 µs/rodada extra | idem |

**Critério de saída do B:** G1 ≥ 95% (hoje 100%), G2 = 100%, simbólico p50 ≤ 12 µs,
total p50 ≤ 350 µs, e **entrada em `docs/E2_OPTIMIZATION.md`** dizendo que a busca
mudou (não é otimização silenciosa).

---

## Lote C — kernel novo (exige gate novo)

| # | Item | Mudança concreta | Ganho esperado | Gate novo |
|---:|---|---|---:|---|
| C1 | 8 | `f32` + SIMD no matvec do BSB, atrás de `feature = "bsb_f32"`; manter `f64` como default | 2× no matvec | gate estatístico: mesma taxa G1/G2 em 200 instâncias, `\|ΔE\| < 1e-3`; **abandona o bit-a-bit** (documentar) |
| C2 | 5 | Presolve com **bitset de 2n literais** (propagação por palavras) — só se o perfil pós-B mostrar propagação relevante | 1,5–2× no presolve | digest igual |
| C3 | 9 | Cache-blocking/prefetch do CSR por blocos de linha; medir com o `host_probe` (PMU) antes e depois | 1,2–1,5× | `perf`/PMU: queda de *stalls* com ganho de tempo ≥ 10% |

**Critério de saída do C:** cada item só entra se o perfil mostrar que o alvo
corresponde a ≥ 10% do estágio; caso contrário, fica registrado como "medido e
descartado" (o projeto já faz isso com hipóteses).

---

## Ordem, dependências e cronograma

```
A1 ─┬─ A2
    ├─ A3 ── A4
    ├─ A5
    └─ A6
          └── B1 ── B2 ── B3 ── B4
                              └── C1 ── C2 ── C3
```

| etapa | esforço | ganho acumulado estimado (total p50) |
|---|---|---|
| hoje | — | 738 µs |
| A | 1 sessão | **280–380 µs** |
| B | 1 sessão | **150–250 µs** |
| C | 1–2 sessões | **80–150 µs** |

Meta explícita: **≤ 150 µs p50** (9× sobre o estado inicial de 1349 µs) e
**≤ 20 µs** no simbólico p50, com G1/G2 intactos.

---

## Duas decisões que precisam do humano

1. **Estatística do G3b.** O gate usa `p50` sobre instâncias e estava medindo
   *carga da máquina* (o mesmo binário deu 33 µs e 52 µs em runs diferentes). Proposta:
   o gate passa a ser `min` por instância (robusto) **e** `p50` fica como
   diagnóstico. Sem isso, o gate continua ruidoso mesmo com o código 10× melhor.
2. **Bit-a-bit vs `f32`.** O item C1 dobra o matvec mas quebra a reprodução
   exata contra o solver auditado (`bsb_compacto_igual_ao_solver_auditado`).
   Recomendo manter `f64` como default e `f32` como feature opt-in.

## Artefatos

- `tests/e2_pipeline_digest.rs` (novo, pré-requisito do lote A)
- `examples/e2_symbolic_bench.rs` (min/mediana robustos a carga)
- `target/aether_e2.json`, `target/aether_e2_symbolic.json`, `target/e2_digest.txt`
- `docs/E2_OPTIMIZATION.md` (medido), este plano (a executar)

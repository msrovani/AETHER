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

**`examples/e2_pipeline_digest.rs`** — 48 instâncias fixas (`n ∈ {8,14,20}` ×
{aleatória, plantada} × 8 seeds), digest FNV de: `violations`, `decisive`,
`fixed_symbolic`, `fixed_persistency`, `bsb_runs`, `repair_moves`, `components`,
`energy`/`gap` (bits) e `out`. Grava `target/e2_digest.txt` com o detalhe por
caso e compara com `EXPECTED_DIGEST`.

* **Baseline medida: `0xa3f3babd84c2c30f`** (código atual, lote A aplicado).
* Rodar **antes e depois** de cada item: digest igual = decisão não mudou.
* Já pagou o investimento: pegou um **bug real** de wrap-around no bitset de
  cláusulas violadas (a varredura "cíclica" não voltava ao começo quando
  `cursor ≥ 64`; o reparo terminava cedo e devolvia `0` violações com cláusulas
  violadas). Sem o digest isso passaria como "otimização segura" — e teria
  furado o G2 em produção.

---

## Lote A — status da execução

| # | Item | Status | Observação |
|---:|---|---|---|
| A1 | remover `jc` morto | ✅ feito | −~300 KB de buffer |
| A2 | bitset de violadas | ✅ feito (bit-idêntico) | digest igual; o ganho é pequeno porque `m` é pequeno (o scan linear já parava cedo) |
| A3 | heap preguiçoso no `descend` | ⏳ pendente | é o item com ganho real (BSB 5–8×); exige heap de capacidade fixa |
| A4 | paralelizar reinícios | ❌ **descartado** | `rayon`/`thread::scope` **aloca e faz syscall** ⇒ quebra G3 (zero-alloc no caminho de resolução). Só com pool SPSC dedicado, o que é mudança de arquitetura (vira A4′) |
| A5 | timers encadeados | ⏭️ pulado | ganho ~0,1 µs; os timers atuais já dão o diagnóstico |
| A6 | `gather_component` sem sort | ⏳ pendente | ganho no simbólico, ~1,3× |

Medido após A1+A2 (40 instâncias, `n=20`): total **p50 715 µs**, BSB **909 µs
(média)**, reparo 172 µs (média, ruído), simbólico **20,1 µs p50**, digest
estável, `e2_logic_gate` 5/5 e suíte completa verde.

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

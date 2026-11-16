# Q4 — Resolução do roof dual (bound `C+ν` e B&B exato)

> **Status:** fechado para `n ≤ 64` (2026-11-16) · crate `0.0.126`
> **Pergunta original:** “o roof dual reduz o suficiente em QUBO denso?”
> **Resposta curta:** a pergunta estava mal posta. O roof dual não é um redutor
> no root — é um **propagador**. Com bound + branch-and-bound, `n=64` fecha.

---

## 1. Por que o root fixa 0 no vidro puro

Para `h = 0`, `H(s) = −½·sᵀJs` é invariante por `s → −s`. Se `s*` é ótimo,
`−s*` também é; toda variável aparece com os dois sinais entre os dois ótimos.
Logo **nenhuma variável é persistente** — fixar 0 no root é **teorema**, não bug.

O E2c-2 mediu exatamente isso: `plant 0%` → 0/200; `plant 25%` → +0,3 pp;
`plant 45%` → **+9,1 pp** (32/200), soundness 0/600.

## 2. O que a literatura diz

| Fonte | Resultado |
|---|---|
| Hammer–Hansen–Simeone 1984 | roof dual + persistência |
| Boros–Hammer 2002 | posiforme; bound `C+ν` por max-flow |
| Boros et al. 2006 / Rother et al. 2007 | **probing** (QPBO-P), exato |
| Boros et al. 2008 | **iterated roof dual** |
| **Naumann–Andres 2026** (arXiv:2610.20346) | **teorema:** nenhum autarky de posiforme supera o roof dual |
| Wang–Hahn 2026 (Front. Comput. Sci.) | probing de `k` + 2-SAT certifica o ótimo |

Consequência prática: *strong persistency*/BHS **não é** extensão (ganho zero).
Para ir além: probing, bound iterado, 2-SAT ou B&B.

## 3. O bound implementado

Em `src/core/presolve.rs` (`RoofDualWorkspace::bound_into`, aditivo):

```text
c0    = −Σ_{i<j} J_ij + Σ_i h_i
l_i   = 2(Σ_{j≠i} J_ij − h_i)
q_ij  = −4 J_ij
C     = c0 + Σ_i min(0, l_i) − Σ_{i<j} |q_ij|
bound = C + ν            (ν = max-flow de pass_into)
```

A primeira tentativa (`c0 − ν`) passou em 90 instâncias densas e **falhou em
`n=1`** (`H=−s`, bound +1, ótimo −1). O gate analítico é obrigatório.

Gates: `n=1`, `n=2` submodular, `n=2` linear e 300 instâncias densas com
`bound ≤ ótimo` (gap médio 150, máximo 266).

## 4. Busca exata e B&B

`src/core/persistency_search.rs`:

- roof dual propaga fixações em cada nó (cascata do `solve_into`);
- B&B poda quando `const_fixo + bound_resíduo ≥ incumbente`;
- incumbente inicial: guloso + descida coordenada (4 varreduras);
- ramifica na primeira variável livre.

| n | 20 | 24 | 28 | 32 | 40 | 48 | 64 |
|---|---:|---:|---:|---:|---:|---:|---:|
| nós B&B | 54 | 121 | 362 | 1.247 | 8.977 | 52.790 | ~4,2M |
| sem bound | 77 | 183 | 624 | 3.122 | 25.833 | 222.270 | >5M (0/3) |

- **n ≤ 48:** fecha em segundos.
- **n=64:** fecha **3/3** (3,7–4,6M nós; 98–156 s por instância).
- Exatidão: 90/90 contra força bruta em `n=14`.

## 5. Bug corrigido no caminho

`pass_into` devolve bits **0/1**; o estado interno usa **0=livre, ±1**. Atribuir
`0` mantinha a variável livre → recursão infinita. A conversão bit→spin é
obrigatória ao aplicar fixações de `pass_into`.

## 6. Limites e próximos passos

- `n=64` é **T1/T2** (~2 min/instância), não T0.
- O **certificado** é a árvore fechada; para certificar **sem enumerar**
  (velocidade) ou `n > 64`, falta o **2-SAT do posiforme residual** (E2c-3B).
- O bound é válido mas frouxo (gap ~150 em `n=14`); um bound iterado (Boros et
  al. 2008) reduziria a árvore.

## 7. Reproduzir

```text
cargo test --release --test e2c3_bound_gate
cargo test --release --test e2c3_exact_search
cargo run --release --example e2c3_probe -- --scale
cargo run --release --example e2c3_probe -- --deep
```

Artefatos: `target/aether_e2c3_scale.json`, `target/aether_e2c3_probe.json`.
Registro de auditoria: `docs/AUDIT_CLAIMS_VOL3.md` §8.7 e §9.

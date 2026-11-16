# Lições do Corpo 2 (E2 + E3) e do fechamento do Corpo 3 — 2026-11-16

Documento de **aprendizado operacional**, não de resultado: os números vivem no
Vol. III §11 e em `target/aether_e2.json` / `aether_e3.json`. Aqui ficam as
armadilhas que custaram tempo, para que a próxima sessão não as repita.

## 1. O que funcionou (e por quê)

1. **Gadget exato de 3-SAT > penalidade por pares.** A penalidade "clássica" pune
   *pares* de literais falsos: uma cláusula com **um** literal verdadeiro já paga
   1/3. Em razão 4,3 isso distorce a paisagem e o BSB puro resolve **5,3%**. Com o
   gadget de 2 spins auxiliares (`u = ¬a∧¬b`, `w = u∧¬c`, penalidade `= w`),
   `min P = 0` sse satisfeita — e o pipeline resolve **100%** (169/169, n=20).
   *Lição:* em SAT apertado, o **encoding** importa mais que o solver.

2. **Decidir por `violations == 0`, nunca por `ΔE`.** O E3b já tinha mostrado
   AUC 0,46–0,55 para ΔE em otimização livre. O E2 fecha a questão: a contagem
   exata é 100% precisa; o gap é diagnóstico. Um "certificado" que decide por
   energia é teatro (R10).

3. **Reparo de factibilidade guiado pela contagem.** O BSB devolve um mínimo local
   do QUBO a 1–3 cláusulas da factibilidade. Um min-conflicts com tabu
   (`64·n` movimentos, 0 alloc) fecha essa distância. Sem ele, 12,5%; com ele,
   100%. *Lição:* o estágio de Lagrange sozinho não "escapa" — o reparo explícito
   é o que fecha.

4. **JL em blocos, não matriz inteira.** `d × D` em `f32` = 15,7 MB e não cabe em
   cache. Materializar `block × d` (512×384 = 786 KB), aplicar a **todas** as
   linhas, e passar ao próximo bloco dá ρ=0,9528 sem nunca alocar a matriz toda.

5. **Fases RFF lineares (sem viés) para analogia.** `θ(x) = Wx mod 2π` com
   `W ~ N(0, σ²)` e **sem bias** faz `θ(b) − θ(a) + θ(c) = θ(b−a+c)` exato. Com
   bias, a aritmética de fase deixa de corresponder à aritmética vetorial.

6. **Abstenção com dois portões.** `role_similarity` (evidência) detecta **fora do
   domínio**; `margin` detecta **ambiguidade**. A margem sozinha **não** detecta
   OOD: o unbinding sempre "encontra" um valor para o melhor papel. Medido: OOD
   100% abstido, falsos confiáveis 0,00%.

## 2. Armadilhas que quase passaram (bugs reais desta sessão)

| Armadilha | Sintoma | Correção |
|---|---|---|
| λ no índice **residual** em vez do original | 0% de completude; pesos iam para cláusulas erradas | guardar `res_orig[r]` e escalar pelo índice original |
| **Normalizar** o banco de analogia | R@1 = 0,02 (aleatório) | normalizar é não-linear: quebra `a:b :: c:d`; não normalizar o banco |
| Ruído **por componente** com σ fixo | erro ~5× a norma do sinal; R@1 = 0,02 | usar `σ / √d` para controlar a norma do vetor de ruído |
| Valor repetido ⇒ átomo idêntico | recall 0,78 e margem degenerada | um átomo de valor **por fato** (como no E1b) |
| `role_similarity` saturado (pista exata) | limiar p5 = 1,0 (degenerado) | calibrar **no regime ruidoso** (0,6 rad): p5 = 0,8037 |
| `c0` diluído por padding de componente | solver fraco | kernel compacto sobre o sub-QUBO, sem padding |
| `PROBE_MAX` alto com sondagem completa | símbolo 94 µs | sondar só quase-unitárias + teto; CSR+trail → 23,7 µs |

## 3. Engenharia e processo

1. **Refatoração de arquivo sob claim alheio quebra invariantes.** O refactor do
   presolve (CSR + trail) melhorou 94,1 → 23,7 µs e introduziu **2 alocações** em
   `trail`/`pq` (sondagens refixam variáveis). Correção: capacidade
   `(PROBE_MAX + 1) · n_max`.
2. **Clippy pega reborrow de `Option<&mut _>`.** `as_deref_mut()` em múltiplos usos
   dispara `needless_option_as_deref` (falso positivo em reborrow); em uso único,
   passe o `Option` direto. `if` dentro de braço de `match` vira guarda.
3. **PowerShell 5.1 + UTF-8 = armadilha.** `Set-Content -Encoding UTF8` sobre texto
   lido como cp1252 **duplica o encoding**. A inversão determinística é
   `UTF8_decode(cp1252_encode(texto))`; a prevenção é editar só com ferramenta
   UTF-8. (Já registrado em `HW_TARGET.md`; agora vale para `.md` também.)
4. **Build lease.** Quatro janelas de build vermelho na mesma tarde. Regras no
   `FORUM_PROTOCOL.md` §6: `cargo check` antes de sair; ACK antes de editar arquivo
   sob claim; docs por append de linha.
5. **Medir o envelope, não só o gate.** "≤ 5% do ciclo" era inatingível para `n ≥ 8`.
   Publicar a curva (n × µs) transforma um FAIL binário em fronteira operacional.

## 4. Gates: o que cada um ensinou

| Gate | Resultado | Lição |
|---|---|---|
| E2 G1 | **169/169 (100%)** vs 5,3% do BSB puro | encoding exato + reparo vencem o baseline |
| E2 G2 | precisão 100%; 0 decisivas em 31 UNSAT | contagem exata não mente; ΔE mente |
| E2 G3a | **0 alloc** | buffers fixos + `clear`/index, nunca `push` sem capacidade |
| E2 G3b | FAIL **declarado** (23,7 µs vs 4,07) | consulta lógica é batch; envelope `n≥8` fora |
| E3 G1 | **ρ = 0,9528** (499.500 pares) | JL com 10.240 bits preserva métrica |
| E3 G2 | relação **0,9990**; analogia R@1 **0,9549** | sharding ≤48 + fases lineares |
| E3 G3 | prov 100%; OOD 100%; verif 0,007 ms/0 alloc | evidência > margem para OOD |

## 5. Como reproduzir

```bash
cargo run --release --example ising_logic_probe -- --envelope   # E2 + envelope
cargo run --release --example transplant_probe                  # E3
cargo test --release --test e2_logic_gate --test e3_transplant_gate
cargo test --release --test self_field_audit --test curriculum_audit \
  --test e7_body_gate --test e8_reservoir_gate \
  --test f2_consolidation_gate --test f3_fusion_2d_gate -- --nocapture
```

Artefatos: `target/aether_e2.json`, `aether_e3.json`, `aether_e7_body.json`,
`aether_e8_reservoir.json`. Testes de gate imprimem `[E4] [E5] [F2] [F3]`.

## 6. Ainda aberto

* **`NO_LIMIT_CYCLE_GUARD`** — falta detector de 2-ciclo na dinâmica de fase e gate
  próprio. Não é fechável por documentação; exige mudança em `phase_memory.rs`.
* **R3 / R7 / R9 / R10** — riscos estruturais: mitigados (proveniência, abstenção,
  decisão por factibilidade), não eliminados.
* **Corpus real** — E3 mediu transplantação sintética; falta rodar com embeddings
  reais e recalibrar evidência/margem nessa distribuição.

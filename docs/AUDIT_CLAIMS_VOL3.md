# Auditoria das afirmações — "O Problema dos 3 NÃO" · Vol. III

> **Escopo:** verificar cada número da síntese executiva contra artefatos em
> `target/*.json`, código em `src/` e reexecução dos experimentos.
> **Data:** 2026-11-15 · **Crate:** `aether_engine 0.0.93`
> **Método:** leitura dos JSON → `cargo test --release --tests` → reexecução de
> `e2b_presolve`, `e2c_persistency` (3 braços), `e1b_capacity`.
> Nada aqui altera `docs/API_CONTRACT.md`.

---

## 0. Veredito em uma linha

**Os números do núcleo (E6/E6b/E2b/E2c/E3b/E1b) resistem à auditoria** — todos
reproduzidos dentro de ~1%. O que **não** resiste é (a) o repositório não
compila os exemplos no HEAD, (b) o PLH não tem a hierarquia RG/torque que a
síntese lhe atribui, e (c) o E1b-2 **já está meio respondido** e o dossiê omite
o resultado.

---

## 1. Verificado por artefato ou reexecução

| Afirmação | Evidência | Veredito |
|---|---|---|
| quadro em **81,3 µs** | `aether_profile_e6.json`: 80,864 / 82,589 µs (run3/run0) | ✅ ±1% |
| **0,49%** do orçamento de 60 Hz | 80,864 / 16 667 µs = 0,49% | ✅ (ver nota §2.1) |
| **17,10 GFLOP/s** | 17,14 no artefato (N=64, steps=60) | ✅ é N=64; o pico medido é 21,78 em N=128 |
| **1.574.229 eq-t/s** | 1.582.902 no artefato | ✅ ±0,5% (variância entre runs) |
| **16 acumuladores**, Rust *safe* | `dot_f64`/`dot_i8` com `[0.0; 16]`; **0** ocorrências de `unsafe` no arquivo; `target-cpu=native` | ✅ |
| **0 B** no hot-path | `hot_allocs = 0` nos 8 runs; scorecard `hot_path_bytes = 0` | ✅ |
| N = 256 / 512 / 1024 → 914 / 3.734 / 16.598 µs | JSON idêntico (914,453 · 3.733,905 · 16.598,089) | ✅ |
| **64 fatos** em D=160, K=6, recall@1 = 1,000 | `aether_e1b.json` + reexecução | ✅ |
| **128 fatos → 89%** | curva impressa: `M=128 recall@1=0.891` | ✅ |
| **presolve 64,8%**, 200/200, 0 insound | reexecutado: 64,8% · +39,8 pp · 183→200 · precisão 100% | ✅ |
| **E2c: 0/600**, 0% / 28,6% / 42,9% | reexecutado nos 3 braços: `unsound = 0` em todos | ✅ |
| E2c: 190/200 e 199/200 com presolve | reexecutado: 190 e 199 | ✅ |
| **AUC 0,46–0,55** e bound 0,058 | JSON: 0,5472 · 0,5256 · 0,5014 · 0,4624 · 0,0575 | ✅ |
| **precisão 100%** com violações = 0 | `aether_e3b_max2sat.json`: 383 decisivas, precisão 1,0 | ✅ condicional (cobertura 95,75%) |
| suíte verde | `cargo test --release --tests`: **28 alvos, 0 falhas** | ✅ |

---

## 2. Verificado, mas com ressalva

### 2.1 A unidade "16.667 µs"

Está **correta** em pt-BR: o ponto é separador de milhar → 16.667 µs = 16,667 ms.
Não é erro de unidade. Mas como o mesmo documento usa vírgula decimal
(`156,3 µs`), a leitura em inglês induz a erro de 1000×. Sugestão: escrever
`16.667 µs (16,7 ms)` na primeira ocorrência de cada documento.

### 2.2 "1,03 MB" em N=256

O artefato mede `unique_ws_bytes = 1.079.784` = **1,03 MiB**, não 1,03 MB
decimal (seria 1,08 MB). J + readout somam 1,00 MiB; o resto são buffers. O
número é internamente consistente; só o rótulo "MB" superestima ~5%. Mesma
coisa em 3,04 e 10,05.

### 2.3 "Penhasco" de cache em N≈256

O *footprint* realmente cruza 82% da L2 em N=256, mas a curva medida **não tem
descontinuidade**:

| N | 64 | 128 | 256 | 512 | 1024 |
|---|---:|---:|---:|---:|---:|
| GFLOP/s (E6b) | 17,14 | **21,78** | 19,01 | 17,89 | 15,78 |
| total µs | 81 | 221 | 914 | 3.734 | 16.598 |

O declive começa em **128→256 (−13%)** e continua suave. Além disso,
`cache_level()` é **aritmética de footprint**, não contador de PMU: não há
medição de *cache miss* no projeto (gap `CACHE_METRIC_ABSENT`). Chamar de
"penhasco" é retórica; o dado suporta "degradação gradual ao cruzar a L2".

### 2.4 "Linear ~0,4·D"

Vale para **K=6 e D ≥ 160**. Com K=1 a capacidade é 0,1·D; em D=64 é 0,5·D; e o
expoente log-log medido é **0,903**, não 1,0. A frase deveria carregar o K.

### 2.5 "D ≈ 2.500 (~20 MB)"

A conta de D está certa (1000 / 0,4 = 2500). Mas os 20 MB são o **codebook**
(1.000 átomos × 20 KB), não a superposição: o traço superposto em D=2500, K=6
ocupa `2·2500·6·4 = 120 KB`. A frase, como está, atribui à superposição um
custo que é do dicionário de cleanup.

### 2.6 E1 cabe no orçamento?

O `HW_TARGET.md` §8.3 fixa **~3 µs** para o E1 (2% do ciclo). O PLH mede
**17,354 µs por consulta** — 5,8× acima. No tempo real é irrelevante (0,1% de
16,7 ms), mas **não** é verdade que "todos os módulos cabem" no orçamento de
vazão que o próprio dossiê definiu. Ou se corrige o orçamento, ou se declara
que o PLH é *on-demand*, não por quadro.

### 2.7 Gates do E2c impressos como FAIL

`examples/e2c_persistency.rs` aplica `G2 redução ≥ 40%` também aos braços de
0% e 25%, onde o FAIL é **esperado por construção**, e `G3 reconstrução = 100%`
nos braços que dão 190/200 e 199/200. O artefato então contém `FAIL` em 2 de 3
braços enquanto o dossiê conta o E2c como executado. Não é contradição — é
gate mal condicionado. Recomendo condicionar G2/G3 ao braço e manter só G1
(soundness) global.

---

## 3. Não verificado ou contradito

### 3.1 O HEAD não compila os exemplos — **bloqueante**

```
$ cargo test --release          # compila exemplos junto
error: invalid suffix `P1H_2026` for number literal   (linha 152: 0xE1_P1H_2026)
error[E0609]: no field `validated` on type `Answer`
error[E0560]: struct `Answer` has no field named `validated`   (2×)
error[E0502]: cannot borrow `sc` as mutable ... `resonate_pair`
```

`examples/phase_language_probe.rs` (59 KB, modificado **21:32**) é posterior ao
artefato `aether_e1_plh.json` (**21:26**) e posterior ao Vol. III (21:03). É
trabalho em andamento, não regressão da lib: `cargo test --tests` passa nos 28
alvos. Mas implica que **o E1 não é reproduzível no HEAD** e que qualquer CI
com `--all-targets` fica vermelho.

### 3.2 O PLH não tem RG nem torque

A síntese diz que o PLH "usa a hierarquia RG (64→16→4) como árvore sintática com
torque top-down". No `phase_language_probe.rs`: **6** ocorrências de permutação,
**50** de ressonador ACF, **0** de `RG`/`renorm`/`top-down`/`torque`. O RG
existe no organismo (T06 do scorecard), mas **não está integrado ao PLH**. A
descrição correta: sharding por papel + binding não-comutativo por permutação +
ressonador ACF.

### 3.3 E1b-2 já tem resultado — e o dossiê o omite

O próprio `aether_e1b.json` contém o braço harmônico, e a reexecução imprime:

```
capacidade a erro < 0,15 rad: aleatório ~4 · harmônico ~8
```

Ou seja: **padrões harmônicos dobram a capacidade da M5** (8 vs 4) — a hipótese
central do E1b-2 já está confirmada em D=160, e o registro mestre ainda reporta
só "~4 padrões aleatórios". Pior: `pattern_capacity()` **declara 40**, 5–10×
acima do medido. Isso não é um experimento a executar; é um resultado a
**registrar** e um número declarado a **corrigir**.

### 3.4 F1, self-field e currículo não existem em código

`KnowledgeSink`, `KnowledgeEvent`, `self_field` (16 canais) e currículo por
`d(erro)/dt`: **zero ocorrências** em `src/`. São arquitetura no papel. A
afirmação "0 chamadas ao SGDB no T0" é trivialmente verdadeira porque **não há
ponte** — não é um invariante medido. O mesmo vale para "o AETHER entrega um
modelo de si de 16 dimensões": hoje o que existe é `should_abstain()` da M5.

### 3.5 Placar e fila desatualizados em um ponto

As "3 respondidas / 1 parcial / 3 abertas" continuam corretas. Mas a fila lista
o **E1 como pendente** quando já há artefato (1.000 fatos, recall 1,000 vs 0,038
plano, SNR 6,75σ, 17,4 µs, 0 alloc). O E1 está *executado e não reproduzível* —
estado diferente de "a executar".

### 3.6 Versão

O Vol. III fala em "crate em 0.0.67+"; `Cargo.toml` está em **0.0.93**.

---

## 4. O que isso muda na fila

| Antes | Depois da auditoria |
|---|---|
| 1º E2c-2 (roof dual) | **0º consertar `phase_language_probe.rs`** (5 erros) — sem isso o E1 não volta a rodar |
| 2º E1b-2 (padrões harmônicos) | **1º registrar o E1b-2 já medido** (harmônico 8 vs aleatório 4) e corrigir `pattern_capacity()` |
| 3º F1 | 2º E2c-2 (roof dual) — inalterado |
| — | 3º F1 |
| — | Ressalvas a corrigir na prosa: RG/torque no PLH, "20 MB", "penhasco", orçamento de 17,4 µs |

**Nada nos números do núcleo precisa ser refeito.** O que precisa é: um build
verde nos exemplos, dois parágrafos de registro (E1b-2) e três correções de
atribuição (RG, 20 MB, penhasco). O resto da tese — núcleo determinístico,
borda sharded, ledger assíncrono — não foi falsificado por nenhum artefato.

---

## 5. Comandos usados

```powershell
cargo test --release --tests            # 28 alvos, 0 falhas
cargo test --release                    # FALHA: examples/phase_language_probe.rs
cargo run --release --example e2c_persistency -- --plant 0.0  --instances 200
cargo run --release --example e2c_persistency -- --plant 0.25 --instances 200
cargo run --release --example e2c_persistency -- --plant 0.45 --instances 200
cargo run --release --example e2b_presolve
cargo run --release --example e1b_capacity
```

Artefatos: `target/aether_profile_e6.json` · `aether_e1b.json` ·
`aether_e1_plh.json` · `aether_e2b.json` · `aether_e2c.json` ·
`aether_e3b.json` · `aether_e3b_max2sat.json` · `aether_bench_throughput.json` ·
`aether_scorecard.json`.

---

## 6. Adendo — desdobramentos no fórum (2026-11-15 21:5x)

Depois da publicação desta auditoria no `FORUM.txt` (MSG 0112/0118/0120), os
donos dos experimentos confirmaram ou corrigiram os pontos abertos:

| Ponto | Desdobramento | Fonte |
|---|---|---|
| Curva da M5 (0,126 / 0,274) | **Confirmado como contaminado por carry-over**: faltava `reset_phases(0)` entre recalls. Valores válidos: **0,0344 / 0,1896** (fator 3,7×). | MSG 0129 |
| `pattern_capacity()` | Docstring errado em `phase_memory.rs:372`; gate como escrito **falha por construção** (M16 = 0,362 > 0,15). Capacidade = maior M com erro < 0,15 → **harmônico entre 8 e 16**, aleatório 4. | MSG 0131 |
| E1 — dois desenhos | Adotada a nomenclatura **E1a** (2 papéis, artefato 1,0000) e **E1b** (4 papéis + ACF, 0,1675). O número antigo não vale para o desenho novo. | MSG 0120/0124 |
| Roof dual | Protótipo externo mede **bound sound** mas **0,000 fixings** em n=14; auditoria exigiu **controle positivo** (variável constante em todos os ótimos, não dominada) antes de aceitar falsificação. | MSG 0126/0132 |
| Build v0.0.95 | 3 erros de tipo corrigidos; **lib compila**, mas `--lib` tem **1 falha**: `emissao_de_termos_preserva_penalidade` (mask=0 → e=−1, want=+1; offset de 2 no QUBO). Gate mantido, não relaxado. | MSG 0125/0136/0145 |
| F1 — R8 | `cue_into` itera **todos** os registros sem filtrar escopo; `scope_filter` existe só em comentário; `cross_scope_hits` nunca é incrementado; o teste de R8 **não chama `cue_into`** → gate vacuamente verde. Roteado ao dono. | MSG 0140 |
| F2 — overflow | Drop-oldest é **inviável em SPSC** (quebraria o invariante lock-free). Condição revisada para **drop-newest + contador + bit no próximo evento**; "0 eventos perdidos" só vale com `drops=0` medido. | MSG 0141/0144 |
| F1 — receita de R8 | Receita aceita: 2 escopos, consulta cruzada deve dar 0; **passo positivo** (injetar B no store de A e esperar 1) é o que impede o gate vazio; P99 com anel cheio; contador separado para payload grande. | MSG 0148/0154 |
| E2c-2 — fecho | Controle positivo **PASS** (estrela K=2..12; 13/13 fixings). Medição correta: vidro puro **0/200** (roof = dominância), plant 25% **+0,3 pp** (1/200), plant 45% **+9,1 pp** (32/200), **0 unsound em 600** nos dois. Extensão real **sob viés**, nula no vidro — não é oráculo global. | MSG 0142/0149/0158 |
| E1 v2 — gates | `tests/e1_language_gate.rs`: **4/5** depois da correção de escala que apontei (`normalize_into` deixava o fasor em `n_fatos/Q14`, ~10³ abaixo de 1, e o ressonador subtraía vetores unitários de uma pista 10³ menor). g1/g3 passaram; resta **g5: p50 112,7 µs** vs 20 µs. | MSG 0152/0160/0166/0168 |

**Regra operacional que ficou acordada:** o auditor é *read-only*; cada erro é
roteado ao dono do claim (0106 = example/docs E1; 0107 = E2/E3; 0110 = roof
dual; 0111 = E1b-2; 0121 = corpo 3; 0130 = F1). Nenhum arquivo de outro claim
foi editado.

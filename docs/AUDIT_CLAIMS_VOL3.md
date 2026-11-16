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
| F1 — R8 (confirmado) | Reprodução independente em caixa-preta: 6 registros (3/escopo); `cue_into` de A **devolve vetor de B**; `cross_scope_hits` fica 0; e o retorno `consulted` está errado com 6 candidatos. Fix acordado: `ScopeId` explícito + `CueHit{key,scope,authority,hits}`. | MSG 0171/0176 |
| E3 — transplant gates | `tests/e3_transplant_gate.rs`: 1 passa, **4 falham** — e os 4 são **de teste**: ρ esperado positivo contra Hamming (por construção é −0,954, logo |ρ| passa); `value_index` comparado a `local` com 40 valores em 200 fatos; OOB 48 vs 50 na analogia; OOD com pista degenerada (`sin·3`, não fasor unitário). | MSG 0186 |
| E2c-2 — gate | Gate corrigido: **`ge` no h=0** (o roof parte do que a dominância fixa) + **estrito sob viés** (k6, δ=0,6, 200 inst). O truque z2 não entra: custa completude (171 vs 185). Soundness ×2 mantidas. | MSG 0172/0177 |
| E2c-2 — fecho | Controle positivo **PASS** (estrela K=2..12; 13/13 fixings). Medição correta: vidro puro **0/200** (roof = dominância), plant 25% **+0,3 pp** (1/200), plant 45% **+9,1 pp** (32/200), **0 unsound em 600** nos dois. Extensão real **sob viés**, nula no vidro — não é oráculo global. | MSG 0142/0149/0158 |
| E1 v2 — gates | `tests/e1_language_gate.rs`: **5/5 PASS** (p50 13,9 µs / p99 35,7 µs) depois da correção de escala que apontei (`normalize_into` deixava o fasor em `n_fatos/Q14`, ~10³ abaixo de 1, e o ressonador subtraía vetores unitários de uma pista 10³ menor). | MSG 0152/0160/0166/0168/0182 |
| E1a — exemplo | `examples/phase_language_probe.rs`: 1.000 fatos, recall **1,0000**, plano **0,0725**, SNR **5,88**, p50 **6,40 µs**, p99 **25,70 µs**, 0 alloc, 436 KiB (34% da L2), abstenção calibrada. O MSG 0169 publicou 5,80/7,80 µs, que **não batem** com o JSON. | MSG 0169/0170 + `target/aether_e1_plh.json` |

**Regra operacional que ficou acordada:** o auditor é *read-only*; cada erro é
roteado ao dono do claim (0106 = example/docs E1; 0107 = E2/E3; 0110 = roof
dual; 0111 = E1b-2; 0121 = corpo 3; 0130 = F1). Nenhum arquivo de outro claim
foi editado.

### 6.1 Fecho — 2026-11-15 22:59, v0.0.99

| Métrica | Valor |
|---|---|
| Alvos de teste verdes | **15** (139 testes somados) |
| Único alvo vermelho | `e3_transplant_gate` — **4/6** |
| Vermelhos restantes | 2 bugs **de teste**: OOB 48 vs 50 na analogia; pista in-domain degenerada no G3 |
| Verificado por mim | E1 5/5 · F1 4/4 (filtro + `CueHit` + `skipped`) · E2c2 5/5 |
| Não auditado por mim | E4/E5/E7/E8/F2/F3 do MSG 0190 (números de terceiros) |
| Auditoria coberta | E6 · E6b · E1b · E2b · E2c · E3b · E1a · E1b-v2 · E2c-2 · F1 · E3 |

O que mudou por causa desta auditoria: a curva da M5 (carry-over, 3,7×), a escala
do PLH (recall 0 → 5/5), o gate de R8 do F1 (vácuo → controle positivo), o gate
do E2c-2 (insatisfazível → `ge` no h=0), o offset do QUBO (−2, corrigido) e a
triagem dos 4 gates do E3. Nenhum número do núcleo (E6/E6b/E2b/E2c/E3b/E1b)
precisou ser refeito.

---

## 7. Auditoria do Corpo 3 — E4/E5/E7/E8/F2/F3 (2026-11-15 23:40)

Método: reexecução **isolada** (`--test-threads=1 --nocapture`), comparando com
os números do MSG 0190. Todos os gates passam; as ressalvas estão na última
coluna.

| Gate | Publicado (0190) | Re-medido | Veredito |
|---|---|---|---|
| **E4** self-field | ECE 0,3567 → 0,3238; CI p97,5 = −0,0168; T01 1,000; pós-plasticidade 0,0140 vs 0,0562; topologia 0,1507 → 0,1391 | idêntico | ✅ confere |
| **E5** currículo | readaptação 8 (T03 = 150); outliers 7 → 1; 0 alloc | idêntico | ✅ confere |
| **E7** corpo S0 | H1 1,9725 vs 0,5539; H2 0,8808; H3 0,7192 | H1 2,0093 vs 0,7285; H2 0,8930; H3 0,637 | ✅ passa, **variância alta** (drop 63,7–71,9%; alarmes 2–13) |
| **E8** reservoir | frozen 0,0791; joint 0,3132; speedup 2,94×; 615.681 vs 88.220 pred/s | frozen 0,0890; joint 0,2168; speedup 3,23×; 611.708 vs 81.492 | ✅ passa, **margem de 11%** até o teto 0,10 |
| **F2** consolidação | 0 perdidos (320), 0 chamadas T0, gist 1,000, R9 = 1 | idêntico | ✅ confere |
| **F3** fusão 2D | quadrante 100/100, ruído 300/300, perda 0, R8 = 0 | idêntico | ✅ confere |

### 7.1 Três ressalvas que o número esconde

1. **E8 — o baseline é Oja local, não backprop.** `bench_joint` ajusta a física
   por regra de Oja; `regression = max(0, (frozen − joint)/joint)` é
   **unilateral**. "Congelar não custa acurácia" vale contra esse baseline —
   não contra um MLP treinado. O caveat do MSG 0190 está correto, mas a frase
   do dossiê precisa carregar "vs. Oja local".
2. **E1 g5 é flaky por carga.** Isolado: **p50 11,40 µs / p99 14,70 µs** (passa).
   Na suíte paralela: **30,10 µs** (falha). O gate mede *wall time* dentro de um
   `cargo test` concorrente. Ou o CI roda o alvo com `--test-threads=1`, ou a
   latência vira um alvo dedicado — senão "5/5" depende da máquina estar ociosa.
3. **E7 varia entre execuções** (drop 63,7–71,9%, alarmes 2–13). Passa o gate
   (≥30%), mas o artefato de uma execução não é reprodutível; publicar seed e
   intervalo, não um ponto.

### 7.2 `pattern_capacity()` — janela ≠ recall

O valor **40** em `N=160` é a **janela de armazenamento** (`min(N/4, 64)`), não
a capacidade de recall: com pista a 20% oculta e erro < 0,15 rad, mediu-se
**~4 aleatórios / ~8 harmônicos** (E1b-2). Não é bug de código; era docstring
errado (`min(N−1, 64)`) e nome ambíguo. Corrigido só na documentação
(CLAIM 0211), sem mudar comportamento — `e1b_capacity` (7) e
`phase2_m5_hardening` (13) seguem verdes.

### 7.3 Placar das sete perguntas (atualizado)

| # | Pergunta | Status com a evidência de hoje |
|---|---|---|
| 1 | Onde vão os 146 µs? | ✅ E6 — 84,4% no solver |
| 2 | ΔE prevê acerto? | ✅ E3b — não em otimização livre; 100% com violações = 0 |
| 3 | Quantos padrões em D=160? | ✅ E1b — 64 fatos (K=6); M5: 4 aleatórios / 8 harmônicos |
| 4 | Roof dual reduz o suficiente? | ⏳ **parcial** — +9,1 pp sob viés, 0 no vidro puro |
| 5 | Self-field melhora calibração? | ✅ E4 — ECE cai 0,0322 (CI exclui 0) **sem** mexer em T01 |
| 6 | Atrator vira gist sem perder habilidade? | ✅ F2 — retenção 1,000 no harness |
| 7 | A ponte de fase preserva? | ✅ F3 — perda 0,000 no round-trip |

**Placar: 6 respondidas, 1 parcial, 0 abertas** — com o escopo declarado: F2/F3
são harness local (não o SGDB real) e o E4 é um único seed. A pergunta 4
continua sendo a única científica em aberto.

---

## 8. Q4 — o que a literatura diz e o que o protótipo mediu

### 8.1 Três camadas acima do roof dual

| Fonte | O que dá | Custo |
|---|---|---|
| Hammer–Hansen–Simeone 1984 | roof dual + persistência | max-flow |
| Boros–Hammer 2002 | posiforme, bound `C+ν` | `O(n⁵)` Edmonds–Karp |
| Boros et al. 2006 | **probing** de variáveis; preprocessing | `2n` roof duals |
| Rother et al. 2007 (QPBO-P) | probing + contração, exato | `2n` roof duals |
| Boros et al. 2008 | **iterated roof dual**, bound mais forte | `O(n³)` por iteração |
| Shekhovtsov 2014 | **maximum persistency** via LP | LP |
| Naumann–Andres 2026 | **teorema negativo**: todo posiforme ótimo de roof dual já dá o maior domínio de autarkia | — |
| Wang–Hahn 2026 | probing de `k` variáveis + **2-SAT** certifica o ótimo global | `2^k` roof duals |
| Del Pia–Di Gregorio; Boros–Crama–Hammer | odd-cycle / fecho de Chvátal–Gomory: relaxações mais estreitas | separação/LP |

### 8.2 O teorema que fecha a porta errada

Naumann & Andres (arXiv:2610.20346, 2026) provam que **nenhum autarky de
posiforme quadrático fixa mais variáveis que um roof dual ótimo**. Ou seja:
“strong persistency” via autarkias/BHS **não é** uma extensão — é o mesmo
conjunto. Para ir além é preciso sair de autarkias (probing, bound, 2-SAT).

### 8.3 O protótipo (n=14, 200 instâncias/regime, `examples/e2c3_probe.rs`)

| plant | roof fix | probe fix | extra | nós DFS | folhas | folhas/2¹⁴ | sound |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0% | 0,00 | 0,00 | **0,00** | 1.017 | 330 | 2,0% | 20/20 |
| 25% | 4,00 | 4,00 | **0,00** | 59 | 19 | 0,12% | 200/200 |
| 45% | 7,10 | 7,10 | **0,00** | 16 | 5 | 0,032% | 200/200 |

Três leituras:

1. **Probing por união de persistências = 0 extra em 600 instâncias.** Consistente
   com o teorema: a implementação já extrai o autarky máximo.
2. **Busca com propagação de roof dual é exata** (420/420 contra força bruta) e
   reduz a árvore em **50× (plant 0)** a **3.000× (plant 45)**.
3. **No vidro puro, 0 fixações no root é teorema** (simetria `s → −s`): nenhuma
   variável é persistente. Mas **uma ramificação quebra a simetria** e o roof
   dual passa a fixar o resto — 1.017 nós em vez de 16.384.

### 8.4 A resposta para o Q4

A pergunta estava mal posta: **“quantas variáveis o roof dual fixa no root?”**
não é a métrica. Em vidro simétrico a resposta é 0 por teorema, e nenhum método
de persistência muda isso. A métrica útil é **tamanho da árvore** e
**certificado**:

- **E2c-3A (curto prazo, exato):** DFS/DPLL com propagação de roof dual.
  Gate: soundness 420/420; nós médios < 2.000 (plant 0) e < 100 (plant ≥ 25);
  substitui o BSB heurístico por ótimo **certificado**.
- **E2c-3B (certificado global sem enumerar):** probing de `k≈n/2` + 2-SAT do
  posiforme residual (Wang–Hahn 2026). Para `n=14`, `2⁷=128` roof duals por
  instância. Requer expor o **posiforme residual** (hoje `solve_into` devolve só
  `fixed`).
- **E2c-3C (escala até n=64):** B&B com o **bound** `C+ν` — hoje `pass_into`
  calcula o fluxo mas não o expõe. Gate: fechar `n=20..24` com nós ≤ 10⁴.

**Não fazer:** implementar strong persistency/BHS como “extensão do E2c-2” — o
teorema de 2026 garante que o ganho é zero.

### 8.5 E2c-3A implementado (2026-11-16)

| Artefato | Conteúdo |
|---|---|
| `src/core/persistency_search.rs` | `ExactSearch` (workspace 0-alloc após `new`), `solve` exato com roof dual como propagador, `SearchReport{nodes,leaves,root_fixed,exhausted}` |
| `tests/e2c3_exact_search.rs` | 4 gates: soundness 150 casos vs força bruta; root 0 em vidro denso; árvore pequena sob viés; **n=20** |
| `examples/e2c3_probe.rs` | probe (root vs probing) + busca exata via o módulo |
| `target/aether_e2c3_probe.json` | artefato dos três regimes |

Números verificados: `n=14` — **420/420 sound**, nós médios **1.017 / 59 / 16**
(plant 0/25/45), folhas 2,0% / 0,12% / 0,032% de `2¹⁴`; `n=20` sob viés —
**66 nós** contra `2²⁰ = 1.048.576` (fator ≈16.000×). Probing por persistência
continua **0 extra** em 600 instâncias.

**Limite:** o pior caso é exponencial; `n=64` exige o bound (E2c-3C) para podar
por energia. O que está fechado é o Q4 **como medido**: em `n=14–20`, o roof
dual deixa de ser um redutor e passa a ser um propagador exato.

### 8.6 Fronteira de escala (2026-11-16)

Busca exata, plant 45%, 10 instâncias por `n`, orçamento 500k nós:

| n | 20 | 24 | 28 | 32 | 40 | 48 | 64 |
|---|---:|---:|---:|---:|---:|---:|---:|
| fechou | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | **0/10** |
| nós (média) | 77 | 183 | 624 | 3.122 | 25.833 | 222.270 | >5M |
| ms/inst | 0,3 | 0,6 | 2,3 | 12,3 | 117,8 | 1.159 | ~50.000 |

n=64 com 5M nós **não fecha** (três instâncias, ~50 s cada). O penhasco está
entre 48 e 64: sem bound, a busca é exata mas explode. Duas conclusões:

1. **Até n=48, o E2c-3A basta** (segundos por instância).
2. **n=64 exige o E2c-3C** (B&B com bound `C+ν`).

Teste da hipótese “dá para montar o bound só com a API pública”: **não**.
`pass_into` devolve o fluxo, mas `c0 + flow` varia de +110 a +353 contra ótimos
de ≈ −100 — falta a constante do posiforme. O bound precisa ser exposto pelo
dono do `presolve.rs`:

```rust
pub fn bound_into(&mut self, n, j, h, fixed: &mut [Option<i8>]) -> f64
// retorna C+ν (bound), preenche fixed; solve_into inalterado
```

Gate proposto: `bound ≤ ótimo` em 600 instâncias (força bruta n=14) + relatório
de gap. Com isso, `ExactSearch` ganha um modo B&B (podar quando
`const_fixo + bound_resíduo ≥ incumbente`).

**Tentativa (autorizada) e recuo:** implementei `bound_into` como `c0 − flow`
e rodei o gate. Passou em **90 instâncias densas** (gap médio 141,6), mas
**falhou no caso analítico n=1**: `H=−s`, ótimo −1, bound devolveu **+1**. Não é
um bound válido. Causa: `pass_into` não acumula a constante dos termos
negativos do posiforme. Para `n=1`, `l₀=−2` e o bound correto é
`c0 + min(0,l₀) = 1−2 = −1`. A correção exige somar `C` dentro de `pass_into`
(linear negativo e constante da decomposição quadrática) — não é um wrapper.
**Revertido**, árvore verde (45 alvos, 0 falhas); o E2c-3C fica aberto até essa
constante existir, com o gate analítico `n=1/n=2` **antes** dos 600 casos.

**E2c-3B** (certificado 2-SAT): precisa do **posiforme residual** (termos
lineares/quadráticos após o flow), hoje não exposto. Para n=14, `k=7` → 128
roof duals por instância; o gate é a taxa de certificação global.

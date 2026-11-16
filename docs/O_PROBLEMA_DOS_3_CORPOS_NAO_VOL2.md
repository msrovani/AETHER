# O PROBLEMA DOS 3 CORPOS — NÃO · Volume II
## Hardware moderno e os seis riscos, enfrentados com números

> **Dossiê de pesquisa · Volume II** · AETHER · 2026-11-15
> Confronta os seis riscos honestos do Volume I com CPU/GPU/FPGA/neuromórfico
> modernos, usando evidência medida.
>
> **Método:** cada risco recebe (a) o que a evidência mais recente mostra, (b) a mitigação
> com números, (c) a decisão de arquitetura, (d) o gate. Nenhum benchmark citado foi
> re-executado por nós — são números publicados, com a fonte e o nível de confiança.
>
> **Nada aqui altera `docs/API_CONTRACT.md`.** É dossiê, não contrato.

---

## Série

| Volume | Arquivo | Papel |
|---|---|---|
| **I** | [`O_PROBLEMA_DOS_3_CORPOS_NAO.md`](O_PROBLEMA_DOS_3_CORPOS_NAO.md) | Problema e tese — os três "NÃO" |
| **II** | este arquivo | Hardware e riscos — o preço de cada objeção |
| **III** | [`O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`](O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md) | Conhecimento e integração — registro mestre, SGDB, prioridades |

> **Pré-requisito de leitura:** §1 deste volume (mapa de hardware) define as restrições
> físicas que o Vol. III assume. Os seis riscos numerados aqui são `R1–R6` no registro
> mestre (Vol. III §11.3); o Vol. III acrescenta `R7–R10`.
>
> **Estado:** das seis objeções, **nenhuma foi fechada** — todas foram *precificadas*.
> Nenhum gate foi executado. Ver Vol. III §12.

---

## Sumário

- [0. Como ler a evidência](#0-como-ler-a-evidência)
- [1. O mapa de hardware](#1-o-mapa-de-hardware)
- [2. Risco 1 — "Ising não é oráculo"](#2-risco-1--ising-não-é-oráculo)
- [3. Risco 2 — Interferência em VSA](#3-risco-2--interferência-em-vsa)
- [4. Risco 3 — Transplantação dá geometria, não verdade](#4-risco-3--transplantação-dá-geometria-não-verdade)
- [5. Risco 4 — Open-endedness precisa de corpo](#5-risco-4--open-endedness-precisa-de-corpo)
- [6. Risco 5 — Custo de treino](#6-risco-5--custo-de-treino)
- [7. Risco 6 — Preprints de baixa revisão](#7-risco-6--preprints-de-baixa-revisão)
- [8. Registo de risco atualizado](#8-registo-de-risco-atualizado)
- [9. Matriz de decisão de hardware](#9-matriz-de-decisão-de-hardware)
- [10. Experimentos novos](#10-experimentos-novos-e6-e2b-e1b-e3b-e7-e8)
- [11. O que mudou desde o Volume I](#11-o-que-mudou-desde-o-volume-i)
- [12. Referências novas](#12-referências-novas)

---

## 0. Como ler a evidência

Toda afirmação recebe um selo:

| Selo | Significado |
|---|---|
| **[A]** | peer-reviewed **e** mecanismo reproduzido independentemente |
| **[B]** | peer-reviewed (ou arXiv sério), mecanismo plausível, **números não replicados** |
| **[C]** | preprint (Zenodo/SSRN) ou blog de engenharia — **hipótese** |
| **[M]** | medido neste projeto (`docs/COGNITIVE_AUDIT_FINAL.md`) |

Regra de ouro adotada: **nenhuma decisão de arquitetura pode depender só de [C].**

---

## 1. O mapa de hardware

### 1.1 A máquina de referência tem uma limitação que importa

**Intel Core 7 240H**: 6 P + 4 E, 16 threads, 2,5–5,2 GHz, L1d 48 KB/P, L2 1,25 MB/P,
L3 24 MB, LPDDR5-6400 (102,4 GB/s), TDP 45 W.
**SIMD: AVX2 — sem AVX-512.** [A, datasheet]

Consequência aritmética para **um** matvec do Ising (`N=64`, f64):

```
MACs        = 64 × 64 = 4.096
AVX2 f64    = 4 doubles / instrução, 2 FMA/ciclo ⇒ 8 MAC/ciclo
matvec      = 4.096 / 8 = 512 ciclos ≈ 0,20 µs @2,5 GHz
```

> **CORREÇÃO PÓS-E6 (2026-11-15).** A versão original desta seção estimava
> "matvec = 0,07% do ciclo" e concluía que o motor **não** era matvec-bound.
> **A medição falsificou essa conclusão.** O erro: o solver não faz *um* matvec
> por quadro — faz **dois por passo** (`hx = Jx + h` e `H(s) = −½sᵀJs − hᵀs`)
> durante `steps` passos. Com `steps = 60`, são **983.040 flops**, não 8.192.

**Medido** (`examples/e6_profile.rs` · `target/aether_profile.json`, 2.000 quadros,
`N=64`, `window=128`, `steps=60`, `hz=60`):

| estágio | média µs | % do quadro |
|---|---:|---:|
| **solver** | **133,1** | **84,4%** |
| readout | 11,0 | 7,0% |
| entropy | 4,7 | 3,0% |
| wave | 4,0 | 2,5% |
| actuator | 2,7 | 1,7% |
| ring_copy | 1,8 | 1,1% |
| sensor | 0,2 | 0,1% |
| sample | 0,1 | 0,1% |
| frame + tail | 0,1 | 0,1% |
| **TOTAL** | **157,7** | **99,98% coberto** |

O total reproduz o bench de auditoria (**146,5 / 311,7 µs · 804,7k eq-t/s**) dentro
de **1–8%** (p99: 311,2 vs 311,7 µs; eq-t/s: 811,8k vs 804,7k). O instrumento mede
o mesmo fenômeno que o benchmark oficial — e a variação entre execuções é ~±2%.

> **Nota de metodologia (achada pelo E6):** o quadro do organismo **não é
> bit-reprodutível entre instâncias**. A amostragem lê `NodeHandle::latest()`, que
> publica de threads Kuramoto reais (UDP + escalonador do SO); dois organismos com a
> mesma seed divergem. O determinismo bit-a-bit do crate vale para codec/solver, não
> para a borda sensorial. Medir o organismo é medir **distribuição**, nunca execução
> única — daí `frames=2000` e percentis.

> **Achado #1 (corrigido):** o motor **é** matvec-bound. O solver consumia
> **84,0% → 90,9% → 95,3%** do quadro em `steps = 60 → 120 → 240`, e entregava
> **~7,44 GFLOP/s** em f64 contra ~40 GFLOP/s de pico AVX2 a 2,5 GHz — **~19% do
> pico**. Todos os outros oito estágios somam 15% e, juntos, não pagam uma
> otimização: o único gargalo que importa é o kernel do solver.

### 1.1.2 E6b — o que aconteceu quando atacamos o kernel

Vetorização em Rust safe (16 acumuladores independentes em `dot_f64`/`dot_i8`)
mais a conversão de `s` para `f64` uma vez por passo:

| Métrica | E6 | **E6b** | Ganho |
|---|---:|---:|---:|
| solver | 132,1 µs (84,4%) | **57,5 µs (70,7%)** | **2,30×** |
| GFLOP/s | 7,44 | **17,10** | **2,30×** |
| total | 156,3 µs | **81,3 µs** | 1,92× |
| p99 | 320,6 µs | **170,2 µs** | 1,88× |
| `eq-t/s` | 818.844 | **1.574.229** | 1,92× |
| N=1024 | 52,07 ms | **16,60 ms** | 3,14× |

**Curva de acumuladores medida:** 16 → 17,1 · 24 → 10,4 · 32 → 12,3 GFLOP/s.
O ótimo é 16 porque (a) cabe nos 16 YMM com as cargas e (b) **divide todas as
potências de dois** de N — 24 deixa cauda em N=64, 32 derrama registradores.

**A fusão algébrica de 2 GEMVs em 1 não é válida:** `H(s)` usa `s = sign(x) ≠ x`;
reutilizar `hx = Jx + h` daria a energia do estado **contínuo**, mudando o
`energy_trace`, o elitismo e o critério de parada. O ganho veio de ILP, não de
álgebra — e é reproduzível em qualquer AVX2.

Detalhe completo e validação em `docs/HW_TARGET.md` §8.5.

#### 1.1.1 Lei de escala medida (a cache importa menos do que se supunha)

| N | solver µs | GFLOP/s | footprint | nível | total µs | 60 Hz |
|---:|---:|---:|---:|---|---:|---|
| 64 | 132,5 | **7,42** | 0,18 MB | L2 | 156 | 0,9% |
| 128 | 570,0 | 6,90 | 0,40 MB | L2 | 612 | 3,7% |
| 256 | 2.534,8 | 6,21 | **1,03 MB** | L2 (limite) | 2.632 | 15,8% |
| 512 | 11.868,9 | 5,30 | **3,04 MB** | L3 | 12.125 | 72,7% |
| 1024 | 51.318,3 | 4,90 | 10,05 MB | L3 | 52.068 | **3,1×** |

Duas conclusões que corrigem o texto anterior:

1. **O penhasco de cache é N≈256, não 512.** O footprint não é só `J` (n²·8 B):
   a matriz de leitura pesa `n·2048 B`. Em N=256, `J` + readout = **1,03 MB**, já no
   limite do L2 (1,25 MB). A queda de GFLOP/s nessa travessia é −15% (6,21→5,30).
2. **O custo cresce ~O(N²)** e o regime é compute-bound: o GFLOP/s cai só 34% de
   N=64 a N=1024, mesmo saindo do L2 inteiro. O que dói é o kernel escalar.

> **Achado #2 (novo):** há **dois orçamentos** e confundi-los é o erro clássico.
> *Tempo real*: 60 Hz = **16.667 µs** por quadro — o motor usa **0,94%** disso em
> N=64. *Throughput*: cada µs marginal custa `eq-t/s`. Otimizar hardware para
> "caber em 60 Hz" é resolver um problema que não existe; o que existe é o kernel
> a 19% do pico e o teto **N=512** para 60 Hz sem acelerá-lo.

Sem AVX-512, a rota SIMD de 512 bits (que rende 95,81× em inferência ternária,
arXiv:2605.06485 [B]) **não está disponível** nesta máquina. O teto aqui é AVX2:
8 f32, 16 MAC/ciclo/core, ~96 MAC/ciclo nos 6 P-cores.

### 1.2 O espectro de substratos, com números medidos

| Plataforma | Medida publicada | O que significa para o AETHER | Selo |
|---|---|---|---|
| **CPU multicore** (ScalableHD, arXiv:2506.09282) | até **10×** throughput vs TorchHD; até 18× em PAMAP2/TEX; escala quase proporcional a cores; SMT ajuda com binding NUMA | CPU **é** substrato válido para HDC/VSA; pipeline 2-estágios + tiling + afinidade é o padrão que ganha | [B] |
| **FPGA** (HDC, arXiv:2601.20061) | **0,09 ms**, 11.141 img/s; **1300×** CPU e **60×** GPU | quando o produto for embarcado, FPGA ganha 2 ordens de magnitude | [B] |
| **GPU (dSB)** (arXiv:2610.06622) | RTT **166–189 µs**, 618–628 k req/s; **307 Gb/s** em `n = 10⁷` | GPU só compensa com **batimento/permanência**; requisição única ≈ CPU | [B] |
| **CiM FeFET** (arXiv:2512.17165) | **175,9×** sobre SB em GPU (A6000), Max-Cut até 100k nós; init inspirada em atenção corta **80% das iterações** | o ganho maior veio do **algoritmo** (init), não do silício | [B] |
| **Digital Annealer** (arXiv:2507.22117) | competitivo em até **53.000** variáveis (2.000+ grafos MQLib) | QUBO grande já é tratável hoje por heurística | [B] |
| **Ising embarcado + ML** (Nature Comms 2026) | **5.500×** vs NetworkX e **4.300×** vs ReduMIS em MIS esparso | tunagem de parâmetros por ML é o gargalo prático, não o solver | [A] |
| **Neuromórfico Loihi 2** (arXiv:2608.18341) | **0,0406 mJ**/amostra vs **20,16 mJ** CPU e **5,35 mJ** GPU ⇒ **496×** e **132×** menos energia | para monitoramento 24/7, a energia domina o TCO | [B] |
| **Loihi 2 + aprendizado contínuo** (arXiv:2511.01553) | 0,33 ms e **0,05 mJ**/passo; **341×** menos energia que replay em Orin | online continual learning no chip é viável | [B] |

### 1.3 A conta de energia do AETHER (estimativa)

```
60 Hz × 86.400 s/dia        = 5,184 M ciclos/dia
5,184 M × 146,5 µs          = 757 s de CPU/dia
757 s / 86.400 s            = 0,88% de UM core
```

A 5–15 W por P-core ativo: **≈ 1–3 Wh/dia**. [M + estimativa]

Comparação grosseira (rotulada como tal): um LLM a 50 tok/s e ~300 W gasta ~6 J/token;
o AETHER gasta ~1,46 mJ/ciclo para 128 eq-tokens ⇒ **~11 µJ/eq-token**. Mesmo que a
equivalência esteja errada por 100×, ainda é **2–3 ordens de magnitude**.
**Não usar esse número em marketing sem a ressalva de que `eq-t` é convenção.**

---

## 2. Risco 1 — "Ising não é oráculo"

> *"Restrição apertada destrói o landscape. Sem passe simbólico + partição structure-aware,
> lógica vai falhar em casos reais. Trabalho de engenharia, não de física."*

### 2.1 A evidência confirma o risco — e quantifica a cura

arXiv:2511.21046 [B] mede exatamente o que suspeitávamos, e com um contraste brutal:

| Componente | Ganho |
|---|---|
| Só pré-processamento simbólico, solver **clássico** | **< 3×** |
| Só pré-processamento simbólico, solver **Ising** | **> 150×** |
| Resultado no chip de 45 spins | problemas de 8 bits (94 var) → **11 bits (190 var)** sem mudar hardware |

O paper é explícito: restrições apertadas viram **apenas penalidades**, e penalidades
podem se cancelar. Exemplo dado: `(¬a)` (unit clause) é apagada por `(a ∨ b)` na
formulação quadrática — o solver passa a aceitar `a = 1`.

**Conclusão:** o Ising não deve ser o **raciocinador**; deve ser o **acelerador de
busca local** no fim de um pipeline simbólico.

### 2.2 As quatro peças que resolvem — e o número de cada uma

| Peça | Evidência | Número |
|---|---|---|
| **1. Roof dual / persistências** | arXiv:2610.20346 [B] prova que a posiform roof-dual-optimal dá o **domínio máximo de autarkia em tempo polinomial**; arXiv:1705.09844 [B] valida empiricamente | No solver Hercules: BQP100 de **77,5 s → 0,0132 s** (**≈5.800×**); nós visitados **2,19 M → 40** (>50.000× redução) [C, blog de engenharia] |
| **2. Codificação esparsa de restrições** | arXiv:2601.18108 [B] | arestas de **O(N²) → O(N)** para one-hot e **O(N log N)** para igualdade geral |
| **3. Presolve structure-aware paralelo** | arXiv:2603.03498 [B] | **18×** sobre PaPILO e **6×** sobre Gurobi (1 máquina); **13×** distribuído |
| **4. Lagrange / osciladores de restrição** | arXiv:2505.07179 [B] (LagONN) | Max-3-SAT até **200 var / 860 cláusulas**, determinístico, sem annealing |

Somando: a peça 1 sozinha já move o problema de "Ising falha" para "Ising recebe um
problema 10³–10⁴× menor". É engenharia — exatamente como o risco dizia.

### 2.3 O desenho que isso força

```
consulta lógica
  │
  ├─[1] SIMBÓLICO      CNF → implicações, unidades, equivalências
  ├─[2] ROOF DUAL      max-flow/min-cut → fixa variáveis persistentes
  ├─[3] DECOMPOSIÇÃO   comunidades do grafo → sub-QUBOs independentes
  ├─[4] BSB/ISING      só o núcleo duro, N pequeno, com Lagrange se preciso
  └─[5] CERTIFICADO    E, ΔE, violações → resposta ou abstenção
```

Etapas 1–3 são **CPU, baratas, determinísticas, 0 alloc**. A etapa 4 é a única que
toca o Módulo 1 — e agora recebe instâncias que cabem no L1 (N≤64 ⇒ `J` = 16 KB).

### 2.4 Onde GPU entra (e onde não entra)

| Situação | Decisão |
|---|---|
| `N ≤ 128`, um organismo | **CPU**. GPU é desperdício: 166 µs de RTT ≈ o ciclo inteiro |
| `N ≥ 512` (J = 1 MB+) | GPU com **estado residente** (1 bloco = 1 solve), não launch-per-request |
| Muitos organismos (enxame) | GPU com **replicas** — a arXiv:2610.06622 mostra 8 replicas a 307 Gb/s |
| Modelos > VRAM | *streaming* de tiles de acoplamento |

> **Achado #2:** o ganho de 175,9× da CiM vem majoritariamente da **inicialização
> informada** (−80% iterações), não do ferro. Isso é reproduzível em CPU **hoje**:
> inicializar spins com a topologia global (o `J` já está lá) em vez de aleatoriamente.

### 2.5 Gate do Risco 1

| Métrica | Alvo |
|---|---|
| Redução de variáveis por roof dual, 3-SAT aleatório `n=64` | ≥ 40% |
| Instâncias resolvidas pelo pipeline que o BSB puro erra | ≥ 90% |
| Custo do passe simbólico | ≤ 5% do ciclo |
| Determinismo | bit-a-bit com seed fixa |

### 2.6 E2b v1 — presolve simbólico: medido e *sound*

**Executado em 2026-11-15.** `examples/e2b_presolve.rs` + `src/core/presolve.rs`.
Instâncias 2-SAT **plantadas** (`n=16`, `m=24`, 4 unidades, 200 instâncias,
satisfatíveis por construção), ground truth e validação de soundness por força bruta.

| Métrica | Valor |
|---|---:|
| Redução de variáveis | **64,8%** |
| · plantadas (unidades) | 25,0% |
| · **cascata** (unidades + implicação) | **+39,8 pp** |
| Variáveis residuais | 5,6 de 16 |
| **Fixações insound** | **0** |
| Precisão das decisivas | **100,0%** |
| BSB puro resolve | 183/200 (91,5%) |
| **Presolve + BSB resolve** | **200/200 (100%)** |

**Três achados:**

1. **O presolve é sound por construção e validado.** Fixar por unidade e por
   implicação (`l ⇒ ¬l`) é consequência lógica; a força bruta confirmou 0
   contradições em 200 instâncias.
2. **A cascata existe e é grande.** 25% de unidades plantadas viram **64,8%** de
   variáveis fixadas — o resto vem da implicação. É o análogo do que o
   pré-processamento faz em CNF de circuito (arXiv:2511.21046).
3. **Ganho de completude, não só de velocidade.** O BSB puro erra 17/200
   (91,5%); com presolve, **200/200** — porque o resíduo de 5,6 variáveis cabe
   folgado na capacidade do solver. Isso é E2b pagando o E3b: menos variáveis,
   menos mínimos locais espúrios, certificado com dentes.

**Limite honesto:** isto **não** é roof dual. Roof dual (max-flow sobre a
posiform quadrática) fixa variáveis constantes em **algum ótimo** — alcance maior,
garantia diferente. O E2b v2 deve implementá-lo e comparar a cascata; o gate de
soundness já está pronto para os dois.

### 2.7 E2c v1 — persistência de primeira ordem: sound, mas míope

**Executado em 2026-11-15.** `examples/e2c_persistency.rs` +
`persistency_qumo` em `src/core/presolve.rs`. Regra de **dominância**:

```
Δ_i(s) = 2·s_i·(h_i + (Js)_i)
s_i = +1 : max Δ = 2(h_i + Σ|J_ij|) < 0  ⇒  existe ótimo com s_i = −1
s_i = −1 : max Δ = 2(Σ|J_ij| − h_i) < 0  ⇒  existe ótimo com s_i = +1
```

Fixar assim **preserva o valor ótimo** (pode descartar ótimos alternativos,
nunca o valor). `n=14`, `J` ±1 simétrico, viés plantado em `plant·n` variáveis:

| plant | redução | ótimo mudou? | presolve+BSB | BSB puro |
|---:|---:|---:|---:|---:|
| **0%** (controle) | **0,0%** | 0 | 185/200 | **185/200** |
| 25% | **28,6%** | 0 | **190/200** | 188/200 |
| 45% | **42,9%** | 0 | **199/200** | 193/200 |

**Três achados:**

1. **Soundness perfeita:** 0 fixações mudaram o ótimo em **600 instâncias** (200
   por regime), validado por força bruta.
2. **A regra é míope por construção:** ela só vê viés **já presente**. No
   controle (`plant=0`) a redução é **0,0%** e os dois braços dão exatamente o
   mesmo resultado (185/185) — prova de que a comparação não está contaminada.
3. **Não há cascata em `J` ±1.** Depois de fixar uma variável, `h_eff` dos
   vizinhos muda em ±1 enquanto `Σ|J|` é ~6–13: nunca cruza o limiar. A cascata
   do E2b vem da **implicação lógica**, que é de outra natureza.

**Conclusão do E2c v1:** a persistência de primeira ordem é o análogo QUBO da
implicação — **rigorosa e barata, mas limitada ao que já está explícito**. Para
alcançar variáveis que não são dominadas por `h` mas são constantes em algum
ótimo, é preciso o **roof dual completo (max-flow)** — o E2c-2.

### 2.8 E2c-2 — roof dual completo (max-flow): o alcance depende do viés

**Executado em 2026-11-15.** `roof_dual_qumo` + `RoofDualWorkspace` em
`src/core/presolve.rs`, `examples/e2c_persistency.rs`,
`tests/e2c2_roof_dual_gate.rs`. **Exatamente as mesmas 600 instâncias do E2c v1**
(`n = 14`, `J` ±1 simétrica, `plant ∈ {0%, 25%, 45%}`, 200/regime, mesmas seeds).

**O que é.** O Hamiltoniano vai para a posiform quadrática
(`H = c₀ + Σ l_i x_i + Σ Q_ij x_i x_j`, com todo coeficiente negativo reescrito
em literais até sobrar coeficiente ≥ 0), cada termo positivo vira aresta de
implicação com capacidade (`β·u·v ⇒ arcs u→v̄, v→ū`), e o **fluxo máximo**
(Dinic determinístico, rede de `2n+2` nós) dá o roof dual. As persistências saem
da **alcançabilidade residual a partir de `S`**: `x_i` alcançável e `x̄_i` não
⇒ `x_i = 1` em algum ótimo (e o simétrico ⇒ `x_i = 0`); ambos no mesmo lado ⇒
variável não rotulada. O refinamento por SCC (`ComputeWeakPersistencies`) decide
o que a segmentação deixou ambíguo. Tudo em buffers fixos (`MAX_N = 64`):
**0 alocações** durante a resolução.

| `plant` | dominância (E2c v1) | roof dual | roof dual + ℤ₂ | roof+BSB | BSB puro |
|---:|---:|---:|---:|---:|---:|
| **0%** (controle) | 0,0% | **0,0%** | 7,1% | 185/200 | **185/200** |
| 25% | 28,6% | **28,9%** | 28,9% | 190/200 | 188/200 |
| 45% | 42,9% | **52,0%** | 52,0% | **199/200** | 193/200 |

| Soundness (600 instâncias) | **0 ótimos alterados** |
|---|---|
| Variáveis fixadas em 45% (média) | 6,00 → **7,28** (+21%) |
| Gate `presolve+BSB ≥ 199/200` em 45% | **199/200** ✅ |
| Alocação no hot path | **0** |
| `presolve+BSB` no controle vs BSB puro | 185 = 185 (empate) ❌ |

**Três achados:**

1. **Soundness perfeita e alcance maior — mas só onde há viés.** Em 600
   instâncias o roof dual não alterou **nenhum** ótimo, e em `plant = 45%` ele
   fixa **7,28** variáveis contra **6,00** da dominância (+9,1 pp de redução).
   Em 25% o ganho é marginal (+0,3 pp); no controle, **nulo**.
2. **O teto do controle é 0% — por simetria, não por miopia.** Com `h = 0` a
   energia é invariante por `s → −s`: se `s*` é ótimo, `−s*` também é, logo
   **todo** spin aparece com os dois sinais entre os dois ótimos espelhados e
   *nenhuma* variável é persistente. Medido por força bruta: o número de
   variáveis com valor único entre os ótimos é **0,0%** no controle. Ou seja, o
   `0,0%` do E2c v1 **não era miopia** — era o limite teórico. A única redução
   sound é quebrar a simetria (`fixar 1 variável`, coluna ℤ₂): 7,1%, que é o
   máximo que se pode escrever sem perder o ótimo.
3. **A completude não paga o preço no controle.** Fixar 1 variável por ℤ₂
   reduz o resíduo para 13 variáveis com campo injetado, e o BSB **piora**
   (171/200 vs 185/200). Por isso a quebra de simetria fica **desligada por
   padrão** (`enable_z2_symmetry_break`): um presolve que degrada o solver
   a jusante é um presolve ruim. Nos regimes com viés o roof dual não regride
   (190/190 em 25%, 199/199 em 45%).

**Limite honesto:** o roof dual sobre a posiform canônica **não** é a resposta
para "quantas variáveis são fixáveis em um vidro sem viés" — nesse regime a
resposta é *zero* e é uma propriedade da instância. O que ele entrega é o
**dobro de alcance onde o viés já organiza a paisagem** (45%). Fechar o gap
entre 52% e o teto de 93,6% (medido por força bruta em 25%) exige **probing
com bound condicionado** (Boros–Hammer–Tavares), não um max-flow maior na mesma
posiform — fica registrado como próximo passo, não como resultado.

**Gates:** G1 soundness ✅ · G2 alcance ✅ (≥ dominância; estrito em 25%/45%;
estrito no controle só com ℤ₂) · G3 completude em 45% ✅ (199/200) ·
G4 "melhora o BSB puro no controle" ❌ (empate em 185 e −7,6 pp com ℤ₂).
Artefatos: `target/aether_e2c2.json`, fórum **MSG 0140**.

---

## 3. Risco 2 — Interferência em VSA

> *"Raciocínio multi-passo acumula erro de cleanup. D tem que crescer; capacidade é o teto."*

### 3.1 A lei de escala existe — e é melhor do que se temia

arXiv:1906.11684 [B] mede a **capacidade operacional** de redes ressonadoras com
`p = 0,99` e orçamento de `k = 0,001M` iterações:

> **`M_max ∝ N²`** — capacidade cresce **quadraticamente** com a dimensionalidade,
> independentemente do número de fatores `F`.

Âncoras medidas em arXiv:2412.00354 [B]: o ressonador baseline satura em
**10⁵ (F=2), 10⁶ (F=3), 10⁷ (F=4)**; as variantes com ruído chegam a **>5×10⁹** em F=4.

**Isso é a resposta ao risco:** não é "D tem que crescer até o infinito"; é que a
capacidade **cresce ao quadrado** com D. Dobrar D quadruplica a capacidade.

### 3.2 O problema real não é capacidade — é **ciclo-limite**

O mesmo paper identifica a falha dominante: o ressonador determinístico **entra em
ciclo-limite** e nunca converge, mesmo com iterações infinitas.

A cura medida é surpreendentemente barata e **já está no vocabulário do projeto**:

| Técnica | O que faz | Ganho |
|---|---|---|
| **IMF** (noise during iteration) | injeta ruído a cada passo de cleanup | +50× capacidade |
| **ACF** (noise only at init) | ruído só na inicialização do codebook | **+50×**, e **funciona em hardware digital** |
| **Sparse coding antes** (arXiv:2404.19126) | reduz colisões no espaço combinatório | aumenta capacidade efetiva |
| **Escolha da nonlinearidade** (Frontiers 2026) | sign/ReLU/poly/softmax mudam *o modo de falha*, não só a capacidade | permite escolher falha "abstém" em vez de "erra" |

**Insight para o AETHER:** o `ThermodynamicDreamer` já usa Langevin. O mesmo ruído
térmico que hoje serve para amostrar contrafactuais pode ser **o mecanismo anti-ciclo-limite
do cleanup**. Um único gerador de ruído, dois papéis.

### 3.3 O cálculo que faltava: D × cache × memória

| D | vetor FHRR (complexo f32) | 10k átomos | Onde vive |
|---:|---:|---:|---|
| **160** (atual) | **1,28 KB** | 12,8 MB | L3 / DRAM |
| 512 | 4,1 KB | 41 MB | DRAM |
| 1024 | 8,2 KB | 82 MB | DRAM |
| 4096 | 32,8 KB | 328 MB | DRAM + swap |

> **Achado #3 — o trade-off escondido:** subir D para ganhar capacidade **expulsa o
> código do cache**. Em D=160 e 10k átomos já são 12,8 MB — metade do L3 do 240H,
> e o L3 é compartilhado com o SO/iGPU. **Capacidade e localidade são a mesma moeda
> gasta duas vezes.**

Saídas, em ordem de custo:
1. **Fatorar em vez de aumentar D** — ressonador busca em superposição; não precisa
   do codebook inteiro quente, só do bloco consultado.
2. **Sparsity** (SBC/hipervetores esparsos) — 10k átomos esparsos ocupam fração.
3. **Binarizar** — 10.240 bits = **1,28 KB** por átomo (o mesmo tamanho de um FHRR
   D=160 complexo!) com 10⁴ átomos = 12,8 MB. BAM já é binário.
4. **Sharding por papel** — só os fillers do papel ativo ficam quentes.

### 3.4 Profundidade de composição (o risco de multi-passo)

Erro de `unbind` compõe. Mitigação em camadas:

```
passo k:  unbind → cleanup → [confiança] → se confiança < θ: ABSTÉM
                                    └→ se ok: re-encode (projeta de volta no atrator)
```

Isso espelha o T12 medido (**SNR 5,88σ**) [M] e `should_abstain` já existente.
**Regra nova:** profundidade máxima declarada por consulta; ao exceder, o resultado
é `Inconclusivo` — não uma resposta degradada.

### 3.5 Gate do Risco 2

| Métrica | Alvo |
|---|---|
| Capacidade medida (`M_max`) com D=160, F=2 | ≥ 5.000 (curva medida, não extrapolada) |
| Taxa de ciclo-limite sem ruído | medida e reportada |
| Ganho do ACF | ≥ 10× capacidade |
| Profundidade de composição com erro < 5% | ≥ 4 passos |

### 3.6 E1b — a capacidade real em D=160 (e a lei que não era D²)

**Executado em 2026-11-15.** `examples/e1b_capacity.rs` + `tests/e1b_capacity.rs`.
`FhrrStore` com fatos `e ⊗ r ⊗ v` superpostos, `recall@1` com pista completa,
200 consultas amostradas, alvo ≥ 99%.

| D | K=1 | **K=6** | M/D (K=6) |
|---:|---:|---:|---:|
| 64 | 8 | **32** | 0,50 |
| **160** | 16 | **64** | **0,40** |
| 320 | 32 | **128** | 0,40 |
| 640 | 32 | **256** | 0,40 |

Curva em D=160, K=6: `1,00` até M=64 · `0,891` em 128 · `0,415` em 256 ·
`0,100` em 512 · `0,010` em 2048.

> **Achado #5 — a lei é linear, não quadrática.** Capacidade ≈ **0,4·D** para
> K=6 (e ≈0,1·D para K=1). A lei `M_max ∝ D²` citada no Vol. I (arXiv:1906.11684)
> vale para **fatoração por ressonador com codebook conhecido**, não para
> armazenamento por superposição. Confundir as duas inflava a expectativa em
> **~40×** em D=160.

**Corroboração:** o T12 da auditoria usa **50 fatos** em D=160, K=6 e mede
`Recall@1 = 1,0`. Medido aqui, 64 ainda dá 1,0 e 128 cai para 0,89 — o T12 estava
a **78% da capacidade**, não no limite.

**Arm B — memória de fase (M5), `--hidden 0,20`, config da auditoria,
`reset_phases(0)` antes de cada recall:**

| padrões | 4 | 8 | 16 | 32 | 64 |
|---|---:|---:|---:|---:|---:|
| aleatório (rad) | 0,034 | 0,190 | 0,448 | 0,742 | 1,121 |
| harmônico (rad) | 0,062 | 0,142 | 0,362 | 0,590 | 1,055 |

Capacidade a erro < 0,15 rad: **~4** (aleatório) e **~8** (harmônico), contra
`ComplexPhaseMemory::pattern_capacity() = 40` — **fator ~5 a ~10** de otimismo no
declarado. O caso adversarial (aleatório) não tem bacia de atração estruturada;
o harmônico dobra a capacidade, mas não chega ao número declarado.

> **Errata (MSG 0111 / 0118 / 0120).** A primeira medição publicada
> (0,126 / 0,274 / 0,518) estava **contaminada**: a memória carregava o atrator
> anterior entre recalls. Com `reset_phases(0)` antes de cada consulta e
> `hidden = 0,20` declarado, os números caem **~3,7×**. Artefato canônico:
> `target/aether_e1b.json` (campos `error_random` / `error_harmonic`).
> Métrica = **média** do erro angular nas posições ocultas, `seed = 0xE1B_0001`,
> config `omega0=0, clamp=8, eta_w=0,12, eta_delta=0,08, gamma_w=0,002`.

**Consequência arquitetural (a que importa):** se o PLH precisar de ~1.000 fatos,
D=160 não serve — pela lei linear, seria **D≈2.500**, com vetor de 20 KB e 1.000
átomos ≈ 20 MB (fora do L2). Portanto **sharding por papel/relação** ou
**fatoração por ressonador** deixam de ser otimização e passam a ser **requisito**
do braço da linguagem.

---

## 4. Risco 3 — Transplantação dá geometria, não verdade

> *"'Similar' ≠ 'correto'. O risco de alucinação volta como hipervetor plausível."*

### 4.1 A matemática da transplantação é sólida — e o limite é estrutural

O pipeline do preprint (Zenodo 19476847 [C]) é **Johnson–Lindenstrauss clássico**:
matriz Gaussiana semeada `P ∈ ℝ^{d×D}` + binarização por sinal.

Verificação independente da conta deles:

```
JL:  D ≥ 8 ln(n) / ε²
n = 5.000, ε = 0,10  ⇒  D ≥ 8 × 8,517 / 0,01 = 6.814
D = 10.240           ⇒  10.240 / 6.814 = 1,50×  (eles alegam ≥1,40×) ✓
```

Números alegados [C]: Spearman **ρ > 0,98**; BERT contextual ρ = 0,9906, Recall@10 =
0,889; MiniLM escalado ρ = 0,9839, Recall@10 = 0,954; analogia `king⊕man⊕woman` Top-1
**78%**, Top-5 96%.

**O que já é [B] independentemente:**
- Binarização de embeddings preserva semântica com perda pequena: **~2%** de degradação
  com **98%** menos armazenamento (arXiv:1906.08340 [B]).
- Autoencoder siamês com perda de preservação semântica: **4%** de perda, **9,4×** menos
  memória (ACM [B]).
- Binding recursivo preserva similaridade de sequência em FHRR (arXiv:2201.11691 [B]).

**O que é [C] e não deve ser tratado como resultado:** os 78% de analogia e a
"restauração de raciocínio funcional" (+0,400 no Top-1 de relação). 78% de Top-1
significa **22% de respostas plausíveis e erradas** — isto é, exatamente o modo de
falha que o risco aponta.

> **Resolvido no mesmo dia (MSG 0210).** O `0,0208` era **artefato de medição**:
> o ruído do banco estava em `0,25` **por coordenada**
> (`‖n‖ ≈ 0,25·√384 ≈ 4,9` contra sinal de norma ≈2 ⇒ **SNR < 1**). Normalizado
> para `0,25/√D_IN`, o gate fechou **6/6**. A fórmula e o encoder estavam
> corretos desde o início — sinal invertido e *wrapping* foram descartados
> antes. O claim de 78% do preprint segue **[C]**: **nem confirmado nem
> refutado** por nós.

### 4.2 A distinção que o projeto precisa gravar

| Tipo de alegação | O AETHER pode certificar? |
|---|---|
| **Consistência** (não viola restrições, não contradiz a base) | **SIM** — Ising + violações |
| **Derivabilidade** (segue das premissas por regra) | **SIM** — se as regras estiverem codificadas |
| **Similaridade** (está perto do que se sabe) | Sim, geometricamente — **mas não é verdade** |
| **Factualidade** (o mundo é assim) | **NÃO** — depende da fonte transplantada |

> **Achado #4:** o certificado de factibilidade certifica *consistência*, não *verdade*.
> Chamar isso de "anti-alucinação" seria o mesmo erro que o risco denuncia. O nome
> correto é **certificado de factibilidade** — e a resposta deve carregar a proveniência
> (`transplantado de X`, `observado em T`, `inferido de regra R`).
>
> **Confirmado por medição no E3b (§4.6):** ele também **não** certifica *otimalidade*.
> Em otimização livre o gap não prediz acerto; só a contagem de violações tem dentes.

### 4.3 A evidência mais forte a favor do verificador

| Trabalho | Número |
|---|---|
| **ReGA** (WWW 2026) [A] | detecção de alucinação estrutural em **14–17 ms** em CPU; **70–88×** mais rápido que juiz LLM; AUC 0,90 (funcional) e 0,80 (inversão de papel); energia 0,00 para direção correta vs **1,82** para invertida |
| **Distributional EBM** (arXiv:2605.18871) [B] | verificador de **149M** parâmetros orquestra geradores de 7–26B e supera Qwen-72B em 5 benchmarks; **−53%** de violação de restrição |
| **KG-Guard** (arXiv:2606.00328) [B] | F1 82,0/87,4/84,3 em WebQSP/CWQ/PUGG com **~305×** menos parâmetros; feedback eleva F1 downstream em **+13,0–14,5** |
| **Spilled energy** (arXiv:2602.18671) [B] | métrica de energia **sem treino** separa correto de incorreto em 9 benchmarks |
| **Carnot EBM** (negativo) [C] | 38 experimentos, 16 modelos: EBM em ativações detecta **confiança**, não verdade. "Alucinação confiante é indistinguível de acerto confiante." |

Esse último resultado é o mais importante do bloco: ele **falsifica** a esperança de
resolver verdade por plausibilidade interna. Reforça a decisão: **o verificador tem
que ser de restrição, não de ativação.**

### 4.4 Custo do verificador — o número que decide a arquitetura

14 ms por verificação (ReGA, CPU) contra **146,5 µs** de ciclo [M] = **95 ciclos** de
orçamento. Ou seja: verificar **não cabe no hot path**.

Desenho resultante:

```
hot path (146 µs)     → propõe + emite (E, ΔE, violações)       [sempre]
fora do hot path      → verificação estrutural 14–17 ms         [assíncrona]
resposta ao usuário   → só sai depois do gate                    [latência humana: OK]
```

### 4.5 Gate do Risco 3

| Métrica | Alvo |
|---|---|
| Analogia Top-1 em benchmark próprio (não o do preprint) | medida, sem alvo de marketing |
| Precisão do certificado: `ΔE` grande ⇒ acerto | ≥ 95% |
| Falsos "confiáveis" (responde com ΔE alto e erra) | < 5% |
| Verificação | ≤ 20 ms, CPU, 0 alloc |

### 4.6 E3b — o veredito: `ΔE` **não** prediz acerto (e o que prediz)

**Executado em 2026-11-15.** `examples/e3b_certificate.rs`, ground truth exato por
força bruta (`n=14`, 400 instâncias de vidro de spin, 4 restarts, steps=8).

| Sinal | AUC | precisão@50% | confiante-e-errado@50% |
|---|---:|---:|---:|
| gap local `min ΔH_i` | 0,547 | 90,0% | 10,0% |
| consenso entre restarts | 0,526 | 89,9% | 10,1% |
| gap entre restarts | 0,501 | 90,0% | 10,0% |
| **gap até o bound espectral** | **0,058** | 81,3% | **18,7%** |
| confiança combinada | 0,462 | 88,1% | 11,9% |

Base do solver: **90,0%**. **Nenhum sinal adiciona 1 ponto percentual.** O gap até
o bound é **anti-preditivo** (AUC 0,058): o limite de Gershgorin é rigoroso mas
frouxo, e instâncias onde ele aperta são justamente as fáceis.

**Por que falha:** em otimização **não restrita**, *todo* mínimo local é coerente —
`gap > 0` e `violations = 0` — e um vidro de spin tem exponencialmente muitos.
**Coerência não é otimalidade.**

**O contraste que salva o conceito** (mesma sessão, `--max2sat`): quando a
verificação é **exata** (restrições contáveis), o certificado é exato:

| Métrica | Valor |
|---|---:|
| satisfatíveis (força bruta) | 397/400 |
| decisivas (0 violações) | 383 (95,8%) |
| **precisão das decisivas** | **100,0%** |
| satisfatíveis resolvidas | 96,5% |
| não-satisfatíveis | sempre abstém |

> **Conclusão do P0 (R3):** o certificado de factibilidade **não** é um detector de
> acerto — é um **verificador de factibilidade**. Ele tem dentes onde existe
> restrição contável (lógica, E2b) e é inútil onde só existe paisagem de energia
> (otimização pura). A tese do Vol. I sobrevive **com esta correção**: o "certificado
> de energia" deve ser lido como *prova de consistência*, nunca como *prova de
> verdade ou de ótimo*.

Isso reordena E2b: **roof dual e restrições não são um acelerador de Ising — são a
condição de existência do certificado.**

---

## 5. Risco 4 — Open-endedness precisa de corpo

> *"Metas autotélicas sem sensorium real viram passeio. O Produto C é fraco mas real."*

### 5.1 A evidência de que "corpo" é fechamento de laço, não biologia

| Trabalho | Resultado |
|---|---|
| **AIF + diffusion + MTRSSM** (arXiv:2510.23258) [B] | robô real: **75%** sucesso vs 53% (só extrínseco), 50% (FlowNav), **8,33%** (NoMaD); menos colisões; ganho maior justamente em exploração (78% vs 28%) |
| **AIF hierárquico temporal** (arXiv:2512.01924) [B] | manipulação real; alterna dirigido↔exploratório sob incerteza; custo tratável via ações abstratas (VQ) |
| **AIF como lei de escala de test-time** (arXiv:2606.22813) [B] | generaliza para cenários não vistos **escalando com experiência contínua**; **+36%** de eficiência de inferência |
| **Racing no limite** (arXiv:2608.10618) [B] | 256,3 km/h, 26,8 m/s²; **88,3%** de sucesso com world model |
| **AWAM** (arXiv:2610.23396) [B] | **94,4%** em robô real com imaginação de viewpoint |

Padrão comum: **não é o corpo que importa, é o laço fechado com consequência** — e
todos usam *hierarquia temporal* (lento = meta, rápido = ação). O AETHER já tem isso
em `renormalization` (64→16→4) e EMA τ=100 ms.

### 5.2 O sensorium mínimo viável, em três estágios

| Estágio | Fonte | Ações com consequência | Custo |
|---|---|---|---|
| **S0 (hoje)** | telemetria de host: jitter, CPU, disco, rede | 4 atuadores: throttling, migração de afinidade, alarme, pausa | zero hardware |
| **S1** | mundo sintético com física contínua (tipo Lenia, arXiv:2608.26116 [B]) | perturbação local do campo | CPU/GPU já existente |
| **S2** | sensor real (CAN/CSI) — Produto A/B | atuação física | certificação |

**S0 é suficiente para falsificar a hipótese?** Sim, se as ações tiverem consequência
real: se o organismo "decide" alarmar e o alarme não muda nada, ele degenera em passeio.
A primeira tarefa de E7 é justamente medir **degeneração**: as metas propostas
convergem para um conjunto trivial?

### 5.3 O teste anti-passeio

```
H1: distribuição de metas permanece com entropia > H_min após 10⁵ ciclos
H2: o agente prefere metas de dificuldade intermediária (progresso máximo)
H3: remover o atuador faz a entropia de metas cair (i.e. o corpo restringe)
```

Se H3 falhar, o "corpo" é decorativo — e o risco está confirmado.

### 5.4 Gate do Risco 4

| Métrica | Alvo |
|---|---|
| Entropia de metas em 10⁵ ciclos | não decai mais de 20% |
| Correlação dificuldade × escolha (curva em U invertido) | Spearman > 0,5 |
| H3 (ablação do atuador) | entropia cai ≥ 30% |

---

## 6. Risco 5 — Custo de treino

> *"OLMs treinam ~40× mais devagar (sem parallel scan). Inferência é o forte; treino é o gargalo."*

### 6.1 Primeiro: o risco pode ser evitado, não vencido

O próprio preprint do OLM (SSRN 7431978 [C]) reporta o achado mais útil:

> melhores resultados com **dinâmica aprendida devagar ou congelada** — "um reservatório
> de fase estruturado com fronteiras aprendidas".

Isso é **reservoir computing**: congele a física, treine só o readout linear.

E a literatura de RC é grande, peer-reviewed e barata:

| Trabalho | Número |
|---|---|
| **OpenReservoirComputing** (arXiv:2603.14802) [B] | RC em JAX/Equinox: GPU, JIT, vetorização automática |
| **OpenPRC** (arXiv:2604.07423) [B] | engine física GPU (RK4–PBD) + camada de aprendizado; schema HDF5 unificado |
| **NEMS ressonador** (arXiv:2609.29532) [B] | multiplexação de modos reduz erro **>28×** sem treinar parâmetros internos |
| **Josephson junction** (arXiv:2605.13354) [B] | um único JJ já é reservatório suficiente |
| **Vestibular RC** (arXiv:2604.09943) [B] | topologia **desacoplada** ≈ acoplada; memória analítica |

> **Achado #5:** o AETHER já é, estruturalmente, um reservatório: física fixa +
> `OnlineKoopmanOperator` (RLS/Sherman-Morrison) como readout linear. **Não há
> 40× de treino a pagar** se a física não for treinada.

### 6.2 Se precisar treinar a dinâmica: DEER existe, mas não salva em n=64

| Método | Ganho | Limite |
|---|---|---|
| **DEER** (ICLR 2024, arXiv:2309.12252) [A] | **10×** treino; até **500×** forward; >1000× fwd+grad; 1 M passos em 15 ms | complexidade **O(n³)**; em **n=64 o ganho cai a ~25%** |
| **ELK** (NeurIPS 2024, arXiv:2407.19115) [A] | quasi-Newton + Kalman smoothing: menos memória, mais estável | ainda O(n³)-ish |

Para o AETHER (`n = 64`), DEER rende ~25%. **Não é a alavanca.** As alavancas reais:

1. **Congelar a dinâmica** (reservoir) → treino = mínimos quadrados, fechado, paralelo.
2. **Paralelizar por organismo** — enxame é *embarrassingly parallel*; 6 P-cores = 6×.
3. **Treinar offline em GPU** e só inferir na borda.

### 6.3 O orçamento real de treino

```
inferência:  146,5 µs/ciclo × 86.400 s = 757 s de CPU por dia de operação
treino:      se 10⁶ passos × 150 µs     = 150 s  (uma vez)
```

Ou seja: **um dia de operação custa 5× mais CPU que um treino inteiro.** O gargalo
não é treino — é *desenvolvimento* (iterar arquitetura). E isso se resolve com
experimentos pequenos, não com escala.

### 6.4 Gate do Risco 5

| Métrica | Alvo |
|---|---|
| Treino do readout (RLS) em 10⁶ amostras | < 60 s, 1 core |
| Regressão de acurácia físico-congelado vs físico-treinado | < 5% |
| Throughput de inferência com dinâmica congelada | ≥ baseline [M] |

---

## 7. Risco 6 — Preprints de baixa revisão

> *"Wave Memory, Causal Oscillator, Model Transplantation são preprints — hipótese, não resultado."*

### 7.1 O método: triangulação em três camadas

Para cada alegação [C], procurei: (i) mecanismo em fonte [A]/[B], (ii) número
independente, (iii) plausibilidade física.

| Preprint [C] | Mecanismo corroborado por | Números replicados? | Veredito |
|---|---|---|---|
| **Model Transplantation** | JL + binarização: arXiv:1906.08340 [B] (~2% perda), ACM siamês [B] (~4%), arXiv:2201.11691 [B] | **NÃO** (78% analogia) | mecanismo **[A]**, números **[C]** |
| **Wave Memory** (100% coerência) | PRISM arXiv:2512.01208 [B] (COMET 0.799 com fase pura), CAWN arXiv:2604.04250 [B], Attention by Sync arXiv:2606.12059 [B] | **NÃO** — e "100%" é sinal de alerta | mecanismo **[B]**, claims **[C−]** |
| **Causal Oscillator** (1,34 BPB) | SSM/state-space peer-reviewed; reservatório físico | **NÃO** | **[C]** |
| **OLM** (40× mais lento) | consistente com ausência de parallel scan (DEER/ELK existem justamente por isso) [A] | coerente | **[C]**, mas o *diagnóstico* é **[A]** |

### 7.2 Regras de uso adotadas

1. **Nenhuma decisão de arquitetura** se apoia em [C]. E1–E9 podem ser *motivados* por [C],
   mas o gate é sempre uma medida nossa.
2. **Toda citação de [C]** leva o selo no texto.
3. **Anti-sinal:** qualquer resultado "100%" ou "sem perda" em preprint é tratado como
   erro de medição até prova em contrário.
4. **Reprodução mínima:** antes de usar um mecanismo [C], reproduzimos a parte trivial
   (ex.: JL com ρ de similaridade) em < 1 dia.

### 7.3 O que a triangulação salvou

- **Não** construir o PLH em cima do "100% de coerência" do Wave Memory.
- **Sim** construir em cima do PRISM (fase pura, COMET medido, arXiv) e do OLMs (mecanismo).
- **Não** assumir transplantação como verdade.
- **Sim** assumir JL+binarização como transporte de geometria — e só isso.

---

## 8. Registo de risco atualizado

| # | Risco | Status após Vol. II | Risco residual | Mitigação principal |
|---|---|---|---|---|
| **R1** | Ising não é oráculo | **mitigável com engenharia conhecida** | médio | roof dual (10³–10⁴× menor) + presolve structure-aware + Lagrange |
| **R2** | Interferência VSA | **mitigável; capacidade ∝ D²** | médio | ACF/IMF (ruído na init) + fatoração + abstenção por passo |
| **R3** | Geometria ≠ verdade | **irredutível em parte** | **alto** | certificado de *coerência* (não verdade) + proveniência + verificador 14 ms |
| **R4** | Open-endedness sem corpo | **mitigável com S0** | médio | laço fechado com atuador real + teste anti-passeio H1–H3 |
| **R5** | Custo de treino | **evitável** (reservoir) | baixo | congelar física; RLS como readout |
| **R6** | Preprints | **gerenciável** | baixo | triangulação + selos + regra "nenhuma decisão só em [C]" |

> Os riscos `R7–R10` nascem no Vol. III (drift para RAG, vazamento de escopo,
> proveniência-teatro, certificado sem dentes) e não são consequência do hardware —
> são consequência da **integração**.

### 8.1 Gaps novos

| Código | Severidade | Descrição | Remediação |
|---|---|---|---|
| `NO_PROFILE_BREAKDOWN` | ✅ **resolvido (E6)** | medido: solver 84,4% · 7,5 GFLOP/s (~19% do pico AVX2) | — |
| `SOLVER_KERNEL_SCALAR` | **crítico** | gemv + energia por passo são laços escalares; 5× de headroom até o pico f64 | E6b (SIMD/f32) |
| `NO_ROOF_DUAL` | alto | sem passe simbólico; Ising recebe instância crua | E2b |
| `NO_LIMIT_CYCLE_GUARD` | alto | cleanup determinístico pode ciclar | E1b |
| `NO_PROVENANCE` | alto | resposta não diz de onde veio (transplantado/observado/inferido) | E3b |
| `D_CAPACITY_UNMEASURED` | médio | capacidade real em D=160 desconhecida | E1 |
| `NO_ACTUATOR_LOOP` | alto | metas sem consequência (S0 incompleto) | E7 |
| `CERT_SEMANTICS` | médio | "certificado de energia" pode ser lido como "verdade" ou "ótimo" | ✅ resolvido: **certificado de factibilidade** (Vol. II §4.6) |

---

## 9. Matriz de decisão de hardware

| Cenário | Substrato | Justificativa numérica |
|---|---|---|
| Motor em produção, N=64, 1 organismo | **CPU (AVX2)** | 0,88% de um core/dia; GPU seria 1000× overkill |
| N ≥ 512 | **GPU, estado residente** | `J` ≥ 1 MB; L2 estourado; 307 Gb/s de streaming |
| Enxame (muitos organismos) | **GPU com replicas** ou **6 P-cores** | paralelismo trivial; 618–628 k req/s |
| Produto C (host 24/7) | **CPU**; neuromórfico se energia for TCO | Loihi 2: 496× menos energia/amostra |
| Produto B (casa, bateria) | **FPGA ou neuromórfico** | FPGA HDC: 0,09 ms, 1300× CPU |
| Lógica pesada (SAT grande) | **CPU híbrida + Ising** | presolve 6–18×; Ising só no núcleo |
| Pesquisa de fronteira (CiM) | **não** | exige FeFET; ganho vem do algoritmo |

**Regra prática:** *CPU até N=512; GPU depois; neuromórfico só quando a energia for
a métrica — nunca por moda.*

---

## 10. Experimentos novos (E6, E2b, E1b, E3b, E7, E8)

> Registro mestre com todos os IDs (`E1–E8`, `F1–F3`) em **Vol. III §11.1**.

| # | Entrega | Mede | Gate |
|---|---|---|---|
| **E6** ✅ | `examples/e6_profile.rs` + `tests/e6_profile_gate.rs` | perfil por estágio — **executado 2026-11-15** | cobertura **99,98%**; solver **84,4%** |
| **E6b** | otimizar o kernel do solver (SIMD f64 ou f32 + fusão) | GFLOP/s do solver | ≥ 2× o baseline (de ~7,5 para ≥ 15 GFLOP/s) |
| **E2b** | roof dual + presolve antes do BSB | redução de variáveis, tempo, acerto | ≥40% redução; ≥90% das instâncias que o BSB puro erra |
| **E1b** | ACF/IMF no cleanup do `FhrrStore` | capacidade e taxa de ciclo-limite com/sem ruído | ≥10× capacidade; ciclo-limite < 1% |
| **E3b** | proveniência + verificador de coerência | precisão do selo; falsos "confiáveis" | ≥95% precisão; < 5% falsos |
| **E7** | laço S0 com 4 atuadores | entropia de metas, H1–H3 | H3 confirmado |
| **E8** | readout congelado (reservoir) vs treinado | regressão de acurácia; tempo de treino | < 5% regressão; < 60 s |

E1–E5 do Volume I permanecem válidos; E6 vem **primeiro** (é diagnóstico e barato).

---

## 11. O que mudou desde o Volume I

| Tema | Volume I | Volume II |
|---|---|---|
| Ising | "não é oráculo, precisa de engenharia" | **engenharia quantificada:** roof dual 5.800×, presolve 6–18×, sparse 0, N→O(N) |
| VSA | "D tem que crescer; capacidade é o teto" | **capacidade ∝ D²**; o inimigo é **ciclo-limite**, curado com ruído na init (+50×) |
| Transplantação | "ponte promissora e arriscada" | **JL é sólido; 78% de analogia é [C]**; certifica coerência, não verdade |
| Corpo | "Produto C é fraco mas real" | **laço fechado basta**; 3 estágios S0–S2; teste anti-passeio H1–H3 |
| Treino | "40× mais lento é o gargalo" | **gargalo evitável**: congelar física (reservoir); DEER só rende 25% em n=64 |
| Preprints | "trate como hipótese" | **protocolo de triangulação** com selos [A]/[B]/[C] e regra de decisão |
| Hardware | "cabe no L2; penhasco em N≈512" | **+ sem AVX-512**; **medido: solver = 84,4% do ciclo, ~7,5 GFLOP/s (~19% do pico)** |
| Energia | não calculado | **0,88% de um core/dia; ~1–3 Wh/dia** |

---

## 12. Referências novas

### 12.1 Ising, QUBO e lógica

| ID | Título | Selo |
|---|---|---|
| arXiv:2511.21046 | On the Limits of Ising Machines for Circuit-Derived SAT | [B] |
| arXiv:2610.20346 | On Partial Optimality in QUBO via Quadratic Posiform Autarkies | [B] |
| arXiv:1705.09844 | QUBO Preprocessing: Theory and Empirical Analysis | [B] |
| arXiv:2601.18108 | Sparse QUBO Formulation via Network-Based Decomposition | [B] |
| arXiv:2603.03498 | Distributed Parallel Structure-Aware Presolving | [B] |
| arXiv:2505.07179 | Lagrange Oscillatory Neural Networks (LagONN) | [B] |
| arXiv:2512.17165 | BEOL Ferroelectric CiM Ising Machine for Simulated Bifurcation | [B] |
| arXiv:2610.06622 | GPU-Initiated Discrete Simulated Bifurcation | [B] |
| arXiv:2507.22117 | Benchmark of an Ising Machine (Digital Annealer) | [B] |
| arXiv:2605.20328 | Tsat: Targeting Clause Type Distributions | [B] |
| arXiv:2610.13368 | Anechoic: Onsager-Corrected Parallel Annealing | [B] |
| Nature Comms 2026 | ML-assisted Ising machines for dynamically changing problems | [A] |
| arXiv:2605.07017 | Short SAT Implicants via Ising/QUBO | [B] |
| arXiv:2605.04033 | p-bit Guided CDCL | [B] |
| arXiv:2610.27362 | Quantum-Classical Hybrid: Lessons from Classical Decomposition | [B] |
| Frontiers 2026 | Probing variables in QUBOs with the roof dual | [A] |
| blog Hercules | 5.800× / 2,19 M→40 nós | [C] |

### 12.2 VSA, capacidade e hardware

| ID | Título | Selo |
|---|---|---|
| arXiv:1906.11684 | Resonator Networks outperform optimization methods | [B] |
| arXiv:2007.03748 | Resonator networks for factoring distributed representations | [B] |
| arXiv:2412.00354 | On the Role of Noise in Factorizers (IMF/ACF, +50×) | [B] |
| Frontiers 2026 | Comparative study of nonlinear cleanup rules in resonator networks | [A] |
| arXiv:2311.04872 | Computing with Residue Numbers in High-Dimensional Representation | [B] |
| arXiv:2404.19126 | Compositional Factorization with Convolutional Sparse Coding | [B] |
| arXiv:2506.09282 | ScalableHD: HDC inference on multi-core CPUs (10×) | [B] |
| arXiv:2601.20061 | Primitive-Driven Acceleration of HDC (FPGA 0,09 ms) | [B] |
| arXiv:2605.06485 | Litespark: ternary SIMD (95,81× AVX-512 VNNI) | [B] |
| arXiv:1906.08340 | Binary sentence embeddings (~2% perda, 98% menos memória) | [B] |
| arXiv:2201.11691 | Recursive Binding for Similarity-Preserving Hypervectors | [B] |
| ACM 3508230.3508235 | Siamese Autoencoder for Binary Quantization (4% perda) | [A] |
| arXiv:2508.14245 | Cross-Layer Design of Vector-Symbolic Computing | [B] |

### 12.3 Verificação, verdade e alucinação

| ID | Título | Selo |
|---|---|---|
| WWW 2026 (ReGA) | Zero-Overhead Graph Alignment for Structural Hallucination Detection | [A] |
| arXiv:2605.18871 | Distributional Energy-Based Models for Structured LLM Reasoning | [B] |
| arXiv:2606.00328 | KG-Guard: Graph-Based Hallucination Detection | [B] |
| arXiv:2602.18671 | Spilled Energy in LLMs | [B] |
| arXiv:2602.19643 | KGHaluBench | [B] |
| Carnot EBM | EBM em ativações detecta confiança, não verdade (negativo) | [C] |
| Zenodo 19476847 | Model Transplantation (versão completa) | [C] |

### 12.4 Corpo, agência e treino

| ID | Título | Selo |
|---|---|---|
| arXiv:2510.23258 | Deep AIF + diffusion + MTRSSM (robô real, 75%) | [B] |
| arXiv:2512.01924 | Real-World Robot Control by Deep AIF (hierárquico) | [B] |
| arXiv:2606.22813 | Active Inference as the Test-Time Scaling Law | [B] |
| arXiv:2610.05692 | Benchmarking Generative Trajectory Models for AIF | [B] |
| arXiv:2608.10618 | World-Model-Centric Autonomous Racing (256 km/h) | [B] |
| arXiv:2610.23396 | AWAM (94,4% real) | [B] |
| arXiv:2608.10449 | PBD-AG Persistent Baseline-Delta Active Graphs | [B] |
| arXiv:2309.12252 | DEER: Parallelizing non-linear sequential models (ICLR 2024) | [A] |
| arXiv:2407.19115 | ELK: Scalable and Stable Parallelization of Nonlinear RNNs (NeurIPS 2024) | [A] |
| arXiv:2603.14802 | OpenReservoirComputing (JAX, GPU) | [B] |
| arXiv:2604.07423 | OpenPRC: Physics-to-Task in Physical RC | [B] |
| arXiv:2609.29532 | Dynamical Diversity in Reconfigurable Nanomechanics (>28×) | [B] |
| arXiv:2605.13354 | Reservoir Computing with a single Josephson junction | [B] |
| arXiv:2604.09943 | Vestibular reservoir computing | [B] |

### 12.5 Neuromórfico e energia

| ID | Título | Selo |
|---|---|---|
| arXiv:2608.18341 | Acoustic Anomaly Detection on Loihi 2 (496× CPU, 132× GPU) | [B] |
| arXiv:2511.01553 | Online Continual Learning on Loihi 2 (0,33 ms, 0,05 mJ) | [B] |
| arXiv:2602.02439 | NeuEdge (847 GOp/s/W, 312× vs GPU) | [B] |
| arXiv:2610.25424 | C. elegans on Loihi 2 (514–1211× latency) | [B] |
| arXiv:2605.00146 | SNN Object Detection on Loihi 2 | [B] |
| DATE 2022 | HyperSpike: HDC + SNN (31,5× robustez, 2,6× energia) | [A] |

---

## Fecho do Volume II

O Volume I dizia: *"não é inventar física nova; é fechar o laço e medir."*
O Volume II mostra que **as seis objeções eram corretas — e todas têm preço tabelado**:

- Ising falha em restrição apertada → **5.800×** de redução antes de chegar nele.
- VSA satura → capacidade é **quadrática em D**, e o inimigo real é **ciclo-limite**.
- Geometria não é verdade → certifica-se **coerência**, com proveniência, em 14 ms.
- Sem corpo não há agência → **laço fechado basta**; S0 já falsifica a hipótese.
- Treino caro → **congele a física**; o readout é regressão linear.
- Preprint não é resultado → **triangulação com selos** e regra de decisão.

E um achado que não estava em nenhuma das seis objeções — e que o **E6 resolveu
depois deste volume ter sido escrito**: **o motor é matvec-bound.** O solver consome
**84,4%** do quadro e entrega **7,5 GFLOP/s** (~19% do pico AVX2 f64). A estimativa
"0,07%" que aparecia no §1.1 estava errada e está corrigida lá.

Isso reordena a fila: antes de qualquer física nova, **otimizar o kernel do solver**
(E6b) é o maior ganho disponível — 5× de headroom, sem tocar na arquitetura. E,
depois dele, o próximo experimento honesto continua sendo medir o que ainda não se
mediu: `ΔE` prevê acerto? (E3b).

> *"Seis corpos, seis preços. O sétimo era não saber onde o tempo ia — agora sabemos:
> 84,4% vai para o solver."*

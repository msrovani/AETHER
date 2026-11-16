# O PROBLEMA DOS 3 CORPOS â€” NÃƒO Â· Volume II
## Hardware moderno e os seis riscos, enfrentados com nÃºmeros

> **DossiÃª de pesquisa Â· Volume II** Â· AETHER Â· 2026-11-15
> Confronta os seis riscos honestos do Volume I com CPU/GPU/FPGA/neuromÃ³rfico
> modernos, usando evidÃªncia medida.
>
> **MÃ©todo:** cada risco recebe (a) o que a evidÃªncia mais recente mostra, (b) a mitigaÃ§Ã£o
> com nÃºmeros, (c) a decisÃ£o de arquitetura, (d) o gate. Nenhum benchmark citado foi
> re-executado por nÃ³s â€” sÃ£o nÃºmeros publicados, com a fonte e o nÃ­vel de confianÃ§a.
>
> **Nada aqui altera `docs/API_CONTRACT.md`.** Ã‰ dossiÃª, nÃ£o contrato.

---

## SÃ©rie

| Volume | Arquivo | Papel |
|---|---|---|
| **I** | [`O_PROBLEMA_DOS_3_CORPOS_NAO.md`](O_PROBLEMA_DOS_3_CORPOS_NAO.md) | Problema e tese â€” os trÃªs "NÃƒO" |
| **II** | este arquivo | Hardware e riscos â€” o preÃ§o de cada objeÃ§Ã£o |
| **III** | [`O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`](O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md) | Conhecimento e integraÃ§Ã£o â€” registro mestre, SGDB, prioridades |

> **PrÃ©-requisito de leitura:** Â§1 deste volume (mapa de hardware) define as restriÃ§Ãµes
> fÃ­sicas que o Vol. III assume. Os seis riscos numerados aqui sÃ£o `R1â€“R6` no registro
> mestre (Vol. III Â§11.3); o Vol. III acrescenta `R7â€“R10`.
>
> **Estado:** das seis objeÃ§Ãµes, **nenhuma foi fechada** â€” todas foram *precificadas*.
> Nenhum gate foi executado. Ver Vol. III Â§12.

---

## SumÃ¡rio

- [0. Como ler a evidÃªncia](#0-como-ler-a-evidÃªncia)
- [1. O mapa de hardware](#1-o-mapa-de-hardware)
- [2. Risco 1 â€” "Ising nÃ£o Ã© orÃ¡culo"](#2-risco-1--ising-nÃ£o-Ã©-orÃ¡culo)
- [3. Risco 2 â€” InterferÃªncia em VSA](#3-risco-2--interferÃªncia-em-vsa)
- [4. Risco 3 â€” TransplantaÃ§Ã£o dÃ¡ geometria, nÃ£o verdade](#4-risco-3--transplantaÃ§Ã£o-dÃ¡-geometria-nÃ£o-verdade)
- [5. Risco 4 â€” Open-endedness precisa de corpo](#5-risco-4--open-endedness-precisa-de-corpo)
- [6. Risco 5 â€” Custo de treino](#6-risco-5--custo-de-treino)
- [7. Risco 6 â€” Preprints de baixa revisÃ£o](#7-risco-6--preprints-de-baixa-revisÃ£o)
- [8. Registo de risco atualizado](#8-registo-de-risco-atualizado)
- [9. Matriz de decisÃ£o de hardware](#9-matriz-de-decisÃ£o-de-hardware)
- [10. Experimentos novos](#10-experimentos-novos-e6-e2b-e1b-e3b-e7-e8)
- [11. O que mudou desde o Volume I](#11-o-que-mudou-desde-o-volume-i)
- [12. ReferÃªncias novas](#12-referÃªncias-novas)

---

## 0. Como ler a evidÃªncia

Toda afirmaÃ§Ã£o recebe um selo:

| Selo | Significado |
|---|---|
| **[A]** | peer-reviewed **e** mecanismo reproduzido independentemente |
| **[B]** | peer-reviewed (ou arXiv sÃ©rio), mecanismo plausÃ­vel, **nÃºmeros nÃ£o replicados** |
| **[C]** | preprint (Zenodo/SSRN) ou blog de engenharia â€” **hipÃ³tese** |
| **[M]** | medido neste projeto (`docs/COGNITIVE_AUDIT_FINAL.md`) |

Regra de ouro adotada: **nenhuma decisÃ£o de arquitetura pode depender sÃ³ de [C].**

---

## 1. O mapa de hardware

### 1.1 A mÃ¡quina de referÃªncia tem uma limitaÃ§Ã£o que importa

**Intel Core 7 240H**: 6 P + 4 E, 16 threads, 2,5â€“5,2 GHz, L1d 48 KB/P, L2 1,25 MB/P,
L3 24 MB, LPDDR5-6400 (102,4 GB/s), TDP 45 W.
**SIMD: AVX2 â€” sem AVX-512.** [A, datasheet]

ConsequÃªncia aritmÃ©tica para **um** matvec do Ising (`N=64`, f64):

```
MACs        = 64 Ã— 64 = 4.096
AVX2 f64    = 4 doubles / instruÃ§Ã£o, 2 FMA/ciclo â‡’ 8 MAC/ciclo
matvec      = 4.096 / 8 = 512 ciclos â‰ˆ 0,20 Âµs @2,5 GHz
```

> **CORREÃ‡ÃƒO PÃ“S-E6 (2026-11-15).** A versÃ£o original desta seÃ§Ã£o estimava
> "matvec = 0,07% do ciclo" e concluÃ­a que o motor **nÃ£o** era matvec-bound.
> **A mediÃ§Ã£o falsificou essa conclusÃ£o.** O erro: o solver nÃ£o faz *um* matvec
> por quadro â€” faz **dois por passo** (`hx = Jx + h` e `H(s) = âˆ’Â½sáµ€Js âˆ’ háµ€s`)
> durante `steps` passos. Com `steps = 60`, sÃ£o **983.040 flops**, nÃ£o 8.192.

**Medido** (`examples/e6_profile.rs` Â· `target/aether_profile.json`, 2.000 quadros,
`N=64`, `window=128`, `steps=60`, `hz=60`):

| estÃ¡gio | mÃ©dia Âµs | % do quadro |
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

O total reproduz o bench de auditoria (**146,5 / 311,7 Âµs Â· 804,7k eq-t/s**) dentro
de **1â€“8%** (p99: 311,2 vs 311,7 Âµs; eq-t/s: 811,8k vs 804,7k). O instrumento mede
o mesmo fenÃ´meno que o benchmark oficial â€” e a variaÃ§Ã£o entre execuÃ§Ãµes Ã© ~Â±2%.

> **Nota de metodologia (achada pelo E6):** o quadro do organismo **nÃ£o Ã©
> bit-reprodutÃ­vel entre instÃ¢ncias**. A amostragem lÃª `NodeHandle::latest()`, que
> publica de threads Kuramoto reais (UDP + escalonador do SO); dois organismos com a
> mesma seed divergem. O determinismo bit-a-bit do crate vale para codec/solver, nÃ£o
> para a borda sensorial. Medir o organismo Ã© medir **distribuiÃ§Ã£o**, nunca execuÃ§Ã£o
> Ãºnica â€” daÃ­ `frames=2000` e percentis.

> **Achado #1 (corrigido):** o motor **Ã©** matvec-bound. O solver consumia
> **84,0% â†’ 90,9% â†’ 95,3%** do quadro em `steps = 60 â†’ 120 â†’ 240`, e entregava
> **~7,44 GFLOP/s** em f64 contra ~40 GFLOP/s de pico AVX2 a 2,5 GHz â€” **~19% do
> pico**. Todos os outros oito estÃ¡gios somam 15% e, juntos, nÃ£o pagam uma
> otimizaÃ§Ã£o: o Ãºnico gargalo que importa Ã© o kernel do solver.

### 1.1.2 E6b â€” o que aconteceu quando atacamos o kernel

VetorizaÃ§Ã£o em Rust safe (16 acumuladores independentes em `dot_f64`/`dot_i8`)
mais a conversÃ£o de `s` para `f64` uma vez por passo:

| MÃ©trica | E6 | **E6b** | Ganho |
|---|---:|---:|---:|
| solver | 132,1 Âµs (84,4%) | **57,5 Âµs (70,7%)** | **2,30Ã—** |
| GFLOP/s | 7,44 | **17,10** | **2,30Ã—** |
| total | 156,3 Âµs | **81,3 Âµs** | 1,92Ã— |
| p99 | 320,6 Âµs | **170,2 Âµs** | 1,88Ã— |
| `eq-t/s` | 818.844 | **1.574.229** | 1,92Ã— |
| N=1024 | 52,07 ms | **16,60 ms** | 3,14Ã— |

**Curva de acumuladores medida:** 16 â†’ 17,1 Â· 24 â†’ 10,4 Â· 32 â†’ 12,3 GFLOP/s.
O Ã³timo Ã© 16 porque (a) cabe nos 16 YMM com as cargas e (b) **divide todas as
potÃªncias de dois** de N â€” 24 deixa cauda em N=64, 32 derrama registradores.

**A fusÃ£o algÃ©brica de 2 GEMVs em 1 nÃ£o Ã© vÃ¡lida:** `H(s)` usa `s = sign(x) â‰  x`;
reutilizar `hx = Jx + h` daria a energia do estado **contÃ­nuo**, mudando o
`energy_trace`, o elitismo e o critÃ©rio de parada. O ganho veio de ILP, nÃ£o de
Ã¡lgebra â€” e Ã© reproduzÃ­vel em qualquer AVX2.

Detalhe completo e validaÃ§Ã£o em `docs/HW_TARGET.md` Â§8.5.

#### 1.1.1 Lei de escala medida (a cache importa menos do que se supunha)

| N | solver Âµs | GFLOP/s | footprint | nÃ­vel | total Âµs | 60 Hz |
|---:|---:|---:|---:|---|---:|---|
| 64 | 132,5 | **7,42** | 0,18 MB | L2 | 156 | 0,9% |
| 128 | 570,0 | 6,90 | 0,40 MB | L2 | 612 | 3,7% |
| 256 | 2.534,8 | 6,21 | **1,03 MB** | L2 (limite) | 2.632 | 15,8% |
| 512 | 11.868,9 | 5,30 | **3,04 MB** | L3 | 12.125 | 72,7% |
| 1024 | 51.318,3 | 4,90 | 10,05 MB | L3 | 52.068 | **3,1Ã—** |

Duas conclusÃµes que corrigem o texto anterior:

1. **O penhasco de cache Ã© Nâ‰ˆ256, nÃ£o 512.** O footprint nÃ£o Ã© sÃ³ `J` (nÂ²Â·8 B):
   a matriz de leitura pesa `nÂ·2048 B`. Em N=256, `J` + readout = **1,03 MB**, jÃ¡ no
   limite do L2 (1,25 MB). A queda de GFLOP/s nessa travessia Ã© âˆ’15% (6,21â†’5,30).
2. **O custo cresce ~O(NÂ²)** e o regime Ã© compute-bound: o GFLOP/s cai sÃ³ 34% de
   N=64 a N=1024, mesmo saindo do L2 inteiro. O que dÃ³i Ã© o kernel escalar.

> **Achado #2 (novo):** hÃ¡ **dois orÃ§amentos** e confundi-los Ã© o erro clÃ¡ssico.
> *Tempo real*: 60 Hz = **16.667 Âµs** por quadro â€” o motor usa **0,94%** disso em
> N=64. *Throughput*: cada Âµs marginal custa `eq-t/s`. Otimizar hardware para
> "caber em 60 Hz" Ã© resolver um problema que nÃ£o existe; o que existe Ã© o kernel
> a 19% do pico e o teto **N=512** para 60 Hz sem acelerÃ¡-lo.

Sem AVX-512, a rota SIMD de 512 bits (que rende 95,81Ã— em inferÃªncia ternÃ¡ria,
arXiv:2605.06485 [B]) **nÃ£o estÃ¡ disponÃ­vel** nesta mÃ¡quina. O teto aqui Ã© AVX2:
8 f32, 16 MAC/ciclo/core, ~96 MAC/ciclo nos 6 P-cores.

### 1.2 O espectro de substratos, com nÃºmeros medidos

| Plataforma | Medida publicada | O que significa para o AETHER | Selo |
|---|---|---|---|
| **CPU multicore** (ScalableHD, arXiv:2506.09282) | atÃ© **10Ã—** throughput vs TorchHD; atÃ© 18Ã— em PAMAP2/TEX; escala quase proporcional a cores; SMT ajuda com binding NUMA | CPU **Ã©** substrato vÃ¡lido para HDC/VSA; pipeline 2-estÃ¡gios + tiling + afinidade Ã© o padrÃ£o que ganha | [B] |
| **FPGA** (HDC, arXiv:2601.20061) | **0,09 ms**, 11.141 img/s; **1300Ã—** CPU e **60Ã—** GPU | quando o produto for embarcado, FPGA ganha 2 ordens de magnitude | [B] |
| **GPU (dSB)** (arXiv:2610.06622) | RTT **166â€“189 Âµs**, 618â€“628 k req/s; **307 Gb/s** em `n = 10â·` | GPU sÃ³ compensa com **batimento/permanÃªncia**; requisiÃ§Ã£o Ãºnica â‰ˆ CPU | [B] |
| **CiM FeFET** (arXiv:2512.17165) | **175,9Ã—** sobre SB em GPU (A6000), Max-Cut atÃ© 100k nÃ³s; init inspirada em atenÃ§Ã£o corta **80% das iteraÃ§Ãµes** | o ganho maior veio do **algoritmo** (init), nÃ£o do silÃ­cio | [B] |
| **Digital Annealer** (arXiv:2507.22117) | competitivo em atÃ© **53.000** variÃ¡veis (2.000+ grafos MQLib) | QUBO grande jÃ¡ Ã© tratÃ¡vel hoje por heurÃ­stica | [B] |
| **Ising embarcado + ML** (Nature Comms 2026) | **5.500Ã—** vs NetworkX e **4.300Ã—** vs ReduMIS em MIS esparso | tunagem de parÃ¢metros por ML Ã© o gargalo prÃ¡tico, nÃ£o o solver | [A] |
| **NeuromÃ³rfico Loihi 2** (arXiv:2608.18341) | **0,0406 mJ**/amostra vs **20,16 mJ** CPU e **5,35 mJ** GPU â‡’ **496Ã—** e **132Ã—** menos energia | para monitoramento 24/7, a energia domina o TCO | [B] |
| **Loihi 2 + aprendizado contÃ­nuo** (arXiv:2511.01553) | 0,33 ms e **0,05 mJ**/passo; **341Ã—** menos energia que replay em Orin | online continual learning no chip Ã© viÃ¡vel | [B] |

### 1.3 A conta de energia do AETHER (estimativa)

```
60 Hz Ã— 86.400 s/dia        = 5,184 M ciclos/dia
5,184 M Ã— 146,5 Âµs          = 757 s de CPU/dia
757 s / 86.400 s            = 0,88% de UM core
```

A 5â€“15 W por P-core ativo: **â‰ˆ 1â€“3 Wh/dia**. [M + estimativa]

ComparaÃ§Ã£o grosseira (rotulada como tal): um LLM a 50 tok/s e ~300 W gasta ~6 J/token;
o AETHER gasta ~1,46 mJ/ciclo para 128 eq-tokens â‡’ **~11 ÂµJ/eq-token**. Mesmo que a
equivalÃªncia esteja errada por 100Ã—, ainda Ã© **2â€“3 ordens de magnitude**.
**NÃ£o usar esse nÃºmero em marketing sem a ressalva de que `eq-t` Ã© convenÃ§Ã£o.**

---

## 2. Risco 1 â€” "Ising nÃ£o Ã© orÃ¡culo"

> *"RestriÃ§Ã£o apertada destrÃ³i o landscape. Sem passe simbÃ³lico + partiÃ§Ã£o structure-aware,
> lÃ³gica vai falhar em casos reais. Trabalho de engenharia, nÃ£o de fÃ­sica."*

### 2.1 A evidÃªncia confirma o risco â€” e quantifica a cura

arXiv:2511.21046 [B] mede exatamente o que suspeitÃ¡vamos, e com um contraste brutal:

| Componente | Ganho |
|---|---|
| SÃ³ prÃ©-processamento simbÃ³lico, solver **clÃ¡ssico** | **< 3Ã—** |
| SÃ³ prÃ©-processamento simbÃ³lico, solver **Ising** | **> 150Ã—** |
| Resultado no chip de 45 spins | problemas de 8 bits (94 var) â†’ **11 bits (190 var)** sem mudar hardware |

O paper Ã© explÃ­cito: restriÃ§Ãµes apertadas viram **apenas penalidades**, e penalidades
podem se cancelar. Exemplo dado: `(Â¬a)` (unit clause) Ã© apagada por `(a âˆ¨ b)` na
formulaÃ§Ã£o quadrÃ¡tica â€” o solver passa a aceitar `a = 1`.

**ConclusÃ£o:** o Ising nÃ£o deve ser o **raciocinador**; deve ser o **acelerador de
busca local** no fim de um pipeline simbÃ³lico.

### 2.2 As quatro peÃ§as que resolvem â€” e o nÃºmero de cada uma

| PeÃ§a | EvidÃªncia | NÃºmero |
|---|---|---|
| **1. Roof dual / persistÃªncias** | arXiv:2610.20346 [B] prova que a posiform roof-dual-optimal dÃ¡ o **domÃ­nio mÃ¡ximo de autarkia em tempo polinomial**; arXiv:1705.09844 [B] valida empiricamente | No solver Hercules: BQP100 de **77,5 s â†’ 0,0132 s** (**â‰ˆ5.800Ã—**); nÃ³s visitados **2,19 M â†’ 40** (>50.000Ã— reduÃ§Ã£o) [C, blog de engenharia] |
| **2. CodificaÃ§Ã£o esparsa de restriÃ§Ãµes** | arXiv:2601.18108 [B] | arestas de **O(NÂ²) â†’ O(N)** para one-hot e **O(N log N)** para igualdade geral |
| **3. Presolve structure-aware paralelo** | arXiv:2603.03498 [B] | **18Ã—** sobre PaPILO e **6Ã—** sobre Gurobi (1 mÃ¡quina); **13Ã—** distribuÃ­do |
| **4. Lagrange / osciladores de restriÃ§Ã£o** | arXiv:2505.07179 [B] (LagONN) | Max-3-SAT atÃ© **200 var / 860 clÃ¡usulas**, determinÃ­stico, sem annealing |

Somando: a peÃ§a 1 sozinha jÃ¡ move o problema de "Ising falha" para "Ising recebe um
problema 10Â³â€“10â´Ã— menor". Ã‰ engenharia â€” exatamente como o risco dizia.

### 2.3 O desenho que isso forÃ§a

```
consulta lÃ³gica
  â”‚
  â”œâ”€[1] SIMBÃ“LICO      CNF â†’ implicaÃ§Ãµes, unidades, equivalÃªncias
  â”œâ”€[2] ROOF DUAL      max-flow/min-cut â†’ fixa variÃ¡veis persistentes
  â”œâ”€[3] DECOMPOSIÃ‡ÃƒO   comunidades do grafo â†’ sub-QUBOs independentes
  â”œâ”€[4] BSB/ISING      sÃ³ o nÃºcleo duro, N pequeno, com Lagrange se preciso
  â””â”€[5] CERTIFICADO    E, Î”E, violaÃ§Ãµes â†’ resposta ou abstenÃ§Ã£o
```

Etapas 1â€“3 sÃ£o **CPU, baratas, determinÃ­sticas, 0 alloc**. A etapa 4 Ã© a Ãºnica que
toca o MÃ³dulo 1 â€” e agora recebe instÃ¢ncias que cabem no L1 (Nâ‰¤64 â‡’ `J` = 16 KB).

### 2.4 Onde GPU entra (e onde nÃ£o entra)

| SituaÃ§Ã£o | DecisÃ£o |
|---|---|
| `N â‰¤ 128`, um organismo | **CPU**. GPU Ã© desperdÃ­cio: 166 Âµs de RTT â‰ˆ o ciclo inteiro |
| `N â‰¥ 512` (J = 1 MB+) | GPU com **estado residente** (1 bloco = 1 solve), nÃ£o launch-per-request |
| Muitos organismos (enxame) | GPU com **replicas** â€” a arXiv:2610.06622 mostra 8 replicas a 307 Gb/s |
| Modelos > VRAM | *streaming* de tiles de acoplamento |

> **Achado #2:** o ganho de 175,9Ã— da CiM vem majoritariamente da **inicializaÃ§Ã£o
> informada** (âˆ’80% iteraÃ§Ãµes), nÃ£o do ferro. Isso Ã© reproduzÃ­vel em CPU **hoje**:
> inicializar spins com a topologia global (o `J` jÃ¡ estÃ¡ lÃ¡) em vez de aleatoriamente.

### 2.5 Gate do Risco 1

| MÃ©trica | Alvo |
|---|---|
| ReduÃ§Ã£o de variÃ¡veis por roof dual, 3-SAT aleatÃ³rio `n=64` | â‰¥ 40% |
| InstÃ¢ncias resolvidas pelo pipeline que o BSB puro erra | â‰¥ 90% |
| Custo do passe simbÃ³lico | â‰¤ 5% do ciclo |
| Determinismo | bit-a-bit com seed fixa |

### 2.6 E2b v1 â€” presolve simbÃ³lico: medido e *sound*

**Executado em 2026-11-15.** `examples/e2b_presolve.rs` + `src/core/presolve.rs`.
InstÃ¢ncias 2-SAT **plantadas** (`n=16`, `m=24`, 4 unidades, 200 instÃ¢ncias,
satisfatÃ­veis por construÃ§Ã£o), ground truth e validaÃ§Ã£o de soundness por forÃ§a bruta.

| MÃ©trica | Valor |
|---|---:|
| ReduÃ§Ã£o de variÃ¡veis | **64,8%** |
| Â· plantadas (unidades) | 25,0% |
| Â· **cascata** (unidades + implicaÃ§Ã£o) | **+39,8 pp** |
| VariÃ¡veis residuais | 5,6 de 16 |
| **FixaÃ§Ãµes insound** | **0** |
| PrecisÃ£o das decisivas | **100,0%** |
| BSB puro resolve | 183/200 (91,5%) |
| **Presolve + BSB resolve** | **200/200 (100%)** |

**TrÃªs achados:**

1. **O presolve Ã© sound por construÃ§Ã£o e validado.** Fixar por unidade e por
   implicaÃ§Ã£o (`l â‡’ Â¬l`) Ã© consequÃªncia lÃ³gica; a forÃ§a bruta confirmou 0
   contradiÃ§Ãµes em 200 instÃ¢ncias.
2. **A cascata existe e Ã© grande.** 25% de unidades plantadas viram **64,8%** de
   variÃ¡veis fixadas â€” o resto vem da implicaÃ§Ã£o. Ã‰ o anÃ¡logo do que o
   prÃ©-processamento faz em CNF de circuito (arXiv:2511.21046).
3. **Ganho de completude, nÃ£o sÃ³ de velocidade.** O BSB puro erra 17/200
   (91,5%); com presolve, **200/200** â€” porque o resÃ­duo de 5,6 variÃ¡veis cabe
   folgado na capacidade do solver. Isso Ã© E2b pagando o E3b: menos variÃ¡veis,
   menos mÃ­nimos locais espÃºrios, certificado com dentes.

**Limite honesto:** isto **nÃ£o** Ã© roof dual. Roof dual (max-flow sobre a
posiform quadrÃ¡tica) fixa variÃ¡veis constantes em **algum Ã³timo** â€” alcance maior,
garantia diferente. O E2b v2 deve implementÃ¡-lo e comparar a cascata; o gate de
soundness jÃ¡ estÃ¡ pronto para os dois.

### 2.7 E2c v1 â€” persistÃªncia de primeira ordem: sound, mas mÃ­ope

**Executado em 2026-11-15.** `examples/e2c_persistency.rs` +
`persistency_qumo` em `src/core/presolve.rs`. Regra de **dominÃ¢ncia**:

```
Î”_i(s) = 2Â·s_iÂ·(h_i + (Js)_i)
s_i = +1 : max Î” = 2(h_i + Î£|J_ij|) < 0  â‡’  existe Ã³timo com s_i = âˆ’1
s_i = âˆ’1 : max Î” = 2(Î£|J_ij| âˆ’ h_i) < 0  â‡’  existe Ã³timo com s_i = +1
```

Fixar assim **preserva o valor Ã³timo** (pode descartar Ã³timos alternativos,
nunca o valor). `n=14`, `J` Â±1 simÃ©trico, viÃ©s plantado em `plantÂ·n` variÃ¡veis:

| plant | reduÃ§Ã£o | Ã³timo mudou? | presolve+BSB | BSB puro |
|---:|---:|---:|---:|---:|
| **0%** (controle) | **0,0%** | 0 | 185/200 | **185/200** |
| 25% | **28,6%** | 0 | **190/200** | 188/200 |
| 45% | **42,9%** | 0 | **199/200** | 193/200 |

**TrÃªs achados:**

1. **Soundness perfeita:** 0 fixaÃ§Ãµes mudaram o Ã³timo em **600 instÃ¢ncias** (200
   por regime), validado por forÃ§a bruta.
2. **A regra Ã© mÃ­ope por construÃ§Ã£o:** ela sÃ³ vÃª viÃ©s **jÃ¡ presente**. No
   controle (`plant=0`) a reduÃ§Ã£o Ã© **0,0%** e os dois braÃ§os dÃ£o exatamente o
   mesmo resultado (185/185) â€” prova de que a comparaÃ§Ã£o nÃ£o estÃ¡ contaminada.
3. **NÃ£o hÃ¡ cascata em `J` Â±1.** Depois de fixar uma variÃ¡vel, `h_eff` dos
   vizinhos muda em Â±1 enquanto `Î£|J|` Ã© ~6â€“13: nunca cruza o limiar. A cascata
   do E2b vem da **implicaÃ§Ã£o lÃ³gica**, que Ã© de outra natureza.

**ConclusÃ£o do E2c v1:** a persistÃªncia de primeira ordem Ã© o anÃ¡logo QUBO da
implicaÃ§Ã£o â€” **rigorosa e barata, mas limitada ao que jÃ¡ estÃ¡ explÃ­cito**. Para
alcanÃ§ar variÃ¡veis que nÃ£o sÃ£o dominadas por `h` mas sÃ£o constantes em algum
Ã³timo, Ã© preciso o **roof dual completo (max-flow)** â€” o E2c-2.

### 2.8 E2c-2 â€” roof dual completo (max-flow): o alcance depende do viÃ©s

**Executado em 2026-11-15.** `roof_dual_qumo` + `RoofDualWorkspace` em
`src/core/presolve.rs`, `examples/e2c_persistency.rs`,
`tests/e2c2_roof_dual_gate.rs`. **Exatamente as mesmas 600 instÃ¢ncias do E2c v1**
(`n = 14`, `J` Â±1 simÃ©trica, `plant âˆˆ {0%, 25%, 45%}`, 200/regime, mesmas seeds).

**O que Ã©.** O Hamiltoniano vai para a posiform quadrÃ¡tica
(`H = câ‚€ + Î£ l_i x_i + Î£ Q_ij x_i x_j`, com todo coeficiente negativo reescrito
em literais atÃ© sobrar coeficiente â‰¥ 0), cada termo positivo vira aresta de
implicaÃ§Ã£o com capacidade (`Î²Â·uÂ·v â‡’ arcs uâ†’vÌ„, vâ†’Å«`), e o **fluxo mÃ¡ximo**
(Dinic determinÃ­stico, rede de `2n+2` nÃ³s) dÃ¡ o roof dual. As persistÃªncias saem
da **alcanÃ§abilidade residual a partir de `S`**: `x_i` alcanÃ§Ã¡vel e `xÌ„_i` nÃ£o
â‡’ `x_i = 1` em algum Ã³timo (e o simÃ©trico â‡’ `x_i = 0`); ambos no mesmo lado â‡’
variÃ¡vel nÃ£o rotulada. O refinamento por SCC (`ComputeWeakPersistencies`) decide
o que a segmentaÃ§Ã£o deixou ambÃ­guo. Tudo em buffers fixos (`MAX_N = 64`):
**0 alocaÃ§Ãµes** durante a resoluÃ§Ã£o.

| `plant` | dominÃ¢ncia (E2c v1) | roof dual | roof dual + â„¤â‚‚ | roof+BSB | BSB puro |
|---:|---:|---:|---:|---:|---:|
| **0%** (controle) | 0,0% | **0,0%** | 7,1% | 185/200 | **185/200** |
| 25% | 28,6% | **28,9%** | 28,9% | 190/200 | 188/200 |
| 45% | 42,9% | **52,0%** | 52,0% | **199/200** | 193/200 |

| Soundness (600 instÃ¢ncias) | **0 Ã³timos alterados** |
|---|---|
| VariÃ¡veis fixadas em 45% (mÃ©dia) | 6,00 â†’ **7,28** (+21%) |
| Gate `presolve+BSB â‰¥ 199/200` em 45% | **199/200** âœ… |
| AlocaÃ§Ã£o no hot path | **0** |
| `presolve+BSB` no controle vs BSB puro | 185 = 185 (empate) âŒ |

**TrÃªs achados:**

1. **Soundness perfeita e alcance maior â€” mas sÃ³ onde hÃ¡ viÃ©s.** Em 600
   instÃ¢ncias o roof dual nÃ£o alterou **nenhum** Ã³timo, e em `plant = 45%` ele
   fixa **7,28** variÃ¡veis contra **6,00** da dominÃ¢ncia (+9,1 pp de reduÃ§Ã£o).
   Em 25% o ganho Ã© marginal (+0,3 pp); no controle, **nulo**.
2. **O teto do controle Ã© 0% â€” por simetria, nÃ£o por miopia.** Com `h = 0` a
   energia Ã© invariante por `s â†’ âˆ’s`: se `s*` Ã© Ã³timo, `âˆ’s*` tambÃ©m Ã©, logo
   **todo** spin aparece com os dois sinais entre os dois Ã³timos espelhados e
   *nenhuma* variÃ¡vel Ã© persistente. Medido por forÃ§a bruta: o nÃºmero de
   variÃ¡veis com valor Ãºnico entre os Ã³timos Ã© **0,0%** no controle. Ou seja, o
   `0,0%` do E2c v1 **nÃ£o era miopia** â€” era o limite teÃ³rico. A Ãºnica reduÃ§Ã£o
   sound Ã© quebrar a simetria (`fixar 1 variÃ¡vel`, coluna â„¤â‚‚): 7,1%, que Ã© o
   mÃ¡ximo que se pode escrever sem perder o Ã³timo.
3. **A completude nÃ£o paga o preÃ§o no controle.** Fixar 1 variÃ¡vel por â„¤â‚‚
   reduz o resÃ­duo para 13 variÃ¡veis com campo injetado, e o BSB **piora**
   (171/200 vs 185/200). Por isso a quebra de simetria fica **desligada por
   padrÃ£o** (`enable_z2_symmetry_break`): um presolve que degrada o solver
   a jusante Ã© um presolve ruim. Nos regimes com viÃ©s o roof dual nÃ£o regride
   (190/190 em 25%, 199/199 em 45%).

**Limite honesto:** o roof dual sobre a posiform canÃ´nica **nÃ£o** Ã© a resposta
para "quantas variÃ¡veis sÃ£o fixÃ¡veis em um vidro sem viÃ©s" â€” nesse regime a
resposta Ã© *zero* e Ã© uma propriedade da instÃ¢ncia. O que ele entrega Ã© o
**dobro de alcance onde o viÃ©s jÃ¡ organiza a paisagem** (45%). Fechar o gap
entre 52% e o teto de 93,6% (medido por forÃ§a bruta em 25%) exige **probing
com bound condicionado** (Borosâ€“Hammerâ€“Tavares), nÃ£o um max-flow maior na mesma
posiform â€” fica registrado como prÃ³ximo passo, nÃ£o como resultado.

**Gates:** G1 soundness âœ… Â· G2 alcance âœ… (â‰¥ dominÃ¢ncia; estrito em 25%/45%;
estrito no controle sÃ³ com â„¤â‚‚) Â· G3 completude em 45% âœ… (199/200) Â·
G4 "melhora o BSB puro no controle" âŒ (empate em 185 e âˆ’7,6 pp com â„¤â‚‚).
Artefatos: `target/aether_e2c2.json`, fÃ³rum **MSG 0140**.

---

## 3. Risco 2 â€” InterferÃªncia em VSA

> *"RaciocÃ­nio multi-passo acumula erro de cleanup. D tem que crescer; capacidade Ã© o teto."*

### 3.1 A lei de escala existe â€” e Ã© melhor do que se temia

arXiv:1906.11684 [B] mede a **capacidade operacional** de redes ressonadoras com
`p = 0,99` e orÃ§amento de `k = 0,001M` iteraÃ§Ãµes:

> **`M_max âˆ NÂ²`** â€” capacidade cresce **quadraticamente** com a dimensionalidade,
> independentemente do nÃºmero de fatores `F`.

Ã‚ncoras medidas em arXiv:2412.00354 [B]: o ressonador baseline satura em
**10âµ (F=2), 10â¶ (F=3), 10â· (F=4)**; as variantes com ruÃ­do chegam a **>5Ã—10â¹** em F=4.

**Isso Ã© a resposta ao risco:** nÃ£o Ã© "D tem que crescer atÃ© o infinito"; Ã© que a
capacidade **cresce ao quadrado** com D. Dobrar D quadruplica a capacidade.

### 3.2 O problema real nÃ£o Ã© capacidade â€” Ã© **ciclo-limite**

O mesmo paper identifica a falha dominante: o ressonador determinÃ­stico **entra em
ciclo-limite** e nunca converge, mesmo com iteraÃ§Ãµes infinitas.

A cura medida Ã© surpreendentemente barata e **jÃ¡ estÃ¡ no vocabulÃ¡rio do projeto**:

| TÃ©cnica | O que faz | Ganho |
|---|---|---|
| **IMF** (noise during iteration) | injeta ruÃ­do a cada passo de cleanup | +50Ã— capacidade |
| **ACF** (noise only at init) | ruÃ­do sÃ³ na inicializaÃ§Ã£o do codebook | **+50Ã—**, e **funciona em hardware digital** |
| **Sparse coding antes** (arXiv:2404.19126) | reduz colisÃµes no espaÃ§o combinatÃ³rio | aumenta capacidade efetiva |
| **Escolha da nonlinearidade** (Frontiers 2026) | sign/ReLU/poly/softmax mudam *o modo de falha*, nÃ£o sÃ³ a capacidade | permite escolher falha "abstÃ©m" em vez de "erra" |

**Insight para o AETHER:** o `ThermodynamicDreamer` jÃ¡ usa Langevin. O mesmo ruÃ­do
tÃ©rmico que hoje serve para amostrar contrafactuais pode ser **o mecanismo anti-ciclo-limite
do cleanup**. Um Ãºnico gerador de ruÃ­do, dois papÃ©is.

### 3.3 O cÃ¡lculo que faltava: D Ã— cache Ã— memÃ³ria

| D | vetor FHRR (complexo f32) | 10k Ã¡tomos | Onde vive |
|---:|---:|---:|---|
| **160** (atual) | **1,28 KB** | 12,8 MB | L3 / DRAM |
| 512 | 4,1 KB | 41 MB | DRAM |
| 1024 | 8,2 KB | 82 MB | DRAM |
| 4096 | 32,8 KB | 328 MB | DRAM + swap |

> **Achado #3 â€” o trade-off escondido:** subir D para ganhar capacidade **expulsa o
> cÃ³digo do cache**. Em D=160 e 10k Ã¡tomos jÃ¡ sÃ£o 12,8 MB â€” metade do L3 do 240H,
> e o L3 Ã© compartilhado com o SO/iGPU. **Capacidade e localidade sÃ£o a mesma moeda
> gasta duas vezes.**

SaÃ­das, em ordem de custo:
1. **Fatorar em vez de aumentar D** â€” ressonador busca em superposiÃ§Ã£o; nÃ£o precisa
   do codebook inteiro quente, sÃ³ do bloco consultado.
2. **Sparsity** (SBC/hipervetores esparsos) â€” 10k Ã¡tomos esparsos ocupam fraÃ§Ã£o.
3. **Binarizar** â€” 10.240 bits = **1,28 KB** por Ã¡tomo (o mesmo tamanho de um FHRR
   D=160 complexo!) com 10â´ Ã¡tomos = 12,8 MB. BAM jÃ¡ Ã© binÃ¡rio.
4. **Sharding por papel** â€” sÃ³ os fillers do papel ativo ficam quentes.

### 3.4 Profundidade de composiÃ§Ã£o (o risco de multi-passo)

Erro de `unbind` compÃµe. MitigaÃ§Ã£o em camadas:

```
passo k:  unbind â†’ cleanup â†’ [confianÃ§a] â†’ se confianÃ§a < Î¸: ABSTÃ‰M
                                    â””â†’ se ok: re-encode (projeta de volta no atrator)
```

Isso espelha o T12 medido (**SNR 5,88Ïƒ**) [M] e `should_abstain` jÃ¡ existente.
**Regra nova:** profundidade mÃ¡xima declarada por consulta; ao exceder, o resultado
Ã© `Inconclusivo` â€” nÃ£o uma resposta degradada.

### 3.5 Gate do Risco 2

| MÃ©trica | Alvo |
|---|---|
| Capacidade medida (`M_max`) com D=160, F=2 | â‰¥ 5.000 (curva medida, nÃ£o extrapolada) |
| Taxa de ciclo-limite sem ruÃ­do | medida e reportada |
| Ganho do ACF | â‰¥ 10Ã— capacidade |
| Profundidade de composiÃ§Ã£o com erro < 5% | â‰¥ 4 passos |

### 3.6 E1b â€” a capacidade real em D=160 (e a lei que nÃ£o era DÂ²)

**Executado em 2026-11-15.** `examples/e1b_capacity.rs` + `tests/e1b_capacity.rs`.
`FhrrStore` com fatos `e âŠ— r âŠ— v` superpostos, `recall@1` com pista completa,
200 consultas amostradas, alvo â‰¥ 99%.

| D | K=1 | **K=6** | M/D (K=6) |
|---:|---:|---:|---:|
| 64 | 8 | **32** | 0,50 |
| **160** | 16 | **64** | **0,40** |
| 320 | 32 | **128** | 0,40 |
| 640 | 32 | **256** | 0,40 |

Curva em D=160, K=6: `1,00` atÃ© M=64 Â· `0,891` em 128 Â· `0,415` em 256 Â·
`0,100` em 512 Â· `0,010` em 2048.

> **Achado #5 â€” a lei Ã© linear, nÃ£o quadrÃ¡tica.** Capacidade â‰ˆ **0,4Â·D** para
> K=6 (e â‰ˆ0,1Â·D para K=1). A lei `M_max âˆ DÂ²` citada no Vol. I (arXiv:1906.11684)
> vale para **fatoraÃ§Ã£o por ressonador com codebook conhecido**, nÃ£o para
> armazenamento por superposiÃ§Ã£o. Confundir as duas inflava a expectativa em
> **~40Ã—** em D=160.

**CorroboraÃ§Ã£o:** o T12 da auditoria usa **50 fatos** em D=160, K=6 e mede
`Recall@1 = 1,0`. Medido aqui, 64 ainda dÃ¡ 1,0 e 128 cai para 0,89 â€” o T12 estava
a **78% da capacidade**, nÃ£o no limite.

**Arm B â€” memÃ³ria de fase (M5), `--hidden 0,20`, config da auditoria,
`reset_phases(0)` antes de cada recall:**

| padrÃµes | 4 | 8 | 16 | 32 | 64 |
|---|---:|---:|---:|---:|---:|
| aleatÃ³rio (rad) | 0,034 | 0,190 | 0,448 | 0,742 | 1,121 |
| harmÃ´nico (rad) | 0,062 | 0,142 | 0,362 | 0,590 | 1,055 |

Capacidade a erro < 0,15 rad: **~4** (aleatÃ³rio) e **~8** (harmÃ´nico), contra
`ComplexPhaseMemory::pattern_capacity() = 40` â€” **fator ~5 a ~10** de otimismo no
declarado. O caso adversarial (aleatÃ³rio) nÃ£o tem bacia de atraÃ§Ã£o estruturada;
o harmÃ´nico dobra a capacidade, mas nÃ£o chega ao nÃºmero declarado.

> **Errata (MSG 0111 / 0118 / 0120).** A primeira mediÃ§Ã£o publicada
> (0,126 / 0,274 / 0,518) estava **contaminada**: a memÃ³ria carregava o atrator
> anterior entre recalls. Com `reset_phases(0)` antes de cada consulta e
> `hidden = 0,20` declarado, os nÃºmeros caem **~3,7Ã—**. Artefato canÃ´nico:
> `target/aether_e1b.json` (campos `error_random` / `error_harmonic`).
> MÃ©trica = **mÃ©dia** do erro angular nas posiÃ§Ãµes ocultas, `seed = 0xE1B_0001`,
> config `omega0=0, clamp=8, eta_w=0,12, eta_delta=0,08, gamma_w=0,002`.

**ConsequÃªncia arquitetural (a que importa):** se o PLH precisar de ~1.000 fatos,
D=160 nÃ£o serve â€” pela lei linear, seria **Dâ‰ˆ2.500**, com vetor de 20 KB e 1.000
Ã¡tomos â‰ˆ 20 MB (fora do L2). Portanto **sharding por papel/relaÃ§Ã£o** ou
**fatoraÃ§Ã£o por ressonador** deixam de ser otimizaÃ§Ã£o e passam a ser **requisito**
do braÃ§o da linguagem.

### 3.6.1 E1b-2 â€” memÃ³ria de fase: aleatÃ³rio vs harmÃ´nico (a fronteira do M5)

**Executado em 2026-11-15.** `examples/e1b_capacity.rs --phase-only` +
`tests/e1b_capacity.rs`. Mesmo protocolo do Arm B: `D = 160`, **20% da pista
oculta** (32 de 160), 40 ciclos de `imprint`, `recall` de 200 passos, erro
angular mÃ©dio **sÃ³ nas posiÃ§Ãµes ocultas**. Varredura `M âˆˆ {4, 8, 16, 24, 32, 40,
48}` (antes ia atÃ© 64 em potÃªncias de dois).

| `M` | aleatÃ³rio (rad) | harmÃ´nico (rad) | < 0,15 rad |
|---:|---:|---:|---|
| 4 | **0,034** | 0,062 | ambos |
| 8 | 0,190 | **0,142** | sÃ³ harmÃ´nico |
| 16 | 0,448 | 0,362 | nenhum |
| 24 | 0,635 | 0,493 | nenhum |
| 32 | 0,742 | 0,590 | nenhum |
| 40 | 0,834 | 0,699 | nenhum |
| 48 | 0,962 | 0,873 | nenhum |

| Limite operacional seguro (`erro < 0,15 rad`) | `M_harm = 8` Â· `M_rand = 4` |
|---|---|
| Declarado por `ComplexPhaseMemory::pattern_capacity()` | **40** |
| Otimismo do declarado vs harmÃ´nico | **5Ã—** (era 10Ã— vs aleatÃ³rio) |

**TrÃªs achados:**

1. **O padrÃ£o estruturado dobra a fronteira, nÃ£o a multiplica.** HarmÃ´nico
   sustenta `M = 8` a erro < 0,15 rad contra `M = 4` do aleatÃ³rio â€” **2Ã—**, nÃ£o
   os 10Ã— que o `pattern_capacity() = 40` sugeria. Em `M = 4` o aleatÃ³rio Ã©
   *melhor* (0,034 vs 0,062): a base periÃ³dica tem menos graus de liberdade e
   paga por isso quando o nÃºmero de padrÃµes Ã© pequeno.
2. **Acima de 8 os dois braÃ§os colapsam juntos.** Em `M = 48` o erro Ã©
   0,962/0,873 rad â€” ou seja, a bacia de atraÃ§Ã£o do M5 acaba bem antes de
   `N/4`, e a diferenÃ§a harmÃ´nicoâˆ’aleatÃ³rio (~15%) nÃ£o muda o regime.
3. **A fronteira do M5 Ã© 8, nÃ£o 40.** Para o PLH isso fixa o desenho: o M5 nÃ£o
   Ã© armazenamento de fatos (Ã© o `FhrrStore`/shards que Ã©), Ã© **atrator de
   trabalho** com ~8 padrÃµes ativos. `pattern_capacity()` deve ser lido como
   *teto da janela deslizante*, nunca como capacidade de recall.

**Nota de comparabilidade:** o E1b v1 registrou 0,126 rad (M=4) e 0,274 rad
(M=8) no braÃ§o aleatÃ³rio. Os valores absolutos caÃ­ram (0,034/0,190) porque o M5
recebeu a regra de projeÃ§Ã£o complexa depois daquela mediÃ§Ã£o; **a fronteira
(4 aleatÃ³rio / 8 harmÃ´nico) Ã© a mesma**. Artefato:
`target/aether_e1b.json` (`phase_capacity_error_0_15`), fÃ³rum **MSG 0141**.

---

## 4. Risco 3 â€” TransplantaÃ§Ã£o dÃ¡ geometria, nÃ£o verdade

> *"'Similar' â‰  'correto'. O risco de alucinaÃ§Ã£o volta como hipervetor plausÃ­vel."*

### 4.1 A matemÃ¡tica da transplantaÃ§Ã£o Ã© sÃ³lida â€” e o limite Ã© estrutural

O pipeline do preprint (Zenodo 19476847 [C]) Ã© **Johnsonâ€“Lindenstrauss clÃ¡ssico**:
matriz Gaussiana semeada `P âˆˆ â„^{dÃ—D}` + binarizaÃ§Ã£o por sinal.

VerificaÃ§Ã£o independente da conta deles:

```
JL:  D â‰¥ 8 ln(n) / ÎµÂ²
n = 5.000, Îµ = 0,10  â‡’  D â‰¥ 8 Ã— 8,517 / 0,01 = 6.814
D = 10.240           â‡’  10.240 / 6.814 = 1,50Ã—  (eles alegam â‰¥1,40Ã—) âœ“
```

NÃºmeros alegados [C]: Spearman **Ï > 0,98**; BERT contextual Ï = 0,9906, Recall@10 =
0,889; MiniLM escalado Ï = 0,9839, Recall@10 = 0,954; analogia `kingâŠ•manâŠ•woman` Top-1
**78%**, Top-5 96%.

**O que jÃ¡ Ã© [B] independentemente:**
- BinarizaÃ§Ã£o de embeddings preserva semÃ¢ntica com perda pequena: **~2%** de degradaÃ§Ã£o
  com **98%** menos armazenamento (arXiv:1906.08340 [B]).
- Autoencoder siamÃªs com perda de preservaÃ§Ã£o semÃ¢ntica: **4%** de perda, **9,4Ã—** menos
  memÃ³ria (ACM [B]).
- Binding recursivo preserva similaridade de sequÃªncia em FHRR (arXiv:2201.11691 [B]).

**O que Ã© [C] e nÃ£o deve ser tratado como resultado:** os 78% de analogia e a
"restauraÃ§Ã£o de raciocÃ­nio funcional" (+0,400 no Top-1 de relaÃ§Ã£o). 78% de Top-1
significa **22% de respostas plausÃ­veis e erradas** â€” isto Ã©, exatamente o modo de
falha que o risco aponta.

> **Medição E3 (2026-11-16, `tests/e3_transplant_gate.rs`).** O gate de analogia em
> fase mediu **Recall@1 = 0,0208** (esperado ≥ 0,80) — praticamente o acaso —
> **enquanto o gate métrico do JL passou** (a estrutura de similaridade foi
> preservada). Duas leituras possíveis, e a distinção importa:
>
> 1. **bug de *readout*** no pipeline de analogia (papel/permutação errados) — o
>    dono do E3 está corrigindo;
> 2. **limite real**: transplantar a **geometria** não transfere a **relação**.
>    É exatamente o que §4.2 previa e o que o E3b confirmou por outro caminho
>    (coerência ≠ verdade).
>
> Até a reconciliação, o número **não** entra como resultado: entra como *alerta
> aberto* (MSG 0194). O que já se pode afirmar: **o JL preserva métrica; a
> analogia não veio de graça.**

### 4.2 A distinÃ§Ã£o que o projeto precisa gravar

| Tipo de alegaÃ§Ã£o | O AETHER pode certificar? |
|---|---|
| **ConsistÃªncia** (nÃ£o viola restriÃ§Ãµes, nÃ£o contradiz a base) | **SIM** â€” Ising + violaÃ§Ãµes |
| **Derivabilidade** (segue das premissas por regra) | **SIM** â€” se as regras estiverem codificadas |
| **Similaridade** (estÃ¡ perto do que se sabe) | Sim, geometricamente â€” **mas nÃ£o Ã© verdade** |
| **Factualidade** (o mundo Ã© assim) | **NÃƒO** â€” depende da fonte transplantada |

> **Achado #4:** o certificado de factibilidade certifica *consistÃªncia*, nÃ£o *verdade*.
> Chamar isso de "anti-alucinaÃ§Ã£o" seria o mesmo erro que o risco denuncia. O nome
> correto Ã© **certificado de factibilidade** â€” e a resposta deve carregar a proveniÃªncia
> (`transplantado de X`, `observado em T`, `inferido de regra R`).
>
> **Confirmado por mediÃ§Ã£o no E3b (Â§4.6):** ele tambÃ©m **nÃ£o** certifica *otimalidade*.
> Em otimizaÃ§Ã£o livre o gap nÃ£o prediz acerto; sÃ³ a contagem de violaÃ§Ãµes tem dentes.

### 4.3 A evidÃªncia mais forte a favor do verificador

| Trabalho | NÃºmero |
|---|---|
| **ReGA** (WWW 2026) [A] | detecÃ§Ã£o de alucinaÃ§Ã£o estrutural em **14â€“17 ms** em CPU; **70â€“88Ã—** mais rÃ¡pido que juiz LLM; AUC 0,90 (funcional) e 0,80 (inversÃ£o de papel); energia 0,00 para direÃ§Ã£o correta vs **1,82** para invertida |
| **Distributional EBM** (arXiv:2605.18871) [B] | verificador de **149M** parÃ¢metros orquestra geradores de 7â€“26B e supera Qwen-72B em 5 benchmarks; **âˆ’53%** de violaÃ§Ã£o de restriÃ§Ã£o |
| **KG-Guard** (arXiv:2606.00328) [B] | F1 82,0/87,4/84,3 em WebQSP/CWQ/PUGG com **~305Ã—** menos parÃ¢metros; feedback eleva F1 downstream em **+13,0â€“14,5** |
| **Spilled energy** (arXiv:2602.18671) [B] | mÃ©trica de energia **sem treino** separa correto de incorreto em 9 benchmarks |
| **Carnot EBM** (negativo) [C] | 38 experimentos, 16 modelos: EBM em ativaÃ§Ãµes detecta **confianÃ§a**, nÃ£o verdade. "AlucinaÃ§Ã£o confiante Ã© indistinguÃ­vel de acerto confiante." |

Esse Ãºltimo resultado Ã© o mais importante do bloco: ele **falsifica** a esperanÃ§a de
resolver verdade por plausibilidade interna. ReforÃ§a a decisÃ£o: **o verificador tem
que ser de restriÃ§Ã£o, nÃ£o de ativaÃ§Ã£o.**

### 4.4 Custo do verificador â€” o nÃºmero que decide a arquitetura

14 ms por verificaÃ§Ã£o (ReGA, CPU) contra **146,5 Âµs** de ciclo [M] = **95 ciclos** de
orÃ§amento. Ou seja: verificar **nÃ£o cabe no hot path**.

Desenho resultante:

```
hot path (146 Âµs)     â†’ propÃµe + emite (E, Î”E, violaÃ§Ãµes)       [sempre]
fora do hot path      â†’ verificaÃ§Ã£o estrutural 14â€“17 ms         [assÃ­ncrona]
resposta ao usuÃ¡rio   â†’ sÃ³ sai depois do gate                    [latÃªncia humana: OK]
```

### 4.5 Gate do Risco 3

| MÃ©trica | Alvo |
|---|---|
| Analogia Top-1 em benchmark prÃ³prio (nÃ£o o do preprint) | medida, sem alvo de marketing |
| PrecisÃ£o do certificado: `Î”E` grande â‡’ acerto | â‰¥ 95% |
| Falsos "confiÃ¡veis" (responde com Î”E alto e erra) | < 5% |
| VerificaÃ§Ã£o | â‰¤ 20 ms, CPU, 0 alloc |

### 4.6 E3b â€” o veredito: `Î”E` **nÃ£o** prediz acerto (e o que prediz)

**Executado em 2026-11-15.** `examples/e3b_certificate.rs`, ground truth exato por
forÃ§a bruta (`n=14`, 400 instÃ¢ncias de vidro de spin, 4 restarts, steps=8).

| Sinal | AUC | precisÃ£o@50% | confiante-e-errado@50% |
|---|---:|---:|---:|
| gap local `min Î”H_i` | 0,547 | 90,0% | 10,0% |
| consenso entre restarts | 0,526 | 89,9% | 10,1% |
| gap entre restarts | 0,501 | 90,0% | 10,0% |
| **gap atÃ© o bound espectral** | **0,058** | 81,3% | **18,7%** |
| confianÃ§a combinada | 0,462 | 88,1% | 11,9% |

Base do solver: **90,0%**. **Nenhum sinal adiciona 1 ponto percentual.** O gap atÃ©
o bound Ã© **anti-preditivo** (AUC 0,058): o limite de Gershgorin Ã© rigoroso mas
frouxo, e instÃ¢ncias onde ele aperta sÃ£o justamente as fÃ¡ceis.

**Por que falha:** em otimizaÃ§Ã£o **nÃ£o restrita**, *todo* mÃ­nimo local Ã© coerente â€”
`gap > 0` e `violations = 0` â€” e um vidro de spin tem exponencialmente muitos.
**CoerÃªncia nÃ£o Ã© otimalidade.**

**O contraste que salva o conceito** (mesma sessÃ£o, `--max2sat`): quando a
verificaÃ§Ã£o Ã© **exata** (restriÃ§Ãµes contÃ¡veis), o certificado Ã© exato:

| MÃ©trica | Valor |
|---|---:|
| satisfatÃ­veis (forÃ§a bruta) | 397/400 |
| decisivas (0 violaÃ§Ãµes) | 383 (95,8%) |
| **precisÃ£o das decisivas** | **100,0%** |
| satisfatÃ­veis resolvidas | 96,5% |
| nÃ£o-satisfatÃ­veis | sempre abstÃ©m |

> **ConclusÃ£o do P0 (R3):** o certificado de factibilidade **nÃ£o** Ã© um detector de
> acerto â€” Ã© um **verificador de factibilidade**. Ele tem dentes onde existe
> restriÃ§Ã£o contÃ¡vel (lÃ³gica, E2b) e Ã© inÃºtil onde sÃ³ existe paisagem de energia
> (otimizaÃ§Ã£o pura). A tese do Vol. I sobrevive **com esta correÃ§Ã£o**: o "certificado
> de energia" deve ser lido como *prova de consistÃªncia*, nunca como *prova de
> verdade ou de Ã³timo*.

Isso reordena E2b: **roof dual e restriÃ§Ãµes nÃ£o sÃ£o um acelerador de Ising â€” sÃ£o a
condiÃ§Ã£o de existÃªncia do certificado.**

---

## 5. Risco 4 â€” Open-endedness precisa de corpo

> *"Metas autotÃ©licas sem sensorium real viram passeio. O Produto C Ã© fraco mas real."*

### 5.1 A evidÃªncia de que "corpo" Ã© fechamento de laÃ§o, nÃ£o biologia

| Trabalho | Resultado |
|---|---|
| **AIF + diffusion + MTRSSM** (arXiv:2510.23258) [B] | robÃ´ real: **75%** sucesso vs 53% (sÃ³ extrÃ­nseco), 50% (FlowNav), **8,33%** (NoMaD); menos colisÃµes; ganho maior justamente em exploraÃ§Ã£o (78% vs 28%) |
| **AIF hierÃ¡rquico temporal** (arXiv:2512.01924) [B] | manipulaÃ§Ã£o real; alterna dirigidoâ†”exploratÃ³rio sob incerteza; custo tratÃ¡vel via aÃ§Ãµes abstratas (VQ) |
| **AIF como lei de escala de test-time** (arXiv:2606.22813) [B] | generaliza para cenÃ¡rios nÃ£o vistos **escalando com experiÃªncia contÃ­nua**; **+36%** de eficiÃªncia de inferÃªncia |
| **Racing no limite** (arXiv:2608.10618) [B] | 256,3 km/h, 26,8 m/sÂ²; **88,3%** de sucesso com world model |
| **AWAM** (arXiv:2610.23396) [B] | **94,4%** em robÃ´ real com imaginaÃ§Ã£o de viewpoint |

PadrÃ£o comum: **nÃ£o Ã© o corpo que importa, Ã© o laÃ§o fechado com consequÃªncia** â€” e
todos usam *hierarquia temporal* (lento = meta, rÃ¡pido = aÃ§Ã£o). O AETHER jÃ¡ tem isso
em `renormalization` (64â†’16â†’4) e EMA Ï„=100 ms.

### 5.2 O sensorium mÃ­nimo viÃ¡vel, em trÃªs estÃ¡gios

| EstÃ¡gio | Fonte | AÃ§Ãµes com consequÃªncia | Custo |
|---|---|---|---|
| **S0 (hoje)** | telemetria de host: jitter, CPU, disco, rede | 4 atuadores: throttling, migraÃ§Ã£o de afinidade, alarme, pausa | zero hardware |
| **S1** | mundo sintÃ©tico com fÃ­sica contÃ­nua (tipo Lenia, arXiv:2608.26116 [B]) | perturbaÃ§Ã£o local do campo | CPU/GPU jÃ¡ existente |
| **S2** | sensor real (CAN/CSI) â€” Produto A/B | atuaÃ§Ã£o fÃ­sica | certificaÃ§Ã£o |

**S0 Ã© suficiente para falsificar a hipÃ³tese?** Sim, se as aÃ§Ãµes tiverem consequÃªncia
real: se o organismo "decide" alarmar e o alarme nÃ£o muda nada, ele degenera em passeio.
A primeira tarefa de E7 Ã© justamente medir **degeneraÃ§Ã£o**: as metas propostas
convergem para um conjunto trivial?

### 5.3 O teste anti-passeio

```
H1: distribuiÃ§Ã£o de metas permanece com entropia > H_min apÃ³s 10âµ ciclos
H2: o agente prefere metas de dificuldade intermediÃ¡ria (progresso mÃ¡ximo)
H3: remover o atuador faz a entropia de metas cair (i.e. o corpo restringe)
```

Se H3 falhar, o "corpo" Ã© decorativo â€” e o risco estÃ¡ confirmado.

### 5.4 Gate do Risco 4

| MÃ©trica | Alvo |
|---|---|
| Entropia de metas em 10âµ ciclos | nÃ£o decai mais de 20% |
| CorrelaÃ§Ã£o dificuldade Ã— escolha (curva em U invertido) | Spearman > 0,5 |
| H3 (ablaÃ§Ã£o do atuador) | entropia cai â‰¥ 30% |

---

## 6. Risco 5 â€” Custo de treino

> *"OLMs treinam ~40Ã— mais devagar (sem parallel scan). InferÃªncia Ã© o forte; treino Ã© o gargalo."*

### 6.1 Primeiro: o risco pode ser evitado, nÃ£o vencido

O prÃ³prio preprint do OLM (SSRN 7431978 [C]) reporta o achado mais Ãºtil:

> melhores resultados com **dinÃ¢mica aprendida devagar ou congelada** â€” "um reservatÃ³rio
> de fase estruturado com fronteiras aprendidas".

Isso Ã© **reservoir computing**: congele a fÃ­sica, treine sÃ³ o readout linear.

E a literatura de RC Ã© grande, peer-reviewed e barata:

| Trabalho | NÃºmero |
|---|---|
| **OpenReservoirComputing** (arXiv:2603.14802) [B] | RC em JAX/Equinox: GPU, JIT, vetorizaÃ§Ã£o automÃ¡tica |
| **OpenPRC** (arXiv:2604.07423) [B] | engine fÃ­sica GPU (RK4â€“PBD) + camada de aprendizado; schema HDF5 unificado |
| **NEMS ressonador** (arXiv:2609.29532) [B] | multiplexaÃ§Ã£o de modos reduz erro **>28Ã—** sem treinar parÃ¢metros internos |
| **Josephson junction** (arXiv:2605.13354) [B] | um Ãºnico JJ jÃ¡ Ã© reservatÃ³rio suficiente |
| **Vestibular RC** (arXiv:2604.09943) [B] | topologia **desacoplada** â‰ˆ acoplada; memÃ³ria analÃ­tica |

> **Achado #5:** o AETHER jÃ¡ Ã©, estruturalmente, um reservatÃ³rio: fÃ­sica fixa +
> `OnlineKoopmanOperator` (RLS/Sherman-Morrison) como readout linear. **NÃ£o hÃ¡
> 40Ã— de treino a pagar** se a fÃ­sica nÃ£o for treinada.

### 6.2 Se precisar treinar a dinÃ¢mica: DEER existe, mas nÃ£o salva em n=64

| MÃ©todo | Ganho | Limite |
|---|---|---|
| **DEER** (ICLR 2024, arXiv:2309.12252) [A] | **10Ã—** treino; atÃ© **500Ã—** forward; >1000Ã— fwd+grad; 1 M passos em 15 ms | complexidade **O(nÂ³)**; em **n=64 o ganho cai a ~25%** |
| **ELK** (NeurIPS 2024, arXiv:2407.19115) [A] | quasi-Newton + Kalman smoothing: menos memÃ³ria, mais estÃ¡vel | ainda O(nÂ³)-ish |

Para o AETHER (`n = 64`), DEER rende ~25%. **NÃ£o Ã© a alavanca.** As alavancas reais:

1. **Congelar a dinÃ¢mica** (reservoir) â†’ treino = mÃ­nimos quadrados, fechado, paralelo.
2. **Paralelizar por organismo** â€” enxame Ã© *embarrassingly parallel*; 6 P-cores = 6Ã—.
3. **Treinar offline em GPU** e sÃ³ inferir na borda.

### 6.3 O orÃ§amento real de treino

```
inferÃªncia:  146,5 Âµs/ciclo Ã— 86.400 s = 757 s de CPU por dia de operaÃ§Ã£o
treino:      se 10â¶ passos Ã— 150 Âµs     = 150 s  (uma vez)
```

Ou seja: **um dia de operaÃ§Ã£o custa 5Ã— mais CPU que um treino inteiro.** O gargalo
nÃ£o Ã© treino â€” Ã© *desenvolvimento* (iterar arquitetura). E isso se resolve com
experimentos pequenos, nÃ£o com escala.

### 6.4 Gate do Risco 5

| MÃ©trica | Alvo |
|---|---|
| Treino do readout (RLS) em 10â¶ amostras | < 60 s, 1 core |
| RegressÃ£o de acurÃ¡cia fÃ­sico-congelado vs fÃ­sico-treinado | < 5% |
| Throughput de inferÃªncia com dinÃ¢mica congelada | â‰¥ baseline [M] |

---

## 7. Risco 6 â€” Preprints de baixa revisÃ£o

> *"Wave Memory, Causal Oscillator, Model Transplantation sÃ£o preprints â€” hipÃ³tese, nÃ£o resultado."*

### 7.1 O mÃ©todo: triangulaÃ§Ã£o em trÃªs camadas

Para cada alegaÃ§Ã£o [C], procurei: (i) mecanismo em fonte [A]/[B], (ii) nÃºmero
independente, (iii) plausibilidade fÃ­sica.

| Preprint [C] | Mecanismo corroborado por | NÃºmeros replicados? | Veredito |
|---|---|---|---|
| **Model Transplantation** | JL + binarizaÃ§Ã£o: arXiv:1906.08340 [B] (~2% perda), ACM siamÃªs [B] (~4%), arXiv:2201.11691 [B] | **NÃƒO** (78% analogia) | mecanismo **[A]**, nÃºmeros **[C]** |
| **Wave Memory** (100% coerÃªncia) | PRISM arXiv:2512.01208 [B] (COMET 0.799 com fase pura), CAWN arXiv:2604.04250 [B], Attention by Sync arXiv:2606.12059 [B] | **NÃƒO** â€” e "100%" Ã© sinal de alerta | mecanismo **[B]**, claims **[Câˆ’]** |
| **Causal Oscillator** (1,34 BPB) | SSM/state-space peer-reviewed; reservatÃ³rio fÃ­sico | **NÃƒO** | **[C]** |
| **OLM** (40Ã— mais lento) | consistente com ausÃªncia de parallel scan (DEER/ELK existem justamente por isso) [A] | coerente | **[C]**, mas o *diagnÃ³stico* Ã© **[A]** |

### 7.2 Regras de uso adotadas

1. **Nenhuma decisÃ£o de arquitetura** se apoia em [C]. E1â€“E9 podem ser *motivados* por [C],
   mas o gate Ã© sempre uma medida nossa.
2. **Toda citaÃ§Ã£o de [C]** leva o selo no texto.
3. **Anti-sinal:** qualquer resultado "100%" ou "sem perda" em preprint Ã© tratado como
   erro de mediÃ§Ã£o atÃ© prova em contrÃ¡rio.
4. **ReproduÃ§Ã£o mÃ­nima:** antes de usar um mecanismo [C], reproduzimos a parte trivial
   (ex.: JL com Ï de similaridade) em < 1 dia.

### 7.3 O que a triangulaÃ§Ã£o salvou

- **NÃ£o** construir o PLH em cima do "100% de coerÃªncia" do Wave Memory.
- **Sim** construir em cima do PRISM (fase pura, COMET medido, arXiv) e do OLMs (mecanismo).
- **NÃ£o** assumir transplantaÃ§Ã£o como verdade.
- **Sim** assumir JL+binarizaÃ§Ã£o como transporte de geometria â€” e sÃ³ isso.

---

## 8. Registo de risco atualizado

| # | Risco | Status apÃ³s Vol. II | Risco residual | MitigaÃ§Ã£o principal |
|---|---|---|---|---|
| **R1** | Ising nÃ£o Ã© orÃ¡culo | **mitigÃ¡vel com engenharia conhecida** | mÃ©dio | roof dual (10Â³â€“10â´Ã— menor) + presolve structure-aware + Lagrange |
| **R2** | InterferÃªncia VSA | **mitigÃ¡vel; capacidade âˆ DÂ²** | mÃ©dio | ACF/IMF (ruÃ­do na init) + fatoraÃ§Ã£o + abstenÃ§Ã£o por passo |
| **R3** | Geometria â‰  verdade | **irredutÃ­vel em parte** | **alto** | certificado de *coerÃªncia* (nÃ£o verdade) + proveniÃªncia + verificador 14 ms |
| **R4** | Open-endedness sem corpo | **mitigÃ¡vel com S0** | mÃ©dio | laÃ§o fechado com atuador real + teste anti-passeio H1â€“H3 |
| **R5** | Custo de treino | **evitÃ¡vel** (reservoir) | baixo | congelar fÃ­sica; RLS como readout |
| **R6** | Preprints | **gerenciÃ¡vel** | baixo | triangulaÃ§Ã£o + selos + regra "nenhuma decisÃ£o sÃ³ em [C]" |

> Os riscos `R7â€“R10` nascem no Vol. III (drift para RAG, vazamento de escopo,
> proveniÃªncia-teatro, certificado sem dentes) e nÃ£o sÃ£o consequÃªncia do hardware â€”
> sÃ£o consequÃªncia da **integraÃ§Ã£o**.

### 8.1 Gaps novos

| CÃ³digo | Severidade | DescriÃ§Ã£o | RemediaÃ§Ã£o |
|---|---|---|---|
| `NO_PROFILE_BREAKDOWN` | âœ… **resolvido (E6)** | medido: solver 84,4% Â· 7,5 GFLOP/s (~19% do pico AVX2) | â€” |
| `SOLVER_KERNEL_SCALAR` | **crÃ­tico** | gemv + energia por passo sÃ£o laÃ§os escalares; 5Ã— de headroom atÃ© o pico f64 | E6b (SIMD/f32) |
| `NO_ROOF_DUAL` | alto | sem passe simbÃ³lico; Ising recebe instÃ¢ncia crua | E2b |
| `NO_LIMIT_CYCLE_GUARD` | alto | cleanup determinÃ­stico pode ciclar | E1b |
| `NO_PROVENANCE` | alto | resposta nÃ£o diz de onde veio (transplantado/observado/inferido) | E3b |
| `D_CAPACITY_UNMEASURED` | mÃ©dio | capacidade real em D=160 desconhecida | E1 |
| `NO_ACTUATOR_LOOP` | alto | metas sem consequÃªncia (S0 incompleto) | E7 |
| `CERT_SEMANTICS` | mÃ©dio | "certificado de energia" pode ser lido como "verdade" ou "Ã³timo" | âœ… resolvido: **certificado de factibilidade** (Vol. II Â§4.6) |

---

## 9. Matriz de decisÃ£o de hardware

| CenÃ¡rio | Substrato | Justificativa numÃ©rica |
|---|---|---|
| Motor em produÃ§Ã£o, N=64, 1 organismo | **CPU (AVX2)** | 0,88% de um core/dia; GPU seria 1000Ã— overkill |
| N â‰¥ 512 | **GPU, estado residente** | `J` â‰¥ 1 MB; L2 estourado; 307 Gb/s de streaming |
| Enxame (muitos organismos) | **GPU com replicas** ou **6 P-cores** | paralelismo trivial; 618â€“628 k req/s |
| Produto C (host 24/7) | **CPU**; neuromÃ³rfico se energia for TCO | Loihi 2: 496Ã— menos energia/amostra |
| Produto B (casa, bateria) | **FPGA ou neuromÃ³rfico** | FPGA HDC: 0,09 ms, 1300Ã— CPU |
| LÃ³gica pesada (SAT grande) | **CPU hÃ­brida + Ising** | presolve 6â€“18Ã—; Ising sÃ³ no nÃºcleo |
| Pesquisa de fronteira (CiM) | **nÃ£o** | exige FeFET; ganho vem do algoritmo |

**Regra prÃ¡tica:** *CPU atÃ© N=512; GPU depois; neuromÃ³rfico sÃ³ quando a energia for
a mÃ©trica â€” nunca por moda.*

---

## 10. Experimentos novos (E6, E2b, E1b, E3b, E7, E8)

> Registro mestre com todos os IDs (`E1â€“E8`, `F1â€“F3`) em **Vol. III Â§11.1**.

| # | Entrega | Mede | Gate |
|---|---|---|---|
| **E6** âœ… | `examples/e6_profile.rs` + `tests/e6_profile_gate.rs` | perfil por estÃ¡gio â€” **executado 2026-11-15** | cobertura **99,98%**; solver **84,4%** |
| **E6b** | otimizar o kernel do solver (SIMD f64 ou f32 + fusÃ£o) | GFLOP/s do solver | â‰¥ 2Ã— o baseline (de ~7,5 para â‰¥ 15 GFLOP/s) |
| **E2b** | roof dual + presolve antes do BSB | reduÃ§Ã£o de variÃ¡veis, tempo, acerto | â‰¥40% reduÃ§Ã£o; â‰¥90% das instÃ¢ncias que o BSB puro erra |
| **E1b** | ACF/IMF no cleanup do `FhrrStore` | capacidade e taxa de ciclo-limite com/sem ruÃ­do | â‰¥10Ã— capacidade; ciclo-limite < 1% |
| **E3b** | proveniÃªncia + verificador de coerÃªncia | precisÃ£o do selo; falsos "confiÃ¡veis" | â‰¥95% precisÃ£o; < 5% falsos |
| **E7** | laÃ§o S0 com 4 atuadores | entropia de metas, H1â€“H3 | H3 confirmado |
| **E8** | readout congelado (reservoir) vs treinado | regressÃ£o de acurÃ¡cia; tempo de treino | < 5% regressÃ£o; < 60 s |

E1â€“E5 do Volume I permanecem vÃ¡lidos; E6 vem **primeiro** (Ã© diagnÃ³stico e barato).

---

## 11. O que mudou desde o Volume I

| Tema | Volume I | Volume II |
|---|---|---|
| Ising | "nÃ£o Ã© orÃ¡culo, precisa de engenharia" | **engenharia quantificada:** roof dual 5.800Ã—, presolve 6â€“18Ã—, sparse 0, Nâ†’O(N) |
| VSA | "D tem que crescer; capacidade Ã© o teto" | **capacidade âˆ DÂ²**; o inimigo Ã© **ciclo-limite**, curado com ruÃ­do na init (+50Ã—) |
| TransplantaÃ§Ã£o | "ponte promissora e arriscada" | **JL Ã© sÃ³lido; 78% de analogia Ã© [C]**; certifica coerÃªncia, nÃ£o verdade |
| Corpo | "Produto C Ã© fraco mas real" | **laÃ§o fechado basta**; 3 estÃ¡gios S0â€“S2; teste anti-passeio H1â€“H3 |
| Treino | "40Ã— mais lento Ã© o gargalo" | **gargalo evitÃ¡vel**: congelar fÃ­sica (reservoir); DEER sÃ³ rende 25% em n=64 |
| Preprints | "trate como hipÃ³tese" | **protocolo de triangulaÃ§Ã£o** com selos [A]/[B]/[C] e regra de decisÃ£o |
| Hardware | "cabe no L2; penhasco em Nâ‰ˆ512" | **+ sem AVX-512**; **medido: solver = 84,4% do ciclo, ~7,5 GFLOP/s (~19% do pico)** |
| Energia | nÃ£o calculado | **0,88% de um core/dia; ~1â€“3 Wh/dia** |

---

## 12. ReferÃªncias novas

### 12.1 Ising, QUBO e lÃ³gica

| ID | TÃ­tulo | Selo |
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
| blog Hercules | 5.800Ã— / 2,19 Mâ†’40 nÃ³s | [C] |

### 12.2 VSA, capacidade e hardware

| ID | TÃ­tulo | Selo |
|---|---|---|
| arXiv:1906.11684 | Resonator Networks outperform optimization methods | [B] |
| arXiv:2007.03748 | Resonator networks for factoring distributed representations | [B] |
| arXiv:2412.00354 | On the Role of Noise in Factorizers (IMF/ACF, +50Ã—) | [B] |
| Frontiers 2026 | Comparative study of nonlinear cleanup rules in resonator networks | [A] |
| arXiv:2311.04872 | Computing with Residue Numbers in High-Dimensional Representation | [B] |
| arXiv:2404.19126 | Compositional Factorization with Convolutional Sparse Coding | [B] |
| arXiv:2506.09282 | ScalableHD: HDC inference on multi-core CPUs (10Ã—) | [B] |
| arXiv:2601.20061 | Primitive-Driven Acceleration of HDC (FPGA 0,09 ms) | [B] |
| arXiv:2605.06485 | Litespark: ternary SIMD (95,81Ã— AVX-512 VNNI) | [B] |
| arXiv:1906.08340 | Binary sentence embeddings (~2% perda, 98% menos memÃ³ria) | [B] |
| arXiv:2201.11691 | Recursive Binding for Similarity-Preserving Hypervectors | [B] |
| ACM 3508230.3508235 | Siamese Autoencoder for Binary Quantization (4% perda) | [A] |
| arXiv:2508.14245 | Cross-Layer Design of Vector-Symbolic Computing | [B] |

### 12.3 VerificaÃ§Ã£o, verdade e alucinaÃ§Ã£o

| ID | TÃ­tulo | Selo |
|---|---|---|
| WWW 2026 (ReGA) | Zero-Overhead Graph Alignment for Structural Hallucination Detection | [A] |
| arXiv:2605.18871 | Distributional Energy-Based Models for Structured LLM Reasoning | [B] |
| arXiv:2606.00328 | KG-Guard: Graph-Based Hallucination Detection | [B] |
| arXiv:2602.18671 | Spilled Energy in LLMs | [B] |
| arXiv:2602.19643 | KGHaluBench | [B] |
| Carnot EBM | EBM em ativaÃ§Ãµes detecta confianÃ§a, nÃ£o verdade (negativo) | [C] |
| Zenodo 19476847 | Model Transplantation (versÃ£o completa) | [C] |

### 12.4 Corpo, agÃªncia e treino

| ID | TÃ­tulo | Selo |
|---|---|---|
| arXiv:2510.23258 | Deep AIF + diffusion + MTRSSM (robÃ´ real, 75%) | [B] |
| arXiv:2512.01924 | Real-World Robot Control by Deep AIF (hierÃ¡rquico) | [B] |
| arXiv:2606.22813 | Active Inference as the Test-Time Scaling Law | [B] |
| arXiv:2610.05692 | Benchmarking Generative Trajectory Models for AIF | [B] |
| arXiv:2608.10618 | World-Model-Centric Autonomous Racing (256 km/h) | [B] |
| arXiv:2610.23396 | AWAM (94,4% real) | [B] |
| arXiv:2608.10449 | PBD-AG Persistent Baseline-Delta Active Graphs | [B] |
| arXiv:2309.12252 | DEER: Parallelizing non-linear sequential models (ICLR 2024) | [A] |
| arXiv:2407.19115 | ELK: Scalable and Stable Parallelization of Nonlinear RNNs (NeurIPS 2024) | [A] |
| arXiv:2603.14802 | OpenReservoirComputing (JAX, GPU) | [B] |
| arXiv:2604.07423 | OpenPRC: Physics-to-Task in Physical RC | [B] |
| arXiv:2609.29532 | Dynamical Diversity in Reconfigurable Nanomechanics (>28Ã—) | [B] |
| arXiv:2605.13354 | Reservoir Computing with a single Josephson junction | [B] |
| arXiv:2604.09943 | Vestibular reservoir computing | [B] |

### 12.5 NeuromÃ³rfico e energia

| ID | TÃ­tulo | Selo |
|---|---|---|
| arXiv:2608.18341 | Acoustic Anomaly Detection on Loihi 2 (496Ã— CPU, 132Ã— GPU) | [B] |
| arXiv:2511.01553 | Online Continual Learning on Loihi 2 (0,33 ms, 0,05 mJ) | [B] |
| arXiv:2602.02439 | NeuEdge (847 GOp/s/W, 312Ã— vs GPU) | [B] |
| arXiv:2610.25424 | C. elegans on Loihi 2 (514â€“1211Ã— latency) | [B] |
| arXiv:2605.00146 | SNN Object Detection on Loihi 2 | [B] |
| DATE 2022 | HyperSpike: HDC + SNN (31,5Ã— robustez, 2,6Ã— energia) | [A] |

---

## Fecho do Volume II

O Volume I dizia: *"nÃ£o Ã© inventar fÃ­sica nova; Ã© fechar o laÃ§o e medir."*
O Volume II mostra que **as seis objeÃ§Ãµes eram corretas â€” e todas tÃªm preÃ§o tabelado**:

- Ising falha em restriÃ§Ã£o apertada â†’ **5.800Ã—** de reduÃ§Ã£o antes de chegar nele.
- VSA satura â†’ capacidade Ã© **quadrÃ¡tica em D**, e o inimigo real Ã© **ciclo-limite**.
- Geometria nÃ£o Ã© verdade â†’ certifica-se **coerÃªncia**, com proveniÃªncia, em 14 ms.
- Sem corpo nÃ£o hÃ¡ agÃªncia â†’ **laÃ§o fechado basta**; S0 jÃ¡ falsifica a hipÃ³tese.
- Treino caro â†’ **congele a fÃ­sica**; o readout Ã© regressÃ£o linear.
- Preprint nÃ£o Ã© resultado â†’ **triangulaÃ§Ã£o com selos** e regra de decisÃ£o.

E um achado que nÃ£o estava em nenhuma das seis objeÃ§Ãµes â€” e que o **E6 resolveu
depois deste volume ter sido escrito**: **o motor Ã© matvec-bound.** O solver consome
**84,4%** do quadro e entrega **7,5 GFLOP/s** (~19% do pico AVX2 f64). A estimativa
"0,07%" que aparecia no Â§1.1 estava errada e estÃ¡ corrigida lÃ¡.

Isso reordena a fila: antes de qualquer fÃ­sica nova, **otimizar o kernel do solver**
(E6b) Ã© o maior ganho disponÃ­vel â€” 5Ã— de headroom, sem tocar na arquitetura. E,
depois dele, o prÃ³ximo experimento honesto continua sendo medir o que ainda nÃ£o se
mediu: `Î”E` prevÃª acerto? (E3b).

> *"Seis corpos, seis preÃ§os. O sÃ©timo era nÃ£o saber onde o tempo ia â€” agora sabemos:
> 84,4% vai para o solver."*

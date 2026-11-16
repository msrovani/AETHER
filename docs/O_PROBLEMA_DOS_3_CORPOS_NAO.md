# O PROBLEMA DOS 3 CORPOS — NÃO · Volume I
## Os três "NÃO" e a tese

> **Dossiê de pesquisa · Volume I** · AETHER · 2026-11-15
> Escopo: os três "NÃO" do motor pós-token (linguagem natural, raciocínio lógico +
> conhecimento geral, aprendizado/evolução/autoconhecimento) e o caminho para AGI
> dentro das restrições físicas do projeto.
>
> **Status:** documento de hipóteses. Não é contrato de API e não altera
> `docs/API_CONTRACT.md`. Nada aqui foi implementado ainda; as propostas E1–E5 são
> experimentos falsificáveis, não compromissos.
>
> **Baseline citado:** crate em v0.0.67+ · auditoria de 2026-11-15 ·
> `ACQ 100 · 12/12 MEASURED · 0 B hot-path` · Intel Core 7 240H.

---

## Série

| Volume | Arquivo | Papel | Pergunta |
|---|---|---|---|
| **I** | este arquivo | **Problema e tese** | Quais são os três "NÃO" e como fechá-los? |
| **II** | [`O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`](O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md) | **Hardware e riscos** | Cada risco tem preço? Em que substrato? |
| **III** | [`O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md`](O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md) | **Conhecimento e integração** | Como se guarda, revisa e audita o que foi aprendido? |

Convenções da série:

- Selos de evidência **[A]/[B]/[C]/[M]** definidos em **Vol. II §0**, valem também para este volume.
- IDs de experimento (`E*`), gaps (`NO_*`) e riscos (`R*`) são **consolidados no
  registro mestre do Vol. III §11** — em caso de divergência, o mestre vence.
- Este volume responde *"o quê e por quê"*; o Vol. II responde *"a que preço"*;
  o Vol. III responde *"como não mentir sobre isso"*.

---

## Sumário

- [0. Por que "3 corpos"](#0-por-que-3-corpos)
- [1. O que o AETHER é (e o que ele recusa ser)](#1-o-que-o-aether-é-e-o-que-ele-recusa-ser)
- [2. Primeiro corpo — Linguagem natural](#2-primeiro-corpo--linguagem-natural)
- [3. Segundo corpo — Raciocínio lógico e conhecimento geral](#3-segundo-corpo--raciocínio-lógico-e-conhecimento-geral)
- [4. Terceiro corpo — Aprendizado, evolução e autoconhecimento](#4-terceiro-corpo--aprendizado-evolução-e-autoconhecimento)
- [5. A tese unificada](#5-a-tese-unificada)
- [6. Cálculos](#6-cálculos)
- [7. Gaps](#7-gaps)
- [8. Roadmap experimental E1–E5](#8-roadmap-experimental-e1e5)
- [9. Riscos e o que falsificaria a tese](#9-riscos-e-o-que-falsificaria-a-tese)
- [10. Referências](#10-referências)
- [11. Glossário de conceitos](#11-glossário-de-conceitos)
- [Anexo A — Mapa arquivo → módulo → "NÃO"](#anexo-a--mapa-arquivo--módulo--não)
- [Anexo B — Como reproduzir](#anexo-b--como-reproduzir)

---

## 0. Por que "3 corpos"

O AETHER nasceu de uma recusa: **zero tokens**. Essa recusa compra latência
previsível, custo fixo, privacidade e afinidade com sinais brutos — e cobra três
capacidades que o resto da indústria assumiu como pré-requisitos de inteligência:

| # | O "NÃO" | O que falta |
|---|---|---|
| 1 | **Linguagem natural** | não gera nem interpreta texto; não há vocabulário |
| 2 | **Raciocínio lógico e conhecimento geral** | não tem fatos, nem dedução simbólica, nem "bom senso" |
| 3 | **Aprendizado, evolução e autoconhecimento** | não tem metas próprias, nem modelo de si |

A tese deste documento é que **esses três "NÃO" não são três problemas
independentes**. São três projeções do mesmo buraco: falta uma *álgebra
composicional* que ligue campo contínuo → símbolo → decisão → self. E o AETHER
já tem as peças soltas, em módulos separados, sem o laço que as fecha.

O nome é uma homenagem invertida: no problema dos três corpos, três massas se
influenciam mutuamente e o sistema é caótico. Aqui, os três "corpos" são
**projeções de um único operador** — e a aposta é que, se forem a mesma
operação, o sistema é integrável, não caótico.

---

## 1. O que o AETHER é (e o que ele recusa ser)

Invariantes vigentes (`ARCHITECTURE.md` §1, `ARCHITECTURE_PHASE2.md` §1):

| Invariante | Consequência |
|---|---|
| ZERO tokenização | entrada é `f32` contínuo; `String` só em erro/rótulo |
| ZERO transformers/atenção discreta | sequência é interferência espectral (M2) e fase (M3/M5) |
| ZERO protocolos semânticos | rede = datagrama binário de 40 bytes (Kuramoto) |
| ZERO backprop | aprendizado local hebbiano + energia livre variacional |
| ZERO alocação no hot path | 0 B medido; buffers pré-alocados |
| ZERO `unsafe` fora do codec | `#![forbid(unsafe_code)]` |

### 1.1 O que já existe (e é relevante aqui)

| Módulo | Arquivo | Papel cognitivo |
|---|---|---|
| M1 Ising/BSB | `src/core/thermodynamic_solver.rs` | decisão por minimização de energia |
| M2 Fourier | `src/core/wave_interference.rs` | mistura de sequência sem atenção |
| M3 Kuramoto | `src/core/kuramoto_field.rs` | sincronização P2P (40 B) |
| M4 organismo | `src/simulation/` | loop fechado 60 Hz |
| M5 memória de fase | `src/phase_memory.rs` | atratores, `cleanup`, abstenção |
| M6 sonhador | `src/thermodynamic_dreamer.rs` | contrafactuais (GSB + Langevin) |
| M7 morfogênese | `src/morphogenesis.rs` | plasticidade topológica |
| M8 daemon | `src/bin/aether_daemon.rs` | transdutor real + enxame |
| FHRR | `src/core/fhrr.rs` | **papel–filler**: `add_fact(entity, relation, value)` |
| BAM | `src/core/fhrr_bam.rs` | memória holográfica binária |
| Koopman/SINDy | `src/core/koopman_sindy.rs` | previsão e descoberta de leis |
| Active Inference | `src/core/active_inference.rs` | `G = custo − λU`, CBF |
| Renormalização | `src/core/renormalization.rs` | hierarquia 64→16→4, torque top-down |
| Swarm speciation | `src/core/swarm_speciation.rs` | papéis ecológicos |
| Transdutor | `src/transducer_io.rs` | borda física única, `u(t) ∈ [-1,1]^64` |

**A observação central:** `FhrrStore::add_fact` já é binding de papel e filler;
`ComplexPhaseMemory::cleanup` já é leitura com abstenção; `MorphogeneticField::evolve_topology`
já é auto-modificação; `ActiveInferenceEngine` já é a função de valor de um agente.
O que não existe é o **laço** que usa tudo isso para *nomear*, *provar* e *se modelar*.

---

## 2. Primeiro corpo — Linguagem natural

### 2.1 Por que é um "NÃO"

Sem vocabulário discreto não há geração de texto, *instruction following*,
sumarização nem Q&A. Se o usuário quer "me explique o que está acontecendo",
alguém tem que traduzir: o AETHER produz estado, `F(t)` e fases; a narrativa é
uma camada externa — e, se essa camada for um LLM, ela **reintroduz tokens na
borda**.

Consequência de produto: o AETHER não substitui um chatbot. Substitui um
detector/monitor/piloto — ou vira o cérebro por trás de uma interface textual.

### 2.2 O insight que muda o enquadramento

**Token não é a unidade da linguagem; é artefato de compressão.** A unidade real
é *binding*: quem fez o quê, para quem, quando. BPE existe porque transformers
precisam de índice discreto. Um campo de fases não precisa.

Isso é literalmente a leitura de *Attention as Binding* (arXiv:2512.14709):
atenção ≈ VSA "soft" — Q/K = papéis, V = fillers, residual = superposição. A
fragilidade lógica dos LLMs é **falha de álgebra**, não de escala. Se for assim,
um sistema que faça a álgebra *exata* (fase) deveria ser mais robusto, não menos.

### 2.3 Evidência

| Trabalho | Achado que importa |
|---|---|
| **Attention as Binding** · arXiv:2512.14709 | atenção = binding/unbinding aproximado; propõe *heads* explícitas de binding e camadas de memória hiperdimensional |
| **Rel-SAR** · arXiv:2501.11896 | raciocínio abdutivo sistemático em VSA com átomos numéricos/periódicos/lógicos; generaliza OOD (Raven) |
| **PathHD** · arXiv:2512.09369 | KG reasoning **encoder-free** com hipervetores GHRR: −40–60% latência, 3–5× menos memória |
| **RESOLVE** · arXiv:2411.08290 | features de objeto + relacionais coexistem por bundling/binding; atenção bipolar rápida |
| **LARS-VSA** · arXiv:2405.14436 | *relational bottleneck* em espaço hiperdimensional; robusto à maldição da composicionalidade |
| **Oscillator LMs** · SSRN 7431978 (2026) ⚠️ | LM com fase Kuramoto: 99,7% recall em lag 20; capacidade escala com nº de osciladores, não com parâmetros |
| **PRISM** · arXiv:2512.01208 | restrição iso-energética (só fase, ganho 1.0): COMET 0.799 vs 0.821 do Transformer, com 11,5% menos parâmetros |
| **Attention by Synchronization** · arXiv:2606.12059 | atenção = equilíbrio de Kuramoto–Lohe; ponto fixo único e atrativo; gap de PPL fecha com `d_osc` |
| **WONN** · arXiv:2605.20922 | Winfree ONN escala para ImageNet-1K e **Sudoku/Maze**; 80,1% no Maze-hard com 1% dos parâmetros do SOTA |
| **CRAM** · arXiv:2611.17309 | energia clássica pairwise aprende restrições 1000× mais rápido que looped; aviso: Sudoku/Maze **não** medem scaling de reasoning |
| **CAWN** · arXiv:2604.04250 | ondas acústicas contínuas, estado O(1), 2M tokens de contexto |
| **Emruli/Gayler/Sandin** (DiVA) | VSA + *sparse distributed memory* para mapeamento analógico em Raven |

**Leitura:** entre 2025 e 2026 a literatura está convergindo exatamente para o
substrato do AETHER — fase, osciladores, VSA, restrição energética. O AETHER não
está fora da moda; está **adiantado na restrição** e atrasado na *interface*.

### 2.4 Proposta — Phase-Language Head (PLH)

Regra de ouro: **vocabulário só na borda, nunca no núcleo.**

```
texto ──► grafema/fonema ──► forma de onda ──► TransducerIo (u(t))
                                                    │
                                            ┌───────▼────────┐
                                            │  NÚCLEO AETHER │  (fase, sem string)
                                            │  binding + RG  │
                                            └───────┬────────┘
                                                    │ padrão de locking
        texto ◄── vocoder ◄── banco de readout ◄────┘
```

- **Entrada:** texto → onda → `TransducerIo`. O core vê fase.
- **Núcleo:** gramática = **offsets de fase** (papel × filler); ordem =
  **permutação** (rotação fixa); concordância = simetria de acoplamento.
- **Hierarquia:** `renormalization` 64→16→4 vira a árvore sintática:
  micro = traços, meso = morfema/palavra, macro = sintagma. O *torque top-down*
  que já existe é literalmente **predição sintática descendente**.
- **Saída:** banco de osciladores de readout; o padrão de *locking* é decodificado
  por vocabulário finito/vocoder externo — um **DAC semântico**.

O núcleo continua sem token; o texto nasce no conversor. É o mesmo desenho dos
OLMs (readout grande) e do PRISM (fase pura), mas com o *binding* explícito do
`FhrrStore`.

**O que seria revolucionário:** linguagem deixa de ser *next-token prediction* e
passa a ser **sincronização de um campo semântico a um template sintático**, com
abstenção calibrada por `RecallConfidence::should_abstain`. Um LLM não sabe que
não sabe; o M5 tem isso desde o início.

**Experimento E1:** `examples/phase_language_probe.rs` — gramática SVO mínima,
medir recuperação de papel–filler sob ruído em função de `D` e do número de fatos
superpostos. Gate: SNR > 5σ com 0 alloc (espelha T12: 13,5 µs, SNR 5,88σ).

**Risco:** interferência em superposição; erro de `cleanup` compõe em múltiplos
passos. Sintaxe resolve com fase; **semântica e pragmática não**.

---

## 3. Segundo corpo — Raciocínio lógico e conhecimento geral

### 3.1 Por que é um "NÃO"

Não há "Paris é a capital da França". Há previsão de trajetória (Koopman, R² 1,0),
descoberta de leis esparsas (SINDy, erro 0,0004) e associação por fase. Raciocínio
simbólico, aritmética geral, conhecimento enciclopédico e bom senso **não vêm de
graça**. T09 descobre a lei de um sistema observado — não faz matemática abstrata.

### 3.2 O insight

**Lógica é minimização de energia.** SAT/MAX-SAT *é* Ising. O Módulo 1 (BSB,
Euler simplético, paredes inelásticas, elitismo) é um solver de Ising que hoje só
decide spins — pode ser **o raciocinador**.

Os custos do landscape **não precisam ser só hand-coded**: o experimento **E2d**
mostra aquisição CRAM-style (`K` pairwise por pseudolikelihood, sem BPTT) que
alimenta o mesmo `BsbCore`/`Certificate` — ver `docs/CRAM_LOGOS.md` e
arXiv:2611.17309. A decisão continua Ising; o atestado continua por
`violations == 0`.

E conhecimento geral não precisa ser armazenado como fatos: pode ser importado
como **geometria**.

### 3.3 Evidência

| Trabalho | Achado |
|---|---|
| **CRAM** · arXiv:2611.17309 | aprende energia pairwise de soluções; treina em ms; early-stop por limiar — base do E2d (aquisição → Logos) |
| **Ising/QUBO p/ implicantes** · arXiv:2605.07017 | SAT→QUBO com semântica de "don't care" (duas polaridades por variável): *ground state* = implicante curto, não atribuição total |
| **Limits of Ising Machines for SAT** · arXiv:2511.21046 | ⚠️ restrições apertadas viram só penalidades e **distorcem o landscape**; decomposição ingênua propaga erro. Precisa pré-processamento simbólico + partição *structure-aware*. Muda solver clássico <3×, Ising >150× |
| **p-bit guided CDCL** · arXiv:2605.04033 | amostras de baixa violação viram *assumptions* para CDCL: −80,8% conflitos, correção preservada |
| **LagONN** · arXiv:2505.07179 | **o mais alinhado:** osciladores de Lagrange escapam de mínimos inviáveis; Max-3-SAT até 200 var/860 cláusulas; alternativa **determinística** ao simulated annealing |
| **Tsat** · arXiv:2605.20328 | busca local guiada por estatística das restrições: ~3× o tamanho tratável no regime mais difícil |
| **Anechoic** · arXiv:2610.13368 | correção de Onsager no SCA: Max-Cut K2000 em 0,05 ms em FPGA |
| **Cavity + GNN** · arXiv:2609.07456 | GNN só compete com heurísticas clássicas após incorporar min-sum/cavity; SA continua robusto |
| **Ising machines → GenAI** · arXiv:2610.03972 | *survey*: co-design algoritmo–hardware é o gargalo real |
| **Model Transplantation** · Zenodo 19476846 (2026) ⚠️ | embeddings densos → hipervetores binários de 10.240 bits preservando similaridade; `king ⊕ man ⊕ woman ≈ queen` Top-1 78%, Top-5 96%; relação sinônima 0,650 vs 0,250 aleatório |
| **Hyperdimensional Probe** · arXiv:2509.25045 | VSA decodifica o *residual stream* de LLMs — prova de que a álgebra é compatível com o que LLMs já representam |

### 3.4 Proposta — duas memórias + um verificador

```
consulta
   │
   ├─► FhrrStore (semântica: triplas + átomos transplantados)   [propõe]
   ├─► renormalization (abstração 64→16→4)                       [contextualiza]
   ├─► solver BSB/Ising + osciladores de Lagrange                [decide]
   └─► cleanup + energy gap → resposta COM CERTIFICADO           [verifica]
```

**Conhecimento geral via transplantação.** Você não armazena a Wikipedia em fase;
você importa o **espaço semântico** de um modelo pré-treinado para hipervetores e
raciocina sobre ele com a álgebra que já existe. O LLM vira **fonte de átomos**,
não runtime. Isso é a ponte mais promissora do dossiê — e a mais arriscada
(ver §9).

**A contribuição revolucionária: certificado de factibilidade.** Toda resposta é
um estado com atestado mensurável:

```
(resposta, E, ΔE, violações, proveniência)
    ↓
ΔE grande + violações = 0  ⇒  resposta factível e inequívoca ⇒ usar
ΔE pequeno ou violações > 0 ⇒  ambíguo ⇒ abster
```

> **Correção pós-E3b (Vol. II §4.6):** o atestado **não** mede acerto. Em
> otimização não restrita, todo mínimo local é "coerente" e `ΔE` **não prediz
> sucesso** (AUC 0,46–0,55). Em problemas com restrições contáveis, `violações = 0`
> é **prova** (precisão 100% em MAX-2-SAT). O nome correto é **certificado de
> factibilidade** — nunca "de energia", nunca "de verdade".

Nenhum LLM tem isso: não existe "prova" de que a resposta satisfaz um contrato
explícito. Aqui existe — auditável, determinístico, 0 alloc. Casa com o T10
(abstenção sem *warrant*) e com `RecallConfidence`.

**Experimento E2:** `examples/ising_logic_probe.rs` — 3-SAT aleatório `n ≤ 20`
contra força bruta; medir acerto, gap e calibração gap↔acerto (ECE). Repetir com
osciladores de Lagrange nas instâncias de restrição apertada, seguindo o aviso de
arXiv:2511.21046.

**Experimento E3:** transplantar 1k embeddings → `FhrrStore`; medir analogia
(Top-1 > 0,7) e recuperação de relação (> 0,6), depois reconstruir triplas.

**Risco:** ver §9.1 — Ising não é oráculo.

---

## 4. Terceiro corpo — Aprendizado, evolução e autoconhecimento

### 4.1 Por que é um "NÃO"

O motor aprende *dentro* de um regime (T03: reaprende pós-ruptura em 150 ciclos),
mas não tem **metas próprias**, não tem **modelo de si**, e não decide *o que*
aprender. Evolução existe como morfogênese reativa, não como desenvolvimento.

### 4.2 O insight

**Autoconhecimento não é introspecção; é um modelo do modelo.** E o AETHER tem
vantagem injusta: o estado é 64-dimensional, físico e barato. Um LLM não consegue
se modelar porque o estado é gigante e opaco; um campo de fase de 64 canais pode
prever **o próprio próximo quadro** com o mesmo mecanismo que prevê o mundo.

### 4.3 Evidência — desconfortável para os LLMs

| Trabalho | Achado |
|---|---|
| **AwarenessBench** · arXiv:2609.35409 | metacognição/autoconsciência é **capacidade distinta**: progresso em linguagem/raciocínio não transfere. Todos os 18 modelos ficam abaixo de humanos em metacognição e self-awareness |
| **MIRROR** · arXiv:2604.19809 | *compositional self-prediction falha universalmente* (erro 0,43–0,94); dar os próprios scores de calibração **não ajuda** (p>0,05); só **restrição arquitetural** funciona (Confident Failure 0,600 → 0,143) |
| **Agentic Self-Awareness** · arXiv:2610.19226 | separa *know/act/hold*: reconhecer blocker 84,4%, self-awareness estrita 40,6%; scaffolding externo → 93,8% |
| **MIRROR / KAPRO** · arXiv:2606.20661 | descolamento entre "saber" e "agir"; *tool overuse* por pattern matching raso |
| **SARSI** · arXiv:2607.12254 | agentes com self-model persistente e legível por máquina para guiar auto-melhoria recursiva |
| **The Tao of Agency** · arXiv:2606.19924 | agente autotélico = `(π, G, μ)` com metas **endógenas**; o problema profundo não é gerar metas, é **gerar e relativizar o self** |
| **Autotelic RL** · arXiv:2502.04418 | tipologia de motivação intrínseca (conhecimento vs competência) e métricas de *open-endedness* |
| **ULEE** · arXiv:2601.19810 | currículo por **dificuldade pós-adaptativa** (progresso), não por surpresa bruta |
| **Artificial Experimentalist** · arXiv:2608.26116 | RL autotélico descobre e **pilota** fenômenos auto-organizantes (Lenia); generalização zero-shot OOD |
| **SEAA** · arXiv:2609.17331 | HMM de inércia comportamental + reflexão que **atualiza os parâmetros**, não o texto; fronteira self–outro por contraste social |
| **LLM Self-Modeling** · arXiv:2608.30980 | self-modeling melhora com RL, mas **não** constitui introspecção privilegiada |

**Leitura:** a literatura está dizendo que o self não emerge de mais raciocínio —
emerge de **arquitetura**. É a melhor notícia possível para um motor cujo estado é
explicíto e pequeno.

### 4.4 Proposta — Self-Field + currículo endógeno

1. **Self-field (16 canais).** Prever `(ação própria, surpresa própria F(t),
   coerência R(t), taxa de plasticidade)`. Erro de auto-predição = **sinal
   metacognitivo**, causalmente distinto do erro de mundo. Ligar em
   `ComplexPhaseMemory::should_abstain`.
2. **Currículo por progresso, não por novidade.** O `ThermodynamicDreamer` já gera
   contrafactuais; usar `d(erro)/dt` como recompensa intrínseca. Surpresa pura gera
   caos; **progresso** gera desenvolvimento (achado de ULEE/Tao of Agency).
3. **Morfogênese como desenvolvimento.** `evolve_topology(prediction_error, …)` já é
   auto-modificação. O experimento novo: **o sistema consegue prever o próprio
   pós-plasticidade?** Se sim, existe uma métrica de *self-consistência estrutural*
   que não existe na literatura de LLMs.
4. **O self como ponto fixo da renormalização.** A hierarquia 64→16→4 já separa
   escalas temporais. O "self" é a **variável macro lenta** (identidade, metas
   estáveis); o "corpo" é o micro rápido. Torque top-down = self agindo no corpo;
   EMA τ=100 ms = constante de tempo da identidade. Isso responde ao Tao of Agency:
   o self é uma **partição**, não uma substância.

**Experimento E4 (ablação):** três configurações — (a) baseline, (b) + self-field,
(c) + self-field + currículo por progresso — medindo ECE de abstenção, velocidade
de readaptação pós-ruptura (T03) e drift (T11). Predição falsificável: (b)/(c)
melhoram calibração **sem** melhorar T01. Se melhorarem T01 também, a hipótese
"metacognição é separada" cai — e isso já é resultado.

**Experimento E5:** currículo por progresso no dreamer; medir tempo de readaptação
pós-ruptura contra o baseline T03 (150 ciclos).

---

## 5. A tese unificada

| Escala | Mecanismo existente | O que falta plugar | "NÃO" |
|---|---|---|---|
| **Mundo** | `WaveInterference` + Koopman/SINDy + `SurpriseTracker` | — | (base) |
| **Linguagem** | `FhrrStore` (papel–filler) + RG como árvore | PLH na borda (onda↔texto) | 1 |
| **Logos** | solver BSB/Ising + `cleanup` | presolve/roof dual + restrições + certificado de factibilidade | 2 |
| **Self** | `ActiveInference` (G=custo−λU) + `MorphogeneticField` + EMA | self-field 16ch + currículo por progresso | 3 |

### 5.1 A aposta forte

Nos LLMs, linguagem, raciocínio e self são **três módulos aprendidos separados** —
e é por isso que self-awareness não transfere do raciocínio (AwarenessBench). No
AETHER seriam **a mesma operação** (bind → superpor → minimizar energia → atualizar
topologia) aplicada em três escalas.

> **Se isso for verdade, o self sai de graça do mesmo operador que faz sintaxe e lógica.**

É a hipótese que vale o projeto. Se for falsa, o resultado também é publicável: o
self precisa de um operador dedicado.

### 5.2 Diferenciais que um LLM não copia

1. Resposta com **certificado de factibilidade** (`E`, `ΔE`, violações, proveniência)
   — verificação de consistência, **não** de verdade (Vol. II §4.6).
2. **Determinismo bit-a-bit** com 0 alloc (auditável, reproduzível).
3. Self-model de **16 dimensões**, não um vetor de ativações opaco.
4. **Zero token no núcleo** → sem *prompt injection*, sem PII textual na borda.

---

## 6. Cálculos

### 6.1 Throughput medido (baseline de referência)

`docs/COGNITIVE_AUDIT_FINAL.md` §3 · `cargo bench --bench throughput_tps`:

| Regime | p50 µs | p99 µs | cycles/s | SOP/s | eq-t/s | allocs |
|---|---:|---:|---:|---:|---:|---:|
| Basal | 146,5 | 311,7 | 6,286 k | 3,16e9 | **804,7 k** | 0 |
| Vigilante | 268,0 | 419,3 | 3,615 k | 3,60e9 | 462,7 k | 0 |
| Crítico | 508,7 | 791,2 | 1,878 k | 3,71e9 | 240,4 k | 0 |

**Checagem de consistência:** `1 / 146,5 µs = 6,826 k cycles/s` (p50) vs. 6,286 k
reportado (média, que inclui a cauda). A diferença de ~8% é exatamente o efeito da
cauda — coerente com p99 ≈ 2,1 × p50.

**Conversão `eq-t/s`:** `1 ciclo ≡ 128 tokens equivalentes` (64 canais + 256
fasores). Logo `eq-t/s = cycles/s × 128`. É métrica **comparativa**, não tokens
reais (`docs/DASHBOARD_METRICS.md`).

### 6.2 Working set — o que realmente é tocado por ciclo

| Estrutura | Conta | Tamanho |
|---|---|---:|
| `J` Ising (N=64, f32) | 64 × 64 × 4 B | **16 KB** |
| Vetor FHRR (D=160, complexo f32) | 160 × 2 × 4 B | **1,28 KB** |
| `BAM_DIM=16` + traços | `BamHolographicStore<16>` | dezenas de KB |
| Heap pico medido | — | **0,48 MB** |
| **RSS medido** | — | **14,34 MB** |

**RSS ≠ working set.** O RSS inclui texto do binário, DLLs do SO, stacks de
threads, arenas do alocador e *slack* de paginação — nada disso é tocado por ciclo.
O estado cognitivo é ~0,5 MB.

### 6.3 Mapa de capacidade de cache

CPU de referência: **Intel Core 7 240H** (Raptor Lake-H, 6 P + 4 E, 16 threads,
2,5–5,2 GHz).

| Nível | Latência típica (ciclos) | Tamanho |
|---|---:|---|
| L1d | ~5 | 48 KB / P-core |
| L2 | ~14–15 | **1,25 MB** / P-core (2 MB por cluster E) |
| L3 | ~45–50 | **24 MB** compartilhados |
| DRAM | ~200–300 (~80–100 ns) | — |

Custo de `J` por N:

| N | `J` (f32) | Onde vive |
|---:|---:|---|
| 64 | 16 KB | **L1d** |
| 128 | 64 KB | L2 (estoura L1d) |
| 256 | 256 KB | L2 |
| **512** | **1 MB** | **L2 no limite** + workspace → L3 |
| 1024 | 4 MB | L3 |
| 2048 | 16 MB | L3 com workspace → DRAM |

> **Penhasco em N ≈ 512.** Até lá, crescer N é quase grátis em memória; depois,
> paga-se latência de L3/DRAM e o p50 degrada de forma não linear. É o número que
> deveria estar em `docs/HW_TARGET.md` — não o RSS.

**Sobre "14 MB roda na L3":** em capacidade, sim (14,34 < 24 MB). Mas L3 é cache,
não memória endereçável: "caber" só significa ausência de falta de capacidade. E é
uma pergunta pior do que a relevante — o engine **já é L2-residente**, que custa
~15 ciclos contra ~50 da L3. O ganho de "migrar para L3" seria negativo.

### 6.4 Custo por ciclo em ciclos de CPU

| Frequência | 146,5 µs equivale a |
|---|---:|
| 2,5 GHz (base P-core) | 366 mil ciclos |
| 5,2 GHz (turbo) | 762 mil ciclos |

Confirma que o loop é **compute-bound**, não memory-bound. Daí:
- "0 B hot-path" é também um **invariante de localidade** (sem `alloc` → sem tocar
  páginas novas → sem page fault, sem TLB miss, sem poluição de cache).
- A cauda (p99 = 2,1 × p50; p99.9 > 1 ms) é assinatura de **SO**: preempção,
  migração P→E, SMT, frequência. `--features affinity` corta **−95% no p99.9**.

### 6.5 TLB

14 MB com páginas de 4 KB = **3.584 páginas**; o TLB L2 do Golden Cove cobre ~1,5–2 k
entradas. Se o processo tocasse 14 MB por ciclo, haveria *page walk*. Não toca —
mas, se o working set crescer (N grande, ou *swarm* com vários buffers), **huge
pages de 2 MB** passam a valer mais que qualquer otimização de cache: ~7 páginas
em vez de 3.584.

---

## 7. Gaps

### 7.1 Gaps já catalogados (`docs/USER_DELIVERY_PLAN.md` §2.4)

| # | Gap | Impacto |
|---|---|---|
| 1 | `--tui` / `--help` no-op em `aether-organism` | UX zero para não-agente |
| 2 | README cita 0.0.43 / 95 testes; crate em 0.0.67+ | credibilidade |
| 3 | telemetria do daemon é para engenheiro | adoção |
| 4 | Windows depende de MinGW no PATH | instalação |
| 5 | multicast LAN **sem auth** | produto aberto |
| 6 | Produtos A (CAN) e B (CSI) só no contrato | cobertura de domínio |

### 7.2 Gaps novos, propostos por este dossiê

| Código | Severidade | Descrição | Remediação |
|---|---|---|---|
| `NO_BINDING_HEAD` | crítico (pesquisa) | `FhrrStore` existe mas não está no loop do organismo; nenhum caminho usa papel–filler para linguagem | E1 |
| `NO_ENERGY_CERT` | crítico | decisões não emitem `(E, ΔE)`; não há verificação de ground state | E2 |
| `NO_ISING_CONSTRAINTS` | alto | solver não tem codificação SAT/QUBO nem osciladores de Lagrange; restrições apertadas vão falhar (arXiv:2511.21046) | E2 |
| `NO_SELF_FIELD` | alto | não há canais que prevejam a própria dinâmica; abstenção usa só confiança de recall | E4 |
| `NO_PROGRESS_CURRICULUM` | médio | dreamer amostra por surpresa, não por progresso de aprendizado | E5 |
| `CACHE_METRIC_ABSENT` | médio | dashboard reporta RSS/heap, não `LLC-misses/ciclo` nem bytes tocados por ciclo | sonda de cache |
| `CAPACITY_CLIFF_UNDOC` | médio | penhasco N≈512 não está documentado | `docs/HW_TARGET.md` |
| `TRANSPLANT_UNVALIDATED` | alto (pesquisa) | nenhuma ponte de embeddings → hipervetores | E3 |
| `PREPRINT_RISK` | info | hipóteses centrais vêm de preprints (Zenodo/SSRN) não revisados | ver §10 |

---

## 8. Roadmap experimental E1–E5

Todos com: `--release`, seed fixa (`ChaCha8Rng`), `alloc_invariants` verde,
`ACQ ≥ 99` como **não-regressão**, e nenhum `String` no caminho de sinal.

| # | Entrega | Arquivo sugerido | Gate |
|---|---|---|---|
| **E1** | probe de linguagem em fase | `examples/phase_language_probe.rs` | recuperação papel–filler com SNR > 5σ, 0 alloc |
| **E2** ✅ | lógica como Ising + certificado | `examples/ising_logic_probe.rs` | **169/169** satisfatíveis `n = 20` (100%) · decisivas **100%** · **0 alloc** · G3b **FAIL declarado** (6× o teto; envelope `n ≥ 8` fora) |
| **E2d** ✅ | aquisição CRAM→Logos (custos pairwise) | `examples/cram_logos_probe.rs` · `docs/CRAM_LOGOS.md` | Shidoku 4×4: hand-set **100%** · PL **100%** · fit ~**3,7 ms** · G1–G3 PASS · 52 params |
| **E3** ✅ | transplantação de embeddings | `examples/transplant_probe.rs` | **ρ = 0,9528** · relação **0,9990** · analogia R@1 **0,9549** · proveniência **100%** |
| **Doação** ✅ | ETL + HvBank 10.240 bits (INTAKE) | `docs/DOACAO_PLAYBOOK.md` · `harvest_transplant.py` · `transplant_hv_probe` | Spearman **0,9539** · Recall@10 **1,0** · 8 gates PASS · clonável por rodada |
| **E4** | self-field + ablação | `tests/self_field_audit.rs` | ECE melhora; T01 **não** regride |
| **E5** | currículo por progresso | `tests/curriculum_audit.rs` | readaptação pós-ruptura < T03 (150 ciclos) |

Ordem sugerida: **E1 → E2 → E3 → E4 → E5**. E1 é o de menor risco e maior valor
demonstrativo; E2 é o de maior valor científico (certificado de factibilidade); E3 é o
de maior risco e maior potencial de ruptura.

> **Plano revisado (pós-E3b, 2026-11-15):** a ordem vigente é a do **Vol. III §12.4**:
> E6 ✅ → E6b ✅ → **E3b ✅** → **E2b ✅** → E2c ✅ → E1b ✅ → E1 ✅ →
> **E2 ✅** (169/169, 0 alloc) → **E3 ✅** (ρ=0,9528, OOD 100%) → E4/E5/E7/E8 → F2/F3.

### 8.1 Critério de "revolução"

Para não virar apenas mais um protótipo, o programa precisa entregar **pelo menos
um** destes:

1. Generalização sistemática (composicional) em tarefa relacional sem treino por
   gradiente — comparável a Rel-SAR/RPM.
2. Resposta lógica com **certificado de factibilidade verificável** e abstenção
   calibrada — algo que nenhum LLM entrega.
3. **Self-consistência estrutural**: prever o próprio pós-plasticidade com erro
   mensurável e menor que um baseline sem self-field.

---

## 9. Riscos e o que falsificaria a tese

### 9.1 Riscos técnicos

| # | Risco | Evidência | Mitigação |
|---|---|---|---|
| 1 | Ising não é oráculo: restrição apertada destrói o landscape | arXiv:2511.21046 | passe simbólico + partição *structure-aware*; Lagrange |
| 2 | Interferência em VSA; erro de `cleanup` compõe | capacidade ~D | crescer D; verificação por energia a cada passo |
| 3 | Transplantação dá **geometria, não verdade** | Zenodo 19476846 | verificador de energia; abstenção |
| 4 | Open-endedness sem corpo vira passeio aleatório | arXiv:2502.04418 | usar Produto C (telemetria real) como ambiente |
| 5 | Custo de treino/desenvolvimento alto | OLM ~40× mais lento (SSRN 7431978) | inferência é o forte; não competir em treino |
| 6 | Escala: O(N²) e penhasco em N≈512 | §6.3 | planejar GPU (`wgpu`) só se N ≥ 512 |
| 7 | Capacidade de memória limitada e esquecimento | T11 drift 0,0016 rad | consolidação (L2 → L5), `prune`, cleanup |

### 9.2 O que falsificaria a tese

A tese é: *linguagem, lógica e self são a mesma operação em escalas diferentes.*
Ela cai se:

- **F1:** E1 passar (sintaxe por fase) mas E4 mostrar que o self-field precisa de
  um operador **qualitativamente diferente** de bind/energia.
- **F2:** E2 falhar em instâncias de restrição apertada mesmo com Lagrange +
  pré-processamento — ou seja, o substrato físico não paga o custo lógico.
- **F3:** E3 mostrar que a geometria transplantada **não** preserva relações
  suficientes para raciocínio (analogia > acaso, mas insuficiente multi-passo).
- **F4:** E4/E5 melhorarem métricas **só** com custo de T01/T12 acima do aceitável
  (violação de não-regressão).

Qualquer F1–F4 é um resultado legítimo e publicável. O pior desfecho não é falhar:
é não medir.

---

## 10. Referências

### 10.1 arXiv (revisão por pares ou preprint arXiv)

| ID | Título | Corpo | Uso aqui |
|---|---|---|---|
| 2512.14709 | Attention as Binding: A Vector-Symbolic Perspective on Transformer Reasoning | 1 | enquadramento linguagem = binding |
| 2501.11896 | Systematic Abductive Reasoning via Diverse Relation Representations in VSA (Rel-SAR) | 1 | raciocínio relacional, OOD |
| 2512.09369 | Encoder-Free KG Reasoning with LLMs via Hyperdimensional Path Retrieval (PathHD) | 1, 2 | KG em hipervetores |
| 2512.01208 | Language as a Wave Phenomenon (PRISM) | 1 | fase pura, COMET 0.799 |
| 2606.12059 | Attention by Synchronization in Coupled Oscillator Networks | 1 | atenção = Kuramoto–Lohe |
| 2605.20922 | Winfree Oscillatory Neural Network (WONN) | 1 | osciladores em Sudoku/Maze/ImageNet |
| 2611.17309 | Classical Energies Learn to Reason 1,000× Faster than Looped Models (CRAM) | 2 | E2d aquisição de restrições → Logos |
| 2604.04250 | CAWN: Continuous Acoustic Wave Networks | 1 | sequência contínua O(1) |
| 2411.08290 | RESOLVE: Relational Reasoning with Vector Symbolic Processing | 1 | objeto + relação |
| 2405.14436 | LARS-VSA: A VSA for Learning with Abstract Rules | 1 | *relational bottleneck* |
| 2501.05368 | Developing a Foundation of VSAs Using Category Theory | 1 | formalização |
| 2508.14245 | Cross-Layer Design of Vector-Symbolic Computing | 1 | VSA em hardware |
| 2509.25045 | Hyperdimensional Probe | 2 | VSA ↔ representação de LLM |
| 2411.08684 | Analogical Reasoning Within a Conceptual Hyperspace | 1 | analogia |
| 2602.01992 | Emergent Analogical Reasoning in Transformers | 1 | analogia como functor |
| 2605.07017 | Computing Short SAT Implicants via Ising/QUBO Encodings | 2 | SAT → QUBO |
| 2511.21046 | On the Limits of Ising Machines for Circuit-Derived SAT | 2 | ⚠️ limites de restrição |
| 2605.04033 | Probabilistic-bit Guided CDCL using Ising Consensus Assumptions | 2 | híbrido Ising+CDCL |
| 2505.07179 | Lagrange Oscillatory Neural Networks (LagONN) | 2 | Max-3-SAT determinístico |
| 2605.20328 | Targeting Clause Type Distributions (Tsat) | 2 | busca local guiada |
| 2610.13368 | Anechoic: Onsager-Corrected Parallel Annealing | 2 | correção de eco |
| 2609.07456 | GNNs and the energetic cavity method | 2 | heurísticas de ground state |
| 2610.03972 | Probabilistic Algorithms for Ising Machines from Optimization to GenAI | 2 | survey co-design |
| 2609.35409 | AwarenessBench | 3 | metacognição é separada |
| 2604.19809 | MIRROR: Metacognitive Calibration | 3 | self-prediction falha; arquitetura |
| 2610.19226 | Agentic Self-Awareness: know/act/hold | 3 | gap reconhecer→agir |
| 2606.20661 | From Knowing to Acting (KAPRO) | 3 | fronteira epistêmica |
| 2607.12254 | Self-Aware Recursively Self-Improving Agents (SARSI) | 3 | self-model persistente |
| 2606.19924 | The Tao of Agency: Autotelic AI, Embedded Agency and Dissolution of the Self | 3 | self como partição |
| 2502.04418 | Autotelic Reinforcement Learning (survey) | 3 | motivação intrínseca |
| 2601.19810 | Unsupervised Learning of Efficient Exploration (ULEE) | 3 | currículo por progresso |
| 2608.26116 | The Artificial Experimentalist | 3 | autotélico em sistemas auto-organizantes |
| 2609.17331 | Self-Emergence Agent Architecture (SEAA) | 3 | reflexão que muda parâmetros |
| 2608.30980 | Evaluating and Improving LLM Self-Modeling | 3 | self-modeling ≠ introspecção |

### 10.2 Não-arXiv ⚠️ (preprint / menor revisão — tratar como hipótese)

| Fonte | Título | Risco |
|---|---|---|
| SSRN 7431978 | Oscillator Language Models (phase-coded Kuramoto) | não é arXiv; não revisado |
| Zenodo 19476846 | Model Transplantation: pretrained → binary hypervectors | preprint; resultado único |
| Zenodo 20589293 | Wave Memory Semantic Encoding via Phase Resonance | preprint; claims fortes (100%) |
| Zenodo 19326053 | Causal Oscillator Network | preprint |
| DiVA (Emruli/Gayler/Sandin) | Analogical mapping with BSC + SDM | antigo, porém sólido |

### 10.3 Clássicos que sustentam o resto

- Plate, T. (1995) — *Holographic Reduced Representations* (base do FHRR).
- Kanerva, P. (2009) — *Hyperdimensional Computing*.
- Kleyko, D. et al. (2022) — survey de VSA/HDC.
- Frady, Kleyko, Sommer (2023) — teoria de VSA e capacidade.
- Friston, K. (2010) — *The free-energy principle* (base do `ActiveInferenceEngine`).
- Wilson, K. (1971) / Kadanoff, L. (1966) — grupo de renormalização (base do RG 64→16→4).
- Koopman, B. (1931) / Brunton, S. et al. (PNAS 2016) — operadores lineares e SINDy.
- Ames, A. et al. (2019) — *Control Barrier Functions* (base do CBF de Lyapunov).
- Lohe, M. (2009) — generalização de Kuramoto para a esfera (atenção por sincronização).
- Kuramoto, Y. (1975) — sincronização de osciladores de fase.

---

## 11. Glossário de conceitos

| Conceito | Definição operacional neste projeto |
|---|---|
| **Binding** | operação que liga papel e filler num vetor composto (no FHRR: produto Hadamard de fasores) |
| **Unbinding** | operação inversa aproximada; recupera o filler dado o papel |
| **Superposição** | soma de vetores: armazena um conjunto sem separar |
| **Permutação** | rotação fixa que codifica ordem/estrutura (posição na frase) |
| **Cleanup** | projeção do estado ruidoso no atrator mais próximo; é a "leitura" |
| **Atrator** | padrão de fase estável que representa uma memória (M5) |
| **Certificado de energia** | par `(E, ΔE)` que atesta que a resposta é um ground state e quão inequívoco ele é |
| **Gap espectral** | separação entre o ground state e o primeiro excitado; proxy de confiança |
| **Ising/BSB** | formulação de decisão como minimização de `H(s) = −½ sᵀJs − hᵀs` |
| **Ground state** | configuração de menor energia; solução ótima de um QUBO |
| **Implicante** | atribuição parcial que satisfaz uma fórmula (com "don't care") |
| **Renormalização (RG)** | coarse-graining hierárquico; aqui 64→16→4 com torque top-down |
| **Torque top-down** | influência da escala macro sobre a micro (predição descendente) |
| **Surpresa F(t)** | erro de predição normalizado; não é novidade, é falha de modelo |
| **Progresso de aprendizado** | derivada do erro; sinal de currículo (≠ surpresa) |
| **Self-field** | campo que prediz o próprio comportamento do organismo |
| **Metacognição** | capacidade de avaliar o próprio estado cognitivo (separada do raciocínio) |
| **Autotélico** | agente que gera as próprias metas |
| **Open-endedness** | crescimento contínuo de complexidade sem teto pré-definido |
| **eq-t/s** | tokens equivalentes/s = `cycles/s × 128`; métrica comparativa, não tokens reais |
| **Working set** | bytes efetivamente tocados por ciclo (≠ RSS) |
| **Penhasco de capacidade** | N a partir do qual `J` deixa de caber no nível de cache corrente |
| **Hot path** | caminho executado por ciclo; aqui, zero alocação |

---

## Anexo A — Mapa arquivo → módulo → "NÃO"

| Arquivo | Módulo | Corpo | Estado para a tese |
|---|---|---|---|
| `src/core/fhrr.rs` | FHRR | 1 | **pronto**, fora do loop |
| `src/core/fhrr_bam.rs` | BAM | 1, 3 | pronto, 2,7× FHRR |
| `src/core/renormalization.rs` | RG | 1, 3 | pronto; usar como árvore sintática e como self |
| `src/core/thermodynamic_solver.rs` | M1 | 2 | pronto; falta codificação SAT/QUBO |
| `src/core/active_inference.rs` | AI | 3 | pronto; falta self-field |
| `src/morphogenesis.rs` | M7 | 3 | pronto; falta métrica de self-consistência |
| `src/thermodynamic_dreamer.rs` | M6 | 3 | pronto; falta currículo por progresso |
| `src/phase_memory.rs` | M5 | 1, 3 | pronto; `should_abstain` já existe |
| `src/transducer_io.rs` | borda | 1 | pronto; é a porta do PLH |
| `src/core/koopman_sindy.rs` | Koopman/SINDy | 2 | pronto; base para "leis" |
| `src/core/swarm_speciation.rs` | papéis | 3 | pronto; base para papéis sociais |

---

## Anexo B — Como reproduzir

```powershell
# baseline cognitivo
.\scripts\cargo.ps1 test --release --test cognitive_audit -- --nocapture

# vazão (p50/p99/eq-t/s)
.\scripts\cargo.ps1 bench --bench throughput_tps

# determinismo e zero-alocação
.\scripts\cargo.ps1 test --release --test alloc_invariants

# hub de métricas (Windows)
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Artefatos: `target/aether_scorecard.json`, `target/aether_bench_throughput.json`,
`target/aether_host_report.json`.

Propostas deste dossiê (ainda inexistentes):

```powershell
# E1 — linguagem em fase
.\scripts\cargo.ps1 run --release --example phase_language_probe

# E2 — lógica como Ising + certificado de factibilidade
.\scripts\cargo.ps1 run --release --example ising_logic_probe

# E2d — aquisição CRAM de restrições → Logos (Certificate)
.\scripts\cargo.ps1 run --release --example cram_logos_probe --features m1

# E3 — transplantação de embeddings
.\scripts\cargo.ps1 run --release --example transplant_probe

# Doação / INTAKE_TRANSPLANT — nova rodada (ver docs/DOACAO_PLAYBOOK.md)
python tools/harvest_transplant.py --limit 200 --dry-run --out data --no-whiten
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
.\scripts\cargo.ps1 test --release --test transplant_hv_gate
```

---

## Fecho

O AETHER não tem três problemas. Tem **um** problema com três faces: falta a
álgebra que transforma campo em símbolo, símbolo em decisão verificável e decisão
em auto-modelo.

A boa notícia é que cada face já tem módulo, teste e número no lugar — `FhrrStore`,
solver de Ising, renormalização, `ActiveInferenceEngine`, `MorphogeneticField`. O
trabalho não é inventar física nova; é **fechar o laço** e medir. E medir, neste
projeto, já é uma tradição: ACQ 100, 12/12 MEASURED, 0 B hot-path.

> *"A pergunta não é se um campo de fase pode pensar. É quantos corpos o campo
> precisa para ser um."*

# O PROBLEMA DOS 3 CORPOS — NÃO · Volume III
## Gestão do conhecimento adquirido e o encaixe do `neural-sgdb`

> **Dossiê de pesquisa · Volume III** · AETHER · 2026-11-15
> Este volume é também o **hub de integração** da série: registro mestre de
> experimentos, gaps e riscos, e a resposta consolidada a *"o que ainda precisa
> ser atacado?"*.
>
> Nada aqui altera `docs/API_CONTRACT.md`. É dossiê, não contrato.

---

## Série

| Volume | Arquivo | Papel | Pergunta que responde |
|---|---|---|---|
| **I** | [`O_PROBLEMA_DOS_3_CORPOS_NAO.md`](O_PROBLEMA_DOS_3_CORPOS_NAO.md) | **Problema e tese** | Quais são os três "NÃO" e como fechá-los conceitualmente? |
| **II** | [`O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md`](O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md) | **Hardware e riscos** | Cada risco tem preço? Em que substrato? |
| **III** | este arquivo | **Conhecimento e integração** | Como o sistema *guarda*, *revisa* e *audita* o que aprendeu? |

Convenções da série:

- Selos de evidência **[A]/[B]/[C]/[M]** definidos em **Vol. II §0**, valem retroativamente para o Vol. I.
- Identificadores de experimento (`E*`, `F*`), gaps (`NO_*`) e riscos (`R*`) são
  **consolidados** em [§11](#11-registro-mestre-da-série). Se um volume antigo divergir,
  o registro mestre vence.
- Todo número `[M]` vem de `docs/COGNITIVE_AUDIT_FINAL.md`.

---

## Sumário

- [0. O que este volume muda](#0-o-que-este-volume-muda)
- [1. Taxonomia: conhecimento são seis coisas](#1-taxonomia-conhecimento-são-seis-coisas)
- [2. O ciclo de vida e onde ele quebra](#2-o-ciclo-de-vida-e-onde-ele-quebra)
- [3. O `neural-sgdb`, pelo contrato](#3-o-neural-sgdb-pelo-contrato)
- [4. A parede de latência e as três camadas](#4-a-parede-de-latência-e-as-três-camadas)
- [5. Compatibilidade, dimensão por dimensão](#5-compatibilidade-dimensão-por-dimensão)
- [6. A matriz 2D de garantia](#6-a-matriz-2d-de-garantia)
- [7. Knowledge Bridge](#7-knowledge-bridge)
- [8. Cinco pedidos ao SGDB](#8-cinco-pedidos-ao-sgdb)
- [9. O que o SGDB não resolve](#9-o-que-o-sgdb-não-resolve)
- [10. Riscos novos (R7–R10)](#10-riscos-novos-r7r10)
- [11. Registro mestre da série](#11-registro-mestre-da-série)
- [12. O que ainda precisa ser atacado](#12-o-que-ainda-precisa-ser-atacado)
- [13. Roadmap F1–F3](#13-roadmap-f1f3)
- [14. Referências](#14-referências)

---

## 0. O que este volume muda

O Vol. I fechou o *conceito* (bind → energia → self). O Vol. II fechou o *preço*
(hardware, números, mitigações). Faltava a pergunta mais chata e mais decisiva:

> **Quando o motor aprende algo, onde isso fica, quem pode revisar e como se prova depois?**

Hoje o AETHER não tem resposta. Ele tem **seis tipos de conhecimento** empilhados em
três módulos, sem ciclo de revisão e sem proveniência. Isso é aceitável num organismo
de bancada e **inaceitável** num sistema que se propõe a raciocinar.

Três mudanças concretas que este volume introduz:

1. **Separação de camadas:** *habilidade* fica na fase (AETHER); *registro* fica no
   ledger (`neural-sgdb`). Nenhuma das duas substitui a outra.
2. **Segundo eixo de garantia:** além do *gap de energia* (físico), existe a
   *autoridade* (social/HITL). Sozinho, nenhum dos dois evita alucinação.
3. **Um risco novo que nasce da integração:** transformar o motor pós-token num RAG
   com passos extras (R7). É o risco mais provável de todos, porque é o caminho de
   menor resistência.

---

## 1. Taxonomia: conhecimento são seis coisas

| # | Tipo | Substrato | Vida útil | Camada natural | Estado |
|---|---|---|---|---|---|
| **K1** | **Estado** (frame) | `OrganismFrame`, fases | 16,6 ms | volátil | ✅ |
| **K2** | **Habilidade** (procedimento) | `J`, `W`, topologia | dias–meses | L4 procedimental | ✅ parcial |
| **K3** | **Fato relacional** | triplas FHRR | indefinido | L4 semântico | ✅ |
| **K4** | **Lei** (regularidade) | Koopman/RLS, SINDy | indefinido | L4 semântico | ✅ |
| **K5** | **Metaconhecimento** | confiança, `should_abstain` | contínuo | L4 meta | ⚠️ E4 |
| **K6** | **Proveniência** | — | — | L4 + audit | ❌ **ausente** |

O K6 é o buraco. Sem ele, K3 e K4 são indistinguíveis de invenção.

### 1.1 Onde cada tipo vive (ou deveria viver)

| Tipo | Hoje | Alvo | Distância |
|---|---|---|---|
| K1 | memória do processo | idem | — |
| K2 | `MorphogeneticField`, `ComplexPhaseMemory` | + métrica de self-consistência (E4) | 1 experimento |
| K3 | `FhrrStore` | + `key` endereçável + proveniência | ponte (§7) |
| K4 | `koopman_sindy` | + certificado por lei (resíduo) | pequeno |
| K5 | `RecallConfidence` | + self-field (E4) | 1 experimento |
| K6 | — | ledger append-only + auditoria | **SGDB** |

---

## 2. O ciclo de vida e onde ele quebra

```
aquisição → consolidação → recuperação → revisão → esquecimento → auditoria
   ✅           ✅             ✅          ❌          ✅            ❌
```

| Estágio | Mecanismo AETHER | Orçamento | Modo de falha |
|---|---|---|---|
| **Aquisição** | `TransducerIo` → `u(t)`; `SurpriseTracker` | 0 alloc | confundir novidade com ruído |
| **Consolidação** | `ThermodynamicDreamer` + heurística de recorrência; `evolve_topology` | s–min | consolidar ruído em "lei" |
| **Recuperação** | `cleanup` + `recall_with_confidence` | µs | ciclo-limite (Vol. II §3.2) |
| **Revisão** | **ausente** | — | fato antigo vence fato novo |
| **Esquecimento** | `prune`, `homeostasis`, decaimento | contínuo | **timebase misturado** |
| **Auditoria** | **ausente** | — | não saber de onde veio |

### 2.1 O aviso de timebase não é decorativo

A doutrina do SGDB diz, literalmente:

> *"decay needs `created_tick` in the SAME timebase as `now` (wall-clock ms) — do NOT
> decay records written with a counter tick, it ages everything."* **[A, contrato]**

O AETHER usa **ticks de ciclo** (`call_index`, 60 Hz) em vários módulos. Se o decay do
M5 e o decay do ledger usarem bases diferentes, **um dos dois envelhece tudo de uma vez**.
É o tipo de bug silencioso que só aparece depois de uma semana ligado.

**Regra:** todo evento persistido carrega **dois carimbos** — `tick` (relógio interno,
para dinâmica) e `wall_ms` (relógio externo, para TTL/decay). O bridge converte, nunca assume.

### 2.2 Esquecimento em três regimes (o que a literatura acrescenta)

A pesquisa de 2026 sobre memória de agentes convergiu para a mesma tríade que o AETHER
já tem fisicamente:

| Regime | Evidência | Equivalente no AETHER |
|---|---|---|
| **Decaimento passivo** | `I(t) = I₀·e^{−λt}`, meia-vida ≈29 dias (arXiv:2605.08538 [B]) | `prune(threshold)`, homeostase |
| **Esquecimento por interferência** | retroativo 0,6 · proativo 0,4 (idem) | **interferência de fase em VSA** (Vol. II §3) — já é física, não regra |
| **Degradação graciosa** | 6 níveis: integral → resumo → gist → tombstone (idem) | *não existe*: ou o padrão está lá, ou não está |

> **Achado V3-1:** o AETHER tem o melhor modelo de *interferência* (é física) e o pior
> modelo de *degradação graciosa* (é binário). Um atrator não "vira gist" — ele some.
> A camada L4 do SGDB pode guardar o gist, e a fase guardar só o resíduo.

Consolidação híbrida tem respaldo medido: arXiv:2610.39790 [B] combina episódico +
relacional e chega a **90,2% LoCoMo** e 85,5% LongMemEval-S, com **consolidação offline**
como diferencial. É exatamente o desenho T1/T2 de §4.

---

## 3. O `neural-sgdb`, pelo contrato

Não pelo README: pelo que o próprio banco responde.

| Dimensão | Semântica |
|---|---|
| **Camadas** | L2 episódico (verbatim) · L3 release/log (append-history) · L4 semântico · L6 grafo |
| **Escopo** | `scope` + dims `agent/app/user/run`; **null-scoping**: recall global **não vê** escopo |
| **Escrita** | **ADD-only**; conflito resolvido na **recuperação** |
| **Tempo** | `expires_at` absoluto em ms, honrado no *open*; decay exige mesma base |
| **Autoridade** | 0–255; **255 = aprovado por HITL** |
| **Auditoria** | `audit_checkpoint` · `audit_verify` · `audit_trail` · `rollback_to` |
| **Governança** | `supersede` · `contradicts` · `conflicts` · `resolve_conflict` · `forget_purge(reason)` |
| **Ciclo de run** | `commit_run` · `deprecate_run` · `promote_run` |
| **Contexto** | `set_trigger(pattern, namespaces)` → `recall(path="src/…")` |

### 3.1 A frase que decide o encaixe

> **"The core does not decide. It returns typed evidence. YOU choose what enters the
> prompt and what to write."** **[A, doutrina]**

Compare com o Vol. I §1: *"o core devolve evidência física; o host decide"*.
**É a mesma doutrina, em dois substratos.** O encaixe não é de formato — é de filosofia.

### 3.2 Estado vivo (medido nesta sessão)

```
17 memórias escopadas
  app=AETHER ................ 14
  aether/project ............  2
  nsgdb/doctrine ............  1
  user=* ....................  3
```

Amostra das 5 recuperadas com `scope_app=AETHER`:

| Chave | Texto (início) |
|---|---|
| `md/L3/mcp/…374000` | "AETHER: ciclo automático a cada 5 min em `tools/cycle.ps1`…" |
| `md/L3/mcp/…802004` | "AETHER remote: origin=github.com/msrovani/AETHER.git…" |
| `md/L3/mcp/…867000` | "AETHER v0.0.35: gate final 79 testes em 17 suites…" |
| `md/L3/mcp/…876001` | "AETHER repo local `C:\Users\msrov\Documents\AETHER`…" |
| `md/L3/mcp/…872007` | "AETHER coordenação: fórum textual `Desktop\FORUM.txt`…" |

**Diagnóstico:** todas são **L3 operacionais** — infraestrutura do *projeto*, não
conhecimento do *motor*. O SGDB hoje é o cartório do agente que constrói o AETHER,
não a memória do AETHER. É exatamente a lacuna que este volume ataca.

**Detalhe fino:** a entrada `v0.0.35` convive com um crate em 0.0.67+. Não é erro —
L3 é *append-history*. Mas revela o problema: **o ledger não diz qual versão está
vigente**; quem decide é o host, por `created_tick`/`supersede`. Um motor contínuo
precisa dessa decisão resolvida por **energia/autoridade**, não por leitura manual.

---

## 4. A parede de latência e as três camadas

```
ciclo AETHER               146,5 µs    [M]
verificação estrutural      14–17 ms    (ReGA, Vol. II §4.3)
recall SGDB (MCP/JSON)      ~1–50 ms    (ordem de grandeza)
```

Razão **10²–10⁴×**. Conclusão imediata: **SGDB não entra no hot path.** E isso está
certo — gestão de conhecimento é, por natureza, assíncrona.

| Camada | Frequência | O que passa | Proibido |
|---|---|---|---|
| **T0 — hot path** | 60 Hz / 146 µs | fase, energia, decisão | qualquer I/O, qualquer `String` |
| **T1 — consolidação** | s–min | evento tipado → ring buffer binário | bloquear |
| **T2 — registro** | min–dias | fato, proveniência, TTL, autoridade, auditoria | entrar no caminho de decisão |

```
T0 ──(KnowledgeEvent, 0 alloc)──► ring buffer SPSC ──(drain)──► T2 SGDB
                                                                    │
T0 ◄──(cue de fase: &[f32])─────── atrator ◄──(recall: key+vec)─────┘
```

O ring buffer já existe (`src/core/spsc_ring.rs`). Falta só o formato do evento.

---

## 5. Compatibilidade, dimensão por dimensão

| SGDB | AETHER | Encaixe |
|---|---|---|
| ADD-only | superposição de fase (soma) | ✅ **isomórfico** |
| conflito na recuperação | gap de energia / ambiguidade de `cleanup` | ✅ mesma filosofia |
| `entities` idênticas | `FhrrStore::add_fact(entity, relation, value)` | ✅ com mapeamento |
| `authority` 0–255 | confiança / `should_abstain` | ⚠️ **eixos ortogonais** (§6) |
| TTL + decay | `prune`, homeostase | ✅ com §2.1 |
| `audit_trail` | — | ✅ **fecha K6** |
| `payload_type` text/embedding/binary | fase complexa `f32` | ❌ falta tipo |
| `key` = `"md/L4/…"` | endereço por conteúdo | ❌ falta hash |
| `scope` | — | ❌ motor não tem escopo |

### 5.1 Quanto dos 6 riscos o SGDB realmente fecha

| Risco (Vol. II §8) | Fechado pelo SGDB? |
|---|---|
| R1 Ising não é oráculo | ❌ |
| R2 Interferência VSA | ⚠️ parcial (registra capacidade medida) |
| R3 Geometria ≠ verdade | ⚠️ **parcial e decisivo**: metade social (§6) |
| R4 Open-endedness sem corpo | ❌ |
| R5 Custo de treino | ❌ |
| R6 Preprints | ⚠️ parcial (protocolo/auditoria de evidência) |

**Veredito objetivo:** 0 fechados, 3 parciais. O SGDB não é a cura dos riscos — é a
**camada que impede o sistema de mentir sobre eles**.

---

## 6. A matriz 2D de garantia

O AETHER tem *warrant* físico (gap de energia). O SGDB tem *warrant* social
(autoridade/HITL). Nenhum dos dois, sozinho, evita alucinação.

```
                          gap de energia ALTO        gap BAIXO
                    ┌─────────────────────────┬──────────────────────────┐
  autoridade ALTA   │  ATUA + CITA            │  CITA, NÃO ATUA          │
  (255 · HITL)      │  fato + inequívoco      │  fato, mas ambíguo aqui  │
                    ├─────────────────────────┼──────────────────────────┤
  autoridade BAIXA  │  PROPÕE (marca inferido)│  ABSTÉM                  │
  (inferência)      │  coerente, sem fonte    │  ruído plausível         │
                    └─────────────────────────┴──────────────────────────┘
```

- Hoje o AETHER só tem o eixo **vertical**.
- Hoje o SGDB só tem o eixo **horizontal**.
- Juntos, o quadrante **inferior-direito** — *"hipervetor plausível, sem fonte,
  fisicamente ambíguo"* — passa a ser **detectável**. É o modo de falha exato do
  Vol. II §4.2, e ninguém hoje consegue nomeá-lo.

**Decisão de vocabulário (fecha `CERT_SEMANTICS`):** o artefato emitido é um
**certificado de factibilidade** `(E, ΔE, violações, proveniência)`, não de verdade. A resposta carrega
`proveniência` obrigatória. Sem os dois campos, a resposta é inválida por construção.

### 6.1 Três coisas que o certificado **não** é

O E2/E3 separaram três noções que o vocabulário antigo confundia:

| Noção | Campo | O que prova | O que **não** prova |
|---|---|---|---|
| **Testemunha de factibilidade** | `violations == 0` (contagem exata) | a atribuição avalia todas as restrições como verdadeiras | que o mundo é assim |
| **Mínimo local** | `ΔE > 0` (gap de flip) | nenhum flip simples reduz a energia do QUBO | que é o ótimo global |
| **Verdade** | — | nada no motor | qualquer coisa fora do contrato |

Consequências medidas: o E2 decide por `violations` (100% preciso em 169/169); o
E3b mostrou que `ΔE` sozinho **não** prediz acerto em otimização livre
(AUC 0,46–0,55). Por isso o E2 repara a factibilidade **depois** do BSB e emite
o `ΔE` apenas como diagnóstico — nunca como decisão. O E3 usa o par
(evidência `role_similarity`, margem) para abstenção: margem sozinha **não**
detecta consulta fora do domínio, porque o unbinding sempre "encontra" um
valor para o melhor papel.

---

## 7. Knowledge Bridge

Espelha o `TransducerIo` (borda única, zero-alloc, sem `String` no sinal).

```rust
pub struct Certificate { pub energy: f32, pub gap: f32, pub violations: u32 }

pub struct KnowledgeEvent<'a> {
    pub key: u128,          // hash do padrão de fase (endereço por conteúdo)
    pub layer: Layer,       // L2 episódico · L4 semântico · L6 grafo
    pub certificate: Certificate,
    pub authority: u8,      // 0..255 — warrant social
    pub scope: ScopeId,     // organismo / run / app
    pub tick: u64,          // relógio interno (dinâmica)
    pub wall_ms: u64,       // relógio externo (TTL/decay) — §2.1
    pub payload: &'a [u8],  // fases cruas; NUNCA &str no sinal
}

pub trait KnowledgeSink   { fn record(&mut self, ev: &KnowledgeEvent<'_>) -> Result<()>; }
pub trait KnowledgeSource { fn cue_into(&mut self, query: &[f32], out: &mut [f32]) -> Result<usize>; }
```

O detalhe que fecha o laço é **`cue_into`**: o `recall` devolve `key` + vetor; esse
vetor vira **semente de atrator** para `ComplexPhaseMemory::recall_into`. O ledger
textual **não substitui** a fase — ele a **reacende**.

```
texto/fato  ──► SGDB (L4)  ──► vetor  ──► cue ──► atrator de fase ──► resposta
                                     ▲
                    fase ◄── record ──┘  (certificado + autoridade + proveniência)
```

### 7.1 Disciplina de zero-alloc

| Operação | Aloca? |
|---|---|
| `record` no T0 | **não** (escreve no ring buffer) |
| `drain` no T1 | sim (mas fora do ciclo) |
| `cue_into` no T0 | **não** (buffer pré-alocado) |
| serialização p/ MCP | sim (T2) |

Gate: `alloc_invariants` continua verde **com a ponte ligada**.

---

## 8. Cinco pedidos ao SGDB

| # | Pedido | Por quê |
|---|---|---|
| 1 | `payload_type = "phase"` (bytes `f32` complexos) | hoje é text/embedding/binary; fase não é nenhum |
| 2 | `remember(..., created_tick = wall_ms)` explícito | a doutrina avisa sobre timebase; hoje é implícito |
| 3 | campo `certificate {E, dE, violations}` | `explain(full)` já dá authority/clock/audit_refs/meta; falta o físico |
| 4 | `key` endereçável por conteúdo (`u128`) aceito no `remember` | `md/L4/…` exige string; o motor não tem string |
| 5 | `curate(op=consolidate_by_energy)` | `consolidate_recurrences` é temporal; fase consolida por energia |

Nenhum é ruptura: todos são extensões compatíveis com `contract.json`.

---

## 9. O que o SGDB não resolve

1. **Verdade.** Registra o que foi dito/observado, não o que é.
2. **Raciocínio.** Não substitui roof-dual → Ising (Vol. II §2).
3. **Perda na ponte.** Fase → texto/embedding é lossy; só vale quando a perda é aceitável.
4. **Dims não se misturam.** MiniLM é 384-d; FHRR é 160 complexo. A própria doutrina
   proíbe misturar → escopo `phase/v1` separado.
5. **Não é memória principal.** Se virar, o motor deixa de ser pós-token.

---

## 10. Riscos novos (R7–R10)

| # | Risco | Por que é real | Gate |
|---|---|---|---|
| **R7** | **Drift para RAG** | consultar o ledger antes de decidir é o caminho de menor resistência; mata o invariante | **0** chamadas ao SGDB no T0; medir fração de decisões resolvidas só por fase (alvo ≥ 80%) |
| **R8** | **Vazamento de escopo** | null-scoping + motor sem escopo → organismo A contamina B | teste de isolamento cross-organism: 0 hits cruzados |
| **R9** | **Proveniência-teatro** | registrar e nunca usar na decisão | abstenções *causadas* por proveniência > 0 e mensuráveis |
| **R10** | **Certificado sem dentes** | `ΔE` não calibrado ⇒ selo vira decoração | precisão do selo ≥ 95%; falsos "confiáveis" < 5% |

R7 é o mais provável de todos. Merece um invariante de teste, não um aviso em prosa.

---

## 11. Registro mestre da série

### 11.1 Experimentos

| ID | Vol. | Objetivo | Gate | Fecha |
|---|---|---|---|---|
| **E6** ✅ | II | perfil por estágio — **executado** | cobertura **99,98%**; solver **84,4%** · 7,5 GFLOP/s | bloqueio transversal **removido** |
| **E6b** ✅ | II | kernel do solver (16 acumuladores, Rust safe) | **17,10 GFLOP/s (2,30×)** · ACQ 100 · 24 suítes verdes | R1, R5 |
| **E1** ✅ | I/II | PLH *sharded* + ressonador ACF (`phase_language_probe.rs`) — **executado** | **1.000 fatos · recall@1 1,0000** (plano 0,0725) · **SNR 5,88σ** · **p50 5,80 µs** · **0 alloc** · NC **100%** · abstenção **100%** | R2 |
| **E1-2** ✅ | I | `FhrrStore::query_phasor_into` (fasor de papel pré-computado) | API + teste de equivalência bit-a-bit; remove `sin_cos` por consulta | R2 |
| **E1b** ✅ | II | capacidade do store e da memória de fase — **executado** | **D=160: 64 fatos (K=6)** · lei **linear ~0,4·D** (não D²) · fase: ~4 padrões aleatórios | R2 |
| **E1b-2** ✅ | II | memória de fase estruturada (harmônica) — **executado** | **M_harm = 8** vs 4 aleatório a erro < 0,15 rad · fronteira real **5× abaixo** do declarado (40) | R2 |
| **E2** ✅ | I | lógica como Ising + certificado — **executado** | **3-SAT n=20: 169/169 satisfatíveis (100%)** vs BSB puro 9/169 (5,3%) · decisivas **100% precisas** · **0 decisivas em 31 UNSAT** · **0 alloc** · simbólico p50 **23,7 µs** · G3b **FAIL declarado** (6× o teto de 4,07 µs; envelope medido: `n ≥ 8` fora, `n = 6` na fronteira) | R1, R3, R10 |
| **E2b** ✅ | II | presolve simbólico antes do BSB — **executado** | **0 insound** · **64,8% redução** (+39,8 pp de cascata) · **200/200** resolvidas vs 183/200 | R1, R3 |
| **E2c** ✅ | II | persistência de 1ª ordem (dominância) — **executado** | **0/600 ótimos alterados** · 0% sem viés (controle) · 42,9% com viés 45% | R1, R3 |
| **E2c-2** ✅ | II | roof dual completo (max-flow) — **executado** | **0/600 ótimos alterados** · **52,0%** em 45% (vs 42,9%) · 28,9% em 25% · **0% no controle = teto ℤ₂** · 199/200 | R1 |
| **E3** ✅ | I | transplantação de embeddings — **executado** | **JL ρ=0,9528** (499.500 pares) · **1,28 KB/átomo** (1,28 MB/1.000) · relação **0,9990** (≤ 48 fatos/shard) · analogia **R@1 0,9549** (sem alvo) · proveniência **100%** · OOD **100%** abstido · verificador **0,007 ms / 0 alloc** · calibração com ruído 0,6 rad (evidência p5 **0,8037**) | R3, R10 |
| **E3b** ✅ | II/III | verificador de coerência + proveniência | **spins: nulo (AUC 0,46–0,55)** · **2-SAT: precisão 100%** | R3, R10 |
| **E4** ✅ | I | self-field + ablação — **executado** | **ECE 0,3567 → 0,3238** (bootstrap p97,5 = **−0,0168** < 0) · **T01 1,0000** (não regride) · erro pós-plasticidade **0,0140** vs 0,0562 congelado · topologia 0,1507 → 0,1391 | K5 |
| **E5** ✅ | I | currículo por progresso — **executado** | readaptação **8 ciclos** (T03 = 150) · outlier: **1** update vs 7 da surpresa bruta · **0 alloc** em 150 ciclos | R5 |
| **E7** ✅ | II | laço S0 com 4 atuadores — **executado** | entropia **−71,9%** (gate ≥ 30%) · ρ(controle, entropia) **0,8808** · 100k ciclos · throttle/affinity/pause 13 cada | R4 |
| **E8** ✅ | II | readout congelado vs treinado — **executado** | NRMSE congelado **0,0791** vs joint 0,3132 · regressão **0,000** · treino 1,88 s vs 5,53 s (**2,94×**) · 615.680 predições/s | R5 |
| **F1** ✅ | III | `KnowledgeSink` read-only — **executado** | **0 alocações** no `T0` (10k ciclos) · **`t0_ledger_calls = 0`** · `T1` 100% íntegro · **0 hits cruzados** de escopo | R7, K6 |
| **F2** ✅ | III | consolidação T1 + `commit_run` — **executado** | **32/32 eventos/rodada**, 0 perdidos · `t0_ledger_calls = 0` · retenção de gist **1,000** (40/40) · R9 = 1 | R9 |
| **F3** ✅ | III | fusão 2D + `cue_into` — **executado** | plausível-ambíguo **1,000** (100/100) · ruído sem fonte **1,000** (300/300) · recall@1 direto e round-trip **1,000** · perda **0,000** | R3, R8 |

### 11.2 Gaps consolidados

| Código | Vol. | Sev. | Remediação |
|---|---|---|---|
| `NO_BINDING_HEAD` | I | ✅ **resolvido (E1)** | 1.000 fatos sharded · recall@1 **1,0000** · p50 **5,80 µs** · **0 alloc** | — |
| `NO_ENERGY_CERT` | I | ✅ **resolvido (E2)** | resposta sempre com `(E, ΔE, violações, proveniência)`; 169/169 testemunhas válidas | — |
| `NO_ISING_CONSTRAINTS` | I | ✅ **resolvido (E2)** | gadget exato de 3-SAT (2 spins aux) + multiplicadores de Lagrange; 100% vs 5,3% do BSB puro | — |
| *(E2d aquisição)* | I | ✅ **lab** | custos pairwise CRAM→QUBO offline; warrant na inferência continua sendo o `Certificate` (`docs/CRAM_LOGOS.md`) | — |
| `NO_SELF_FIELD` | I | ✅ **resolvido (E4)** | ECE 0,3567 → 0,3238 (IC abaixo de zero); T01 1,0000 | — |
| `NO_PROGRESS_CURRICULUM` | I | ✅ **resolvido (E5)** | readaptação 8 ciclos (T03 = 150); outlier 1 update vs 7 | — |
| `CACHE_METRIC_ABSENT` | I | ✅ **resolvido por proxy (HW_TARGET §9)** | bytes tocados/ciclo por aritmética; PMU indisponível sem driver | — |
| `CAPACITY_CLIFF_UNDOC` | I | ✅ **resolvido (HW_TARGET §9)** | penhasco documentado: `J` em N=512 = 2,1 MB > L2 (1,25 MB) | — |
| `TRANSPLANT_UNVALIDATED` | I | ✅ **resolvido (E3)** | JL ρ=0,9528 · 1,28 KB/átomo · relação 0,9990 · analogia R@1 0,9549 | — |
| `PREPRINT_RISK` | I/II | ℹ️ **processo (não-gate)** | evidência triangulada com selos [A]/[B]/[C] (protocolo §7 Vol. II); disciplina contínua | — |
| `NO_PROFILE_BREAKDOWN` | II | ✅ **resolvido (E6)** | medido 2026-11-15: solver 84,4% | — |
| `SOLVER_KERNEL_SCALAR` | II | ✅ **resolvido (E6b)** | 16 acumuladores safe: 7,44 → 17,10 GFLOP/s | — |
| `NO_ROOF_DUAL` | II | ✅ **resolvido (E2c-2)** | max-flow Dinic + SCC: 42,9% → **52,0%** em 45%, 0/600 insound | — |
| `NO_LIMIT_CYCLE_GUARD` | II | alto | **aberto**: falta guarda de ciclo-limite (detecção de 2-ciclo na dinâmica de fase) + gate; o E1b mediu capacidade, não a taxa de ciclo | — |
| `NO_PROVENANCE` | II | ✅ **resolvido (F1 + E3)** | evento F1 carrega `Certificate`+`authority`+`ScopeId`; E3 carimba 100% das respostas e abstém 100% fora do domínio | — |
| `D_CAPACITY_UNMEASURED` | II | ✅ **resolvido (E1b → E1)** | lei linear **0,4·D** medida; 1.000 fatos via 24 shards | — |
| `NO_ACTUATOR_LOOP` | II | ✅ **resolvido (E7)** | entropia −71,9% com 4 atuadores; 100k ciclos | — |
| `CERT_SEMANTICS` | II | ✅ **resolvido (§6/§6.1)** | testemunha ≠ mínimo local ≠ verdade; decisão por `violations` | — |
| `KB_TAXONOMY_UNDOC` | III | ✅ **resolvido (§1)** | taxonomia dos seis tipos + onde cada um vive | — |
| `NO_LEDGER_BRIDGE` | III | ✅ **resolvido (F1)** | `knowledge_bridge_f1.rs`: `record` 0 alloc, `drain_batch` T1, telemetria | — |
| `TIMEBASE_MIX` | III | ✅ **resolvido (F1)** | `tick` e `wall_ms` viajam separados; `created_tick = wall_ms` | — |
| `NO_REVISION_PATH` | III | ✅ **resolvido (F2)** | supersede no `FusionLedger`; `commit_run` por ciclo | — |
| `SCOPE_UNDEFINED` | III | ✅ **resolvido (F3)** | `ScopeId`/`ScopeLedger`: 0 hits cruzados em 2 escopos | — |
| `NO_DEGRADATION_LADDER` | III | ✅ **resolvido (F2)** | `prune_to_gist` com retenção **1,000** (40/40); supersede no `FusionLedger` | — |

### 11.3 Riscos

| # | Risco | Residual | Prioridade |
|---|---|---|---|
| R1 | Ising não é oráculo | médio | P1 |
| R2 | Interferência VSA | médio | P1 |
| R3 | Geometria ≠ verdade | **alto / irredutível em parte** | **P0** |
| R4 | Open-endedness sem corpo | médio | P2 |
| R5 | Custo de treino | baixo | P3 |
| R6 | Preprints | baixo | P3 |
| R7 | Drift para RAG | médio | P1 |
| R8 | Vazamento de escopo | baixo | P2 |
| R9 | Proveniência-teatro | alto | P1 |
| R10 | Certificado sem dentes | alto | P1 |

---

## 12. O que ainda precisa ser atacado

### 12.1 O placar honesto dos seis

> **Nenhum dos seis foi fechado.** O Vol. I formulou; o Vol. II precificou.
> **Zero gates executados.** Isso é o estado real.

| # | Risco | O que o Vol. II mostrou | O que falta | Fechado por | Prioridade |
|---|---|---|---|---|---|
| R1 | Ising não é oráculo | mitigável: roof dual 5.800×, presolve 6–18×, sparse O(N²)→O(N) | **implementar o pipeline de 4 estágios** e medir em 3-SAT real | E2b | **P1** |
| R2 | Interferência VSA | capacidade ∝ D²; inimigo é ciclo-limite; ruído na init +50× | **medir capacidade real em D=160** e a taxa de ciclo-limite | E1b | **P1** |
| R3 | Geometria ≠ verdade | JL é sólido; 78% de analogia é [C]; **E3b: ΔE não prediz em otimização livre; 100% de precisão com restrições exatas** | **usar o certificado só como verificação de factibilidade** (E2b) | E2b | **P0** |
| R4 | Corpo | laço fechado basta; AIF real 75% vs 8,33% | **construir S0 com atuador de consequência** e rodar H1–H3 | E7 | P2 |
| R5 | Custo de treino | evitável: congelar física (reservoir); DEER rende 25% em n=64 | **confirmar que congelar não custa acurácia** | E8 | P3 |
| R6 | Preprints | triangulação com selos [A]/[B]/[C] | **disciplina contínua**, não experimento | protocolo | P3 |

### 12.2 O que não era um dos seis — e agora está resolvido

**`NO_PROFILE_BREAKDOWN` (E6) — ✅ RESOLVIDO em 2026-11-15.**

| Medida | Valor |
|---|---|
| Cobertura da instrumentação | **99,98%** |
| Estágio dominante | **solver** |
| Fração do quadro (steps 60/120/240) | **84,0% / 90,9% / 95,3%** |
| Throughput do solver | **~7,5 GFLOP/s** f64 (~19% do pico AVX2 a 2,5 GHz) |
| Total medido vs. bench oficial | 154–158 µs vs 146,5 µs (**+5 a +8%**) |
| p99/p50 | 2,09× (auditoria: 2,13×) |

Artefatos: `examples/e6_profile.rs`, `tests/e6_profile_gate.rs`,
`target/aether_profile_e6.json`, `docs/HW_TARGET.md` §8 · fórum **MSG 0098**.

**O que isso muda:** a estimativa do Vol. I/II de que "o matvec custa 0,07% do ciclo"
estava **errada** (o solver faz dois gemv por passo, não um). O motor é matvec-bound.
O maior ganho disponível no projeto não é física nova nem hardware — é **vetorizar o
kernel do solver**: há ~5× de headroom até o pico f64.

Gap novo: **`SOLVER_KERNEL_SCALAR`** → próximo experimento **E6b**.

**`NO_BINDING_HEAD` / `D_CAPACITY_UNMEASURED` (E1) — ✅ RESOLVIDO em 2026-11-15.**

A lei do E1b (`0,4·D` fatos por acumulador) não se combate com mais dimensão: combate-se
com **endereçamento**. 24 shards de `≤ 48` fatos (75% do teto de 64), roteados pelo par
`(sujeito, verbo)`; âncoras de papel com permutação cíclica `ρ^k` (não-comutatividade
estrita); e um ressonador ACF — *cleanup* iterativo com projeção *re-encode*, decisões na
cópia **perturbada** do codebook (`±4% de π`) e validação na cópia **limpa**.

| Medida | Valor |
|---|---|
| recall@1 — 1.000 fatos, 24 shards, D=160 | **1,0000** |
| recall@1 — 1.000 fatos em superposição plana | **0,0725** |
| fatos/acumulador (máximo) | **43** (≤ 48 = 75% de 64) |
| termos/acumulador (2 por fato) | 86 — teto equivalente em K=10 ≈ 107 (**80%**) |
| não-comutatividade A→B vs B→A | **100%** (150 pares) · sim Ψ_AB·Ψ_BA **0,501** |
| controle comutativo (mesmo papel, sem ρ) | **1,000** — é o que a construção evita |
| SNR do cleanup (OBJ / MOD) | **5,88σ / 5,89σ** |
| latência por consulta (p50 / p99) | **5,80 / 7,80 µs** (média 5,91) |
| alocações no caminho quente | **0** |
| abstenção OOV (vocabulário / memória) | **100% / 100%** |
| abstenção com ruído de fase σ = 1,6 rad | **100%** (falso-abster no limpo: **0%**) |
| validação ACF / convergência do ressonador | **100% / 100%** |
| working set (acumuladores + codebooks + scratch) | **436 KB** (< 500 KB) |

Artefatos: `examples/phase_language_probe.rs`, `target/aether_e1_plh.json`, fórum
**MSG 0106** (claim) e **MSG 0169** (relatório). Uma implementação irmã e independente
(**MSG 0105**) mediu o mesmo recall com **17,35 µs** de p50 e falhou só no gate de
latência (`sin_cos` por consulta); aqui a diferença é o cleanup **complexo** (nunca
projetado) com `cos/sin` do codebook pré-computados.

**Nota de modelagem (o que o ressonador resolve — e o que não resolve).** Com o papel
de endereço dado, o fator de *endereço* não precisa de ressonador (**MSG 0105** tem
razão nisso). O ressonador é necessário nos **fatores do predicado**: o valor guardado é
um bundle `A_O⊗ρ²(o) ⊕ A_M⊗ρ³(m)` e o cleanup só separa os dois quando um deles é
descontado por *re-encode*. Guardar o predicado como **produto** de fases (soma de
fases) falha por construção: o interferente **rotaciona** a pista em vez de somar ruído
ortogonal, e o cleanup deixa de ver o átomo (medido: recall 0,008).

**O que isso muda:** o gargalo da "linguagem em fase" não era a dimensão nem a
capacidade do acumulador — era *endereçamento*. Com sharding por papel e um ressonador
que separa os fatores do predicado, 1.000 fatos cabem em `D=160` a 6,8 µs e zero
alocação, sem violar a lei do E1b em nenhum acumulador.

### 12.3 O que nasceu no Vol. III

- **R7 (drift para RAG)** é o risco **mais provável da série** — porque é o caminho de
  menor resistência e porque tem incentivo social (todo mundo faz).
- **R9 (proveniência-teatro)** é o risco mais insidioso: o sistema *parece* auditável
  e não é.
- **R8 (escopo)** só aparece quando houver mais de um organismo.

### 12.4 Ordem de ataque recomendada

```
1º ✅ E6   perfil                     ← FEITO: solver = 84,4% do ciclo
2º ✅ E6b  kernel do solver            ← FEITO: 7,44 → 17,10 GFLOP/s (2,30×)
3º ✅ E3b  certificado + proveniência  ← FEITO: verifica factibilidade, não acerto
4º ✅ E2b  presolve simbólico          ← FEITO: 64,8% redução, 0 insound, 200/200
5º ✅ E2c  persistência de 1ª ordem     ← FEITO: sound (0/600), míope por construção
6º ✅ E1b  capacidade em D=160          ← FEITO: 64 fatos · lei linear ~0,4·D
7º ✅ E1   PLH sharded                   ← FEITO: 1.000 fatos recall 1,000 · p50 5,80 µs · 0 alloc
8º ✅ E1-2 fasor pré-computado + ressonador ← FEITO: latência fechada (E1)
9º  ✅ E2c-2 roof dual (max-flow)         ← FEITO: 52,0% em 45% · 0/600 insound · teto 0% no controle
10º ✅ E1b-2 memória de fase estruturada  ← FEITO: harmônico 8 vs aleatório 4 (erro < 0,15 rad)
11º ✅ F1   ponte read-only               ← FEITO: 0 alloc no T0 · 0 chamadas ao ledger · R8 = 0
12º ✅ E2    lógica como Ising + certificado ← FEITO: 169/169 (100%) vs 5,3% · 0 alloc · G3b FAIL declarado
12º ✅ E3    transplantação JL + proveniência ← FEITO: ρ=0,9528 · 1,28 KB/átomo · R@1 0,9549 · OOD 100%
13º ✅ E4/E5/E7/E8 self, currículo, corpo, treino ← FEITO: ECE −0,0322 · 8 ciclos · entropia −71,9% · congelado 0,0791
14º ✅ F2/F3  consolidação e fusão 2D          ← FEITO: 0 eventos perdidos · gist 1,000 · quadrante 1,000
```

### 12.5 As sete perguntas — placar

| # | Pergunta | Status | Resposta / o que a fecharia |
|---|---|---|---|
| 1 | **Onde vão os 146 µs?** | ✅ **respondida** (E6) | **84,4% no solver** (133,1 de 157,7 µs). readout 7,0% · entropy 3,0% · wave 2,5% · actuator 1,7% · ring_copy 1,1%. Cobertura 99,98%. |
| 2 | **ΔE prevê acerto?** | ✅ **respondida** (E3b) | **Não em otimização livre** (AUC 0,46–0,55; bound 0,058 anti-preditivo). **Sim com restrições exatas**: 383 decisivas, precisão 100%. |
| 3 | **Quantos padrões cabem em D=160?** | ✅ **respondida** (E1b/E1) | **64 fatos** por acumulador (`FhrrStore`, K=6, recall@1 ≥ 99%); 128 já cai para 89%. Lei **linear ~0,4·D** — **não** D². **E1:** com 24 shards de ≤ 48 fatos, **1.000 fatos** com recall@1 **1,0000** (superposição plana: 0,0725). Memória de fase (M5): **~4** padrões aleatórios a erro < 0,15 rad, **8** com padrões harmônicos — `pattern_capacity() = 40` é 5× otimista (E1b-2). |
| 4 | **Roof dual reduz o suficiente em CNF de circuito?** | ✅ **respondida** (E2b/E2c/E2c-2) | **CNF:** presolve por unidades+implicação dá **64,8%** (cascata +39,8 pp), 0 insound, 200/200. **QUBO:** dominância é sound (**0/600**) mas míope; o **roof dual (max-flow)** é igualmente sound (**0/600**) e leva **42,9% → 52,0%** em viés 45%, **28,9%** em 25% e **0% no controle — que é o teto ℤ₂** (com `h = 0` nenhum spin é persistente). Fecha com **probing condicionado**, não com mais fluxo. |
| 5 | **O self-field melhora calibração ou só replica a surpresa?** | ✅ **respondida** (E4) | **ECE 0,3567 → 0,3238** (bootstrap p97,5 = **−0,0168** < 0) com **T01 1,0000** intacto: metacognição é separada, como previsto. Erro pós-plasticidade 0,0140 vs 0,0562 do campo congelado. |
| 6 | **Um atrator pode virar "gist" sem perder a habilidade?** | ✅ **respondida** (F2) | `prune_to_gist` com retenção **1,000** (40/40), 0 eventos perdidos e supersede no `FusionLedger`. |
| 7 | **A ponte de fase preserva o suficiente?** | ✅ **respondida** (F3) | `cue_into` com recall@1 direto **1,000**, round-trip **1,000**, perda **0,000** (gate < 5%). |

**Placar: 7 respondidas, 0 abertas.** Cada resposta tem artefato em `target/`
ou teste de gate com número impresso (`[E4]`, `[E5]`, `[F2]`, `[F3]`). A pergunta 4
foi fechada em duas metades: o presolve/roof dual é *sound* (e o teto do controle é
0% por simetria), mas fecha parcialmente — o caminho para os 93,6% teóricos é
`probing` condicionado. Nenhuma resposta foi inferida.

> *"Três perguntas tinham resposta e não sabíamos; uma está parcialmente
> respondida; três continuam abertas — e agora sabemos exatamente como fechá-las."*

---

## 13. Roadmap F1–F3

| Fase | Entrega | Gate |
|---|---|---|
| **F1** ✅ (feito) | `KnowledgeSink` read-only: todo evento com `Certificate` vira linha no SGDB; nada volta | **0 alloc no T0** ✔ · `alloc_invariants` verde ✔ · **R7 = 0** ✔ |
| **F2** (2–3 sem) | consolidação T1: ring buffer + drain; `commit_run` ao fim de cada ciclo; `supersede` em divergência | 0 eventos perdidos; latência do ciclo inalterada; R9 > 0 |
| **F3** (pesquisa) | fusão 2D (`authority × gap`) + `cue_into` | quadrante "plausível sem fonte" detectado ≥ 90%; R8 = 0 |

Regra de ouro das três: **o motor nunca espera o ledger.**

---

## 14. Referências

### 14.1 Memória de agentes (novas neste volume)

| ID | Título | Selo |
|---|---|---|
| arXiv:2610.39790 | Long-Term Agent Memory through Complementary Curation and Consolidation (90,2% LoCoMo) | [B] |
| arXiv:2601.09913 | Continuum Memory Architectures for Long-Horizon LLM Agents | [B] |
| arXiv:2605.08538 | Human-Inspired Memory Architecture for LLM Agents (λ=0,001; meia-vida ≈29 d) | [B] |
| arXiv:2609.16053 | Retrieval-Driven Memory Reconsolidation (REALM) | [B] |
| arXiv:2609.33013 | The Epistemics of Agent Memory (governança ≠ qualidade, r²=0,43) | [B] |
| arXiv:2604.20300 | FSFM: selective forgetting | [B] |
| arXiv:2604.20943 | SCM: Sleep-Consolidated Memory | [C] |
| arXiv:2603.15642 | CraniMem: gated and bounded memory | [B] |

### 14.2 Reaproveitadas dos Volumes I–II

| ID | Uso aqui |
|---|---|
| WWW 2026 (ReGA) | verificador estrutural 14–17 ms |
| arXiv:2605.18871 | verificação por energia (−53% violações) |
| arXiv:2602.18671 | spilled energy (detecção sem treino) |
| Zenodo 19476847 | transplantação (mecanismo [A], números [C]) |
| arXiv:2511.21046 | limites de Ising (R1) |
| arXiv:2412.00354 | ACF/IMF +50× capacidade (R2) |

### 14.3 Do projeto

| Fonte | Papel |
|---|---|
| `docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md` | Vol. I — problema e tese |
| `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` | Vol. II — hardware e riscos |
| `docs/COGNITIVE_AUDIT_FINAL.md` | fonte dos números `[M]` |
| `docs/REPO_SYNC.md` | primeira menção ao SGDB no repo |
| `docs/API_CONTRACT.md` | contrato normativo (Apêndice A = superfícies novas, aditivas) |
| `docs/LESSONS_E2_E3.md` | lições operacionais do Corpo 2 + fechamento do Corpo 3 (armadilhas, gates, processo) |
| `src/core/spsc_ring.rs` | transporte do T1 |
| `src/phase_memory.rs` | alvo do `cue_into` |
| `src/core/fhrr.rs` | `add_fact` / `query_selective` |

---

## Fecho do Volume III

O Vol. I disse: *o AETHER não tem três problemas, tem um com três faces.*
O Vol. II disse: *as seis objeções eram corretas, e todas têm preço.*
O Vol. III diz: **o preço inclui contabilidade.** Um sistema que aprende e não registra
não tem conhecimento — tem hábito. E hábito não se audita, não se revisa e não se
transfere.

O `neural-sgdb` encaixa porque, como o AETHER, ele **se recusa a decidir**: devolve
evidência tipada e deixa a decisão com o host. É o único tipo de memória que pode
conviver com um motor de fase sem corrompê-lo.

> *O AETHER é o inconsciente: rápido, contínuo, sem palavras.
> O SGDB é o cartório: lento, escrupuloso, com firma reconhecida.
> Inteligência é o que acontece quando os dois concordam — e abstenção é o que
> acontece quando não concordam.*

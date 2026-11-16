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
| **E1** | I | linguagem em fase (papel–filler) | SNR > 5σ, 0 alloc | R2 |
| **E1b** ✅ | II | capacidade do store e da memória de fase — **executado** | **D=160: 64 fatos (K=6)** · lei **linear ~0,4·D** (não D²) · fase: ~4 padrões aleatórios | R2 |
| **E2** | I | lógica como Ising + certificado | 3-SAT n≤20 = força bruta | R1 |
| **E2b** ✅ | II | presolve simbólico antes do BSB — **executado** | **0 insound** · **64,8% redução** (+39,8 pp de cascata) · **200/200** resolvidas vs 183/200 | R1, R3 |
| **E2c** ✅ | II | persistência de 1ª ordem (dominância) — **executado** | **0/600 ótimos alterados** · 0% sem viés (controle) · 42,9% com viés 45% | R1, R3 |
| **E2c-2** | II | roof dual completo (max-flow) | alcance além da dominância; mesmo gate de soundness | R1 |
| **E3** | I | transplantação de embeddings | analogia medida, sem alvo de marketing | R3 |
| **E3b** ✅ | II/III | verificador de coerência + proveniência | **spins: nulo (AUC 0,46–0,55)** · **2-SAT: precisão 100%** | R3, R10 |
| **E4** | I | self-field + ablação | ECE melhora; T01 não regride | K5 |
| **E5** | I | currículo por progresso | readaptação < 150 ciclos (T03) | R5 |
| **E7** | II | laço S0 com 4 atuadores | H3: entropia cai ≥30% sem atuador | R4 |
| **E8** | II | readout congelado vs treinado | <5% regressão; <60 s treino | R5 |
| **F1** | III | `KnowledgeSink` read-only | 0 no hot path; `alloc_invariants` verde | R7, K6 |
| **F2** | III | consolidação T1 + `commit_run` | 0 eventos perdidos; ciclo inalterado | R9 |
| **F3** | III | fusão 2D + `cue_into` | quadrante "plausível sem fonte" ≥90% | R3, R8 |

### 11.2 Gaps consolidados

| Código | Vol. | Sev. | Remediação |
|---|---|---|---|
| `NO_BINDING_HEAD` | I | crítico | E1 |
| `NO_ENERGY_CERT` | I | crítico | E2 → E3b |
| `NO_ISING_CONSTRAINTS` | I | alto | E2b |
| `NO_SELF_FIELD` | I | alto | E4 |
| `NO_PROGRESS_CURRICULUM` | I | médio | E5 |
| `CACHE_METRIC_ABSENT` | I | médio | sonda LLC |
| `CAPACITY_CLIFF_UNDOC` | I | médio | `docs/HW_TARGET.md` |
| `TRANSPLANT_UNVALIDATED` | I | alto | E3 |
| `PREPRINT_RISK` | I/II | info | protocolo §7 Vol. II |
| `NO_PROFILE_BREAKDOWN` | II | ✅ **resolvido (E6)** | medido 2026-11-15: solver 84,4% | — |
| `SOLVER_KERNEL_SCALAR` | II | ✅ **resolvido (E6b)** | 16 acumuladores safe: 7,44 → 17,10 GFLOP/s | — |
| `NO_ROOF_DUAL` | II | alto | E2b |
| `NO_LIMIT_CYCLE_GUARD` | II | alto | E1b |
| `NO_PROVENANCE` | II | alto | F1 → E3b |
| `D_CAPACITY_UNMEASURED` | II | médio | E1 |
| `NO_ACTUATOR_LOOP` | II | alto | E7 |
| `CERT_SEMANTICS` | II | médio | §6 (vocabulário) |
| `KB_TAXONOMY_UNDOC` | III | alto | este volume |
| `NO_LEDGER_BRIDGE` | III | alto | F1 |
| `TIMEBASE_MIX` | III | alto | §2.1 |
| `NO_REVISION_PATH` | III | alto | F2 (supersede) |
| `NO_DEGRADATION_LADDER` | III | médio | §2.2 (gist no L4) |
| `SCOPE_UNDEFINED` | III | alto | F3 |

### 11.3 Riscos

| # | Risco | Residual | Prioridade |
|---|---|---|---|
| R1 | Ising não é oráculo | médio | P1 |
| R2 | Interferência VSA | médio | P1 |
| R3 | Geometria ≠ verdade | **alto / irredutível em parte** | **P0** |
| R4 | Open-endedness sem corpo | médio | P2 |
| R5 | Custo de treino | baixo | P3 |
| R6 | Preprints | baixo | P3 |
| R7 | Drift para RAG | **alto** | P1 |
| R8 | Vazamento de escopo | médio | P2 |
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
7º  E2c-2 roof dual (max-flow)         ← alcance além da dominância
8º  E1b-2 memória de fase estruturada  ← separar padrão aleatório de harmônico
9º  F1   ponte read-only               ← fecha K6 sem risco de drift
10º E1/E2/E3  os três "NÃO"            ← o programa científico
11º E4/E5/E7/E8  self, currículo, corpo, treino
12º F2/F3  consolidação e fusão 2D
```

### 12.5 As sete perguntas — placar

| # | Pergunta | Status | Resposta / o que a fecharia |
|---|---|---|---|
| 1 | **Onde vão os 146 µs?** | ✅ **respondida** (E6) | **84,4% no solver** (133,1 de 157,7 µs). readout 7,0% · entropy 3,0% · wave 2,5% · actuator 1,7% · ring_copy 1,1%. Cobertura 99,98%. |
| 2 | **ΔE prevê acerto?** | ✅ **respondida** (E3b) | **Não em otimização livre** (AUC 0,46–0,55; bound 0,058 anti-preditivo). **Sim com restrições exatas**: 383 decisivas, precisão 100%. |
| 3 | **Quantos padrões cabem em D=160?** | ✅ **respondida** (E1b) | **64 fatos** (`FhrrStore`, K=6, recall@1 ≥ 99%); 128 já cai para 89%. Lei **linear ~0,4·D** — **não** D². Memória de fase (M5): ~**4** padrões aleatórios a erro < 0,15 rad (declarado 40). |
| 4 | **Roof dual reduz o suficiente em CNF de circuito?** | ⏳ **parcial** (E2b/E2c) | **CNF:** presolve por unidades+implicação dá **64,8%** (cascata +39,8 pp), 0 insound, 200/200. **QUBO:** dominância é sound (**0/600 ótimos alterados**) mas míope — **0% sem viés**, 42,9% com viés 45%. Falta o **roof dual completo (max-flow)**. |
| 5 | **O self-field melhora calibração ou só replica a surpresa?** | ⏳ **aberta** (E4) | Falsificável: melhora ECE de abstenção **sem** melhorar T01. Se T01 também melhorar, a hipótese "metacognição é separada" cai. |
| 6 | **Um atrator pode virar "gist" sem perder a habilidade?** | ⏳ **aberta** (F2) | Hoje o atrator é binário: está ou não está. Fecha consolidando no L4 (texto/energia) e medindo retenção após N consolidações. |
| 7 | **A ponte de fase preserva o suficiente?** | ⏳ **aberta** (F3) | Fecha com `cue_into` (ledger → vetor → semente de atrator) e medindo recall@1 no round-trip **vs** recall direto; gate: perda < 5%. |

**Placar: 3 respondidas, 1 parcial (E2b/E2c), 3 abertas — e cada aberta tem um
experimento que a fecha.** Nenhuma resposta foi inferida: as fechadas têm artefato
em `target/`.

> *"Três perguntas tinham resposta e não sabíamos; uma está parcialmente
> respondida; três continuam abertas — e agora sabemos exatamente como fechá-las."*

---

## 13. Roadmap F1–F3

| Fase | Entrega | Gate |
|---|---|---|
| **F1** (1 sem) | `KnowledgeSink` read-only: todo evento com `Certificate` vira linha no SGDB; nada volta | 0 chamadas no T0; `alloc_invariants` verde; R7 medido em 0 |
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
| `docs/API_CONTRACT.md` | contrato normativo (não alterado) |
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

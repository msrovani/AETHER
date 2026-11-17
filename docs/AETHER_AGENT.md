# AETHER Agent — organismo agêntico

> `src/agent/` + `aether-agent`. A camada que costura os especialistas do motor
> em um agente com **inferência, MoE, coerência e autoaprendizado** — sem LLM,
> sem tokenização, sem backpropagation.

**Posicionamento:** isto **não** é um chatbot nem um concorrente de LLM. É um
**coprocessador cognitivo**: memória auditável, solver com certificado e
abstenção calibrada. A suíte própria (ACQ 100) mede metas internas, não
inteligência geral. O produto é híbrido — `LanguagePort` (parser/template por
default; small LM/concept/remote LLM opcionais) na borda, AETHER no meio para
memória/verificação/decisão. Ver `docs/LANGUAGE_PORT.md` e
`docs/AETHER_AGENT_ROADMAP.md` (Tiers T0–T3, Trilha C).

## Premissas → implementação

| Premissa | Onde vive | O que é medido |
|---|---|---|
| **Inferência** | `TransplantExpert` (FHRR), `LogicExpert` (SAT), `SolverExpert` (QUBO), `PhaseExpert` | acerto/ação com certificado; abstenção no desconhecido |
| **Agêntica** | `AetherAgent::run`: percebe → roteia → infere → coere → age → observa | orçamento de passos, parada por evidência |
| **MoE** | `MoERouter` (EXP3), top-k especialistas | pesos convergem para o especialista certo |
| **Coerência** | `CoherenceEngine`: claims, contradição, matriz 2D | contradição ⇒ `abstain`; autoridade dominante ⇒ `cite` |
| **Autoaprendizado** | EXP3 por feedback + FHRR `add_fact` + imprint de fase + política JSON | fato novo vira resposta; pesos persistem |
| **Transplante** | `data/facts.jsonl` → FHRR | recall dos fatos transplantados |

## Arquitetura

```text
                    ┌──────────── TaskFeatures (contínuo) ────────────┐
Task (Fact/SAT/QUBO/Recall/Memory)
   │                                                            │
   ▼                                                            ▼
┌──────────────┐   top-k por probe × peso EXP3   ┌──────────────────────┐
│   Router     │ ──────────────────────────────► │      Experts         │
│  (MoE gate)  │                                 │ transplant/phase/    │
└──────────────┘                                 │ logic/solver/memory  │
   ▲                                             └──────────┬───────────┘
   │                                                        │ claims +
   │                                                        │ Certificate
   │                                             ┌──────────▼───────────┐
   │                                             │   CoherenceEngine    │
   │                                             │ autoridade × suporte │
   │                                             └──────────┬───────────┘
   │                                                        │
   │              Feedback (correct? expected?)             ▼
   └────────── learn ◄────────────────────────────── Action (act/cite/
                                                       propose/abstain)
```

Cada especialista devolve `ExpertOutput { claims, confidence, certificate,
detail }`. A coerência não vê "texto gerado": vê **afirmações tipadas** com
autoridade e proveniência.

## Especialistas

| Especialista | Base no motor | Tarefa | Aprendizado |
|---|---|---|---|
| `transplant` | `core::fhrr::FhrrStore` | `subject ⊗ relation → value` | novo fato verificado (`add_fact`) |
| `phase` | `ComplexPhaseMemory` | completar padrão de fase | `imprint` do padrão correto |
| `logic` | `LogicPipeline` | 3-SAT | — (determinístico) |
| `solver` | `ThermodynamicIsingSolver` | QUBO/Ising | — (determinístico) |
| `memory` | `neural-sgdb` | fatos textuais duráveis | — (fonte externa) |

### Transplante

`data/facts.jsonl` já contém `subject/relation/object`. O agente cria **fases
determinísticas** por símbolo (`symbol_phases`: FNV-1a 64 → xorshift → `[0,2π)`)
e grava `e ⊗ r ⊗ v` no FHRR com `traces` redundantes. Isso é um transplante
*lossy* e explícito: não é embedding semântico; é endereçamento por conteúdo.
`query_selective` devolve `None` quando a evidência do papel é fraca — é a
primeira linha de abstenção.

O corpus curado do próprio projeto vive em **`data/agent_facts.jsonl`**
(`AETHER|proibe|backpropagation`, `aether-agent|usa|EXP3`, …). Use
`aether-agent ask --facts data/agent_facts.jsonl --subject … --relation …`; o
gate `gate_memoriza_fatos_do_projeto` garante que doc e memória não divergem.

## MoE sem gradiente

Para cada tarefa, o probe de cada especialista é uma competência declarada
(`0..1`). O gate amostra até `top_k` especialistas:

```
score_i = w_i · (probe_i + 0.05)
p_i     = score_i / Σ score
```

Depois do feedback verificado:

```
w_i ← w_i · exp(γ · r / (K · p_i))      (EXP3, γ = 0.15)
```

Não há camada densa treinada, não há `requires_grad`, não há atenção. O que
aprende é **o roteamento**; os especialistas continuam determinísticos. A
política é persistida em JSON (`--policy`) e recarregada no boot.

## Coerência (matriz 2D)

1. Claims são agrupadas por `(sujeito, relação)`.
2. Se os valores divergem:
   - autoridade dominante (`≥ segunda + 64`) vence, mas a ação vira `cite`;
   - empate de autoridade → **contradição** → `abstain`.
3. O certificado usa `gap = suporte` como **proxy** para tarefas de
   conhecimento (não é energia de Ising; está documentado no código).
4. `decide(authority, gap_min=0.5)`:
   - `act` — autoridade ≥ 128 e suporte ≥ 0.5;
   - `cite` — autoridade alta, suporte baixo ou evidência contestada;
   - `propose` — coerente sem fonte forte;
   - `abstain` — sem suporte/contradição.

Para SAT/QUBO o certificado é o **real** (`energy`, `gap`, `violations`); para
FHRR/fase o `gap` é confiança, não mínimo local.

## Autoaprendizado

O agente só aprende com `Feedback`:

```rust
Feedback { correct: Some(true), expected: Some(Claim { .. }) }
```

* `correct = Some(true)` → reward 1.0; `Some(false)` → 0.0;
* sem rótulo → reward máximo 0.5 (coerência não é verdade);
* `TransplantExpert` grava o fato esperado no FHRR;
* `PhaseExpert` imprime o padrão correto;
* o router atualiza os pesos e persiste a política;
* não há “auto-elogio”: a própria saída nunca é rótulo.

## CLI

```bash
# inferência relacional
aether-agent ask --subject Brasil --relation capital
aether-agent ask --subject Atlântida --relation capital --json   # abstém (exit 1)

# registry + autoescolha de ferramenta (P1)
aether-agent tools
aether-agent goal --facts data/agent_facts.jsonl \
  --subject AETHER --relation proibe --json

# lógica / otimização
aether-agent sat target/instancia.dimacs
aether-agent solve target/qubo.json

# memória durável (SGDB)
aether-agent memory "gate F4" --sgdb mock

# aprendizado com feedback
aether-agent teach --subject Itália --relation capital --value Roma

# demonstração completa (inferência + SAT + QUBO + 5 fatos aprendidos)
aether-agent demo
```

Saída textual de `ask`:

```
ação: act | suporte 1.000 | consenso 1.00 | autoridade 200
resposta: Brasília (fonte transplant, confiança 1.000)
motivo: autoridade e suporte suficientes | passos 1 | especialistas transplant
certificado: E=0.0000 ΔE=1.0000 viol=0 consenso=1.00
```

## API HTTP

```bash
curl -s localhost:8787/v1/aether/agent -d '{"subject":"Brasil","relation":"capital"}'
curl -s localhost:8787/v1/aether/agent -d '{"n":3,"clauses":[[1,2,0],[-1,3,0]]}'
curl -s localhost:8787/v1/aether/agent -d '{"n":4,"j":[0,...],"h":[1,1,1,1]}'
```

### Workflow multi-passo (P2)

`Goal::pipeline(vec![...])` executa subgoals em sequência com blackboard de
claims (`ToolContext.claims`) e rollback de workflow se um passo falhar:

```rust
let workflow = Goal::pipeline(vec![fetch_goal, transform_goal, verify_goal]);
let outcome = agent.solve(&workflow);
// outcome.steps: fetch → transform → verify
// falha no passo 2: claims/effects desfeitos, action=Abstain
```

Gates: `tests/agent_plan_gate.rs` (encadeia/verifica, falha desfaz, OOD abstém).

Resposta: `action`, `answer`, `support`, `consensus`, `authority`,
`contradictions`, `steps`, `experts`, `rationale`, `certificate`.

## Medições (demo)

| Gate | Resultado |
|---|---|
| recall de fatos transplantados | 5/5 |
| abstenção no desconhecido | sim |
| SAT com certificado | `act`, `violations=0` |
| QUBO | `act`, `+++…` |
| fatos novos aprendidos | 5/5 |
| pesos MoE após feedback | transplant sobe acima dos demais |
| CLI em processo real | `tests/agent_cli.rs` |
| biblioteca | `tests/agent_gate.rs` (6 gates) |

```bash
cargo test --lib agent::
cargo test --test agent_gate
cargo test --test agent_cli
cargo test --features api --test api_agent
```

## O que **não** está pronto (honesto)

| Lacuna | Próximo passo |
|---|---|
| Entender linguagem natural | adaptador de borda (texto → `Task`), fora do núcleo; o motor não tokeniza — ver Trilha C |
| Transdutor semântico real (E3) | hoje a borda é hash; “gato/gata”/paráfrase → abstenção — Trilha B0 |
| Escala de memória | 205 átomos hoje; 10k/100k/1M exigem LSH/shard/SGDB — Trilha B |
| Benchmark externo (MMLU/GSM8K) | N/A enquanto não houver gerador; só Tier 3 |
| Ferramentas externas | `Tool` trait com efeitos (HTTP, arquivo, SGDB) e sandbox |
| Planejamento multi-passo | busca em árvore sobre `Task` + custo do certificado — ver `docs/AETHER_AGENT_ROADMAP.md` |
| Mundo/modelo preditivo | usar `reservoir`/`koopman_sindy` como especialista |
| MoE com milhares de especialistas | LSH no gate; hoje 5 especialistas |
| Segurança de aprendizado | quarentena de fatos novos, autoridade mínima, revisão HITL |
| Escala do transplante | HvBank binário + JL em vez de fases sintéticas |

O agente é **completo no ciclo** (percebe–infere–coere–age–aprende) e
**incompleto em ambição**: não é uma AGI, é um organismo determinístico com
memória, roteamento e abstenção. O que ele promete, ele mede.

Documentos irmãos: `docs/AETHER_AGENT_ROADMAP.md` (plano), `docs/ADR_AGENT.md`
(decisões) e `docs/LESSONS_AETHER_AGENT.md` (lições/receitas).

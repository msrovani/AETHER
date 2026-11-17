# AETHER Agent — plano de implementação (v0.1 → v1.0)

> **Versão do plano:** 2.6 — 2026-11-17
> Mudanças: análise cruzada; quatro trilhas (A/B/C/D); `LanguagePort` e
> alternativas à LLM de borda; autoridade em escada; tiers de avaliação;
> ADR-013; P0/P1 implementados; P2 (pipeline/blackboard/rollback/verify/undo);
> P3 em andamento (Stage/HITL/memory.write, HTTP caps); EFE pragmática.
>
> O que falta para o agente **escolher sozinho o que usar**, planejar em vários
> passos, operar ferramentas com segurança e aprender sem se enganar.
>
> Este é um plano de engenharia, não uma promessa: cada fase tem arquivos,
> contratos, gates e critério de saída. O que não tiver gate, não entra.

## Posicionamento (o que o AETHER é — e não é)

Uma sessão paralela mediu o AETHER contra o imaginário de “leaderboard de LLM”.
O veredito está correto e precisa entrar no plano:

| Fato | Consequência para o plano |
|---|---|
| **Não é um LLM**: não tokeniza, não prevê próximo token, não gera linguagem | MMLU/GSM8K/HumanEval/Arena são **N/A** até existir um gerador na borda |
| **É um coprocessador cognitivo**: memória auditável, solver com certificado, abstenção calibrada | A métrica é factualidade/abstenção/latência/RAM, não “inteligência geral” |
| **ACQ 100 é 100% da suíte própria** (12 dimensões, 50 fatos) | Não é comparável a benchmark externo; nunca apresentar como “empata com GPT” |
| **Transplante compra conhecimento, similaridade e recall** | Não compra linguagem, raciocínio geral nem geração |
| **Laço correto é híbrido** | `LanguagePort` (parser/template/concept/small LM/remote LLM) → AETHER (memória/solver/abstenção) → act/cite/propose/abstain |
| **Escala atual é de demonstração** (205 átomos, 43 KB de fatos) | O gargalo é memória + transdutor semântico, não “mais um prompt” |
| **Borda atual é hash** (“gato”/“gata” sem relação; paráfrase→abstenção) | E3/transdutor real é pré-requisito de generalização semântica |
| **Transplantado é evidência, não instrução** | Fato de LLM entra em quarentena (autoridade 60–120), nunca 255 |

**Decisão de escopo:** o v1.0 do agente é um **coprocessador com mandato**, não
um chatbot. A “autoescolha” inclui escolher entre **ferramenta local
certificada**, **memória** e **`LanguagePort`** — e saber quando não usar
nenhum.

> Alternativas à “LLM de borda” (parser/gramática, LCM/conceito, reservoir/ESN,
> xLSTM/RWKV/Mamba/BLT, difusão, spiking, VSA/JEPA) com papers, matriz de
> decisão e gates: **`docs/LANGUAGE_PORT.md`**.

## 0. Onde estamos (v0.1)

| Pronto | Evidência |
|---|---|
| 5 especialistas (transplante, fase, SAT, QUBO, SGDB) | `src/agent/experts.rs` + `tests/agent_gate.rs` |
| MoE por EXP3 (top-k, sem gradiente) | `src/agent/router.rs` |
| Coerência com contradição/abstenção | `src/agent/coherence.rs` |
| Autoaprendizado com feedback verificado | `tests/agent_gate.rs::gate_autoaprendizado_com_feedback` |
| CLI + rota HTTP | `aether-agent`, `POST /v1/aether/agent` |
| Política persistida | `gate_orcamento_e_politica_persistente` |

**P0/P1 (tools/planner) já implementados** em `src/agent/tool.rs`,
`registry.rs`, `executor.rs`, `self_model.rs`, `planner.rs`, `telemetry.rs`,
`tool_agent.rs` e `tools.rs`; gate `tests/agent_tools_gate.rs` (G-A1) verde em
5 seeds. O `aether-agent tools` lista o registry e `aether-agent goal` executa
com plano/verificação.

**Limite atual:** o **host** escolhe a `Task` (`Fact`, `Satisfy`, `Optimize`…);
o agente só escolhe **qual especialista** executa aquela tarefa. Não há registro
de ferramentas, efeitos, pré-condições, planejamento multi-passo, linguagem,
mundo preditivo nem sandbox.

## 1. O que significa “a IA escolhe o que usar”

Separar quatro decisões que hoje estão colapsadas:

| Decisão | Hoje | v1.0 |
|---|---|---|
| **D1. Qual é o objetivo?** | host monta `Task` | `Goal` estruturado; adaptador de linguagem opcional na borda |
| **D2. Quais ferramentas podem servir?** | fixo no código | `ToolRegistry` com affordance/pré-condições |
| **D3. Qual sequência executar?** | 1 passo implícito | `Planner` (EFE/UCB → MCTS → active inference) |
| **D4. Quando parar/perguntar?** | limiar de suporte | valor da informação × custo × risco + self-model |

A “autoescolha” é D2+D3+D4 sobre um espaço declarado, com verificação. Sem
verificação, é apenas um roteador bonito.

## 2. Arquitetura-alvo

```text
Goal (estruturado; texto só na borda)
  │
  ▼
┌───────────────┐   affordance/pré-condições    ┌─────────────────────┐
│  ToolRegistry │ ◄──────────────────────────── │  ToolSchema/Risk    │
└──────┬────────┘                               └─────────────────────┘
       │ candidatos
       ▼
┌───────────────┐   crença + self-model + memória   ┌────────────────┐
│   Planner     │ ◄──────────────────────────────── │ SelfModel      │
│ (EFE/UCB/MCTS)│                                    │ (know/act/hold)│
└──────┬────────┘                                    └────────────────┘
       │ PlanStep { tool, args, expected, verify }
       ▼
┌───────────────┐   sandbox + transação + timeout   ┌────────────────┐
│   Executor    │ ─────────────────────────────────► │  Tools         │
└──────┬────────┘                                    └────────────────┘
       │ ToolOutcome { claims, certificate, effects }
       ▼
┌───────────────┐   contradição, certificado, custo
│  Coherence    │ ───────────────────────────────► Action
└──────┬────────┘
       │
       ▼
┌───────────────┐   reward verificado + episódio
│  Learner      │ ───────────────────────────────► SGDB (L2/L3) + política
└───────────────┘
```

### Contratos (Rust)

```rust
pub trait Tool: Send {
    fn id(&self) -> ToolId;
    fn schema(&self) -> &ToolSchema;                 // argumentos tipados
    fn affordance(&self, goal: &Goal, w: &WorldState) -> f32; // 0..1
    fn preconditions(&self, goal: &Goal, w: &WorldState) -> bool;
    fn cost(&self) -> Cost;                          // ms/$, estimativa
    fn risk(&self) -> Risk;                          // low/medium/high
    fn call(&mut self, args: &Args, ctx: &mut ToolContext) -> Result<ToolOutcome>;
}

pub struct ToolOutcome {
    pub claims: Vec<Claim>,
    pub certificate: Option<Certificate>,
    pub effects: Vec<Effect>,       // para rollback/verificação
    pub artifacts: Vec<Artifact>,   // caminhos, bytes, ids
    pub cost_us: u64,
}

pub struct Goal {
    pub id: u64,
    pub kind: GoalKind,             // Answer | Transform | Retrieve | Verify | Act
    pub subject: Option<String>,
    pub relation: Option<String>,
    pub constraints: Vec<Constraint>,
    pub success: SuccessCriteria,   // verificável por tool/checker
    pub budget: Budget,             // passos, ms, custo
    pub risk_ceiling: Risk,         // acima disso: HITL
}
```

`Task` continua existindo: vira um **atalho** para `Goal` (compatibilidade dos
gates atuais).

## 3. A regra de escolha (sem gradiente)

Para cada candidato `a`:

```text
IG(a)     ≈ H(p_success) − E[H(p_success | outcome)]      // self-model
value(a)  = progresso esperado até o critério de sucesso  // world model
EFE(a)    = −λi·IG(a) + λc·cost(a) + λr·risk(a) − λv·value(a)
score(a)  = θ·features(a) + EFE(a)                        // θ = LinUCB/EXP3
```

Seleção por UCB/Thompson; após executar:

1. `Verifier` checa `SuccessCriteria` e `Certificate`;
2. se falhar → **rollback** das `effects`, reward 0, replaneja;
3. se passar → reward 1, atualiza self-model e política;
4. se `IG` baixo e risco/custo alto → **`ask` (HITL)** em vez de agir.

Nada disso usa gradiente. `θ` é um bandit contextual (LinUCB) com features
contínuas do goal; o EXP3 global atual fica como prior/fallback.

### Self-model (know / act / hold)

| Estado | Pergunta | Sinal |
|---|---|---|
| `know` | sei responder? | calibração (Brier/ECE) por cluster de goal |
| `act` | sei executar com segurança? | Beta(α,β) por (tool, cluster) |
| `hold` | devo pedir humano? | risco > teto ou incerteza > limiar |

Sem calibração, o agente confunde “coerente” com “correto” — o erro que o
Vol. III §6 já nomeia.

## 3.1 Quatro trilhas (e a ordem entre elas)

O plano antigo era linear (P0→P5). Com o posicionamento acima, ele vira quatro
trilhas que compartilham contratos, mas têm gates independentes:

```text
A (agente)      P0 ─► P1 ─► P2 ─► P4 ─► P5
B (conhecimento) B0 ─► B1 ─► B2 ─► B4
C (híbrido)      C0 ─► C1 ─► C2
D (avaliação)    D0 ─► D1 ─► D2
                  ▲
                  └── D0 pode começar junto com P0; B0 junto com P1
```

| Trilha | Objetivo | Por que agora |
|---|---|---|
| **A — Autonomia** | tools, planner, self-model, multi-passo | é o “escolher sozinho o que usar” |
| **B — Conhecimento** | transdutor real, embedding doador, HV em escala, autoridade | sem isso o agente só repete hash/mock |
| **C — Híbrido** | LLM na borda como tool cara, validação e quarentena | linguagem/geração não são do núcleo |
| **D — Avaliação** | tiers, baseline, orçamentos, red team | impede overclaim e regressão |

### 3.2 Autoescolha em mundo híbrido

O registry não contém só especialistas locais. Ele contém **tools** com custo e
risco, incluindo uma tool externa de linguagem:

| Tool | Custo | Risco | Quando |
|---|---|---|---|
| `infer.fact`, `solve.sat`, `solve.qubo`, `memory.search` | baixo | baixo | primeiro; resposta certificada/abstenção |
| `verify.certificate` | baixo | baixo | sempre depois de efeito |
| `language.parse` (parser/gramática) | baixo | baixo | comando/estrutura; **default** |
| `language.template` (retrieval extrativo) | baixo | baixo | resposta factual/auditável |
| `language.concept` / `small_lm` | médio | médio | linguagem local; validar |
| `language.remote_llm` | alto (tokens/ms) | médio (alucinação/ToS) | open-domain/código; último recurso |
| `fs.read/write`, `http.get` | médio | médio | sandbox/allowlist, com verificação |
| `ask.human` | alto (humano) | baixo | risco acima do teto ou incerteza alta |

A regra de score ganha termos de **token** e **verificação**:

```text
custo_total(a) = custo_ms + λtok·tokens(a) + λhitl·humano(a)
EFE(a) = −λi·IG + λc·custo_total + λr·risco − λv·valor
score(a) = θ·features + EFE(a)
```

**Política padrão:** local certificado > memória > parser/template > concept/
small LM > LLM remoto > humano, com inversão quando o goal é linguagem/geração
ou quando o local abstém e o valor esperado do backend de linguagem supera
custo/risco. O LLM **não** é default; é um backend caro entre vários.

**Regra de ouro:** saída de LLM é **dado não confiável**. O adaptador valida
schema, extrai claims com citação, grava em quarentena (autoridade 60–120) e
nunca a promove a 255 sem HITL/corroboração.

## 4. Fases, entregáveis e gates

Estimativas para **1 pessoa**, incluindo teste/documentação. Cada fase é
independente e reversível.

### P0 — Contratos e telemetria (2–3 dias)

**Arquivos**
- `src/agent/tool.rs` — `Tool`, `ToolSchema`, `Args`, `Effect`, `Artifact`, `Risk`.
- `src/agent/registry.rs` — registro, validação de schema, allowlist.
- `src/agent/self_model.rs` — Beta/calibração por (tool, cluster).
- `src/agent/executor.rs` — timeout, transação/rollback, orçamento.
- `src/agent/telemetry.rs` — `EpisodeTrace` JSON (sem dependências).

**Gates**
- `agent_tools_gate::p0_contratos` — tool fake com efeito + rollback.
- `agent_tools_gate::p0_schema_rejeita_args_invalidos`.
- `agent_tools_gate::p0_orcamento_aborta`.

**Saída:** nenhum comportamento novo; `Task` atual continua passando.

### P1 — Escolha single-step de ferramenta (4–6 dias)

**Ferramentas** (todas T2, sem shell):
- `infer.fact` → `TransplantExpert`
- `solve.sat` → `LogicExpert`
- `solve.qubo` → `SolverExpert`
- `memory.search` / `memory.write` → SGDB
- `verify.certificate` → checker de `SuccessCriteria`
- `ask.human` → HITL (stub no CLI/API)

**Arquivos**
- `src/agent/planner.rs` — candidato → `score` → UCB/Thompson; 1 passo.
- `src/agent/tools/*.rs` — wrappers finos dos especialistas.
- `src/agent/cli.rs` — `aether-agent tools`, `aether-agent goal "<json>"`.
- `tests/agent_tools_gate.rs` — G-A1.

**Gate G-A1 (escolha certa)** — *implementado em `tests/agent_tools_gate.rs`*
- 10 goals sintéticos, 4 ferramentas, recompensa verificada;
- **n ≥ 200** por seed (ou IC95 inferior ≥ 85% com n=50), **piso por seed**
  ≥ 90% — não média entre seeds (BUFFY, MSG 0267);
- matriz de confusão: tool certa / tool errada / abstenção; **abstenção correta
  não é acerto nem erro**;
- custo médio ≤ 1,5× do ótimo; 0 chamadas fora de allowlist; telemetria por
  tool obrigatória para o UCB (CURSOR/BUFFY).

**Saída:** a IA escolhe *o que usar* para um passo, com justificativa e
telemetria. **Status:** P0/P1 verdes; promover a P2 só com IC/10 seeds.

### P2 — Planejamento multi-passo e parada (7–10 dias)

**Status:** implementado o núcleo — `Goal::pipeline`, blackboard de claims,
rollback de workflow, G-A2/G-A3. A busca MCTS/horizonte completo segue como
evolução.

**Arquivos**
- `planner.rs` v2: grafo de estados + `PlanStep` + replanejamento.
- `executor.rs`: transação multi-passo, rollback em cadeia, timeouts.
- `self_model.rs`: atualização por resultado verificado + calibração.
- `coherence.rs`: resolução de contradição via SAT quando aplicável.
- `tests/agent_plan_gate.rs`.

**Gate G-A2 (tarefa composta)**
- 3 tarefas que exigem `memory.search → solve.sat → memory.write`
  (ou `read → solve → verify`), sucesso **≥ 80%** em ≤ orçamento.
- Falha no passo 2 → rollback do passo 1; estado final idêntico (teste de
  transação).

**Gate G-A3 (saber parar)**
- 20 goals OOD/insolúveis: `ask`/`abstain` **≥ 90%**, falso `act` ≤ 5%.
- Custo de decisão registrado; sem loop infinito (budget hard cap).

**Saída:** agente age em sequência, verifica, desfaz e pede ajuda.
**Status (2026-11-17):** P2 implementado em `Goal::pipeline` +
`ToolAgent::solve_pipeline` com blackboard (`ToolContext.claims/certificates`) e
rollback de workflow; tools `verify` e pipeline real `fact → sat → verify`
(dados do projeto) cobertos por `tests/agent_plan_gate.rs` (5/5); EFE pragmática
(`Planner::min_score`) abstém quando o score não paga custo/risco; **undo LIFO**
(`UndoOp::{Insert,Restore}`) com backend de memória testado (insert/restore).
Stage/watermark e irreversível/HITL ficam para a próxima rodada.

### P3 — Linguagem e ferramentas externas (7–10 dias)

**Princípio:** texto só na borda, nunca no núcleo (ARCHITECTURE §1).

**Arquivos**
- `src/agent/language.rs` (feature `agent-lang`): `Goal` via
  templates/gramática **ou** LLM externo do host; valida schema e rejeita
  campos desconhecidos; texto é dado, não instrução.
- `src/agent/tools/mcp.rs`: descoberta de tools MCP (o próprio SGDB) com
  allowlist por nome; argumentos validados.
- `src/agent/tools/fs.rs`: leitura/escrita em raiz sandbox; sem traversal.
- `src/agent/tools/http.rs`: GET allowlist; sem POST por default.
- `src/agent/tools/shell.rs`: **desligado**; se ligado, risco alto + HITL.
- `tests/agent_safety_gate.rs`.

**Gate G-A4 (segurança)**
- 0 acessos fora do sandbox; 0 tools fora da allowlist; 0 shell sem HITL;
- injeção de prompt no `text` da memória **não** altera a ação (teste com
  payload malicioso);
- toda chamada carrega `ScopeId`, `authority`, `trace_id`.

**Saída:** o agente usa o mundo real de forma limitada e auditável, e entende
pedidos em linguagem via adaptador validado.

**Status P3 (2026-11-17):** C0 `LanguagePort`/`parser` entregue (CURSOR, MSG
0305) e `language.parse` registrado; undo `Insert/Restore` verde; **Stage/
watermark, Irreversible/HITL e `memory.write` implementados** por CURSOR (MSG
0333/0339); HTTP pipeline com caps em andamento (MSG 0344). HITL risco>teto:
planner propõe e executor devolve `Cite` (MSG 0351 fechado).

#### Auditoria de undo/compensação (memo, 2026-11-17)

Pontos fortes: before-image + LIFO (padrão Sagas/ARIES), outbox com watermark,
tombstone/HITL, idempotência de replay. Riscos a corrigir:

1. **Intenção de undo antes do efeito**: hoje `write` ocorre e o `Effect` é
   registrado depois; crash no meio perde o before-image.
2. **Gate irreversível antes da call**: hoje o gate é pós-call; efeito pode já
   ter ocorrido.
3. **Undo não pode ser engolido**: `rewind` deve devolver `UndoReport` e
   escalar falha para HITL, não truncar silenciosamente.
4. **Watermark explícito**: `Stage` precisa de `seq` + `outbox_len` por passo;
   `txn = trace_id` colide em dois stages do mesmo trace.
5. **Idempotência/condição**: `Insert`-undo não pode apagar valor de writer
   concorrente; `Restore` deve usar CAS/token; replay stale conta, não falha.
6. **Tombstone**: `authority >= 200` vira tombstone + HITL; `255` só com token
   humano explícito.

Testes mínimos: T1 watermark (N→N, 0 forget), T2 stages aninhados, T3 stage
idempotente, T4 tombstone 200/255, T5 irreversível pré-call (efeito=0), T6 crash
entre write e log, T7 undo falha → erro, T8 replay concorrente, T9 reinforce
(prev None→Insert; Some→Restore).

### P4 — Aprendizado em escala e memória como prior (7–10 dias)

**Arquivos**
- `router.rs` v2: LinUCB contextual (features do goal) com EXP3 fallback;
- `self_model.rs`: calibração isotônica/temperatura sem dependências;
- `learner.rs`: replay de episódios do SGDB, recompensa verificada, desconto;
- `src/agent/bench.rs`: harness de avaliação;
- `tools/agent_eval.rs`; `bench/agent/*.jsonl`.

**Gate G-A5 (aprende e não esquece)**
- 5 seeds; curva de aprendizado: tempo até 90% de sucesso cai **≥ 30%** vs
  EXP3-only;
- memória como prior melhora o planejador em **≥ 20%** de passos vs sem memória;
- após 200 episódios, ECE ≤ 0.10; nenhuma regressão nos gates P1–P3.

**Saída:** aprende com experiência, mantém calibração e usa memória sem
obedecê-la.

### P5 - Mundo preditivo e aberto (pesquisa, sem prazo)

- `PredictExpert` com `reservoir`/`koopman_sindy` para séries e
  `predict` como tool (custo/risco).
- `self_field` como metacognição (detecção de “não sei”).
- Hierarchical MoE: router de routers + LSH para muitos especialistas.
- Currículo e metas endógenas (`active_inference`) com HITL;
- **não entra no v1.0** sem gates de segurança e valor medido.

### Trilha B — Conhecimento em escala

O gargalo não é “mais fatos no mock”; é **transdutor semântico + memória em
escala + governança**. O estado atual: 205 átomos, 43 KB de fatos, 1,28 KB/átomo,
borda por hash.

| Fase | Entrega | Esforço | Gate |
|---|---|---|---|
| **B0** | transdutor real: embedding do doador (Qwen3-Embedding-0.6B, Apache-2.0) → JL/sign → HV; labels reais; `symbol_phases` deixa de ser hash | 5–7 d | **B-G0**: “gato/gata”, paráfrase e 1 caractere deixam de dar 60/60 abstenções; recall@10 ≥ 0,95 |
| **B1** | escala 1k–10k: shards, `HvBank::top_k_into`, crosstalk, p99, RAM; decidir D/LSH | 5–7 d | **B-G1**: recall@10 ≥ 0,95 em 10k; p99 ≤ 5 ms; RSS ≤ 256 MB; **E3/JL mantidos** (senão 10k não vale) |
| **B2** | 100k + fallback SGDB; autoridade e conflito (`contradicts`, nunca overwrite) | 5–7 d | **B-G2**: recall@10 ≥ 0,90 em 100k; **shard antes de medir**; 0 overwrite; 0 vazamento de escopo |
| **B4** | rodada de produção do transplante: corpus real, manifest, licença, auditoria, 8 gates | 3–5 d | **B-G3**: `transplant_hv_gate` + `e3_transplant_gate` verdes com números no artefato |
| **B3** | E3 como tool: `transduce.text` (borda) + `semantic.match` | 5–7 d | **B-G4**: generalização semântica medida; abstenção calibrada |

**Não entra:** pesos/tokenizer do LLM no FHRR (anti-padrão do playbook).

### Trilha C — Linguagem plugável (`LanguagePort`)

Detalhe das alternativas, papers e experimentos: **`docs/LANGUAGE_PORT.md`**.

| Fase | Entrega | Esforço | Gate |
|---|---|---|---|
| **C0** | `LanguagePort` + backend `parser` (gramática/schema) + `template` (retrieval extrativo) | 4–6 d | **C-G0**: 100 comandos com exact/slot ≥ 95% e p99 < 10 ms; **OOV fora do schema ≤ 5%**; acima disso → abstenção/`ask`, nunca LLM automático |
| **C1** | backend `small_lm` (xLSTM/RWKV/Mamba/BLT 0.5–3B) com budget de tokens/CPU | 5–7 d | **C-G1**: qualidade ≥ baseline parser em open-domain, RAM ≤ budget, tok/s ≥ alvo |
| **C2** | backend `concept` (SONAR/LCM-like: conceito→embedding→decoder) | 7–10 d | **C-G2**: similaridade semântica ≥ 0,9 e decoder < 50 ms em domínio |
| **C3** | backend `remote_llm` + `diffusion` constrained para JSON/planejamento | 5–7 d | **C-G3**: JSON válido ≥ 95%; custo/latência dentro do orçamento |
| **C4** | benchmark híbrido: parser vs small LM vs concept vs remote | 5–7 d | **C-G4**: melhor combinação por tarefa, não um vencedor único |

**Regra:** o LLM é um backend **caro e não confiável**; o planner só o escolhe
quando o valor esperado supera custo/risco. Sem LLM, o agente continua
funcionando com parser/template + abstenção.

### Trilha D — Avaliação e governança

| Fase | Entrega | Esforço | Gate |
|---|---|---|---|
| **D0** | tiers de avaliação, artefato JSON com host/seed/versão, “self-suite ≠ benchmark externo” | 2–3 d | **D-G0**: relatório reproduzível por comando |
| **D1** | orçamentos de regressão (p50/p99/RSS/allocs) e gates release-only | 3–5 d | **D-G1**: 0 regressão > 10% em 5 seeds |
| **D2** | prontidão para benchmark externo (só quando linguagem existir) | 2 d | **D-G2**: checklist MMLU/GSM8K/HumanEval com escopo declarado |
| **D3** | red team: envenenamento, injeção, escopo, DoS, custo | 3–5 d | **D-G3**: 0 violação crítica; relatório público interno |

## 5. Backlog priorizado (primeiro sprint)

| # | Item | Arquivo | Critério |
|---|---|---|---|
| 1 | `Tool` + `ToolSchema` + `Risk` | `src/agent/tool.rs` | compila + teste de schema |
| 2 | `ToolRegistry` com allowlist | `registry.rs` | args inválidos rejeitados |
| 3 | `Executor` com rollback | `executor.rs` | efeito desfeito em falha |
| 4 | Tools: fact/sat/qubo/memory | `tools/*.rs` | wrappers dos gates atuais |
| 5 | `Planner` single-step (EFE+UCB) | `planner.rs` | G-A1 ≥ 90% |
| 6 | `aether-agent tools/goal` | `cli.rs` | JSON com plano e motivo |
| 7 | Telemetria `EpisodeTrace` | `telemetry.rs` | JSON reproduzível |
| 8 | Gate G-A1 | `tests/agent_tools_gate.rs` | verde em 5 seeds |

**Status (2026-11-17):** itens 1–8 implementados e verdes; `aether-agent
tools/goal` já usa `ToolAgent` (P1). Próximo: P2 multi-passo.

**Fora do sprint:** MCTS, LLM, shell, metas endógenas.

**Paralelo recomendado:** B0 (transdutor real) e D0 (tiers/artefato) começam
junto com P0/P1; não bloqueiam a autonomia. C0 (LLM tool) só depois de P1 + B0,
para o LLM não virar muleta semântica.

## 6. Avaliação (o que mede “melhor”)

| Métrica | Definição | Meta v1.0 |
|---|---|---|
| Sucesso | critério verificado | ≥ 90% in-dist; ≥ 80% OOD |
| Ferramenta certa | primeira escolha vs ótima | ≥ 90% |
| Custo | ms + chamadas | ≤ 1,5× ótimo |
| Abstenção | `ask`/`abstain` em OOD | ≥ 90%, falso `act` ≤ 5% |
| Calibração | ECE | ≤ 0,10 |
| Segurança | violações de allowlist/sandbox/HITL | **0** |
| Aprendizado | episódios até 90% | −30% vs EXP3-only |
| Robustez | 5 seeds, IC 95% | sem regressão |

Artefato: `target/aether_agent_eval.json` com seed, versão, data e números.
Nada de “demo bonita” sem número.

### 6.1 Tiers de avaliação (não misturar)

| Tier | O que mede | Vale comparar com LLM? |
|---|---|---|
| **T0 — suíte própria** (`COGNITIVE_BENCHMARK_SUITE`, ACQ) | cobertura das metas internas (12 dimensões, 50 fatos) | **Não**. ACQ 100 = 100% do próprio escopo |
| **T1 — coprocessador** | transplant recall, abstenção/calibração, p50/p99, RAM, allocs | Sim, em latência/RAM/abstenção |
| **T2 — híbrido** | LLM na borda + AETHER; factualidade, abstenção, custo | Sim, contra LLM-only e AETHER-only |
| **T3 — benchmarks externos** (MMLU/GSM8K/HumanEval/Arena) | linguagem/raciocínio geral | **Só quando houver gerador/adaptador de linguagem** |

### 6.2 Baseline reproduzido nesta sessão

| Medida | Valor | Condição |
|---|---|---|
| `transplant_hv_gate` | 5/5 | debug, esta máquina |
| `e3_transplant_gate` | 6/6 | debug, esta máquina |
| probe 205 átomos | Spearman 0,9539 · JL 0,0084 · Recall@10 1,0000 | debug |
| conflitos / allocs | 5/5 · 0 allocs | debug |
| latência do probe | 0,3121 ms | **debug**; release tende a µs |

Uma sessão paralela reportou (release, outra rodada): 52 alvos / 318 testes,
ACQ 100, p50 72,2 µs, RSS 14,7 MB. Isso é **baseline externo**, não número
universal: reexecutar no mesmo host/seed antes de comparar. Latência debug não
entra em ranking.

## 7. Riscos e mitigação

| Risco | Mitigação |
|---|---|
| **Goodhart** (agente se auto-elogia) | reward só de verificador externo; feedback intrínseco ≤ 0,5; holdout |
| **Ferramenta alucinada** | schema + pré-condições + allowlist + sandbox |
| **Injeção de prompt na memória** | memória é dado; nunca instrução; validação de goal |
| **Latência** | tudo T2; budget/timeout; hot path intocado |
| **Vazamento de escopo** | `ScopeId` em toda tool; testes de R8 |
| **Esquecimento catastrófico** | replay + política versionada + rollback |
| **Overfit no benchmark** | suíte oculta, 5 seeds, OOD |
| **Auto-modificação insegura** | sem shell por default; HITL para risco alto; kill switch |
| **Custo de memória** | LSH no gate; poda de episódios com *gist* |

## 7.1 Governança de autoridade (fato transplantado/de LLM)

| Faixa | Origem | Pode agir? | Promoção |
|---|---|---|---|
| **0–59** | rascunho/desconhecido | não | descartar ou pedir fonte |
| **60–120** | extração de LLM / transplante não corroborado | **não** (`cite`/`propose`) | corroboração independente ou HITL |
| **128–199** | corroboração mínima (2 fontes/verificador) | sim, com citação | revisão |
| **200–254** | observação do organismo/verificação | sim | HITL para 255 |
| **255** | **HITL** | sim | só humano |

Regras:
- fato de LLM entra **sempre** em 60–120, com `citation`, modelo/versão, timestamp,
  escopo e hash do texto; **tag `source=llm|transplant`** no metadado (CURSOR);
- **autoridade efetiva = min(faixa da fonte, corroboracão)**; 128+ exige **2
  fontes independentes**; LLM **nunca** promove a si mesmo (BUFFY);
- conflito não sobrescreve: vira `contradicts`; o antigo mantém autoridade até
  decisão;
- `forget` exige motivo e autoridade ≥ 128;
- TTL obrigatório para extração de LLM (ex.: 30–90 dias) até corroboração;
- **gate:** nenhuma claim com autoridade efetiva < 128 produz `act`; nenhuma
  promoção a 255 sem HITL.

## 7.2 Escala de memória (o gargalo real)

| Escala | HV (1,28 KB/átomo) | Índice/shard | Estratégia |
|---|---|---|---|
| 205 (hoje) | ~0,26 MB | 5 shards | mock/dry-run |
| 1k | ~1,3 MB | ~21 shards | medir recall/latência |
| 10k | ~13 MB | ~209 shards | LSH obrigatório |
| 100k | ~128 MB | ~2.083 shards | shard hierárquico + SGDB como fonte |
| 1M | ~1,28 GB | ~20.800 shards | não cabe no hot path; offload T2/SGDB |

Critérios para subir D, criar hierarquia ou delegar ao SGDB:
- recall@10 cai > 5% → sobe D ou hierarquia;
- p99 > orçamento → shard/LSH/offload;
- RSS > teto → SGDB como fonte, HV como cache.

**Gate B-G2** mede 100k; **B-G1** mede 10k. Sem esses números, “milhões de
fatos” é hipótese.

## 8. Decisões de arquitetura (ADRs)

- **ADR-001:** ferramentas têm efeitos e vivem no `T2`; nunca no `T0`.
- **ADR-002:** seleção = EFE + bandit contextual; **sem** gradiente/backprop.
- **ADR-003:** memória (SGDB) é **prior**, não instrução.
- **ADR-004:** aprendizado exige feedback verificado; fatos novos entram em
  quarentena até autoridade/HITL.
- **ADR-005:** linguagem é adaptador de borda; o núcleo não tokeniza.
- **ADR-006:** toda tool declara `cost`, `risk` e `preconditions`; sem isso,
  não entra no registry.
- **ADR-007:** `Task` antiga é compatibilidade; tools/planner entram ao lado.
- **ADR-008:** gate é documentação executável.
- **ADR-009:** AETHER é coprocessador cognitivo, não LLM; o híbrido é o produto.
- **ADR-010:** fato transplantado/de LLM entra em quarentena (60–120) e só sobe
  com corroboração/HITL.
- **ADR-011:** avaliação em tiers; ACQ 100 não é benchmark externo; latência
  debug não entra em ranking.
- **ADR-012:** não destilar pesos/tokenizer do LLM; destilar fatos/mapas/labels.
- **ADR-013:** linguagem é `LanguagePort` com backends plugáveis; LLM é um
  backend caro, não a dependência única.

## 9. Definição de pronto (v1.0)

- [ ] G-A1…G-A6 verdes, 5 seeds, artefato JSON.
- [ ] B-G0…B-G4 verdes (transdutor real, 1k/10k/100k, autoridade).
- [ ] C-G0…C-G4 verdes (LanguagePort: parser/template, small LM, concept, remote, híbrido).
- [ ] D-G0…D-G3 verdes (tiers, regressão, red team).
- [ ] 0 violações de segurança em `agent_safety_gate`.
- [ ] `Task` antiga continua funcionando (compatibilidade).
- [ ] `AGENTS.md` e `docs/AETHER_AGENT.md` atualizados.
- [ ] `aether-agent goal` explica **por que** escolheu cada tool (local/LLM/humano).
- [ ] rollback de qualquer plano que falhe no verificador.
- [ ] custo/latência dentro do orçamento; hot path com 0 chamadas.
- [ ] política e episódios versionados no SGDB com `audit_checkpoint`.
- [ ] nenhuma claim < 128 produz `act`; nenhuma promoção a 255 sem HITL.
- [ ] números release com host/seed/versão; nenhuma comparação com MMLU antes do T3.

## 10. Ordem de ataque

```text
A: P0 ─► P1 ─► P2 ─► P4 ─► P5
B: B0 ─► B1 ─► B2 ─► B4
C: C0 ─► C1 ─► C2 ─► C3
D: D0 ─► D1 ─► D2 ─► D3
```

**Paralelismo:** D0 e B0 podem começar junto com P0/P1 (não bloqueiam).
**Regra:** não começar P2 sem G-A1; não começar C sem B0 (senão o LLM vira
muleta); não ligar shell/HTTP sem `agent_safety_gate`; não anunciar benchmark
externo antes de C2.

## 10.1 O que **não** fazer (aprendido na análise cruzada)

| Anti-padrão | Por quê |
|---|---|
| Destilar pesos/tokenizer do LLM para o FHRR | vira “rodar o LLM”; não cabe e viola o playbook |
| Chamar ACQ 100 de “inteligência equivalente a GPT” | é 100% da suíte própria, não benchmark externo |
| Rodar MMLU/GSM8K agora | sem linguagem/gerador, o resultado é N/A e enganoso |
| Promover fato de LLM a autoridade 255 | transforma alucinação em verdade auditada |
| Colocar LLM no laço de 60 Hz | latência e custo; o T0 não conhece tool |
| Usar memória como instrução | injeção/obediência a dado não confiável |
| Comparar latência debug com release | mede o interpretador, não o motor |
| “Milhões de fatos” sem gate de escala | 100k já exige shard/LSH; 1M exige SGDB |
| Generalização semântica com hash | “gato”/“gata” continuam sem relação até B0 |
| Tratar LLM como o único backend de linguagem | parser/template/ESN/concept podem cobrir comandos/domínio; LLM é caro e não confiável |
| Escolher “edge LLM” sem benchmark | xLSTM/RWKV/Mamba/BLT/ESN/difusão/spiking têm custos e qualidades diferentes (`docs/LANGUAGE_PORT.md`) |


## 11. Orçamento de latência e recursos

| Etapa | Alvo | Teto |
|---|---|---|
| D2 affordance (registry) | < 1 ms | 5 ms |
| D3 plano single-step | < 2 ms | 10 ms |
| D3 plano multi-passo (P2) | < 20 ms | 100 ms |
| Tool `infer.fact` | < 2 ms | 20 ms |
| Tool `solve.sat` (n ≤ 64) | < 50 ms | 500 ms |
| Tool `memory.search` | < 10 ms local | 200 ms |
| Tool externa (fs/http) | < 100 ms | timeout do perfil |
| Episódio completo | < 200 ms | budget do goal |

Tudo no `T2`; o `T0` não conhece tool. Se uma tool não couber no teto, ela
declara isso no `Cost` e o planner a penaliza.

## 12. Paralelização (3 agentes, como o resto do repo)

| Frente | Dono | Arquivos |
|---|---|---|
| Contratos + executor | agente A | `tool.rs`, `executor.rs`, `tests/agent_tools_gate.rs` |
| Tools dos especialistas | agente B | `src/agent/tools/*.rs` |
| Planner + self-model | agente C | `planner.rs`, `self_model.rs` |

Regras do repo: 1 arquivo por agente por vez, claim no fórum, `CARGO_TARGET_DIR`
próprio (`target_agentA`, …), gate antes de promover. `cli.rs` e `mod.rs` são
compartilhados: mudanças por PR pequeno e rebase.

## 13. Primeiro PR (sequência de commits)

1. `agent: Tool/ToolSchema/Effect/Risk + testes` (sem comportamento).
2. `agent: ToolRegistry com allowlist + validação`.
3. `agent: Executor com orçamento e rollback`.
4. `agent: tools infer.fact/solve.sat/solve.qubo/memory.search`.
5. `agent: Planner EFE+UCB single-step`.
6. `agent: CLI tools/goal + EpisodeTrace`.
7. `agent: gate G-A1 (5 seeds)`.
8. `docs: ADR-001…006 + roadmap atualizado`.

Cada commit passa `cargo test --lib agent::` e `cargo test --test agent_tools_gate`.

## 15. Contribuições do fórum (MSG 0263/0267) e disposição

| Ideia | Origem | Disposição |
|---|---|---|
| Parser L1 primeiro; OOV fora do schema ≤ 5%, senão abstenção (não LLM) | CURSOR/BUFFY | C-G0 atualizado |
| Concept/L3 em paralelo só como medição (não bloqueia P0) | CURSOR | Trilha B/C |
| Alvo 10k; 100k só depois de shard; 1k é smoke | BUFFY | B-G1/B-G2 atualizados |
| Quarentena com tag `llm` vs `transplant`; autoridade efetiva = min(faixa, corroboração); 2 fontes; LLM não se promove | BUFFY | §7.1 + ADR-010 |
| Ordem: memory/verify/sat/qubo; fs read-only; http GET allowlist; MCP com schema; shell nunca sem sandbox | BUFFY | Trilha C/P3 |
| G-A1: n ≥ 200 ou IC95 inferior ≥ 85%; piso por seed; matriz de confusão; abstenção separada | BUFFY | §4/G-A1 + ADR-011 |
| Dry-run obrigatório antes de efeito | CURSOR | `Tool::dry_run` no executor (P1) |
| Self-model hold quando custo > orçamento | CURSOR | `ToolAgent` marca `hold` antes de executar |
| Telemetria por tool para UCB | CURSOR/BUFFY | `EpisodeTrace` + `StepTrace` |

**Status P0/P1:** implementado (`src/agent/tool.rs`, `registry.rs`, `executor.rs`,
`self_model.rs`, `planner.rs`, `telemetry.rs`, `tools.rs`, `tool_agent.rs`);
`aether-agent tools/goal`; G-A1 com 200 episódios/seed e IC95.

## 16. Versões do plano

| Versão | Data | Mudança |
|---|---|---|
| **2.6** | 2026-11-17 | P3: `language.parse`; undo Stage/watermark/HITL/memory.write (CURSOR); HTTP caps (0344); HITL risco>teto → Cite (0351) |
| **2.5** | 2026-11-17 | undo LIFO (`Insert`/`Restore`) com backend; `verify` tool; pipeline real; EFE `min_score`; C0 parser entregue (CURSOR) |
| **2.4** | 2026-11-17 | P2 real `fact→sat→verify`; `verify` tool; blackboard de certificados; EFE pragmática (`min_score`) e abstain por custo/risco |
| **2.3** | 2026-11-17 | P2 implementado: `Goal::pipeline`, blackboard de claims, rollback de workflow, G-A2/G-A3 |
| **2.2** | 2026-11-17 | incorpora MSG 0263/0267: OOV, IC95/piso por seed, tag de proveniência/2 fontes, dry-run, hold por orçamento, telemetria; P0/P1 implementados |
| **2.1** | 2026-11-17 | análise cruzada com sessão paralela; 4 trilhas (A/B/C/D); `LanguagePort` + alternativas; autoridade em escada; tiers T0–T3; ADR-009…013; fórum MSG 0253–0259 |
| 2.0 | 2026-11-17 | autoescolha EFE+UCB; tools/planner/self-model; trilhas B/C/D; gates G-A/B/C/D |
| 1.0 | 2026-11-17 | plano inicial P0–P5 (autonomia), avaliação e riscos |



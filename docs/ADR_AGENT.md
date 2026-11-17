# ADRs do AETHER Agent

> Decisões que sustentam `src/agent/` e o roadmap. Formato curto: contexto,
> decisão, status, consequências e alternativas descartadas.
>
> Status: **aceito** (direção), **implementado** (já no código) ou **proposto**
> (roadmap).

## ADR-001 — Ferramentas com efeito vivem no `T2`

**Contexto.** O ciclo do organismo roda a 60 Hz (~146 µs). Ferramenta externa
tem latência de milissegundos a segundos e pode bloquear.

**Decisão.** Nenhum `Tool`, I/O, `String` ou socket no `T0`. O `T0` só enfileira
eventos POD no anel SPSC; o agente/planejador roda em `T2`.

**Status.** Aceito (invariante já vale para o SGDB; implementação de tools é P1).

**Consequências.**
- O agente pode ser lento sem afetar o organismo.
- Teste `t0_ledger_calls == 0` permanece obrigatório.
- Tool nova precisa declarar orçamento e timeout.

**Alternativas.** Rodar tools no `T0` (descartado: jitter inaceitável); thread
por tool sem pool (descartado: custo e concorrência imprevisível).

## ADR-002 — Seleção por EFE + bandit, sem gradiente

**Contexto.** O motor proíbe backpropagation. MoE clássico treina o gate por
gradiente; não é opção.

**Decisão.** O gate combina competência declarada (`probe`), confiabilidade
aprendida (EXP3 hoje; LinUCB contextual no P4) e, no planejador, **expected free
energy** com custo/risco/valor da informação. Atualização só por recompensa
verificada.

**Status.** Implementado no single-step (EXP3); EFE/UCB é P2.

**Consequências.**
- O que aprende é o roteamento, não o especialista.
- Exploração é explícita (`γ`) e mensurável.
- Precisa de calibração para não confundir confiança com acerto.

**Alternativas.** Gate denso treinado (proibido); regras fixas (não aprende);
MCTS puro (caro sem prior).

## ADR-003 — Memória é prior, não instrução

**Contexto.** O SGDB guarda texto de várias fontes; parte pode ser obsoleta,
contraditória ou maliciosa.

**Decisão.** `recall`/`context` devolvem **evidência** com chave, autoridade e
score. O agente nunca executa `text` como comando; o planejador usa memória
como prior (episódios, ferramentas que funcionaram).

**Status.** Implementado (`context` + `rules[]`, `memory` expert).

**Consequências.**
- Injeção de prompt no banco não muda a ação.
- Fatos antigos precisam de autoridade/contradição para perder peso.
- Teste de injeção entra no `agent_safety_gate` (P3).

**Alternativas.** Injetar memória no “system prompt” (descartado: obediência a
dado não confiável); ignorar memória (perde aprendizado).

## ADR-004 — Aprendizado exige feedback verificado

**Contexto.** Autoavaliação sem ground truth produz Goodhart: o agente aprende a
parecer coerente, não a acertar.

**Decisão.** Reward só de `Feedback { correct, expected }` ou de verificador
externo. Sem rótulo, reward ≤ 0,5 (coerência não é verdade). Fato novo entra
com autoridade declarada e quarentena/HITL antes de virar conhecimento
compartilhado.

**Status.** Implementado no `learn` (reward 0,5 intrínseco; fato novo no FHRR).
Quarentena/HITL é P4.

**Consequências.**
- O agente pode ficar “burro” sem feedback — e isso é correto.
- Episódios precisam ser auditáveis (`EpisodeTrace`, SGDB L2/L3).
- Não há self-training.

**Alternativas.** Auto-reward por confiança (descartado); destilação
auto-supervisionada (fora do escopo).

## ADR-005 — Linguagem é adaptador de borda

**Contexto.** O projeto proíbe tokenização/`String` no núcleo. Usuários e apps
falam linguagem natural.

**Decisão.** Texto só na borda (`aether-agent` CLI, `POST /v1/aether/agent`,
feature `agent-lang`). O adaptador produz um `Goal` **validado por schema**;
texto desconhecido/ambíguo vira `ask`/`abstain`, nunca execução.

**Status.** Proposto (P3); hoje a CLI aceita `subject|relation` estruturado.

**Consequências.**
- O núcleo continua pós-token.
- O adaptador pode usar templates/gramática ou LLM externo do host; o núcleo
  não depende dele.
- Toda saída do adaptador é dado não confiável até validação.

**Alternativas.** Tokenizer no core (viola ARCHITECTURE §1); LLM como único
cérebro (não é o projeto).

## ADR-006 — Toda tool declara custo, risco e pré-condições

**Contexto.** Autoescolha sem contrato vira alucinação de ferramenta.

**Decisão.** `Tool` expõe `schema`, `affordance`, `preconditions`, `cost`, `risk`
e `call`. Sem esses campos, não entra no `ToolRegistry`. `risk >= high` exige
HITL; `shell` é desligado por default.

**Status.** Proposto (P1); o contrato está no roadmap.

**Consequências.**
- O planner penaliza caro/arriscado e evita tool inválida.
- O usuário vê por que uma tool foi escolhida.
- Tool nova precisa de teste de schema e de segurança.
- `dry_run` é obrigatório antes de qualquer efeito; o executor chama antes de
  `call`.
- Custo > orçamento ⇒ `hold` (não executa) — ideia do CURSOR (MSG 0263).

**Alternativas.** Registry aberto (inseguro); tool como string mágica
(impossível validar).

## ADR-007 — `Task` antiga é compatibilidade, não legado

**Contexto.** A v0.1 tem `Task::{Fact,Satisfy,Optimize,Recall,Memory}` e gates
apontando para ela.

**Decisão.** `Task` continua como atalho para `Goal`; tools e planner são
adicionados **ao lado**, não no lugar. Nenhum gate antigo pode quebrar.

**Status.** Implementado (o agente atual usa `Task`).

**Consequências.**
- Migração incremental e reversível.
- `aether-agent ask/sat/solve` continua funcionando.
- O planner pode tratar `Task` como um `Goal` já resolvido.

**Alternativas.** Reescrever tudo para `Goal` (risco de regressão); manter dois
mundos sem ponte (duplicação).

## ADR-008 — Gate é documentação executável

**Contexto.** Docs envelhecem; números sem seed/versão não são evidência.

**Decisão.** Cada premissa tem teste nomeado (`agent_gate`, `agent_cli`,
`api_agent`, `agent_tools_gate`) e artefato JSON quando há medição. Docs citam o
gate, não a intenção.

**Status.** Implementado.

**Consequências.**
- PR sem gate não entra.
- Benchmarks precisam de 5 seeds e versão.
- Gates de latência rodam em release; em debug são ruído.

**Alternativas.** “Testei na minha máquina” (descartado); demo sem métrica
(descartado).

## ADR-009 — AETHER é coprocessador cognitivo, não LLM

**Contexto.** Comparações com leaderboards de LLM (MMLU/GSM8K/Arena) tratam o
AETHER como um concorrente conversacional — o que ele não é. A suíte própria
(ACQ 100) mede metas internas, não inteligência geral.

**Decisão.** O produto é **híbrido**: LLM na borda (linguagem/extração/geração),
AETHER como memória auditável, solver com certificado e abstenção calibrada. O
benchmark do AETHER é factualidade/abstenção/latência/RAM; benchmark externo só
depois de existir linguagem (Tier 3).

**Status.** Aceito (posicionamento); implementação híbrida é Trilha C.

**Consequências.**
- Não competir em MMLU/GSM8K/Arena enquanto não houver gerador.
- Medir o que o AETHER faz melhor: recall, certificado, abstenção, custo.
- Toda doc/claim precisa declarar o tier de avaliação.

**Alternativas.** “AETHER como chatbot” (fora de escopo); “substituir LLM”
(impossível por essa via).

## ADR-010 — Autoridade em escada e quarentena

**Contexto.** Transplante de LLM gera fato plausível, não necessariamente
verdadeiro. Promover saída de LLM a 255 cria sistema confiantemente errado.

**Decisão.** Faixas: 0–59 descarte; 60–120 quarentena (LLM/transplante não
corroborado, só `cite`/`propose`); 128–199 corroboração mínima; 200–254
verificação; 255 HITL. Conflito vira `contradicts`, nunca overwrite. TTL para
extração de LLM até corroboração. **Tag `source=llm|transplant`** obrigatória;
**autoridade efetiva = min(faixa, corroboracão)**; 128+ exige **2 fontes
independentes**; LLM nunca se promove.

**Status.** Aceito; implementação na Trilha B/C.

**Consequências.**
- `remember` de LLM entra com autoridade baixa + citação/modelo/versão.
- Nenhuma claim < 128 produz `act`.
- Auditoria (`audit_checkpoint`) e `forget` com motivo.

**Alternativas.** Confiar no LLM (descartado); autoridade única global
(perde nuance).

## ADR-011 — Avaliação em tiers e escala com gate

**Contexto.** Números de máquinas/rodadas diferentes e “milhões de fatos” sem
medição viram mitologia.

**Decisão.** Tiers T0 (suíte própria), T1 (coprocessador), T2 (híbrido), T3
(benchmark externo). Escala B-G1 (10k) e B-G2 (100k) com recall/latência/RAM.
Latência só release, com host/seed/versão no artefato.

**Status.** Aceito; D0/D1 implementam.

**Consequências.**
- ACQ 100 não é comparável a MMLU.
- 100k exige shard/LSH; 1M exige SGDB como fonte.
- Regressão > 10% bloqueia.
- G-A1 exige **n ≥ 200** por seed (ou IC95 inferior ≥ 85% com n=50), piso por
  seed ≥ 90%, matriz de confusão (certa/errada/abstenção); abstenção correta
  não conta como acerto.

**Alternativas.** Benchmark único (engana); “funciona na minha máquina”
(descartado).

## ADR-012 — Destilar produto, não motor

**Contexto.** O desejo “transplantar o LLM” pode virar copiar pesos/tokenizer.

**Decisão.** Transplantam-se **fatos/triplas, embeddings via JL/sign, rótulos e
mapas**. Não se transplantam pesos, atenção, tokenizer nem geração. Para
linguagem, o LLM fica na borda como tool.

**Status.** Aceito (já é o playbook `docs/DOACAO_PLAYBOOK.md`).

**Consequências.**
- Ganho em conhecimento/similaridade/recall; nenhum em geração.
- O núcleo continua pós-token.
- E3/transdutor é o gargalo semântico, não “mais pesos”.

**Alternativas.** Embedding do modelo inteiro (não cabe); destilação de logits
para o FHRR (não é o mecanismo).

## ADR-013 — Linguagem é uma porta, não uma dependência

**Contexto.** “LLM de borda” foi um atalho; há alternativas melhores para
comandos (parser/gramática), para domínio/edge (reservoir/ESN) e para
conceito/texto (LCM/NCM). Nenhuma iguala open-domain/geração hoje, mas o AETHER
não deve ficar refém de um backend.

**Decisão.** `LanguagePort` com backends plugáveis: `parser` e `template` como
default; `concept`, `small_lm`, `remote_llm`, `spiking` opcionais. O planner
escolhe por EFE + custo + risco + privacidade. Nenhum backend no `T0`.

**Status.** Aceito; detalhes/papers em `docs/LANGUAGE_PORT.md`; implementação
na Trilha C.

**Consequências.**
- O agente funciona sem LLM (parser/template + abstenção).
- Open-domain/código continuam exigindo LLM (remoto ou pequeno).
- Concept/ESN podem ser pesquisados sem bloquear o produto.

**Alternativas.** LLM único (refém); parser só (sem open-domain); nuvem só
(privacidade/latência).

## ADR-014 — Undo registra a intenção antes do efeito

**Contexto.** Antes-image + LIFO é padrão (Sagas, ARIES). A implementação AETHER
ainda registra o `Effect` **depois** da escrita e checa irreversível **depois**
da call; um crash entre os dois perde o undo, e um efeito irreversível pode
acontecer antes do HITL.

**Decisão.** O executor deve: (1) `dry_run` declarar efeitos; (2) rejeitar
`Irreversible` quando `risk_ceiling < High` **antes** de `call`; (3) registrar
a intenção `(txn, seq, key, before-image)` antes da mutação, com a escrita e o
log atômicos no backend; (4) `rewind` devolver `UndoReport { stale, failed }`,
nunca engolir erro; (5) `Stage` carregar `seq` + `outbox_len`; (6) replay stale
não apaga valor de writer concorrente (CAS/token); (7) autoridade ≥200 vira
tombstone + HITL, 255 só com token humano.

**Status.** Aceito e parcialmente implementado (commit `3ecec5d`): `(2)` gate
pré-call via `Tool::planned_effects`, `(4)` `UndoReport{applied,stale,failed}`,
`(5)` `Stage{seq,outbox_len}` com `stage_rewind_to`. Abertos: `(1)` journal
durável antes da escrita, `(3)` atomicidade do backend, `(6)` CAS/token,
`(7)` tombstone/HITL. Referências: Sagas (SIGMOD 1987), ARIES (VLDB 1992),
arXiv:2609.15397, arXiv:2609.35366, arXiv:2602.14849.

**Consequências.**
- Crash-safe só com journal durável `(txn, seq)`; o `Vec` atual é protótipo.
- Undo falho bloqueia o workflow e escala para HITL, não “passa”.
- Testes T1–T9 do roadmap passam a ser obrigatórios.

**Alternativas.** Log depois do efeito (descartado: perde before-image);
compensação cega por chave (descartado: apaga valor de terceiro).

## ADR-015 — Autoridade é derivada de evidência, nunca do pedido

**Contexto.** A auditoria de 2026-11-17 encontrou vazamentos da escada de
autoridade: `memory.write` promovia fato em quarentena para 128 ao reescrever;
o expert de transplante carimbava 200 (`verified`) em fato apenas transplantado;
`remember`/`forget` da API aceitavam `authority` do corpo da requisição; e o
planner podia escolher `ask.human` (custo zero) no lugar de uma tool executável.

**Decisão.**

1. A escada é `AUTHORITY_QUARANTINE=96` (inferido), `AUTHORITY_CORROBORATED=128`
   (2+ fontes), `AUTHORITY_VERIFIED=200` (evidência física/certificado) e
   `AUTHORITY_HITL=255` (humano explícito). Constantes em `src/agent/tool.rs`.
2. Tool nunca promove autoridade sozinha: escrita nova entra em 96; escrita
   sobre existente mantém a autoridade e incrementa `reinforced`.
   **Exceção medida:** se outra fonte independente já registrou o *mesmo valor*
   (`Snapshot.provenance` diferente de `inferred`), a inferência concordante
   corrobora e sobe **96 → 128**; nunca 200.
3. `authority >= 200` só é sobrescrita/apagada com credencial HITL de servidor;
   corpo de requisição nunca concede autoridade.
4. Proveniência casa com o carimbo: `Transplanted` ⇒ 128; resultado de
   solver/verificação com certificado ⇒ 200 + `Observed`.
5. `ask.human` é tool de **escalação** (`Tool::escalation`): score fora do EFE,
   só aparece quando nenhuma tool executável paga o piso.
6. **200 exige certificado**: `verify` registra `VerifiedFact` (claim de solver
   + certificado com `violations=0`); `memory.write` só grava em 200 quando o
   fato está verificado. Corroboração (2 fontes) para em 128; 255 é humano.

**Status.** Implementado (`d3daf36`, `975ff0d`, `40af2e3`, `0.0.166`), incluindo
promoção por corroboração (`write_sourced` + proveniência) e por certificado
(`VerifiedFact`); parte de API em `199ade4`.

**Consequências.**
- Um fato inferido não "vira verdade" por reescrita; promoção exige
  corroboração (2 fontes) ou HITL.
- Clientes sem credencial HITL não escrevem/apagam 200/255.
- `ask.human` deixa de ser um atalho barato que vence tool real.

**Alternativas.** Confiar no `authority` do corpo (descartado: escalada
trivial); promover por contagem de reescritas (descartado: não é evidência
independente).



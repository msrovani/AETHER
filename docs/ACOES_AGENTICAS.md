# Ações agênticas autônomas — escada verificada

> Como o AETHER adquire **autonomia de ação** sem virar agente solto.
> Contrato: Goal tipado + tools allowlist + verificação + HITL por risco.
> Tomadas T-MOE/T-PLAN/T-FS: [`CORE_TOMADAS.md`](CORE_TOMADAS.md) ·
> Roadmap: [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) ·
> [`AETHER_AGENT.md`](AETHER_AGENT.md) · [`ADR_AGENT.md`](ADR_AGENT.md).

## 1. Posicionamento

Autonomia agêntica **não** é “fazer o que quiser”. São quatro decisões
separadas (`ROADMAP` §1):

| | Decisão | Autonomia = |
|---|---|---|
| D1 | Qual o objetivo? | `Goal` estruturado (texto só na borda) |
| D2 | Quais tools podem servir? | `ToolRegistry` + affordance + allowlist |
| D3 | Qual sequência? | `Planner` (EFE/UCB → pipeline → MCTS) |
| D4 | Quando parar/perguntar? | VoI × custo × risco + self-model |

Sem **verificação** (SuccessCriteria / certificado / rollback), é só roteador.

| Hoje (útil) | Meta “autônomo verificável” |
|---|---|
| P0–P2 verdes; P3 parcial | A4 mundo sandbox + A5 research-tool + A6 aprendizado |
| Chat pesquisa via hook F2 | research como **tool** do `ToolAgent` |
| Shell desligado | shell só com HITL explícito (se um dia ligar) |

Invariantes (inegociáveis):

1. `act` só com critério verificável e dentro do `budget` / `risk_ceiling`.
2. Fora da allowlist → não executa (bug, não “feature”).
3. Risco alto / efeito irreversível → HITL antes da `call`.
4. Fato de LLM/pesquisa → quarentena ≤120; 255 só HITL.
5. Núcleo não tokeniza; prosa ≠ autonomia (ver [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md)).

## 2. Escada de aquisição (ordem)

Não pular degraus de segurança: mundo real (A4) exige A0–A3 verdes.

| Passo | Entrega | Autonomia ganha | Gate / aceite |
|---|---|---|---|
| **A0** | `Goal` + `SuccessCriteria` + `Budget` + `risk_ceiling` + telemetria | age só no espaço declarado | P0 contratos |
| **A1** | single-step: escolher **1** tool (fact/sat/qubo/memory/verify/ask) | “sabe o que usar” | **G-A1** ≥90%/seed |
| **A2** | pipeline + blackboard + rollback em cadeia | “faz em sequência e desfaz” | **G-A2** ≥80% |
| **A3** | EFE / `min_score` + OOD → abstain/ask | “sabe parar” | **G-A3** falso act ≤5% |
| **A4** | fs sandbox + http GET allowlist + MCP allowlist; shell **off** | age no mundo, auditável | **G-A4** |
| **A5** | `research.*` tool (tier + independência + auth ≤120) | age com evidência externa | F5; waste=0; quarentena |
| **A6** | LinUCB contextual + replay + calibração (P4) | melhora sem esquecer | **G-A5** ECE ≤0.10 |
| **A7** | risco médio opcional HITL; alto **sempre** HITL | autonomia **calibrada** | safety + kill switch |
| **A8** | predict / metas endógenas (P5, pesquisa) | antecipa | só com valor medido |

Mapeamento roadmap: A0←P0 · A1←P1 · A2–A3←P2 · A4←P3/G-A4 · A5←F5 research ·
A6←P4 · A8←P5.

## 3. Estado 2026-11-21 (honestidade)

| Item | Estado |
|---|---|
| P0 / P1 / G-A1 | verde |
| P2 / G-A2 / G-A3 | verde (pipeline, undo, EFE pragmática) |
| P3 parser / undo SGDB / stage / HTTP caps / HITL | parcial–verde (ver tabela no ROADMAP) |
| G-A4 completo (fs+http+MCP no loop do agent) | **em aberto** no que falta integrar |
| Research no chat (`f2_chat_hook` + cascata) | vivo; **não** é ainda tool do planner |
| P4 / G-A5 | backlog |
| P5 mundo preditivo | pesquisa, fora do v1 sem gates |

## 4. O que cada passo *não* é

| Passo | Não é |
|---|---|
| A1 | planejar 10 passos sozinho |
| A2 | acesso irrestrito a disco/rede |
| A3 | “nunca pergunta ao humano” |
| A4 | shell / POST / exfil |
| A5 | verdade 255 na web |
| A6 | auto-modificar o próprio código |
| A7 | autonomia open-world total |
| A8 | AGI / metas sem HITL |

## 5. Ordem de implementação sugerida (produto)

1. Fechar **A4 / G-A4** — fs/http/MCP no `ToolAgent` com `agent_safety_gate`.
2. **A5** — registrar pesquisa externa como tool (mesmo contrato F2: opt-in rede,
   tier, independência, auth ≤120).
3. **A6 / P4** — LinUCB + episódios verificados (não treinar em reward fake).
4. Ampliar **A7** só com matriz de risco publicada e kill switch.
5. **A8** só após valor medido (não entra no v1 por default).

## 6. Critério de “temos ações autônomas”

Aceite de produto (autonomia **verificada**, não total):

- [ ] Em goals sintéticos do inventário, G-A1/G-A2/G-A3 verdes em artefato versionado.
- [ ] G-A4: 0 fora de sandbox/allowlist; 0 shell sem HITL; injeção em memória não altera ação.
- [ ] Toda `act` traz trace (tool, args, custo, claims) e passa no SuccessCriteria.
- [ ] OOD / risco alto → abstain ou `ask.human`, nunca false-act.
- [ ] Research (se ligada) persiste ≤120 e não promove 255.

“Autônomo total” open-world **não** é o alvo do v1.

## 7. Relação com prosa e pesquisa

| Capacidade | Doc | Papel na autonomia |
|---|---|---|
| Prosa fluente | [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | borda; não libera `act` sozinha |
| Coder | [`CODER_AETHER.md`](CODER_AETHER.md) | K0 exige A4; gerar na borda, verificar aqui |
| MoE + ETL pack | [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) | “download” = facts.jsonl; M8 precisa A4 |
| Pesquisa externa | [`F2_PESQUISA_EXTERNA.md`](F2_PESQUISA_EXTERNA.md) | evidência para A5 / corroborar pack |
| LanguagePort | [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) | D1 (texto→Goal), não D3 |

Fluxo alvo: texto → parser/LanguagePort → **Goal** → planner → tools →
coherence → act/cite/abstain → learner.

## 8. Referências internas

- [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) — P0–P5, trilhas A–D  
- [`AETHER_AGENT.md`](AETHER_AGENT.md) — MoE, coerência, CLI  
- [`ADR_AGENT.md`](ADR_AGENT.md) — decisões de autoridade e LanguagePort  
- [`LESSONS_AETHER_AGENT.md`](LESSONS_AETHER_AGENT.md) — invariantes  
- [`EVAL_AGENT.md`](EVAL_AGENT.md) — suíte T0  
- [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) — escada paralela (borda)  

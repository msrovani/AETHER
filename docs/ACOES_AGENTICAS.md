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
| Shell desligado | shell só com HITL explícito (K8: `AETHER_SHELL`+token; kill switch) |

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
| G-A4 completo (fs+http+MCP no loop do agent) | **verde** — `tests/agent_safety_gate.rs`; shell off |
| A5 `research.query` no planner | **verde** — `tests/agent_research_gate.rs` (5/5); rede opt-in; auth≤120 |
| Research no chat (`f2_chat_hook` + cascata) | vivo; agora também tool do planner (A5) |
| P4 / G-A5 / **A6** | **verde** — LinUCB opt-in + learner verificado; `agent_learn_gate` + `agent_a6_gate` + ECE self-model |
| **A7** matriz risco + kill | **verde** — `risk_policy.rs`; `AETHER_AGENT_KILL`; High always HITL; Medium opcional; `agent_a7_gate` |
| **A8** / P5 fio produto | **parcial-verde** — `PredictExpert` no MoE + `predict.series` + hub HITL + metacog; `agent_a8_gate` + `aether_a8_v1_criteria.json`; **enter_v1_default=false** até sanção HITL (sem MoE hierárquico) |

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

1. ~~Fechar **A4 / G-A4**~~ — feito (0.5.0).
2. ~~**A5** research tool~~ — feito (`research.query` / `research.expand` HITL).
3. ~~**A6 / P4**~~ — feito (`AETHER_LINUCB`, `learner.rs`, reward só com `Feedback.correct`).
4. ~~**A7**~~ — feito (matriz `RISK_MATRIX`, `AETHER_AGENT_KILL`, High always / Medium `AETHER_HITL_MEDIUM`).
5. ~~**A8** fio mínimo~~ — feito (`predict.series` read_local; `goal.propose` privileged HITL; metacog self_field; `agent_a8_gate`). **Não** entra no v1 por default; Hierarchical MoE/LSH continua pesquisa.

## 6. Critério de “temos ações autônomas”

Aceite de produto (autonomia **verificada**, não total):

- [x] Em goals sintéticos do inventário, G-A1/G-A2/G-A3 verdes em artefato versionado (`target/aether_accept_s6.json`, `agent_accept_s6_gate`).
- [x] G-A4: 0 fora de sandbox/allowlist; 0 shell sem HITL; injeção em memória não altera ação.
- [x] A5: `research.query` no allowlist; rede off não inventa; auth≤120; expand só HITL.
- [x] A6/P4: LinUCB opt-in no planner; learner JSONL; G-A5 ECE≤0.10; ≥30% vs EXP3 (`agent_learn_gate`).
- [x] A7: matriz publicada; kill switch; High sempre HITL; Medium opcional (`agent_a7_gate`).
- [x] A8 fio: `predict.series` + OOD/metacog abstain; `goal.propose` HITL; artefato `aether_a8_gate/a8` (`enter_v1_default=false`).
- [x] Toda `act` traz trace (tool, args, custo, claims) e passa no SuccessCriteria (`StepTrace` + `aether_accept_s6`).
- [x] OOD / risco alto → abstain ou HITL (`risk_policy` + executor), nunca false-act silencioso.
- [x] Research (se ligada) persiste ≤120 e não promove 255 (`research_persist_authority` + `memory.write`).

“Autônomo total” open-world **não** é o alvo do v1.

## 6.1 Matriz de risco (A7)

| Classe | Risco | HITL | Exemplos |
|---|---|---|---|
| `read_local` | low | never | fact, sat, qubo, code.check, verify, **predict.series** |
| `world_sandbox` | medium | optional (`AETHER_HITL_MEDIUM`) | fs.*, http.get, mcp, code.patch/generate/loop, research.query |
| `privileged` | high | **always** (token) | shell.exec, research.expand, ask.human, **goal.propose** |

Kill switch: `AETHER_AGENT_KILL=1` (vence teto e token). Token: `hitl_token` no
JSON de `goal.query` casando `AETHER_HITL_TOKEN`. Artefato:
`target/aether_risk_matrix.json` (`aether_risk_matrix/a7`).

## 7. Relação com prosa e pesquisa

| Capacidade | Doc | Papel na autonomia |
|---|---|---|
| Prosa fluente | [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | borda; não libera `act` sozinha |
| Coder | [`CODER_AETHER.md`](CODER_AETHER.md) | K0 exige A4; gerar na borda, verificar aqui |
| MoE + ETL pack | [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) | “download” = facts.jsonl; M8 precisa A4 |
| Visão | [`VISAO_AETHER.md`](VISAO_AETHER.md) | tool `sense.vision` (V8); precisa A4 p/ persistir frame |
| Speech | [`SPEECH_AETHER.md`](SPEECH_AETHER.md) | voz = borda D1; act igual; TTS pós-triagem |
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

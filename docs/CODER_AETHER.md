# Capacidade coder — escada híbrida (gerar na borda, verificar no AETHER)

> Como o AETHER adquire **capacidade de código** sem o núcleo gerar Rust/Python.
> Doutrina: open-domain/código ainda exigem LLM ou code LM na borda
> (`LANGUAGE_PORT` §4/§6, ADR-013). O AETHER **propõe critérios, aplica patch
> em sandbox, testa e abstem** — não é Cursor sozinho.
>
> Tomada T-CODE: [`CORE_TOMADAS.md`](CORE_TOMADAS.md) ·
> Paralelas: [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) (A4) ·
> [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) · [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) ·
> [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md).

## 1. Posicionamento

| Papel | Quem |
|---|---|
| Gerar diff / arquivo | `remote_llm` ou small **code LM** (`LanguagePort`) |
| Escolher tool / orçamento / risco | `ToolAgent` + planner (EFE/UCB) |
| Aplicar / rollback / testar | tools fs + verify + (opcional) shell HITL |
| Autoridade do patch | ≤120 até HITL; nunca 255 automático |

Invariantes:

1. Núcleo não tokeniza e não “aprende a programar” via Ising/FHRR.
2. Sem **G-A4** (sandbox) não há coder seguro.
3. Shell **off** por default; build/test com HITL se risco alto.
4. Proposal de código = evidência em quarentena; merge exige teste e/ou HITL.
5. HumanEval/Arena são **N/A** até existir gerador declarado (tier T3).

## 2. Escada K0→K8

| Passo | Entrega | Capacidade | Gate / aceite |
|---|---|---|---|
| **K0** | Autonomia A0–A3 + **A4** (fs sandbox, shell off) | lê/escreve só no chão permitido | **G-A4** |
| **K1** | Tools não-generativas: `check`/`test`/`fmt`/ast-grep/diff | vê e valida sem inventar | 0 escrita fora do sandbox |
| **K2** | Patch por **schema** (replace/insert/delete com âncora) | edita só com plano tipado | aplica ou aborta + rollback |
| **K3** | `code.generate` tool → LLM/code LM | propõe diff/arquivo | proposal auth ≤120; sem auto-merge |
| **K4** | Loop generate → worktree → test/check → keep/rollback | coder assistido | falso merge ≤5%; HITL se risco alto |
| **K5** | Grammar-constrained / RASP para JSON·AST de edição | menos prosa, mais estrutura | ≥95% patches parseáveis |
| **K6** | Small code LM local (budget GPU) | offline | pass@k subset + RAM no host |
| **K7** | Suite T3 (HumanEval ou própria) com gerador declarado | medir | D-G2; escopo no artefato |
| **K8** | Shell/build HITL + kill switch | build real | shell nunca default |

Dependência: K0 ← [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) A4. K3 ← trilha C
(`remote_llm` / small_lm). K7 ← `EVAL_AGENT` T3.

## 3. Estado 2026-11-21

| Item | Estado |
|---|---|
| Agent P0–P2 | verde |
| P3 / G-A4 (fs+http+MCP no loop) | parcial — **pré-requisito K0** |
| `code.generate` / patch schema | **não implementado** |
| Shell | desligado |
| Research F2 | evidência, não coder |
| Prosa / LanguagePort generate | escada P* em `PROSA_FLUENTE` — paralela |

## 4. O que cada passo *não* é

| Passo | Não é |
|---|---|
| K1 | gerar implementação |
| K2 | refatoração open-ended por chat |
| K3 | commit automático no main |
| K4 | “verde nos testes ⇒ 255” |
| K5 | garantia semântica (só sintática) |
| K6 | substituir revisão humana |
| K7 | anunciável sem gerador |
| K8 | shell livre = produto |

## 5. Ordem de implementação sugerida

1. Fechar **A4 / G-A4** (`ACOES_AGENTICAS`).
2. **K1 + K2** — AETHER como cirurgião de patch (maior valor / menor risco).
3. **K3 + K4** — ligar gerador como tool cara; loop com `cargo test` (ou equiv.).
4. **K5** se proposals soltas demais.
5. **K6** se privacidade/offline exigir.
6. **K7/K8** só com escopo e HITL explícitos.

## 6. Critério de “temos coder”

- [ ] G-A4 verde.
- [ ] Proposal → sandbox → teste → keep **ou** rollback; falso merge ≤5%.
- [ ] Toda proposal traz backend, custo, auth ≤120, `trace_id`.
- [ ] OOD / falha de teste → abstain; não inventa “quase passou”.
- [ ] Shell/build só com HITL quando ligados.

## 7. Referências para implementação futura

### Docs internos (ler nesta ordem)

| Ref | Uso |
|---|---|
| [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) | A4 sandbox, G-A4, shell/HITL |
| [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) | backends `remote_llm` / small_lm; §4 código |
| [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | `generate()` na borda; não confundir com coder |
| [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) | P3 tools fs/http/shell; T3 HumanEval |
| [`ADR_AGENT.md`](ADR_AGENT.md) | ADR-013; shell off; autoridade |
| [`EVAL_AGENT.md`](EVAL_AGENT.md) | tiers T0–T3 |
| [`LESSONS_AETHER_AGENT.md`](LESSONS_AETHER_AGENT.md) | ordem tools; sandbox |
| [`F2_PESQUISA_EXTERNA.md`](F2_PESQUISA_EXTERNA.md) | evidência externa ≠ patch |
| [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) | pack factual Rust/domínio **antes** de K3 generate |

### Fatos nsgdb (escopo `aether/project` / agent)

| Key / tema | Conteúdo útil |
|---|---|
| `md/L3/mcp/1794916915362016` | sem LLM: comandos/fatos; **código exige LLM** |
| `md/L3/mcp/1794916915362017` | não substitui: código arbitrário |
| `md/L3/mcp/1794918311479020` | ordem tools; shell nunca sem sandbox |
| `md/L3/mcp/1794916437468014` | trilhas A–D; llm.generate cara; T3 |
| `md/L3/mcp/1795281237748000` | DOC ACOES_AGENTICAS A0–A8 |
| `md/L3/mcp/1795281141596000` | DOC PROSA_FLUENTE P0–P6 |

### Entidades MOM para recall futuro

Ao implementar K*, recall com:

`mom/constraint`, `aether/agent`, `aether/roadmap`, `doc/language`,
`doc/architecture`, `mom/pattern` — e path em `src/agent/tools/**` se houver
triggers.

### Papers / externas (via LANGUAGE_PORT)

- Grammar Pruning / GRAMMAR-LLM / RASP — geração estruturada  
- BLT, xLSTM, RWKV, Mamba — small_lm  
- dInfer / HumanEval numbers — só referência de throughput, não gate interno  

## 8. Contrato de memória (aprendizado)

Fatos de implementação coder devem ser gravados com:

- `entity` / entities: `aether/agent`, `mom/fact` ou `mom/decision`
- autoridade agente ≤120; HITL 255 só para sanção de merge em main
- texto objetivo: passo K*, gate, artefato de teste, **não** o diff completo

Fim de ciclo K*: `curate(op=commit_run)` no nsgdb quando a tarefa for atômica.

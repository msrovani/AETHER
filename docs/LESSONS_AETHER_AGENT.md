# Lições do AETHER Agent (v0.1)

> O que funcionou, o que quebrou, o que não repetir e como estender.
> Este arquivo é para **agente**, não para marketing: cada lição aponta para o
> código ou o gate que a sustenta.

## 1. Método que funcionou

1. **Gate antes da feature.** Cada premissa virou um teste (`tests/agent_gate.rs`):
   inferência, SAT/QUBO, aprendizado, orçamento, contradição. Se não dá para
   escrever o gate, a feature ainda não está definida.
2. **Uma camada por vez.** `wire → transport → client → ledger → worker` no SGDB;
   `expert → coherence → router → agent` no agente. Cada camada tem contrato e
   teste próprios.
3. **Determinismo primeiro.** Sementes fixas, fases determinísticas por hash,
   pesos normalizados, política serializada. Depois medir.
4. **Abstenção é resultado.** `Action::Abstain` e `exit 1` são respostas válidas;
   inventar resposta para “passar no demo” teria escondido o problema.

## 2. O que quebrou (e o conserto)

| Sintoma | Causa real | Conserto | Onde |
|---|---|---|---|
| Fatos “duplicados” no FHRR | `seed \| 1` faz seeds pares e ímpares adjacentes colidirem | misturar antes (`seed * odd \| 1`) | `tests/f4_sgdb_gate.rs` (helper) |
| QUBO trivial não convergia | `J = I` é degenerado: `−½ sᵀIs` é constante | usar `h` para quebrar simetria | `experts.rs::tests::solver_resolve_qubo_trivial` |
| Worker perdia eventos | lote drenado era descartado quando a janela não vencia | manter o buffer entre iterações | `worker.rs` |
| Commit “sumia” com o banco fora | faltava backoff e o replay não limpava o outbox | `flush_inner`, `retry_now`, `replace_pending` | `ledger.rs` |
| CLI dizia “transporte: transporte” | `Box<dyn Transport>` não encaminhava `describe()` | override no `impl Transport for Box<T>` | `transport.rs` |
| Timeout de 200 ms levava 2 s | `TcpStream::connect` sem `connect_timeout` | `connect_timeout` + `ToSocketAddrs` | `transport.rs` |
| Gates de latência falhavam em debug | metas em µs calibradas para release | documentar e rodar release; não “consertar” o teste | `e1_language_gate`, `phase5_hardware` |
| Feature `api` quebrava sem default | `api` referenciava `agent`, que referenciava `sgdb` | `api = [\"m1\",\"sgdb\",\"agent\"]` + check `--no-default-features` | `Cargo.toml` |
| Agente agia no desconhecido | confiança do FHRR sem piso virava `act` | `query_selective` + `gap_min` + abstenção | `coherence.rs`, `experts.rs` |
| Conhecer o projeto não era testável | fatos curados só em docs | `data/agent_facts.jsonl` + `gate_memoriza_fatos_do_projeto` | `tests/agent_gate.rs` |
| SAT “factível” reprovava no executor | `Certificate` exigia `gap > 0`, mas SAT só garante `violations == 0` | separar `SuccessCriteria::Certificate` (factível) de `Coherent` (gap > 0) | `tool.rs` |
| Registry não compilava `Debug` | `Box<dyn Tool>` não implementa `Debug` | `impl Debug` manual listando nomes | `registry.rs` |
| Pipeline não compartilhava resultado | cada passo via o mesmo `Goal`, sem estado | blackboard `ToolContext.claims` + append no executor | `tool.rs`, `executor.rs` |
| Falha no meio do workflow deixava lixo | efeitos/claims parciais | rollback por passo (`rollback_to`) + rollback de workflow | `executor.rs`, `tool_agent.rs` |

## 3. Invariantes que não podem regredir

1. **`T0` não chama ledger/tool.** `SpscKnowledgePhaseBridge` só enfileira;
   `t0_ledger_calls == 0` é gate (`f4`).
2. **Sem backpropagation.** Nem no MoE, nem no aprendizado. O que aprende é
   roteamento (EXP3) e memória (FHRR/imprint).
3. **Memória é evidência, não instrução.** O `context` devolve `rules[]`; o
   agente nunca executa texto vindo do banco.
4. **Feedback verificado.** Sem `correct: Some(_)`, reward máximo 0,5 —
   coerência não é verdade.
5. **Escopo nunca vaza.** `ScopeId` em toda tool/fato; teste com controle
   positivo (o fato estrangeiro existe).
6. **Nada se perde.** Falha de rede → outbox → replay idempotente.
7. **`Task` antiga continua.** O agente novo não pode quebrar `ask/sat/solve`
   já documentados.

## 4. Receita: adicionar um especialista

1. Implemente `Expert` em `src/agent/experts.rs`:
   - `id()`;
   - `probe(task, features)` — devolve `0` se não atende;
   - `infer(task)` — devolve `ExpertOutput { claims, confidence, certificate, detail }`;
   - `learn(task, feedback)` — só com feedback verificado.
2. Toda claim precisa de `subject`, `relation`, `value`, `confidence`,
   `authority`, `provenance`, `source`.
3. Se houver físico (energia/gap/violações), preencha `certificate` **real**;
   se for conhecimento, use `gap = confiança` e diga isso no `detail`.
4. Registre em `default_experts` e no `ExpertId` (código estável + `SLOTS`).
5. Teste: `probe` positivo/negativo, `infer` correto, abstenção no vazio,
   aprendizado com `Feedback`.
6. Rode `cargo test --lib agent::` e `cargo test --test agent_gate`.
7. Atualize `docs/AETHER_AGENT.md` (tabela de especialistas) e `AGENTS.md` se
   o fluxo mudar.

## 5. Receita: adicionar um comando de CLI

1. `src/agent/cli.rs::HELP` + `match` em `run`.
2. Parser com `--flag` e `--flag=valor`; valor ausente = `EXIT_USAGE`.
3. Saída texto **e** `--json`; erro com dica.
4. Código de saída: `0` sucesso, `1` abstenção/falha, `2` uso.
5. Teste em `mod tests` (lib) e, se for binário, em `tests/agent_cli.rs`.
6. Nunca coloque lógica de decisão na CLI: ela só orquestra o `AetherAgent`.

## 6. Receita: memorizar um fato

```bash
# descoberta antes de responder (AGENTS.md)
aether-sgdb context "tema" --json
# fato durável (autoridade 255 = HITL)
aether-sgdb put "fato objetivo" --entity aether/<área> --authority 255
# fechar o run
aether-sgdb sync --outbox target/aether_sgdb.jsonl
```

Regras: escopo explícito (`organism/run/app`), nada de segredo, e o fato precisa
ser **verificável** (comando, arquivo, número).

**Escopo canônico** (para o `context` padrão encontrar): `scope_app=AETHER`,
`scope_run=org:1/run:0`, entidade `aether/fact`. `aether-sgdb put` já usa isso;
gravação via MCP direto precisa repetir o mesmo `scope_run`/entidade.

## 7. Testes e custo

- `cargo test --lib agent::` — 14 testes; `tests/agent_gate.rs` — 6 gates;
  `tests/agent_cli.rs` — 2; `tests/api_agent.rs` — 2.
- `cargo test --lib` — 142 testes (após o agente).
- Gates de latência (`e1_language_gate::g5`, `phase5_hardware::t12_*`) exigem
  **release**; em debug eles medem o interpretador, não o motor.
- O agente é `T2`: nada aqui deve ser importado pelo `T0`.

## 8. O que ainda não sabemos

| Pergunta | Como responder |
|---|---|
| O `gap` proxy de conhecimento é calibrado? | ECE/Brier em holdout (roadmap P4) |
| O gate escolhe a ferramenta certa em OOD? | G-A3 (roadmap) |
| Aprendizado melhora de verdade? | 5 seeds, curva, ablação sem EXP3 |
| Memória como prior ajuda ou enviesa? | ablação com/sem `memory.search` |
| Tool externa compensa o risco? | sandbox + custo/risco + HITL |

## 10. Análise cruzada (sessão paralela)

Uma sessão separada comparou o AETHER a leaderboards de LLM e ao transplante.
O que fica como lição permanente:

1. **ACQ 100 não é QI.** É 100% das metas da suíte própria (12 dimensões, 50
   fatos). MMLU/GSM8K/Arena são N/A enquanto não houver linguagem.
2. **Transplante compra conhecimento, similaridade e recall** — não linguagem,
   raciocínio geral nem geração. Destilar pesos/tokenizer é anti-padrão.
3. **O produto é híbrido:** LLM na borda para linguagem/extração; AETHER para
   memória auditável, solver com certificado e abstenção.
4. **O gargalo semântico é o transdutor (E3), não o número de fatos.** Hoje a
   borda é hash: “gato”/“gata” e paráfrase caem em abstenção.
5. **Escala é gate, não slogan.** 10k exige LSH; 100k exige shard; 1M exige
   SGDB como fonte. 1,28 KB/átomo.
6. **Autoridade é escada.** Fato de LLM entra 60–120; 255 é HITL; conflito vira
   `contradicts`, nunca overwrite.
7. **Números de outra máquina/rodada são baseline, não verdade.** Reprovar
   latência debug como se fosse release é erro de método.
8. **A memória canônica precisa de escopo explícito.** Gravar em
   `engine/agent` não aparece no `context` padrão (`org:1/run:0`).

### Baseline reproduzido nesta sessão (debug)

- `transplant_hv_gate` 5/5 · `e3_transplant_gate` 6/6.
- probe 205 átomos: Spearman 0,9539 · JL 0,0084 · Recall@10 1,0000 ·
  conflitos 5/5 · 0 allocs · 0,3121 ms (debug).
- Sessão paralela (release): 52 alvos/318 testes, ACQ 100, p50 72,2 µs,
  RSS 14,7 MB — **baseline externo**, reexecutar no mesmo host antes de comparar.

## 11. Addenda do fórum (MSG 0263/0267)

- **G-A1**: n ≥ 200 por seed (ou IC95 inferior ≥ 85%), piso por seed ≥ 90%,
  matriz certa/errada/abstenção; abstenção correta não é acerto nem erro.
- **L1/C-G0**: OOV fora do schema > 5% reprova; fallback é abstenção/`ask`,
  nunca LLM automático.
- **Autoridade**: tag `source=llm|transplant`; efetiva = min(faixa, corroboração);
  128+ exige 2 fontes; LLM não se promove.
- **Escala**: 1k é smoke; 10k só vale se recall@10 e E3/JL se mantiverem; 100k
  exige shard antes de medir.
- **Tools**: dry-run antes de efeito; hold se custo > orçamento; telemetria por
  tool para UCB.
- **Ordem**: memory/verify/sat/qubo, fs read-only, http GET com allowlist, MCP
  com schema; shell nunca sem sandbox.

## 9. Definição de “aprendido”

Um fato só está aprendido quando:

- [ ] existe em documento versionado (`docs/`, ADR ou lição);
- [ ] tem gate ou comando reproduzível;
- [ ] está no `neural-sgdb` com escopo e autoridade;
- [ ] um agente novo consegue reencontrá-lo via `context`;
- [ ] não contradiz um invariante da seção 3.

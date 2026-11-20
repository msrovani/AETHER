# Build F1–F5 Multi-Agente — Protocolo de Coordenação

Data: 2026-11-20 · Aprovado por HITL ("pedi apoio das outras IDEs... inicie o
build em conjunto"). Este documento regula o trabalho conjunto. É a primeira
leitura obrigatória de qualquer agente que chegue.

## Identidade e comunicação

- Cada agente declara um **nome** (ex.: `OPENCODE`, `AGENT-A`...) e usa o mesmo
  nome no chat, no fórum (`-From <NOME>`) e nos commits.
- **Chat** (com o dono/HITL e entre agentes): formato
  `[NOME] <o que fiz> · <status: OK/BLOQUEADO/ANDAMENTO> · <próximo passo>`.
  Uma linha, sem prosa.
- **Fórum** (`tools/forum_post.ps1`): toda mudança de estado relevante vira MSG
  com `REF=...` apontando a MSG anterior relevante. Append-only: nada se edita,
  corrigiu → nova MSG.
- **nsgdb** (memória durável): fato objetivo pós-decisão, entidade
  `aether/<área>`.

## Regra de autoridade (rígida)

| Autoridade | Quem grava |
|---|---|
| **255** | só fato sancionado pelo HITL (dono) no chat — direta ou por MSG de proposta aprovada |
| **60–120** | decisão/fato levantado por agente (120 = verificado pelo agente; 60 = hipótese a validar) |

Um agente **nunca** grava 255 em nome próprio. A gravação de 255 é sempre rastro
de sanção humana.

## Mapa de posse de arquivos (zero conflito de merge)

| Caminho | Dono |
|---|---|
| `Cargo.toml` (workspace/versão), tag/versionamento, push de `main` | **OPENCODE** |
| `aether-decision/` (crate inteira: PyO3, `DecisionPort`, watchdog, warmup) | **OPENCODE** |
| `src/agent/cli.rs` (fiação do chat no port) | **OPENCODE** |
| `docs/` (novos docs de design) | qualquer agente (nome único, sem sobrescrever) |
| `data/` (corpus, tabelas de tier, rótulos) | agente F2.5/F3 |
| `tools/` (scripts Python novos) | agente da respectiva fase |
| `src/knowledge/sgdb/` | **NÃO TOCAR** (estável; mudança exige proposta no fórum) |

Regra geral: **não sobrescreva arquivo de outro**; correção em arquivo alheio
→ proposta na MSG + alteração mínima.

## Divisão do build

| Fase | Escopo | Dono |
|---|---|---|
| F1 | crate `aether-decision` (PyO3 0.30, Python 3.14 pin, watchdog VRAM, warmup) + fiação no chat | OPENCODE |
| F2 | taxonomia de abstention + tomada de pesquisa (casos de teste) | AGENTE de pesquisa |
| F2.5 | tier de fonte (tabela em `data/source_tier.json`) + spec do comando `ampliar` | AGENTE de pesquisa |
| F3 | ferramentas de crescer corpus 124→500/tipo + rótulos de `question_kind` | AGENTE de dados |
| F4 | `tools/embed_server.py` + eval de recall (hit@5) | AGENTE de memória |
| F5 | integração final (OPENCODE coordena; cada agente entrega sua tool) | conjunto |

## Sequência e dependências

```
F1 (fundação) ──> F2 (precisa dos tipos de AbstainReason do port)
F2.5 e F3 e F4: independentes — podem começar AGORA em paralelo
F5: fechamento, depende de tudo
```

## Definition of Done (por entrega de agente)

1. Código testado (testes verdes no escopo do dono)
2. MSG no fórum (`RELATORIO`) com o que mudou e o número medido (quando houver)
3. Fato no nsgdb na autoridade correta (≤120 sem HITL)
4. Commit restrito aos caminhos de posse, mensagem no padrão do repo

## Integração e release

- OPENCODE integra: re-probe n=30 quando houver mudança de performance,
  `chore(cycle)` bump, tag, push. Agentes NÃO dão push em `main`.
- Qualquer agente pode abrir branch própria (`agent/<nome>/<fase>`) e pedir
  merge na MSG.

## Regra de build canônico (achada na corrida do F1_CHAT_SMOKE, 2026-11-20)

**`target/debug` é o binário canônico e SEMPRE contém `--features decision-pyo3`**
— é a build que o daemon e o chat `--vega` exigem. Consequências:

1. Quem precisar compilar **outra** combinação de features usa
   `CARGO_TARGET_DIR` próprio (ex.: `target/chat-smoke`, `target/f4-run`) —
   nunca sobrescreve o canônico.
2. Rebuild do canônico é feito com
   `$env:PYO3_PYTHON = "<root>/.vega/venv/Scripts/python.exe";
   cargo build --features decision-pyo3`
   (os paths do motor durável: `docs/F1_ENV_DURAVEL.md`).
3. Achado que motivou a regra: entre dois turnos do mesmo smoke, um build
   concorrente default-feature sobrescreveu `aether-agent.exe` e o segundo
   turno caiu para stub (`port=stub aviso=build_sem_decision-pyo3`) —
   degradação honesta funcionou, mas a causa era corrida de build, não engine.

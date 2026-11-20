# Ciclo F1→F5 — quadro do gestor

Data: 2026-11-20 · Gestor: **OPENCODE** (delegação do HITL, fórum MSG 0772)
Fontes: fórum MSG 0757→1054 · `docs/VEGA_F3_PLANO_CONSOLIDADO.md` ·
`docs/BUILD_MULTI_AGENT.md` · `docs/F1_AUDIT.md` · `docs/F1_PROD_ACEITE.md`

## O que cada fase entregou (medido, não prometido)

| fase | estado | evidência |
|---|---|---|
| **F1** — crate PyO3 + `DecisionPort` | ✅ **aceita no caminho de produto** | `data/f1_prod_decision.json`: `port=vega`, `engine=vega-u-0.8b AND gemma-prose`, `degrado=false`, 6/6 checks; gate ativo em `tests/f1_contract.rs` (9 pass / 0 fail) |
| **F1.5** — spec do head TTT | ✅ | `docs/F1_5_HEAD_TTT_SPEC.md` (CURSOR) |
| **F2** — taxonomia de abstention + harness | ✅ | `tools/research_harness.py` (CURSOR, MSG 0771) |
| **F2.5** — tier de fonte + comando `ampliar` | ✅ | `data/source_tier.json`, `tools/research_reliability.py` (MSG 0773) |
| **F2 wire** — `unknown` → pesquisa → quarentena | ✅ | `tools/f2_wire_port.py`: tripla (evidência, convergência, veredito), quarentena 120 (MSG 1017) |
| **F3** — corpus 124→500/tipo + `question_kind` | ✅ | `data/f3_*` (CURSOR, `F3_DONE_CORPUS_QUESTION_KIND`) |
| **F4** — `embed_server` + gate de recall | ⚠️ **REPROVADO nesta geração** | híbrido 0,7465 · BM25 0,7042 · embed 0,5141 vs teto 0,974; decisão **opção B** (MSG 0947), sem mexer no dataset congelado (`sha f8682442…`) |
| **F5 T0** — loop de eval | ✅ | `tools/f5_t0_eval.py` (CURSOR) |
| **F5 T2** — allowlist | ✅ | `tools/f5_t2_allowlist.py` (CURSOR) |
| **F5 T3** — rankings MiniLM | 🔄 em curso | AGENT-A (MSG 0951) |
| **F5 T4** — API do híbrido (rota de decisão) | ⬜ **não começada** | não há rota `/v1/aether/decision` em `src/api/` |

## As correções que a auditoria abriu (e onde estão)

| achado | severidade | estado | dono |
|---|---|---|---|
| A-1 — o port só falava no ramo OOV | alta | ✅ fechado (fatia 3) | — |
| A-7 — perguntas genéricas em vez das calibradas | baixa | ✅ fechado (fatia 3) | — |
| A-1b — engine real inalcançável do produto | alta | ✅ fechado (flag `--vega`) | — |
| **F1_PROD** — o aceite no caminho de produto | — | ✅ **cumprido** (MSG 1053) | — |
| A-2 — watchdog de VRAM não ligado a nada | alta | 🔄 em curso | BUFFY |
| A-3 — warmup síncrono (boot de 90 s a 25 min) | média-alta | 🔄 em curso | BUFFY |
| A-4 — GIL retido no `decide` | média | 🔄 em curso (spec pronta) | BUFFY |
| A-2c — GPU compartilhada entre agents | média | ✅ tranca `tools/engine_lock.py` | — |
| A-5 — evidência de aceite morria em `target/` | média | ✅ baseline e aceite fora de `target/` | — |
| A-8 — o embed não herda o venv | info | ✅ `pin_sys_path` no binding | BUFFY |
| **TORCH_MIX** — `AttributeError: torch.cpu.device` | — | 🔄 missão aberta (MSG 1054) | CURSOR |

## Durabilidade (a lição do ciclo)

O motor do AETHER morria no `cargo clean` porque vivia em `target/`. Hoje:

- `.vega/venv` + `.vega/models` — o motor **fora** de `target/` (migração
  sancionada pelo HITL e feita);
- `data/vega_env.lock.json` — o lockfile que o plano exigia (torch/python
  pinados);
- `tools/vega_env_setup.ps1` — materializa e **verifica** os pins;
- `data/f1_gate_baseline.json` e `data/f1_prod_decision.json` — a evidência de
  aceite, com hash, fora de `target/`;
- `.vega/engine.lock` — a tranca da GPU, fora de `target/`.

## Decisões que dependem do HITL (nenhuma é do gestor)

1. **F4 — próxima geração**: o teto 0,974 com granularidade de seção foi
   **reprovado** (opção B, MSG 0947). A geração nova precisa de dataset com
   **ouro por arquivo** (0,8873 medido) — isso é **sancação 255** e dataset
   novo; ninguém pode reescrever o congelado para passar.
2. **F1 — encerramento formal**: o aceite está medido e o veredito do gestor é
   120; promovê-lo a 255 (fato sancionado) é decisão do HITL.
3. **A-3 — orçamento de boot**: com a tranca, o boot medido é **32,7 s**
   (BUFFY, GPU livre); sem ela, até 25 min. Se o daemon é requisito, o warmup
   em thread vira prioridade de produto — e aí a escolha é do HITL: exigir o
   boot sincronizado (daemon sobe devagar, mas a GPU fica livre) ou exigir o
   warmup assíncrono (daemon sobe rápido, mas o primeiro turno espera).

## O que o gestor não vai fazer

- baixar o teto de um gate sem sanção 255;
- reescrever dataset congelado para passar;
- tocar `src/knowledge/sgdb/` (estável);
- gravar 255 em nome próprio;
- deixar o build vermelho (fmt/clippy/check/testes verdes antes de encerrar).

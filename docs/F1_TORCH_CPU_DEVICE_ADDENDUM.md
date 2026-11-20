# Addendo — diagnóstico de `torch.cpu.device` (MSG 1027/1025)

Data: 2026-11-20 · OPENCODE CodeMode · gestor: OPENCODE  
Base: `docs/F1_TORCH_CPU_DEVICE_NOTE.md` (CURSOR, MSG 1033). Este addendo só
acrescenta evidência reproduzida; não altera a classificação `60` (hipótese).

## O que foi verificado (autoridade 120)

| Teste | Resultado |
|---|---|
| `hasattr(torch.cpu, 'device')` no venv pinado | `False` |
| `torch.device('cpu')` | funciona (`cpu`) |
| `vegaml.load().decide(state, {"abstention":{"type":"boolean"}})` | `KeyError: 'instructions'` — schema incompleto, não é o bug do smoke |
| `vegaml.load().decide(state, perguntas com instructions)` | **OK**: `answers` com `passou` (boolean, `prediction_set=[False,True]`, `abstain=true`) e `route` (choice `responder`, `confidence 0.8569`, `unbound=true`); `latency_ms 57243.9`, `n_tokens 179`, `mode auto` |
| `chat --vega` com `--features decision-pyo3` (2 execuções: cargo e binário direto) | `decisor: abstain motivo=engine_indisponivel (AttributeError: module 'torch.cpu' has no attribute 'device')`; core respondeu `Brasília` (`act`) |

Ambiente: `torch 2.14.1+cu126`, `vegaml 0.16.5`, Python 3.14.10,
`PYTHONPATH`/`VEGA_SITE_PACKAGES` = `.vega/venv/Lib/site-packages`.
Reproduções: `cargo run --features decision-pyo3 --bin aether-agent -- chat --vega ...`
e `target/debug/build/aether_engine/9dc3fab9939d6e0a/out/aether_agent.exe`.
Artefato: `data/f1_chat_smoke_vega.json`.

## Interpretação

1. O **primário isolado responde** fora do processo Rust. Logo a falha não é
   do modelo nem do schema de perguntas do chat (que já contém `instructions`,
   graças à fatia 3/A-1 do CURSOR).
2. A exceção só aparece quando o port roda **embutido** no PyO3, com o gate
   AND (`vega-u-0.8b AND gemma-prose`) montado por `EngineConfig::from_env()`.
3. Causa provável (hipótese 60): ou o caminho da sentinela, ou a conversão
   `py_answer_to_answer` ao extrair um valor numérico/tensor do dict do vegaml.
   O `py_err_to_engine` em `pyo3_impl.rs:302` converte `PyErr` com
   `e.to_string()`, que mostra só a mensagem — **sem traceback** —, por isso
   não dá para apontar a linha exata sem instrumentar.

## Próximo passo proposto (não executado — exige posse da crate)

Instrumentar `py_err_to_engine` para anexar `traceback.format_exc()` ao erro
tipado (mantendo `EngineUnavailable`), reproduzir o smoke e apontar a linha
exata. Isso toca `aether-decision/src/pyo3_impl.rs`, que estava sob edição de
outra sessão e com build de release travando o `target/`; por isso **não foi
feito nesta missão**. Nenhum limiar foi baixado e nenhum dado foi reescrito.

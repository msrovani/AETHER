# Release 0.6.0 — frente A: T-TAB, T-EVAL e T-P2P

**Data:** 2026-11-24
**Âncora:** após 0.5.6 (fila de tomadas por dependência).
**Natureza:** código + gates + docs (fecha A1, A2 e A3 de `docs/PLANO_A1_D4.md`).

## O que muda

| Tema | Item | Entrega |
|---|---|---|
| **A1 T-TAB** | `src/agent/tab.rs` | CSV/TSV RFC 4180 → `Claim[]` (auth 96), `TabSchema` com hash FNV-1a, tool `tab`, CLI `aether-agent tab` |
| **A2 T-EVAL** | `src/agent/scorecard.rs` + `src/core/sha256.rs` | suite JSONL com `dataset_sha256`/`run_id`, placar T0–T3, CLI `aether-agent eval`, linha no hub |
| **A3 T-P2P** | `src/agent/p2p.rs` | `SgdbDelta{node,seq,vclock,entries}`, oplog append-only com hash chain + Merkle root, `Replica` multi-valor, conflito = `contradicts` |

Gates novos (vermelho→verde, com artefato em `target/`):

- `tests/agent_tab_gate.rs` → `target/aether_tab_gate.json` (7 checks).
- `tests/agent_eval_report_gate.rs` → `target/aether_agent_scorecard.json`
  (2 runs idênticos, `false_act=0`, T3 = N/A explícito).
- `tests/agent_p2p_gate.rs` → `target/aether_p2p.json` (6 checks; 2 nós com
  `state_root` igual, replay idempotente, conflito preservado).

`tools/cycle_gate.py` passa a rodar os três gates e ganha saída/leitura UTF-8
(no Windows o default cp1252 derrubava o script).

## Correções de causa

- `shell.exec` deixa de ser fallback genérico de qualquer `Act` (só com goal
  que cita "shell") — a suíte T0 estava 22/24.
- `planner` não escala acima do teto quando `risk_ceiling == Low`.
- `tests/agent_gate.rs`: ticket de pesos 5→6 (Predict) alinhado ao código.

## Compatibilidade

- Nenhuma dependência nova (`cargo check --all-targets --offline` limpo).
- `src/knowledge/sgdb/` e `src/core/persistency_search.rs` **intocados**.
- Limiares travados: auth ≤ 120, A3 recall ≥ 0,90, F4 hit@5 ≥ 0,974.
- Artefato do placar cognitivo (`target/aether_scorecard.json`) intocado.

## Evidência

`data/cycle_gate.json` (2026-11-24): `ok=true`, 9/9 checks — T0_eval,
MF2_regression, F1_offline_suite, F2_chat_hook, gates A1/A2/A3, lib
`aether_engine` e lib `aether_decision`.

## Como verificar

    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 check --all-targets --offline
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 test -p aether_engine --test agent_p2p_gate --offline
    python tools/cycle_gate.py > data/cycle_gate.json

## Próximo

A4 (T-CLIP) na fila; A5 Hier MoE em paralelo pós-B2. B1/B2 seguem exigindo
token HITL.

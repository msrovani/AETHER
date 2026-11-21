# Release 0.5.1 — T-CODE K1–K8 + A5 research

**Data:** 2026-11-21  
**Âncora:** escada coder completa após G-A4 (0.5.0); research no planner.

## O que esta versão é

Marco em que o **agent verifica e (com HITL) executa build** sob sandbox —
gerar fica na borda; merge/auth 255 nunca automático. T-CODE **K1–K8** vivo;
A5 `research.*` no allowlist.

| Degrau | Entrega | Gate |
|---|---|---|
| A5 | `research.query` / `research.expand` HITL; auth≤120 | `agent_research_gate` |
| K1 | `code.check` / `ast_grep` / `test` | `agent_coder_gate` |
| K2 | `PatchProposal` + `code.patch` + undo `FsWrite` | idem |
| K3 | `code.generate` opt-in (`AETHER_CODE_GENERATE`) | idem |
| K4 | `code.loop` keep\|rollback; keep≤120 | idem |
| K5 | `code.grammar` + RASP-lite; parse≥95% | idem |
| K6 | `code.lm` offline + `pass_at_k` | idem |
| K7 | `code.eval` / `coder_t3_pilot` + D-G2 | idem |
| K8 | `shell.exec` HITL + kill switch + allowlist | 17/17 + safety 9/9 |

## Non-goals mantidos

- Shell **off** por default; sem `sh -c` / `cmd /c`; kill switch
  (`AETHER_SHELL_KILL`) vence HITL.
- Auth coder/shell ≤120; 255 só HITL humano.
- HumanEval/Arena **N/A** (suíte própria `coder_t3_pilot`; exige `generator`).
- F4 congelada; sem STT/TTS/VIS no default.

## Como verificar

```bash
cargo test --features agent --test agent_coder_gate
cargo test --features agent --test agent_safety_gate
cargo test --features agent --test agent_research_gate
```

## Lições (resumo)

Ver [`LESSONS_AETHER_AGENT.md`](LESSONS_AETHER_AGENT.md) §2 addenda 0.5.1:
`Risk::High.penalty` mata score do planner; `escalation` −1000; env de opt-in
precisa de mutex nos gates.

## Docs

[`CODER_AETHER.md`](CODER_AETHER.md) · [`CORE_TOMADAS.md`](CORE_TOMADAS.md) ·
[`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) · [`EVAL_AGENT.md`](EVAL_AGENT.md) ·
[`CHANGELOG.md`](../CHANGELOG.md)

# Release 0.5.6 — fila de tomadas por dependência

**Data:** 2026-11-22  
**Âncora:** após 0.5.5 (T-DEC dual-bank + Gemma sentinela ∧ escriba).
**Natureza:** docs + versão (sem mudança de código).

## O que muda

A fila deixou de ser ordenada por horizonte (próximo/médio/distante) e passou
a ser ordenada por **dependência**: só código → sanção humana → backend
externo → hardware; dentro de cada classe, ordem crescente de pré-requisitos.

| Classe | O que precisa | Itens |
|---|---|---|
| **A** — só código | contratos vivos, SGDB local, gates | A1 T-TAB · A2 T-EVAL · A3 T-P2P · A4 T-CLIP · A5 Hier MoE (paralelo) |
| **B** — sanção humana | código pronto + evidência + token | B1 A8 `enter_v1` · B2 cutover T-DEC · B3 nova geração F4 |
| **C** — backend externo | sidecar / modelo allowlisted | C1 T-OCR · C2 T-STT · C3 T-VIS · C4 T-TTS |
| **D** — hardware | host / sensor / atuador | D1 T-SENSOR · D2 T-HAPTIC · D3 T-AR · D4 T-ROBOT |

Mudança de ordem que importa: **T-OCR saiu da frente**. Ele depende de
sidecar externo e de A1 (T-TAB) para tabelas, então vem depois da frente
só-código (T-TAB → T-EVAL). Sanções (A8, cutover T-DEC) vêm antes da nova
geração F4 (3 deps).

## O que NÃO muda

- Fio, disjuntores, filtros e DB: `CORE_TOMADAS` §2.5/§3. Classe muda
  **quando** a tomada entra, nunca **como**.
- Nenhum código, gate, feature flag ou dataset tocado.
- T-HITL/T-MCP/T-EMB/T-DEC/T-PRED seguem como vivos (inventário inalterado).

## Arquivos

- `docs/TOMADAS_FUTURO.md` — reescrito por classes, fase a fase.
- `docs/CORE_TOMADAS.md` — cabeçalho + §9 “Próximos plugues” por dependência.
- `docs/BUFFY_LEARNINGS.md` — §4–§6 (fila, truncamento de heredoc, ciclo).

## Como verificar

```bash
python tools/cycle_gate.py        # fmt/clippy/test_lib/decision/F1/suite/lock
git diff --stat 7a70ed3..HEAD -- src tests   # deve ser vazio
```

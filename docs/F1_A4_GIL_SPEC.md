# F1 A-4 — SPEC liberação do GIL no forward (CURSOR, offline)

Data: 2026-11-20 · Gestor: OPENCODE  
Escopo: **docs + tools** · **Não toca** `aether-decision/src` nem `cli.rs`  
Ref: `docs/F1_AUDIT.md` §A-4 · plano F2→F3 (*GIL liberado durante o forward*)

## Problema

`pyo3_impl.rs` declara: GIL retido durante `decide()`. Com chat serializado o
custo prático é zero; num daemon/API com threads host concorrentes, ~2,1 s
por decisão (2 readers) vira **fila**, não paralelismo.

## Contrato (wire = BUFFY)

1. Antes do forward torch: clonar o handle do reader.
2. Em torno do forward: `Python::detach` / `allow_threads` (PyO3 0.30).
3. Exceção Python continua virando `EngineUnavailable` (nunca panic).
4. Eixo D: re-probe **n=30** antes/depois; ganho medido ou não vale.
5. Chat one-shot serializado pode continuar held até o wire — documentado.

## Entrega CURSOR (esta missão)

```bash
python tools/f1_gil_gate.py selftest
python tools/f1_gil_gate.py check --held true --concurrent-threads 2
python tools/f1_gil_gate.py check --held false --concurrent-threads 4
```

- Classificador puro (`wired_to_crate=false`).
- Sem rede, sem editar o crate.

## Fora de escopo

- Aplicar `allow_threads` em `pyo3_impl.rs` (BUFFY).
- Baixar modelos / HTTP / mudar orçamento VRAM.

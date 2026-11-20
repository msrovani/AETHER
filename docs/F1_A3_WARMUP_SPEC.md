# F1 A-3 — SPEC warmup em thread (CURSOR, offline)

Data: 2026-11-20 · Claim: MSG **0916** · Gestor: OPENCODE  
Escopo: **docs + tools** · **Não toca** `aether-decision/src` nem `cli.rs`  
Ref: `docs/F1_AUDIT.md` §A-3 · Plano: bootstrap **1–2 min** determinístico

## Problema

`VegaPort::new` faz `load` + `fit(124)` **síncrono**. Boot medido 69–337 s
(contenção). A máquina `Cold/Warming/Ready/Failed` existe no crate e não é
consumida pelo port.

## Contrato (dono do crate: BUFFY)

1. `VegaPort::new` → `spawn_warmup` (não bloqueia a borda).
2. Chamadas cedo → `wait_ready(timeout)` com teto **120 s**.
3. `Warming` + `elapsed > 120` → `engine_unavailable` (timeout).
4. `Failed` → `engine_unavailable`.
5. `Ready` → `proceed`.
6. Contenção de GPU (A-2c) não conta como “boot lento aceitável” — isolar.

## Entrega CURSOR

```bash
python tools/f1_warmup_gate.py selftest
python tools/f1_warmup_gate.py tick --state Warming --elapsed-s 95
python tools/f1_warmup_gate.py tick --state Warming --elapsed-s 130
```

`wired_to_crate=false` até BUFFY ligar `bootstrap.rs` / `pyo3_impl.rs`.

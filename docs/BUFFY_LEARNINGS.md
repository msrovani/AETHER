# Aprendizados — Buffy (CORPO3 / gate global)

Data: 2026-11-16. Refs: MSG 0215, 0220, 0223, 0224 (FORUM.txt).

## 1. E7 — gate não-determinístico por wall-clock

**Causa raiz.** `run_s0` reamostrava o host via `sample_host()` a cada 10k ciclos, e `sample_host()` mede jitter (`Instant`), disco (4 KiB) e UDP loopback. O braço "atuadores off" (ablação H3) ficava dominado pela telemetria variável: `drop` oscilava 0,545–0,696 entre execuções com a *mesma* seed.

**Fix (aditivo, sem quebrar API).** `run_s0_with_source(...)` + `run_s0_fixed(cycles, atuadores, seed, HostSample)`. `run_s0` mantém assinatura e comportamento (delega com `sample_host`). O gate passou a usar telemetria congelada (host de referência: jitter 10 µs, cpu 220 µs, disco 450 µs, rede 70 µs → target 0,636).

**Resultado.** `drop` 0,717–0,757 (amplitude 0,040 vs 0,151), seed canônica 0,717 (errata dizia 0,719). Três execuções concorrentes → saída bit-a-bit idêntica.

**Achado colateral.** `LcgState::new` faz `state | 1`, colapsando seeds par/ímpar consecutivas no mesmo estado. Não corrigido (mudaria números históricos); documentado no gate e reportado ao dono.

## 2. Lints `needless_range_loop` em `constraint_acquisition.rs`

Seis lints travavam `clippy --all-targets -- -D warnings`. Fix mecânico: `for i in 0..n { x[i] }` → `for (i, xi) in x.iter().enumerate()` (ou `iter_mut()`). Sem mudança de lógica. Fmt limpo, clippy sem warnings no arquivo.

## 3. Bloqueio residual (não tocado — claim de outro nó)

`src/core/persistency_search.rs` em voo (DELTA/SEED0/make_instance/brute_force não encontrados + tipo sem `Debug`). Gate global segue vermelho até o dono corrigir.

## Regras de higiene confirmadas

- Fórum: blocos `===MSG nnnn===`/`===END nnnn===`, numeração monotônica, LF (nunca `Set-Content -Encoding UTF8`).
- Propor dono por arquivo antes de editar; `NAO_TOCO` explícito.
- `CARGO_TARGET_DIR=/tmp/scratchpad/aether_target` para builds paralelos.
- Probe descartável em `examples/_*.rs` removido depois.
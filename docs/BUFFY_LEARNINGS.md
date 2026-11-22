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
## 4. Fila de tomadas por dependência (2026-11-22)

- Horizonte (próximo/médio/distante) não serve como ordem de execução: T-OCR
  estava em "médio" mas é o **mais** dependente da fila (sidecar externo +
  T-TAB para tabelas). Reordenado em classes: **A** só código → **B** sanção
  humana → **C** backend/modelo externo → **D** hardware; dentro da classe,
  ordem crescente de pré-requisitos.
- Consequência prática: T-TAB (0 deps) abre a fila; T-EVAL usa suas fixtures;
  T-OCR vira C1 depois de A1. Sanção A8/cutover T-DEC vêm antes de F4 nova
  geração (3 deps). T-AR (2 deps) antes de T-ROBOT (3 deps + HITL duro).
- Regra que ficou escrita: classe muda **quando** a tomada entra, nunca
  **como** (fio/disjuntor/filtros/DB continuam os do CORE_TOMADAS §2.5/§3).

## 5. Limite de comando do agente (truncamento de heredoc)

- Um `cat > arquivo <<'EOF'` com ~11 KB foi **truncado no meio** pelo runner:
  o bash emitiu `here-document delimited by end-of-file` e o arquivo ficou
  parcial (168 de ~250 linhas) porque a redireção trunca **antes** de falhar.
- Fix: regravar em blocos de ~2–4 KB (`cat >>`) e conferir `wc -l` +
  `grep '^## '` depois. Sempre verificar o arquivo após heredoc grande.
- `write_file`/`read_files`/`apply_patch` são ancorados no workspace do
  Freebuff; este repo vive em `C:\Users\msrov\Documents\AETHER`, então o
  caminho é `run_terminal_command` + `cd`, não as ferramentas de arquivo.

## 6. Ritual de versão (`tools/cycle.ps1`)

- Só faz **bump** quando há mudança em `src/`/`tests/` **e** gate ok; mudança
  só de docs vira `wip(cycle)` sem bump. Tag automática só com versão
  `< 0.1.0`; daí em diante é pedido humano.
- O script é quem grava `.cycle/state.json` (e `CYCLE_LOG.md` quando há
  mudança). **`.cycle/` é gitignored**: `git add .cycle/state.json` falha e
  aborta a cadeia `&&` — o estado é local, não se commita. `data/cycle_gate.json` **não** é gravado por ele — vem de
  `python tools/cycle_gate.py` (só imprime JSON; redirecionar).
- Release manual (padrão 0.5.x): `Cargo.toml` + `Cargo.lock`, `CHANGELOG.md`,
  `docs/RELEASE_0_5_x.md`, commit `chore(release): ...`; push separado.

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

## 7. Frente A (A1–A3, 2026-11-24) — contratos tipados, gates e artefatos

- **Regra por fase que funcionou:** 1 contrato tipado + 1 gate vermelho→verde +
  1 artefato em `target/` + zero dependência nova. A1 (T-TAB) parseia CSV/TSV
  RFC 4180 à mão e hasheia schema com FNV-1a; A2 escreveu SHA-256 próprio
  (`src/core/sha256.rs`); A3 reusa o mesmo SHA-256 para hash chain + Merkle.
  `cargo check --all-targets --offline` é o juiz: dependência nova é custo real.
- **Placar separado:** o T-EVAL grava `target/aether_agent_scorecard.json`; o
  placar cognitivo (`target/aether_scorecard.json`, do `cognitive_audit`) fica
  intocado — não sobrescrever artefato de outro dono.
- **Suíte T0 estava 22/24 antes da A2.** Causas reais, não testes frouxos:
  `shell.exec` era fallback genérico de qualquer `Act` (agora só com goal que
  cita "shell") e o planner escalava acima do teto quando `risk_ceiling == Low`.
  Corrigir a causa devolveu 24/24 sem baixar asserção.
- **A3 sem LWW:** estado é multi-valor por `(scope, key)`; `apply` deduplica por
  `(nó, seq)` e por valor; valor divergente é preservado e vira `Claim` com
  relação `contradicts`. Merkle root ordena os hashes → mesma raiz com qualquer
  ordem de chegada (comparável entre nós).
- **Limite honesto do A3:** `delta_all` publica só o oplog próprio; forwarding
  para malha 3+ nós é extensão futura. Não fingir convergência de 3 nós.
- **Artefatos são evidência só com `ok=true`:** `aether_tab_gate.json`,
  `aether_agent_scorecard.json`, `aether_p2p.json`; `tools/cycle_gate.py` roda
  os três gates + lib tests antes de versionar.

## 8. `cycle_gate.py` no Windows: UTF-8 em duas pontas

- `subprocess.run(..., text=True)` decodifica com **cp1252** e estoura com
  `UnicodeDecodeError` quando o cargo imprime UTF-8 (acentos/setas) — o gate
  inteiro morria. Fix: `encoding="utf-8", errors="replace"`.
- `print(json.dumps(...))` com `> data/cycle_gate.json`: o stdout também é
  cp1252 e falha com `UnicodeEncodeError` no `→`. Fix:
  `sys.stdout.reconfigure(encoding="utf-8", errors="replace")` no topo.
- `/tmp` do Git Bash **não é** `/tmp` do Python Windows: redirecionar para
  `target/…` e copiar para `data/cycle_gate.json`; conferir o JSON com `python -c`.

## 9. A4 T-CLIP revisada (0.6.1, 2026-11-24) — atrito, ordem de efeitos e corte

- **Detecção de segredo não é binária.** hex32 negando o lote inteiro era falso
  positivo caro (md5/uuid/hash truncado custam um paste de até 256 KiB, sem
  escape). Regra que ficou: classe *forte* (PEM, `sk-`, `ghp_`, `AKIA`, JWT) ->
  deny; hex32 só com contexto de atribuição na linha (`key`/`token`/`secret`/
  `password`/`passwd`/`auth`) -> **redige e segue**. `SecretClass::denies_batch()`
  expressa a assimetria em um lugar só.
- **`redact` calculado e descartado é bug de desenho:** `ingest` montava
  `redaction.text` e só usava `classes()`. O texto aceito/gravado tem de ser o
  redigido — senão a redação é decorativa.
- **Ordem de efeitos > validação.** `process` gravava `code.patch` (via `ingest`)
  e só depois chamava `certify_utterance_template`: lote negado deixava arquivo
  na sandbox. Regra do módulo: certificar antes de qualquer escrita.
- **Gate que não reproduz o defeito não é gate.** O caso "sem claims" usava texto
  puro, que nunca vira patch — o bug era invisível. Trocar a entrada por um diff
  unificado deixou o check vermelho com a mensagem exata e provou o verde depois.
- **Varredura de fonte só vale sem heurística.** `sem_shell` filtrava linhas de
  comentário porque o doc citava `Command::new`. Tirar a citação do doc deixou o
  check literal (10 linhas) e ele continua sendo o único guard do "sem shell".
- **Corte medido por `wc -l`.** A primeira revisão *cresceu* o arquivo; só caiu ao
  remover `Finding`/offsets sem consumidor, `bytes` redundante, `label()` que só
  ecoava string fixa e os 8 `match` repetidos do gate -> helper `record()`.
- **`cargo fmt` sem argumentos formata o crate inteiro:** aparecem diffs em
  `tab.rs`/`p2p.rs`/`scorecard.rs`/`cli.rs`/`dashboard` que não são da sua fase.
  É formato, mas entra no commit e na nota de release para o tree não ficar sujo.

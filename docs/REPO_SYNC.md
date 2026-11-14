# REPO_SYNC — ciclo de sincronizacao (5 min)

Este documento descreve o ciclo automatico que le o projeto, compara com o
repositorio Git, analisa PRs, integra em `main`, versiona e documenta.

## 1. O que roda

| Etapa | Comando/fonte | Efeito |
|---|---|---|
| Ler o projeto | `git status --porcelain` + `cargo metadata` | detecta arquivos novos/alterados e o grafo do crate |
| Comparar com o repo | `git fetch origin` + `git rev-list --count HEAD..origin/main` | detecta divergencia local/remota |
| Analisar PRs | GitHub REST (`/pulls`, check-runs, combined status) | so integra PR `clean` com checks verdes e label `automerge` (ou branch `aether/*`) |
| Merge | `PUT /pulls/{n}/merge` (`squash`, branch removida) | nunca usa `--force`; respeita `no_force_push` do FORUM_PROTOCOL |
| Gate | `cargo check --all-targets` (rapido) / `cargo test --release` (`-Full`) | registrado no log; falha nao bloqueia o snapshot, mas marca o commit |
| Versionar | `Cargo.toml` + `CHANGELOG.md` + tag `v0.0.N` | patch `0.0.x` por ciclo com mudancas |
| Commit | `git add -A` + mensagem `chore(cycle): 0.0.N` | snapshot deterministico |
| Push | `git push origin main` + `git push origin --tags` | somente se houver remote e push habilitado |
| Aprender/registrar | `docs/CYCLE_LOG.md` + `.cycle/state.json` | historico auditavel do ciclo |

## 2. Invocacao

```powershell
# ciclo padrao (check + commit local; push desligado por padrao)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1

# com merge de PRs e push
$env:GITHUB_TOKEN = "ghp_..."      # escopo repo
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1 -PrMerge -Push

# gate completo (cargo test --release)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1 -Full
```

Agendamento (a cada 5 minutos):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 install
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 status
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 remove
```

## 3. Regras de PR

Um PR so e integrado quando **todas** as condicoes valem:

1. repositorio de origem igual ao `origin` (sem fork silencioso);
2. `mergeable_state == "clean"` e `mergeable == true`;
3. todos os check-runs concluidos com `conclusion == "success"` (minimo 1);
4. label `automerge` **ou** branch `aether/*`.

Sem `GITHUB_TOKEN` a etapa de PR e ignorada (nao falha o ciclo).

## 4. Versionamento `0.0.x`

- `Cargo.toml` e a fonte da verdade; `CHANGELOG.md` recebe a entrada do ciclo.
- Versao fora do padrao `0.0.x` e normalizada para `0.0.1` no primeiro ciclo.
- Cada ciclo com mudancas gera uma tag anotada `v0.0.N` e um commit
  `chore(cycle): 0.0.N`.
- Ciclo sem mudancas nao cria commit, nao cria tag e nao incrementa versao.

## 5. Concorrencia (3+ agentes no mesmo diretorio)

- Arquivos de build por agente: `CARGO_TARGET_DIR=target_<agente>` (ignorados no Git).
- O ciclo usa lock `.cycle/lock`; execucoes simultaneas sao ignoradas ate o lock
  expirar (default 10 min).
- Commits sao snapshots: podem capturar trabalho em andamento de outro agente.
  O gate registra o estado real (`gate: ok|fail`) para auditoria posterior.
- Nada de `git reset --hard`, `git clean` ou `push --force` no ciclo.

## 6. Aprendizado e memoria

- Historico legivel: `docs/CYCLE_LOG.md`.
- Estado legivel por maquina: `.cycle/state.json` (ignorado no Git, local).
- Fatos duraveis (politica, decisoes, bloqueios) sao gravados no `neural-sgdb`
  com `scope=app:AETHER` para serem recuperados por qualquer agente.

# AETHER

Motor cognitivo experimental de **computacao pos-token**: a informacao entra e circula como
sinal continuo (forma de onda, campo, fase de oscilador) — sem tokenizacao, sem transformer,
sem protocolo semantico de rede.

Especificacao normativa: [`ARCHITECTURE.md`](ARCHITECTURE.md) e [`docs/API_CONTRACT.md`](docs/API_CONTRACT.md).
Coordenacao entre agentes: [`FORUM_PROTOCOL.md`](FORUM_PROTOCOL.md).

## Pilares

| Feature | Modulo | Papel |
|---|---|---|
| `m1` | `src/core/thermodynamic_solver.rs` | Ising continuo via Bifurcacao Simulada Balistica (bSB) |
| `m2` | `src/core/wave_interference.rs` | operador neural de Fourier / interferencia espectral |
| `m3` | `src/core/kuramoto_field.rs` | sincronizacao de fase P2P (UDP, 40 bytes binarios) |
| `m4` | `src/simulation/*` | organismo em malha fechada + painel ratatui |

## Build

```powershell
.\scripts\cargo.ps1 build --release                 # tudo (m1..m4)
.\scripts\cargo.ps1 check --no-default-features --features m1
.\scripts\cargo.ps1 test --release
```

`scripts\cargo.ps1` apenas prepara o `PATH` do MinGW (`dlltool`) antes de chamar o cargo.
Alvos de build por agente: `CARGO_TARGET_DIR=target_<agente>` (ignorados no Git).

## Ciclo automatico (5 min)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1            # check + commit
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1 -PrMerge -Push
powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 install
```

Politica completa em [`docs/REPO_SYNC.md`](docs/REPO_SYNC.md); historico em
[`docs/CYCLE_LOG.md`](docs/CYCLE_LOG.md); versoes em [`CHANGELOG.md`](CHANGELOG.md).

Regra de ouro do ciclo: **gate verde + mudanca em `src/`/`tests/`** ->
`chore(cycle): 0.0.N` + tag `v0.0.N`; **gate vermelho ou so docs/tools** ->
`wip(cycle): snapshot` sem bump/tag.

## Estado atual

- Versao: `0.0.2` — baseline versionado e publicado.
- `m1` (thermodynamic_solver) e `m3` (kuramoto_field) implementados; `m2` em andamento.
- Remote: `https://github.com/msrovani/AETHER.git` (`main`), push + merge de PRs
  habilitados no ciclo de 5 min.

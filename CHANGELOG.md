# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
Versionamento: `0.0.x` (patch incrementado pelo ciclo automatico a cada conjunto de mudancas).

## [0.0.1] - 2026-11-14

### Changed

- Ciclo automatico: 14 arquivo(s) (.:7, .cargo:1, benches:1, docs:1, scripts:1, src:1, tests:1, tools:1).
- Gate: fail (42s).

- `.cargo/`
- `.gitignore`
- `ARCHITECTURE.md`
- `CHANGELOG.md`
- `Cargo.lock`
- `Cargo.toml`
- `FORUM_PROTOCOL.md`
- `benches/`
- `docs/`
- `rust-toolchain.toml`
- `scripts/`
- `src/`

## [0.0.1] - 2026-11-14

### Added

- Baseline inicial do repositorio: `Cargo.toml`, `Cargo.lock`, `ARCHITECTURE.md`, `FORUM_PROTOCOL.md`, `docs/API_CONTRACT.md`.
- Scaffold `src/lib.rs` + `src/error.rs` com `#![forbid(unsafe_code)]`.
- Modulos de fase 1 em andamento por agente: `thermodynamic_solver` (m1), `wave_interference` (m2), `kuramoto_field` (m3), `closed_loop_organism` + `panel` (m4).
- Testes de invariantes fisicos e de alocacao, benches criterion.
- Automacao de ciclo (`tools/cycle.ps1`) e politica de sincronizacao (`docs/REPO_SYNC.md`).

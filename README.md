# AETHER — motor cognitivo pós-token em Rust

Protótipo experimental de computação baseada em física contínua: **zero tokens,
zero transformers, zero protocolos semânticos de rede, zero backpropagation**.

* **ARCHITECTURE.md** — Fase 1 (Módulos 1–4): organismo reflexivo.
* **ARCHITECTURE_PHASE2.md** — Fase 2 (Módulos 5–8): memória, imaginação,
  morfogênese e encarnação em LAN.
* **docs/API_CONTRACT.md** — contrato normativo de API entre módulos.
* **docs/HW_TARGET.md** — hardware alvo (PC/note/servidor, CPU multithread + GPU AMD/Intel/NVIDIA).
* **docs/HARDENING_REPORT.md** - auditoria, correcoes e medicoes finais (2026-11-15).
* **docs/USER_DELIVERY_PLAN.md** — mapa do entregável ao usuário + plano de ondas (demo → Produto C → A/B).

## Pilares

| Módulo | Feature | Arquivo | Física |
|---|---|---|---|
| 1 | `m1` | `src/core/thermodynamic_solver.rs` | Ising contínuo por Bifurcação Simulada Balística (Euler simplético, paredes inelásticas, elitismo) |
| 2 | `m2` | `src/core/wave_interference.rs` | Operador Neural de Fourier (interferência espectral, invariante à resolução) |
| 3 | `m3` | `src/core/kuramoto_field.rs` | Sincronização de fase P2P por UDP binário de 40 bytes (zero texto) |
| 4 | `m4` | `src/simulation/` | Organismo integrado de loop fechado a 60 Hz |
| 5 | `m5` | `src/phase_memory.rs` | Memória associativa oscilatória com acoplamento complexo (atratores de fase) |
| 6 | `m6` | `src/thermodynamic_dreamer.rs` | GSB na borda do caos + Langevin (amostragem de Boltzmann, incerteza epistêmica) |
| 7 | `m7` | `src/morphogenesis.rs` | Plasticidade topológica por energia livre (crescimento hebbiano + poda metabólica) |
| 8 | `m8` | `src/bin/aether_daemon.rs` | Transdutor físico real + enxame UDP multicast `239.255.42.99:7777` |

## Build

```bash
cargo test --release          # 95 testes: fase 1 + fase 2 + engenharia + FHRR
cargo clippy --all-targets -- -D warnings
cargo run --release --bin aether-organism   # Fase 1: painel em tempo real
cargo run --release --bin aether-daemon     # Fase 2: organismo encarnado
# Windows — dashboard de métricas (hover explica cada medidor):
cargo test --release --test cognitive_audit
cargo run --release --bin aether-dashboard --features dashboard
```


No Windows com a toolchain `x86_64-pc-windows-gnu`, use o wrapper
`.\scripts\cargo.ps1` (ajusta o PATH para os binutils do MinGW) ou coloque
`C:\Users\msrov\Tools\mingw64\mingw64\bin` no PATH (necessário porque os crates
`windows-sys ≥ 0.52` usam *raw-dylib*, que exige `dlltool` + `as`).

## Enxame multi-máquina

```bash
# máquina A
cargo run --release --bin aether-daemon
# máquina B (mesma rede)
cargo run --release --bin aether-daemon
```

As instâncias acoplam fase automaticamente via multicast — observe `R(LAN)`
subir a ~1.0 e a Surpresa `F(t)` reagir a carga de CPU na outra máquina, sem um
único pacote JSON/HTTP.

## Invariantes validados (`cargo test --release`)

1. **Termodinâmico** — `H(s_final) < H(s_0)` estritamente, 50/50 matrizes `J`
   simétricas aleatórias (`N = 64`).
2. **Resolução ondulatória** — o mesmo bloco processa `L = 128/512/2048` com
   pesos bit-a-bit idênticos.
3. **Sincronização sem mensagens** — 4 nós UDP de 40 bytes atingem
   `R(t) > 0.95` em < 3 s (medido: ~0,3 s).
4. **Memória de fase** — recall com 40% de máscara reconstrói fases ocultas
   com erro angular médio < 0.15 rad.
5. **Imaginação** — `U(T=1.5) > U(T=0.001)` e amostras abaixo do estado
   aleatório típico.
6. **Morfogênese** — simetria estrita de `J`, poda no silêncio, zero NaN/Inf.
7. **Engenharia** — `relax_into`/`forward_into` com zero alocação, codec de
   40 bytes com round-trip e rejeição de tamanho, determinismo bit-a-bit.

## Status (2026-11-15)

- Versão do crate: ver `Cargo.toml` (lab cognitivo: **ACQ 100 · 12/12 MEASURED · 0 B** hot-path).
- Superfície de usuário: ainda **demo/lab** (`aether-organism`, `aether-daemon`) — plano em [`docs/USER_DELIVERY_PLAN.md`](docs/USER_DELIVERY_PLAN.md).
- Detalhes de hardening/arquitetura: `docs/HARDENING_REPORT.md`, `ARCHITECTURE_PHASE2.md` §6, `docs/COGNITIVE_AUDIT_REPORT.md`.

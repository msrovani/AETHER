# benches — hardware de referência

`ARCHITECTURE.md` §3 exige que toda meta de performance seja medida em
"hardware de referência declarado". A declaração vive aqui e é atualizada a
cada rodada de benchmark (`criterion` compara contra `.base`).

## Máquina de referência principal

| Campo | Valor |
|---|---|
| Classe | desktop/workstation (ver `docs/HW_TARGET.md`) |
| CPU | x86-64, multithread, preencher modelo/geração na primeira medição |
| Núcleos | preencher (lógicos/físicos) |
| RAM | preencher |
| SO | Windows (toolchain `x86_64-pc-windows-gnu` via `scripts/cargo.ps1`) ou Linux |
| Governor/plano | Linux: `performance`; Windows: "Alto desempenho" |
| `isolcpus`/afinidade | não usado por padrão (`--features affinity` é opcional) |
| Rustflags | `-C target-cpu=native`, LTO fat, `codegen-units=1` |

## Procedimento

```powershell
.\scripts\cargo.ps1 bench --bench solver
.\scripts\cargo.ps1 bench --bench wave
.\scripts\cargo.ps1 bench --bench organism
.\scripts\cargo.ps1 bench --bench kuramoto_latency
```

* TUI/daemon desligado; nada rodando em paralelo.
* Não usar notebook com governor de economia como referência.
* Ao trocar de máquina, zerar `.base` (`--save-baseline new`) e registrar a
  nova máquina acima — comparar medições de máquinas diferentes é inválido.

## Metas contratadas (de `ARCHITECTURE.md` §3)

| Bench | Meta |
|---|---|
| `solver.rs` — passo bSB `N=64`, lote 1 | p99 ≤ 50 µs |
| `solver.rs` — relaxamento 200 passos | ≤ 10 ms |
| `wave.rs` — FFT ida+volta `C=8`, `L=2048` | ≤ 300 µs |
| `kuramoto_latency.rs` — loopback 40 B | p99 ≤ 200 µs |
| `organism.rs` — quadro completo | ≤ 8 ms (60 Hz) |

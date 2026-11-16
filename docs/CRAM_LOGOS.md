# E2d — CRAM → Logos (aquisição de restrições)

> Experimento de lab · feature `m1` · **não** entra no hub / organismo.  
> Fonte externa: [arXiv:2611.17309v1](https://arxiv.org/abs/2611.17309v1)  
> (*Classical Energies Learn to Reason 1,000× Faster than Looped Models*).

## Hipótese

Custos pairwise estilo CRAM (`K` compartilhado por relação linha/coluna/bloco),
aprendidos por **pseudolikelihood** a partir de soluções rotuladas **sem BPTT**
pela relaxação, alimentam um QUBO na convenção do Logos; o `BsbCore` decide e o
oráculo all-diff emite `violations`. O certificado continua sendo de
**factibilidade** (`violations == 0`), não de verdade (E3b).

## O que adotamos do paper / o que não

| Adotado | Não adotado |
|---|---|
| Energia pairwise + pesos interpretáveis | Sudoku-Extreme / Maze como métrica de scaling |
| Treino local (PL), sem BPTT | Modelos looped (HRM/TRM/…) |
| Early-stop por limiar de energia (auxílio de busca) | Declarar acerto só por energia |
| Compactez de params com sharing | Solver externo / constraint programming |

## Mapa AETHER

```text
soluções ──► PL (constraint_acquisition) ──► K,B
pistas   ──► pairwise_to_qubo ─────────────► J,h
                                              │
                                         BsbCore.relax
                                              │
                                         decode + violations
                                              │
                                         Certificate (E, gap, viol, proveniência)
```

- **Aquisição** (`constraint_acquisition`): aprende custos offline.
- **Logos** (`BsbCore` / pipeline E2): decide no landscape Ising.
- **Certificado** (`certificate`): warrant de factibilidade; `Provenance::Axiomatic` (hand-set) ou `Inferred` (PL).

## API

| Peça | Path |
|---|---|
| Oráculo CSP + Shidoku | `src/core/sudoku_csp.rs` |
| `PairwiseEnergy`, PL, `pairwise_to_qubo`, `solve_with_certificate` | `src/core/constraint_acquisition.rs` |
| Probe | `examples/cram_logos_probe.rs` |
| Artefato | `target/aether_cram_logos.json` |

## Como rodar

```powershell
.\scripts\cargo.ps1 run --release --example cram_logos_probe --features m1
.\scripts\cargo.ps1 run --release --example cram_logos_probe --features m1 -- --quick
.\scripts\cargo.ps1 run --release --example cram_logos_probe --features m1 -- --full
```

(No PowerShell, se `--` for engolido, use `cmd /c` ou invoque `cargo.exe` direto com MinGW no `PATH`.)

## Gates

| Gate | Critério |
|---|---|
| **G1** | hand-set factível ≥ 95% |
| **G2** | PL-learned ≥ hand-set − 15 pp **ou** ≥ 80% |
| **G3** | entre factíveis do learned, ≥ 50% com `Certificate::is_coherent` |

Exit code ≠ 0 se qualquer gate falhar.

## Não-objetivos

- Hub Windows (`aether-dashboard`), jobs, M4 organismo / daemon.
- Maze-Unique / path encoding.
- Substituir BSB + Lagrange do `logic_pipeline`.
- Dataset Sudoku-Extreme no CI (lab = 4×4 sintético; `--full` só escala instâncias/pistas).

## Resultados (primeira run verde, default)

| Braço | feas% | coherent | mean_viol | fit_ms | params |
|---|---|---|---|---|---|
| hand_set | 100.0 | 24/24 | 0 | 0 | 52 |
| pl_learned | 100.0 | 24/24 | 0 | ~3.7 | 52 |

Gates G1–G3: **PASS** · `target/aether_cram_logos.json` · `pass: true`.

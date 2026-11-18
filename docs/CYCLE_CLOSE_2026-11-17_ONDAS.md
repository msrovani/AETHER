# Fechamento — quadro de ondas H/M/G/A/K/P/S (2026-11-17)

Versão: **0.0.169**. Host: RTX 3050 6 GB, neural-sgdb stdio
(`…\neural-sgdb\target\mcp-release\release\examples\mcp_server.exe`).
Fórum: MSG 0425–0426 (pedido), 0431 (medidas G/A/S), 0437 (P/K), 0442 (B1).

## Resultado por onda

| Onda | Goal | Veredicto | Número / evidência |
|---|---|---|---|
| H | `fmt --check` + README = crate | **act** | README 0.0.168→0.0.169; fmt limpo |
| M | remember/recall/audit/`contradicts` stdio | **act** | `tests/sgdb_stdio_gate`; notify MCP sem wait |
| G1 | FFT L=8192 B=64 erro≤1e-4 e razão≥2× | **abstain** | erro L2 **4,16e-7**; razão **0,20** |
| G2 | CI `--features gpu` opt-in | **act** | job `gate-gpu` self-hosted + `workflow_dispatch` |
| G3/G4 | JL / Ising | **vigiar** | sem código novo |
| A1 | D1 tetos release ±10% | **act** | p50=p99=1 µs; allocs=6; RSS≈24 MB; `target/d1_budget.json` |
| A2 | MCTS se 4 condições | **abstain** | depth=1, branching=6, Δ=0, p95=0,001 ms |
| A3 | red team 4 vetores | **act** | `tests/agent_redteam_gate` 4/4 |
| A4 | LinUCB ≥30% vs EXP3, ECE≤0,10 | **act** | ganho 0,70; memória −33%; ECE 0,021 |
| K1 | B-G0 labels reais | **act** | rev `97b0c614…`; gato/gata/paráfrase/1 char; recall@10=1,0 |
| K2 | `semantic.match` + abstenção | **act** | `src/agent/semantic.rs` + testes |
| K3 | B1 ≥10k reais | **act** | paws-x 10k; recall@10=1,0; p99=642 µs; RSS≈18 MB |
| K4 | B2 100k | **abstain** | corpus desta rodada tem 10k |
| K5 | escriba 7B | **abstain** | 15,7 GB RAM, sem CUDA; peso cheio fora |
| K6 | B4 produção 8 gates | **parcial** | rodada `20261117_b1` + lab gates 5+6; sem 7B |
| P1–P6 | Produto C | **act** | `--host`/`--human`/`--log`; zip; 2 instâncias peers=1 R=1 |
| S1 | Ratatui default | **act** | `--plain` mantido (decisão registrada) |

## Invariantes confirmados

- Núcleo sem LLM no T0; GPU fora do `default`; laço 60 Hz sem `core::gpu`.
- Conflito = `contradicts`, nunca overwrite.
- Autoridade 60–120 não vira 255 sozinha.
- `data/` da rodada anterior não foi sobrescrita; artefatos em `data/rounds/…`.

## Artefatos

- `data/rounds/20261117_bg0/` — B-G0 (mapa Qwen3-Embedding-0.6B).
- `data/rounds/20261117_b1/` — 10k paws-x + `gates.json`.
- `target/aether-daemon-win.zip` — empacote local (não versionado).
- `scripts/package-daemon.ps1`, `scripts/two-instances.ps1`.

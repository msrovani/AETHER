# Fechamento de ciclo — Doação / Hub / Onda 0 / B0 mínimo

Data: **2026-11-17** · Versão: **0.0.167**  
CLAIM / tópico: `INTAKE_TRANSPLANT` · MSG 0229 / 0244 / 0245 · RELATORIO **MSG 0416** (`CICLO_DOACAO_ONDA0_CLOSE`) · mediação CURSOR **MSG 0418–042x** · trilha agent DEEPSEEK **MSG 0419**

## O que este ciclo fechou

| Frente | Entrega | Evidência |
|--------|---------|-----------|
| Doação lab | ETL dry-run 6 estágios + HvBank AETHV1 + 8 gates | `tools/harvest_transplant.py`, `tests/transplant_hv_gate`, `target/aether_transplant_hv.json` |
| Usabilidade | U1–U6 regeneráveis | `tools/transplant_uso.py`, `target/aether_transplant_uso.json` |
| Hub | Painel Doação·ETL·Transplante + job Probe + archive `data/rounds/hub_*` | `src/dashboard/transplant_view.rs`, `jobs.rs` |
| Playbook | Clone de rodada + origem/destino/stats | `docs/DOACAO_PLAYBOOK.md` §§5–14 |
| Onda 0 | `--help`, `run-demo.ps1`, `--tui`→Ratatui (`features tui`), daemon R/F/SONHANDO | `organism.rs`, `scripts/run-demo.ps1`, `aether_daemon.rs` |
| B0 mínimo | Sidecar HF fora do crate + `--jl-only` | `tools/donor_map_embed.py`, harvest `--jl-only` |
| Docs usuário | USER_GUIDE §6, README 5 min, USER_DELIVERY 0.1–0.6 | docs + README |
| Agent HITL | `VerifiedFact` + rollback de `verified` no HITL | `tool.rs` / `tool_agent.rs` |
| API forget | HUMAN sem store → 500 `ledger_io` (ledger+memory) | `api/mod.rs` (MSG 0413 FIX_1) |
| JL tiled | `act` em lote, 75–122× | o item JL-V4 do board 0414 está encerrado |

## Gates **verdes** neste ciclo (lab)

| Gate / artefato | Critério | Status |
|-----------------|----------|--------|
| Spearman ρ | ≥ 0,95 | PASS (~0,9539) |
| Recall@10 | ≥ 0,90 | PASS (1,0) |
| JL distorção | ≤ 0,12 | PASS |
| Latência top-k | ≤ 1,5 ms | PASS (~0,006–0,3 ms) |
| Alloc query | 0 | PASS |
| Conflitos | ≥ 95% plantados | PASS 5/5 |
| Determinismo / rebuild shards | idêntico | PASS |
| U1–U6 / USO | pass_all | PASS |
| `e3_transplant_gate` | G1–G3 | PASS 6/6 |
| `e3b_certificate` | 5 | PASS |
| `f1_knowledge_bridge_gate` | 4 | PASS |
| `aether-organism --help` | exit 0 | PASS |
| Dashboard `features dashboard` | check | PASS |

## Gates / frentes **ainda necessários** (próximo ciclo)

| ID | Gate / trabalho | Por que ficou aberto |
|----|-----------------|----------------------|
| **B-G0** | Labels reais + “gato/gata” / paráfrase: recall@10≥0,95 sem abstenção cega | Sidecar HF opcional; dry-run sintético não prova B-G0 |
| **B1** | LSH / escala ≥10k átomos no bank | Bank lab ~205 |
| **B4** | Rodada produção completa (corpus+licença+8 gates em números oficiais) | Escriba 7B + remember SGDB real pendentes |
| **SGDB-remember** | `remember` + `audit_checkpoint` + `contradicts` no neural-sgdb (não só mock) | Bridge popula `AtomRecord` do mock; sem URL/stdio estável neste ambiente |
| **Escriba** | GLiNER/REBEL → Qwen2.5-7B → facts.jsonl | Fora do mínimo B0 |
| **FFT-G2** | Kernel WGSL FFT L=8192×B≥16; erro≤1e-4; razão≥2×; `force_off` sem panic; organismo sem `core::gpu` | `docs/GPU_PLAN.md` G2 — projeção 7,4×, sem consumidor de lote |
| **MCTS_gate** | Só se depth>3 ∧ branching>5 ∧ Δsucesso≥10pp ∧ p95≤100ms (MSG 0419) | Fora do sprint; P2 = um passo + rollback |
| **D1** | Orçamentos p50/p99/RSS/allocs **release-only** | Ainda não gate oficial |
| **D3** | Red-team (injeção / envenenamento / escopo / DoS / custo) | Pós-P3 |
| **CI gpu** | Job opt-in `--features gpu` com adapter | Gates JL/Ising rodam à mão |
| **B-G0 / B1 / B4** | Labels reais; LSH≥10k; rodada produção | Doação lab ≠ produção |
| **SGDB-remember** | remember + audit_checkpoint reais | Sem URL/stdio estável neste host |
| **Ratatui default** | `run-demo.ps1` ainda `--plain`; TUI exige `--features tui` | Aceite Onda 0.1 honesto |

### Gates de regressão (rodar ao abrir próximo ciclo)

```powershell
cargo test --release --features api --test api_agent
cargo test --release --test agent_plan_gate
cargo test --release --test agent_undo_sgdb_gate
cargo test --release --test language_port_gate
cargo test --release --test agent_self_model_gate
cargo test --release --test e2c2_roof_dual_gate
cargo test --release --test f1_knowledge_bridge_gate
cargo test --release --features gpu --test gpu_jl_tiled_gate -- --nocapture
cargo test --release --features gpu --test gpu_ising_sweep_gate -- --nocapture
```

## Aprendizados (memória de ciclo)

1. **Doação ≠ pesos**: mapa/escriba doam geometria e triplas; núcleo só bits + keys.
2. **Dry-run é CI; produção é sidecar + `--jl-only`** (MRL 768 no Qwen3-0.6B Apache).
3. **Hub é a superfície humana**: gates sem JSON no Detalhe não contam.
4. **Arquivar rodadas** (`data/rounds/`) antes de sobrescrever `data/`.
5. **Non-goals**: HV fora do T0; auth transplantado ≤240; sem BAM/JIT/governor no CLI de produto.
6. **Forget 255 sem store = 500**: purgar sem túmulo ressuscita no boot; ledger e memory alinhados.
7. **VerifiedFact**: certificado sem violação + claim Logic/Solver autoriza `memory.write` em 200; HITL 255 continua humano.

## Como clonar (receita mínima)

```powershell
.\scripts\run-demo.ps1
python tools/harvest_transplant.py --limit 200 --dry-run --out data --no-whiten
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
python tools/transplant_uso.py
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Playbook: `docs/DOACAO_PLAYBOOK.md`.

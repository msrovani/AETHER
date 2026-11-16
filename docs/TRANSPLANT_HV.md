# TRANSPLANTE HV — relatório de implementação

> CLAIM: MSG **0229** / **0244** / **0245** · Tópico `INTAKE_TRANSPLANT`  
> Spec: [`INTAKE_PLAN.md`](INTAKE_PLAN.md) · **Clonar rodada:** [`DOACAO_PLAYBOOK.md`](DOACAO_PLAYBOOK.md)

## O que foi entregue

| Peça | Path |
|------|------|
| ETL dry-run | `tools/harvest_transplant.py` |
| Probe | `examples/transplant_hv_probe.rs` |
| Gate | `tests/transplant_hv_gate.rs` |
| Núcleo (pós-gates) | `src/core/hv.rs` |
| Evidence | `certificate.rs` (`Evidence`, `decide_with_evidence`) |
| SGDB mock / loader | `src/knowledge/sgdb_bridge.rs` |
| Artefato | `target/aether_transplant_hv.json` |

## Gates medidos (probe, 2026-11-16)

| Gate | Valor | Alvo | Status |
|------|------:|------|--------|
| Spearman (E3 d=384) | **0,9539** | ≥ 0,95 | PASS |
| Recall@10 | **1,000** | ≥ 0,90 | PASS |
| Distorção JL | **0,0084** | ≤ 0,12 | PASS |
| Determinismo | true | idêntico | PASS |
| Latência (~205 átomos) | **~0,006 ms** | ≤ 1,5 ms @10k | PASS |
| Alloc consulta | **0** | 0 B | PASS |
| Conflitos | **5/5** | ≥ 95% | PASS |
| Rebuild shards | ok | byte-plan | PASS |

## Como reproduzir

```powershell
python tools/harvest_transplant.py --limit 200 --dry-run --out data --no-whiten
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
.\scripts\cargo.ps1 test --release --test transplant_hv_gate
```

## Usabilidade (2026-11-16)

| Caso | Resultado |
|------|-----------|
| U1 auto-recuperação | 5/5 |
| U2 resolve SGDB | 9/9 |
| U3 vizinhança | PASS |
| U4 conflitos | 5/5 |
| U5 Evidence | 5/5 |
| U6 latência Rust | **0,006 ms** · 0 alloc |
| CI gates | transplant_hv 5/5 · e3 6/6 · e3b 5/5 · f1 4/4 |

Artefato: `target/aether_transplant_uso.json` · Hub: painel **Doação · ETL · Transplante**.

## Invariantes

- T0 / `step_into` **não** alterados.
- Strings só no SGDB/borda; HV = bits + keys.
- Authority transplantada ≤ 240; HITL = 255.
- Doador documentado: Qwen3-Embedding-0.6B Apache (dry-run = sintético).

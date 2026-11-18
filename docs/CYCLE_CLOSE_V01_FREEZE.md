# CYCLE_CLOSE — freeze v0.1.0

> Medido em 2026-11-18 no perfil `--release`, `CARGO_TARGET_DIR=target_open`,
> sem disputar o `target/` que outros agentes já ocupavam. Tag `v0.1.0` **não**
> foi aplicada.

| Campo | Valor |
|---|---|
| Data | 2026-11-18 |
| Host | ROCKY (o mesmo do `usability_gate.json`) |
| Crate na medição | lib `0.0.174`; auditoria `0.0.175`; smoke `0.0.173` |
| Artefatos | `target/v01_freeze/g0_g9.json`, `aether_scorecard.json`, `cic_abstain.json` |

## Gates G0–G9

| ID | Resultado | Números |
|---|---|---|
| G0 build | PASS | `fmt --check` 0; `clippy -D warnings` (`api,agent`, all-targets) 0; `test --release --lib` 172 passed, 0 failed |
| G1 ACQ | PASS | ACQ 100,00; 12/12 `MEASURED`; `hot_path_bytes` 0 |
| G2 api_eval | PASS | n=60; exact 60/60; ruído 60/60; paráfrase, caixa, 1 caractere, prefixo e escopo alheio abstem 60/60 |
| G3 agente T0 | PASS | eval 24/24, `action_accuracy` 1,0, `false_act` 0; `language_port_gate` 9/9 |
| G4 integridade | PASS | `api_integrity` 10/10; `api_lifecycle` 5/5; `agent_redteam_gate` 4/4 |
| G5 HITL | PASS | `api_agent` 14/14 (forget 255 exige segredo; 200 não purga); certificado 3/3 (200 só com `violations` 0) |
| G6 undo | PASS | `agent_undo_sgdb_gate` 6/6 |
| G7 usabilidade | PASS | `usability_gate` 6/6; p50 0 µs; 6 alocações/chamada (tetos 8 µs e 40) |
| G8 smoke | PASS | fato 42 cita com autoridade 200; OOD `low_similarity`; solve `violations` 0; fronteira contém “cita”. `target/v01_freeze/v01_smoke.json` ok, `smoke_f7.txt` = `F7=PASS` |
| G9 tag | ABERTO | crate segue em 0.0.x. Sem bump, sem tag, sem push de `v0.1.0` |

F2 `cic_abstain_gate` também passou: `false_act_C` 0 em n_C=20 (`target/cic_abstain.json`).

## Fora do freeze (trilha R)

- B-G0 / E1 PAWS consistency (AUC ~0,5983 vs donor ~0,69)
- D1 ColBERT / BGE-M3 / NysHD
- Embed 100k produção / escriba 7B
- FFT-G2 / MSI / cloud / auth / Produto A/B

## Checklist tag

- [x] F1–F7 / G0–G8 verdes neste host
- [x] Este arquivo preenchido
- [ ] README + EXTERNAL_API + V01 alinhados à versão **0.1.0**
- [ ] Pedido humano: bump `0.1.0` + tag + push

# A-5 — Evidência de durabilidade (fora de `target/`)

CLAIM: **MSG 0973** · Deliberação gestor: **MSG 0967** (`MISSAO_CURSOR`).

## Regra

Artefato de **aceite** do ciclo F1→F5 não pode morrer no `cargo clean`.
`target/` é efêmero; `data/` (ou `.vega/`) é o destino durável.

## Como auditar

```bash
python tools/a5_evidence_durability.py selftest
python tools/a5_evidence_durability.py audit
```

Manifesto durável: `data/a5_evidence_manifest.json`  
Cópia efêmera do relatório: `target/a5_evidence_durability.json`

## Migrações propostas (sem tocar arquivos alheios nesta missão)

| Prioridade | Artefato em `target/` | Proposta |
|---|---|---|
| P0 | `f1_decision_smoke.json` | já tem espelho em `data/` via promote; manter promote no fluxo |
| P0 | `embed_eval.json` / `embed_eval_bm25_baseline.json` | copiar números do run REPROVADO para `data/` após ACK |
| P1 | `f1_contract_fit124.json` | copiar rodada limpa BUFFY → `data/` |
| P2 | relatórios T0/T3/MF2 | opcional `*_last.json` em `data/` |
| P3 | `qa_suite_probe` / GPU lock | efêmero OK |

## Fora de escopo

- Não reescreve crates, `cli.rs`, dataset congelado F4.
- Não migra artefatos históricos B0/D1/hub (fora do ciclo VEGA atual).

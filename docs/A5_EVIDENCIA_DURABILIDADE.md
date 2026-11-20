# A5 — evidência / durabilidade (MSG 0967)

Data: 2026-11-20 · CURSOR · Gestor: OPENCODE  
Escopo: auditoria **offline** repo-wide · **não move** arquivos alheios  
DoD: lista do que ainda mora em `target/` + proposta de migração

## Política

Aceite do ciclo **não** pode viver só em `target/` (`cargo clean` apaga).
Canónico = `data/` (+ hash/meta quando aplicável).

## Já durável em `data/`

| Artefato | Nota |
|---|---|
| `data/f1_decision_smoke.json` (+ `.meta.json`) | A-5 promote |
| `data/f1_gate_baseline.json` | Eixo D n=30 |
| `data/embed_eval_set.jsonl` | F4 freeze SHA `f8682442…` |
| `data/a5_evidencia_manifest.json` | saída desta auditoria |

## Em `target/` (precisa plano)

Rodar:

```bash
python tools/a5_evidencia_audit.py
python tools/a5_evidencia_audit.py selftest
```

Classes típicas:

| Classe | Exemplos | Migração proposta |
|---|---|---|
| gate_report | `f1_*_gate.json`, `mf2_*.json`, `f4_gate_hold*.json` | `data/<nome>` ou `data/reports/` |
| measurement | `embed_eval.json`, `embed_eval_bm25_baseline.json` | `data/embed_eval_last.json` (+ meta); set já em `data/` |
| tier_report | `f5_t*.json`, `qa_suite_probe.json`, `vega_path_audit.json` | `data/reports/` |
| quarantine_ephemeral | `research_quarantine_*.jsonl` | efêmero OK; HITL decide persistir |
| ephemeral_log | `log_f1_*.txt`, `*.log` | manter/apagar — não é aceite |
| stray_script | `*.py` em `target/` | `scripts/` ou apagar |

## P0 promote (MSG 0986)

Fatia CURSOR pós-manifesto — só medições P0, só `data/`:

```bash
python tools/a5_p0_promote.py
python tools/a5_p0_promote.py selftest
```

→ `data/embed_eval_last.json`, `data/embed_eval_bm25_baseline.json`,
`data/mf2_gate_regression.json` (+ `.meta.json` sha256).

## P2 promote (tier reports)

```bash
python tools/a5_p2_promote.py selftest
```

→ `data/reports/f5_t0_eval.json`, `f5_t3_eval.json`, `vega_path_audit.json`,
`f1_offline_suite.json` (+ metas). Não sobrepõe P0/P1.

## Fora de escopo

- Editar `aether-decision`, `cli.rs`, dataset F4, `.vega`
- Baixar modelos / HTTP
- Migrar o restante do manifesto (donos de cada tool / HITL)

Autoridade agente: 120.

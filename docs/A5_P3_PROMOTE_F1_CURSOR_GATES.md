# A5 P3 — promove gates F1 CURSOR (gaps pós P0/P1/P2)

Data: 2026-11-20 · Claim: **CLAIM_A5_P3** · Gestor: OPENCODE · Ref: MSG **0967**  
Escopo: copiar `target/` → `data/` (+ `.meta.json` sha256) · **não** aplica migração alheia

## Por quê

Manifesto A5 listou ~28 itens em `target/`. P0 (embed/mf2), P1 (vram/gil/qa/hold)
e P2 (t0/t3/vega/offline_suite) cobriram parte. Restaram gates desta linha
CURSOR (`f1_5_ttt_abstain`, `f1_a1b_vega_check`) e tier T1/T2.

## Comando

```bash
python tools/a5_p3_promote.py selftest
python tools/a5_p3_promote.py
```

Relatório: `data/a5_promote_p3_report.json`

## Fora de escopo

- Dataset F4, crate, `cli.rs`, `.vega`
- Sobrescrever destinos P0/P1/P2 sem necessidade (copy2 idempotente)

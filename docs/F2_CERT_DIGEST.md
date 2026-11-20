# F2 — Digest de certificado ≤320 tokens

Data: 2026-11-20 · CLAIM: **MSG 0778** · Eixo B do plano F2→F3.

## Regra

Estado → reader/pesquisa: **digest ≤320 tokens**, nunca payload bruto.

`digest_certificate_fields` emite só a allowlist `E gap viol consensus prov auth`
mais a pergunta (cortada para caber). Chaves `payload`, `state`, `raw`,
`sources`, `embedding` e `vector` são descartadas — não entram nos 320 tokens.
`gap` fica ≥ 0 e `consensus` fica em 0..1. Autoridade do atestado é copiada
(0..255); a quarentena da pesquisa continua teto 120 no harness.

## Artefatos

| Path | Papel |
|---|---|
| `tools/cert_digest.py` | CLI + selftest |
| `tools/research_harness.py` | `digest_certificate()` usado na tomada unknown |

```bash
python tools/cert_digest.py selftest
python tools/cert_digest.py --state "E=-1.0 gap=0.1 viol=0"
python tools/cert_digest.py --cert-json atestado.json
```

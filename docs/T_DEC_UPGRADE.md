# T-DEC Upgrade — usabilidade Vega ∧ gemma

> Dual-bank: produto fica em `active` até HITL cutover. Candidate nunca
> atende Console/API. Upgrade silencioso é proibido.

## Para qualquer usuário (leia primeiro)

O AETHER **não** é “um modelo Vega”. Há três camadas:

1. **Core** — não gera texto; cita / certifica / abstém.
2. **Tomada T-DEC** (`DecisionPort`) — encaixe de *decisor*; pode ser stub ou motor real.
3. **Readers** — LMs congelados que o motor Vega **pluga** (0.8B hoje; 4B / gemma / 27B via catálogo). Trocar reader ≠ trocar o AETHER.

“Subir o Vega” no hub = **trocar o reader do slot** (primário ou sentinela), com HW + smoke + HITL. Sair do Vega (outro motor) = outro `DecisionPort` — possível na arquitetura, não é o botão de upgrade.

Guia curto no hub: [USER_GUIDE.md](USER_GUIDE.md) §7c.

## Quadro de luz

| Slot | Papel | Lâmpada crítica |
|---|---|---|
| Primário (reader no Vega) | roteia T1 | T1 age no claro |
| Sentinela (reader Gemma) | OOD T2 + C1 | T2 abstém gibberish |
| Escriba (mesmo Gemma) | T-NL-OUT | claims → prosa + P2; path = sentinel |
| AND | `gate::combine` | conflito → HITL |
| Core | MoE/coh | só lê **active** |

Cutover HITL amarra `scribe_path` ao sentinel active (economia).  
Doador Qwen / T-NL-IN / T-EMB **não** entram neste fluxo. Ver [`GEMMA_SENTINEL_SCRIBE.md`](GEMMA_SENTINEL_SCRIBE.md).

## Estados

```text
Idle → Catalog → HwCheck → HitlApproveDownload → Fetch → Attach → Fit
    → Smoke(T1|T2|C1|AND) → Quarantine → HitlCutover → Active
qualquer falha → AbortRollback (active intacto)
```

Artefato: `target/aether_dec_upgrade.json`  
Catálogo: `data/dec_reader_catalog.json`  
Perfis: `.vega/profiles/{active,candidate,previous}/`

## Lâmpadas (banda CORE §3)

HW · Fetch · Boot · Fit · T1 · T2 · C1 · AND · Quarentena · OK geral

OK geral **só** após HITL cutover. Até lá: chip “produto = active (fallback)”.

## HITL (dois carimbos)

1. `dec_upgrade_download` — autoriza fetch HF (rede opt-in).
2. `dec_upgrade_cutover` — promove candidate → active; antigo → `previous/`.

Deny/timeout → AbortRollback. Reverter = 1-click de `previous/` → active.

## Critérios smoke (travados)

- T1 primary: age no billing/claro
- T2 sentinel: abstain no gibberish
- C1: proibido choice errado + abstain=false high-conf
- AND: motivos tipados; sem panic
- Boot/Fit: `degraded=false` no candidate

## Jobs hub

`JobKind::DecUpgrade` (single-flight Lab). Orquestra `tools/dec_upgrade_run.py`
(+ attach/fit/smoke). Cancel → rollback candidate.

## Non-goals

Auto-cutover; candidate no bus; modelo fora da allowlist; prosa gerada por Vega/gemma.

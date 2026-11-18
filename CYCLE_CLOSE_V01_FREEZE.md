# Ciclo fechado — freeze **v0.1.0** (IA usável)

> Gates F1–F7 rodados em `--release` em 2026-11-18 (claim MSG 0625,
> checklist MSG 0650, pedido humano explícito para a tag: "tag").
> Crate no momento da rodada: `0.0.175→0.0.177` (ciclo automático em voo).

## Checklist da tag

- [x] **F1** `agent_eval_suite` — 1/1 PASS (`action_accuracy`, `false_act=0`, n≥24)
- [x] **F2** `cic_abstain_gate` — 1/1 PASS (n=5000; `false_act=0`; AUC 0,6909
      com a régua corrigida pela errata MSG 0577 — teto PAWS não relaxado)
- [x] **F3** `api_agent` 14/14 + `api_eval` 1/1 (`--features api`)
- [x] **F4** `usability_gate` 6/6 + `agent_redteam_gate` 4/4 (parser/OOD não vira `act`)
- [x] **F5** `cognitive_audit` — 12/12 MEASURED (ACQ 100)
- [x] **F6** `fmt --check` 0 · `clippy -D warnings` 0 (lib+tests+bins) · `lib` 172/172
- [x] **F7** `scripts/v01_freeze_smoke.ps1` — PASS 30,5s
      (`target/v01_smoke.json`: `ok=true`; frontier em `/props`;
      cue conhecido → hit; OOD → `abstain low_similarity`;
      solve → certificado `energy −1, gap 2`)

## Promessa registrada no smoke

`hit_known__abstain_ood__frontier_props__no_chatbot` — a IA usável do freeze:
cita/certifica ou abstém; **não** conversa.

## Riscos e ressalvas honestas

* Host com ciclo automático concorrente durante os gates (0.0.175→0.0.177) —
  os sete passaram mesmo assim; rodada 100% limpa antes de qualquer release
  binário adicional é recomendada.
* ACQ 100 ≠ MMLU/Arena (tier T0 ≠ T3) — frase de fronteira em toda superfície.
* Donor PAWS ~0,69 ≠ chatbot; E1 PAWS 0,5983 = achado negativo registrado
  (errata MSG 0577/0578, SGDB supersede feito).

## Não entra no freeze

D1 ColBERT / BGE-M3; NysHD como classificador PAWS; embed 100k produção;
LanguagePort LLM no núcleo; MSI/cloud/auth/i18n; Produto A/B hardware;
MMLU/Arena.

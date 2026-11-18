# G7 — checklist executável US0-aggregate (freeze v0.1)

> Artefato: `target/us_aggregate_gate.json` · Teste: `tests/us_aggregate_gate.rs`
> · Claim fórum: **MSG 0620** · RELATORIO: **MSG 0645** · Data: 2026-11-18

Este documento é a metade **documentada** da opção B do gate G7
(`docs/V01_DELIVERY_PLAN.md` §3.2): "checklist documentado apontando gates
existentes 1:1". O teste `us_aggregate_gate.rs` é a metade **executável** —
ele afirma a existência de cada gate-fonte (o wiring não pode apodrecer) e
valida numericamente cada artefato que já existe, contra os pisos/tetos da
tabela US (`USABILITY_TESTS.md`).

## Tabela 1:1 (US → gate → critério)

| US | Gate-fonte (1 arquivo/1 agente) | Artefato validado | Critério no agregador |
|---|---|---|---|
| U2 fato conhecido | `v01_smoke` (G8) + E3 G3 | `target/v01_smoke.json` | steps: `cue_conhecido` ok ∧ pass==total **ou** plano: `ok=true ∧ known_hits≥1 ∧ known_abstain=false` |
| U3 OOD abstain | `tests/e3_transplant_gate.rs` | `target/aether_e3.json` | `ood_abstained ≥ 1.0 ∧ false_confident == 0.0` |
| U4 paráfrase/digitação | `tests/e3b_certificate.rs` + api_eval | `target/aether_api_eval.json` | casos `paraphrase/case/one_char/prefix/scope` todos `expect=abstain ∧ pass=true` |
| U5 solver certificado | `v01_smoke` (G8) + `certificate.rs` | `target/v01_smoke.json` | steps `solve_certificado` ok **ou** plano `solve_energy` finito ∧ `ok=true` |
| U7 HITL antes da call | `tests/agent_plan_gate.rs` | — | wired (fonte existe) |
| U8 reversibilidade | `tests/agent_plan_gate.rs` + `tests/agent_undo_sgdb_gate.rs` | — | wired |
| U9 autoridade/tombstone | `tests/api_agent.rs` | — | wired |
| U10 entrada abusiva | `tests/agent_redteam_gate.rs` | — | wired |
| U11 contradição sem overwrite | `tests/agent_undo_sgdb_gate.rs` | — | wired |
| U13 parser 95%/OOV≤5% | `tests/language_port_gate.rs` | — | wired |
| (U12 latência) | `tests/usability_gate.rs` | `target/usability_gate.json` | **não duplicado** — gate anterior próprio (D1 per-call, MSG 0428 OP1) |

## Notas de desenho

1. **Sem número sintético.** Artefato ausente → linha `wired` (o gate dono
   cobre o US na própria suíte); artefato presente → número validado
   (FALHOU se violar piso/teto); fonte ausente → `FONTE_AUSENTE` = falha.
2. **Schema-tolerante** ao `v01_smoke.json` (aceita schema com `steps[]` ou
   plano `known_hits/known_abstain/solve_energy` — a sessão paralela iterou
   o smoke às 13:03 durante o desenvolvimento; lição de parser 0567 aplicada:
   booleans parseados, nada truncado em silêncio).
3. **Trilha R não bloqueia**: `target/e1_paws_consistency.json` (AUC E1
   0,5983 < piso 0,60, donor ref 0,6909) entra como `NOTA_R` informativa —
   é fora do escopo G7 por desenho do plano (§2.3).
4. Conversão `wired → pass`: rodar o gate-fonte na janela de cargo livre
   (após o freeze V0.1-C do CURSOR liberar o build) e registrar o artefato
   no `target/` — o agregador passa a validar o número automaticamente.

## Rodar

```powershell
.\scripts\cargo.ps1 test --release --test us_aggregate_gate
```

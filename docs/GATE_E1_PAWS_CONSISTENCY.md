# Gate `e1_paws_consistency` — resultado (MSG 0579)

**Status:** rodado · **PASS = false** (achado negativo honesto) · T0 intacto

| Item | Valor |
|---|---|
| Arquivo | `tests/e1_paws_consistency_gate.rs` (OPENCODE) |
| n | 5000 (2500/2500) |
| AUC E1 | **0,5983** |
| Piso abstain | 0,60 (floor; **não** afrouxado) |
| Donor ref (errata 0567) | **0,6909** |
| Δ vs donor | **−0,0926** |
| Artefato | `target/e1_paws_consistency.json` |

## Leitura

Scaffold n=2000 dava 0,6054; no corpus completo a E1 **cai abaixo do piso**.
Régua real: donor 0,69 / bigram ~0,73. E1 precisa **ganhar do donor** antes de
reivindicar vantagem de composição.

## Próximo (não este arquivo)

- OPENCODE: D1 per-token MaxSim real (`tools/b0_maxsim_colbert.py`, claim 0581)
- CURSOR: mediação + doc + (opt-in) bits+f32 rerank alinhado MSG 0568/0576
- Não editar `phase_language` / `encode_sentence` sem lease

## Referências

MSG 0539, 0546, 0562–0563, 0567, 0577–0581, 0585.

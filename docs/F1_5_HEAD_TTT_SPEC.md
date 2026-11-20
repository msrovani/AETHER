# F1-5 — SPEC head TTT sem contrato de abstenção (CURSOR, offline)

Data: 2026-11-20 · Claim: MSG **0919** · Gestor: OPENCODE  
Escopo: **docs + tools** · **Não toca** `aether-decision/src` nem `cli.rs`  
Ref: `docs/F1_CONTRATO_MEDIDO.md` §6.1 · BUFFY MSG **0909** (cv_accuracy=0.7419 no artefato)

## Problema

Com `fit(124)` o `route` passa a ser respondido pelo **head** (`readout=ttt`).
A answer do head traz **só** `probs`, `confidence`, `cv_accuracy`, `readout`,
`choice`. **Não** existem `abstain`, `unbound`, `prediction_set`,
`settle_steps` nem `energy`.

Consequências (contrato medido):

1. `get("abstain").unwrap_or(false)` transforma "sem sinal" em "agiu".
2. `conformal_set` é impossível nesse caminho (sem `prediction_set`).
3. A proteção OOD do AND-gate fica inteira na **sentinela** — o head nunca abste.

## Contrato (o que o host/port deve fazer — dono do crate: BUFFY/OPENCODE)

1. **Nunca** tratar ausência de `abstain` no head como `abstain=false`.
2. **Sempre** expor `Answer.cv_accuracy` no artefato/meta (mitigação já aplicada
   no crate por BUFFY — esta missão só documenta + classifica offline).
3. **Não inventar** threshold de abstain para o head neste gate — isso é
   **decisão F3 / HITL**.
4. Host que precisar de abstain no caminho TTT: aguardar política F3 ou
   forçar caminho conformal (sentinela) quando OOD.

## Entrega CURSOR (esta missão)

```bash
python tools/f1_5_ttt_abstain.py selftest
python tools/f1_5_ttt_abstain.py check --readout ttt --cv-accuracy 0.7419
python tools/f1_5_ttt_abstain.py check --readout ttt --acted-assumed true
```

- Classificador puro (sem rede, sem PyO3).
- `wired_to_crate=false` — wire/política F3 fica com BUFFY/OPENCODE.
- Ref de visibilidade: `cv_accuracy=0.7419` (artefato republished 0909).

## Fora de escopo

- Editar `aether-decision` / inventar threshold no port.
- Baixar modelos ou abrir HTTP.
- Roubar claims A2/A3/A4 / MF2 / F4.

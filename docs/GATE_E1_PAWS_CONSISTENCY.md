# Gate proposto — `e1_paws_consistency` (MSG 0539 / ACK 0546)

**Status:** proposta ACK’d pelo CURSOR; implementação só com ACK do dono E1
(`phase_language`). Não editar T0.

## Problema

AUC cosine mapa = 0,4945 em PAWS (acaso). B4/NySHD por similaridade **não**
fecha B-G0 label. O discriminador certo é **relação** `parafraseia` /
`não_parafraseia` no corpus + consistência do bundle E1.

## Contrato do gate (rascunho)

| Item | Valor |
|---|---|
| Entrada | `data/rounds/20261117_b0_paws/facts.jsonl`: n=5000, `parafraseia`=2500, `nao_parafraseia`=2500. Campos `subject`, `object`, `relation` |
| Métrica | consistência do bundle E1 entre `subject` e `object`, contra o rótulo. Não é cosine do mapa |
| Critério PASS | AUC ≥ 0,60 (mesmo teto do scaffold E1, que mediu 0,6054 em n=2000) |
| Critério FAIL | AUC abaixo disso é achado negativo honesto. O teto cosine do mapa não desce |
| Critério ABSTAIN | OOV / conflito / energia fora — false_act = 0 |
| Artefato | `target/e1_paws_consistency.json` |
| Dono código | OPENCODE, arquivo novo. Não editar `phase_language` nem `encode_sentence` |
| Fora desta métrica | MaxSim-lite do codebook (AUC 0,4996) hasheia o token em `N_ENT=64`. Não falsifica ColBERT nem BGE-M3 |

## Fora de escopo

- Baixar teto AUC do mapa
- Treinar NySHD para classificar label PAWS
- Mexer em `phase_language` sem lease

## Referências fórum

MSG 0520, 0532, 0539, 0540, 0546, 0548, 0551.

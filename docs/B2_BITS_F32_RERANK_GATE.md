# B2 — gate de shards: roteamento em bits + re-rank em float

> Claim/medições: MSG 0568 (lei de capacidade validada), 0591/0601/0603 (rerank
> probe), 0520 (achado 2). Autor do doc: OPENCODE (2026-11-18).
> Pré-requisitos que já existem: `bank.f32` (15 KB/fato, por round),
> `s2_query_f32.bin` (sidecar `b0_pair_cosines.py --skip-hv`, commit fdeb0b4+
> f10fad), e o ciclo de capacidade E1B **0,4×D = 4096 fatos/shard**.

## Por que este gate existe

Os experimentos A3 mediram dois efeitos separados (MSG 0568):

1. **Capacidade** (fator dominante, confirmado): 5000→2500 fatos subiu o
   recall@1 de **0,7676 → 0,876/0,873** (+11pp). Shard máximo = 4000.
2. **Assimetria de consulta** (gap restante até 0,90): a query é o partner
   (s2 sozinho) contra um fato que é a concatenação `s1|rel|s2` — a
   quantização JL de 1 bit não fecha essa diferença. O probe do rerank
   (0601, n=500, proxy self-query) mostrou `bits 0,880 → bits+float 1,000`.

## Design (3 camadas)

```text
query s2 (f32, 768d) ──JL/sign──▶ bits (10240) ──▶ LSH multi-probe no shard
                                        │
                                        ▼ top-K=32 (bits)
                              re-rank cosseno f32 contra bank.f32 do shard
                                        │
                                        ▼
                              rank-1 = par; score < t_lo → abstain
```

1. **Roteamento em bits** (rápido, 1,28 KB/átomo residente): LSH 4×16
   multi-probe (`hv.rs` já tem single-probe; multi-probe é a lit. do plano
   1809.07471/1310.4136).
2. **Re-rank em f32**: apenas K=32 candidatos por query → 32 cossenos de
   768d = ~49k MACs — barato; exige `bank.f32` do shard (já existe) e
   `s2_query_f32.bin` (sidecar emite agora).
3. **Abstain por banda**: score de re-rank abaixo de `t_lo` (calibrado no
   held-out, como no `cic_abstain_gate`) → `abstain low_similarity` —
   a doutrina, no gate.

## Contrato do gate `b2_shard_gate` (a escrever)

| Métrica | Piso | Nota |
|---|---|---|
| recall@1 por shard **com re-rank** | **≥ 0,90** | fecha a assimetria s2; piso de produção |
| recall@1 bits-only (sem re-rank) | reportado | comparável ao 0,876/0,873 de hoje |
| cobertura da banda de abstain | ≥ 0,50 declarada | acc_coberta ≤ AUC do discriminador (Youden) |
| capacidade por shard | ≤ 4000 | lei E1B; shard acima = FAIL de design |
| roteamento cross-shard | 0 vazamentos no cenário single-shard | cross-shard é a camada LSH do B2 (gates próprios) |

## Custos medidos (para o planejamento)

* Embed GPU: 23,42 txt/s no probe limpo (A2 admitido, 2,17× vs CPU);
  5k fatos = ~4 min; 10k frases do sidecar = ~9 min.
* Bank por shard de 4000: `4000×160×8B = 5,12 MB` + idx 96 KB.
* B2 100k = 25 shards de 4k; roteamento cross-shard ainda não tem gate.

## Anti-escopo

* Não relaxa o piso de abstain (0,60) nem o floor de produção (0,90);
* Não entra no crate sem gate; `bank.f32` por shard é **opcional** — sem ele,
  o shard cai para bits-only com recall@1 ~0,87 (aceito, declarado);
* Não substitui o gate `b0_parafra`/`b0_shard_gate` (regressão).

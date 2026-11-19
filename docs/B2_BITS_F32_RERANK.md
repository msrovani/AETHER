# B2 — desenho bits + re-rank f32 (gate e critérios)

> Data: 2026-11-18 · Sugerido no fórum **MSG 0677** (CURSOR) · trilha R,
> fora do freeze 0.1.0 · Doutrina: teto A3 **0,90 não baixa**.

## 1. Evidência acumulada (por que bits+f32 é o desenho)

| Medida | Valor | Fonte |
|---|---|---|
| Monolito 5000 fatos (sobre capacidade) | recall@1 0,7676 | `target/b0_parafra_gate.json` |
| Shards 2×2500 (dentro da lei E1B 0,4×D) | **0,8760 / 0,8728** (+11 pp) | `target/b0_shard_gate.json` |
| Bits → re-rank f32 (top-32, 500 self-queries) | **0,88 → 1,00** (+12 pp) | `target/hv_bits_float_rerank.json` |
| AUC donor espaço contínuo | 0,6909 | régua MSG 0577 |

Leitura: os dois efeitos são **aditivos e independentes** — capacidade (shards
≤4096) corrige a degradação de memória; re-rank float corrige a perda de
granularidade da quantização 1-bit (JL sign). Literatura de suporte (MSG 0576,
CURSOR): QJL (2406.03482, query float/key 1-bit assimétrico), LiNR/SignOPORP
(2407.13218, 1-bit + full-precision re-rank), KALE (2304.01016), EMVB
(2404.02805, bits + late interaction).

## 2. O que o probe atual NÃO cobre (honestidade)

`hv_bits_float_rerank.json` é **self-query proxy**: a query é o próprio fato
(concatenado) — a simetria favorece o re-rank. O gap real do A3-shard é a
**assimetria de consulta**: query = partner s2 **sozinho** vs fato `s1|rel|s2`
concatenado (declaração do `b0_shard_gate.rs`, MSG 0520). O gate B2 tem de
medir a assimetria, não o proxy.

## 3. Gate proposto — `tests/b2_rerank_gate.rs`

Dados: rodadas `20261117_b0_paws_s0/s1` (já completas) + `emb/bank.f32`.

1. **Routing em bits** (Hamming no bank HV do shard) → top-K=32 candidatos;
2. **Re-rank f32**: cosseno no `emb/bank.f32` entre a query s2 (768d) e os
   candidatos (768d); rank-1 do re-rank;
3. Métricas por shard: `recall1_bits`, `recall1_bits_then_float`, p99 (µs),
   e a versão **duas direções** (fato indexado em `s1|rel|s2` e `s2|rel|s1`
   — correção alternativa da assimetria declarada no `b0_shard_gate.rs`).

| Critério | Alvo |
|---|---|
| `recall1_bits_then_float` (assimétrico, s2→fato) | **≥ 0,90** (teto A3, sem baixar) |
| Duas direções ≥ pior das duas | registrar (diagnóstico da assimetria) |
| p99 por consulta (bits+f32, n=2500) | ≤ 5 ms (teto B-G1) |
| Alloc no caminho armado (Rust) | 0 B |

Artefato: `target/b2_rerank_gate.json` · Sem os dados: skip silencioso
(convenção). Se o re-rank f32 com s2 sozinho **não** chegar a 0,90, o
resultado é **achado negativo honesto** — o caminho seguinte é indexar as
duas direções (item 3) e re-medir; nunca afrouxar o teto.

## 4. Orçamento de latência (doutrina T1/T2)

Routing bits no shard ≤4096: ~µs (medido p99 642 µs no B1 10k). Re-rank f32
top-32: 32×2 dot products @768 — dezenas de µs em Rust (sem `String` no
sinal). Total dentro do teto p99 ≤ 5 ms com folga; o 30,9 s do probe Python
não é o caminho de produção (sidecar de medição).

## 5. Relação com o freeze 0.1.0

Fora do freeze (§2.3 do `V01_DELIVERY_PLAN.md`). Não bloqueia tag; alimenta
a trilha R com o mesmo padrão de honestidade: registrar FALHOU com número,
nunca em silêncio.

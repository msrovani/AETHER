# INTAKE_TRANSPLANT — destilação offline + leitor HV

> CLAIM fórum: **MSG 0229** · Tópico `INTAKE_TRANSPLANT`  
> Doador padrão: `Qwen/Qwen3-Embedding-0.6B` (Apache-2.0) · D_HV = 10.240 bits  
> T0 / `step_into` **intocados**. Núcleo recebe só binário.

## Frase-guia

Não adaptar o organismo; adicionar pipeline de destilação offline + leitor T1.
Geometria ≠ verdade (Vol. II §4). Certificado = factibilidade + proveniência.

## Artefatos

| Path | Papel |
|------|-------|
| `data/manifest.json` | durável (modelo@rev, seed, D, count, hashes) |
| `data/hv/bank.bin` | derivado (header 128 B + u64[160]/átomo) |
| `data/hv/bank.idx` | derivado (24 B/átomo, key u128) |
| `data/emb/whitening.bin` | durável (μ, W ZCA) |
| `target/aether_transplant_hv.json` | gates (descartável) |

## Header `bank.bin` (128 B, little-endian)

| Offset | Campo | Tipo |
|--------|-------|------|
| 0 | magic | `b"AETHV1\0\0"` (8) |
| 8 | version | u32 = 1 |
| 12 | dim_bits | u32 = 10240 |
| 16 | words | u32 = 160 |
| 20 | dim_in | u32 (768 ou 1024) |
| 24 | flags | u32 bit0 = whitened |
| 28 | count | u64 |
| 36 | seed | u64 |
| 44 | sha256_payload | [u8;32] |
| 76 | pad | até 128 |

Payload: `count × words` × u64 LE.

## `bank.idx` (24 B/átomo)

`key_lo u64` · `key_hi u64` · `kind u8` · `lang u8` · `pad[6]`

## ETL (`tools/harvest_transplant.py`)

1. Coleta/tiering (authority: HITL 255 · norma 200–240 · LLM 60–120 · web 30–80)
2. Extração (dry-run sintético; produção: GLiNER/REBEL → Qwen2.5-7B)
3. Dedup + ZCA whitening + L2
4. JL + sign → 160×u64 (`ChaCha8Rng` seed fixa)
5. Shards FHRR ≤48/relação (facts.jsonl)
6. Pack + manifesto; strings → SGDB (não no binário)

```powershell
python tools/harvest_transplant.py --limit 200 --dry-run --out data
```

## Leitor (example → core após gates)

```rust
HvBank { dim, words, bits, keys }
load / project_into / hamming / top_k_into  // 0-alloc no caminho armado
```

## Gates (8)

| Gate | Alvo |
|------|------|
| Spearman | ≥ 0,95 |
| Recall@10 | ≥ 0,90 |
| Distorção JL | ≤ 12% |
| Determinismo | bytes idênticos |
| Latência 10k | ≤ 1,5 ms |
| Alloc consulta | 0 B |
| Conflitos | ≥ 95% |
| Rebuild shards | byte-idêntico |

## Authority

Transplantado: `provenance=Transplanted`, `authority ≤ 240`. **255 = HITL exclusivo.**

## Promoção

Só após gates verdes: nova CLAIM + `src/core/hv.rs` + 1 linha em `mod.rs`.
Claims alheios (`transplant.rs`, `logic_pipeline.rs`, …) **fora**.

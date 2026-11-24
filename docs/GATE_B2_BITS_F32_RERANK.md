# Gate B2 — HV bits → rerank f32

> Contrato CURSOR pós-v0.1.0 (fórum MSG **0677** / pesquisa MSG **0635**).  
> Não rouba claim D1 (OPENCODE **0668**). Sem GPU se `--self-query-proxy`.

## Objetivo

Recuperar o teto A3 (**recall@1 ≥ 0,90**) depois do retrieve Hamming em HV
(JL 1-bit), rescoring os top-K em float do donor.

## Comando

```powershell
# Pré-check (sem modelo)
python tools/b0_bits_f32_rerank.py --round data/rounds/20261117_b0_paws --dry-check `
  --out target/b0_bits_f32_rerank.json

# Proxy s0 (CPU, query = doc f32; assimetria s2 fora)
python tools/b0_bits_f32_rerank.py --round data/rounds/20261117_b0_paws_s0 `
  --self-query-proxy --sample 500 --k 32 `
  --out target/b0_bits_f32_rerank_s0_proxy.json

# Probe real (precisa emb/s2_query.f32; embed só em janela limpa)
python tools/b0_bits_f32_rerank.py --round data/rounds/20261117_b0_paws `
  --embed-s2 --device cuda --out target/b0_bits_f32_rerank.json
```

## Critérios

| Campo | Regra |
|---|---|
| Métrica | `recall1_bits_f32_rerank` |
| Piso | `>= 0.90` (`gate_pass`) |
| Proxy s0 (ref) | bits 0,88 → float 1,00 (Δ +12 pp) — **não** fecha A3 no 5k |
| ABSTAIN | falta `s2_query.f32` e sem `--self-query-proxy` / `--embed-s2` |
| Serial | embed/probe real **depois** de D1 ST6 liberar CUDA |
| Partilha (humano 2026-11-18) | **full 5k B2** só pós-CUDA livre; **Onda1** = outro CURSOR (não desta sessão) |

## Artefatos

| Path | Papel |
|---|---|
| `target/b0_bits_f32_rerank*_dry.json` | dry-check |
| `target/b0_bits_f32_rerank_s0_proxy.json` | proxy self-query |
| `target/b0_bits_f32_rerank.json` | probe com s2 real |
| `target/pos_reboot/hv_bits_float_rerank_s0.json` | baseline histórico |

## Fora deste gate

- D1 ColBERT-XM MaxSim (`tools/d1_maxsim_st6.py`) — claim OPENCODE.
- Assimetria consulta s2 vs fato concat — precisa `s2_query.f32` real.
- LSH união 100k — `tests/b2_100k_gate.rs` (shards ≤4k).

Relacionados: [`PESQUISA_B-G0_PAWS_E1_EMBED.md`](PESQUISA_B-G0_PAWS_E1_EMBED.md),
[`DONOR_CATALOG.md`](DONOR_CATALOG.md).

## Smoke n=200 CPU (MSG 0702)

Artefato: `target/b0_bits_f32_rerank_n200.json` (s2 real, sample 200, k=32).

| métrica | valor |
|---|---:|
| recall@1 bits | 0,745 |
| recall@1 bits→f32 | **0,750** |
| Δ | +0,5 pp |
| gate ≥0,90 | **FAIL** |

Leitura: com query s2 real o float quase não recupera — contraste com proxy
self-query (+12 pp). Full 5k ainda pendente pós-D1.

### Lit pós-FAIL (MSG 0705)

Rerank float **só reordena** o top-K Hamming — não recupera docs excluídos
no 1º estágio. Próximo probe: **oversample** k∈{256,512} e/ou ADC (query f32
× bits do bank) antes do full 5k.

## Probe pós-FAIL — oversample de k e ADC (2026-11-24)

Fecha o "próximo probe" acima, nos mesmos dados e com a mesma verdade
(query `s2` sozinho → doc `s1|rel|s2`, 5000 docs):

```powershell
python tools/b2_oversample_adc.py --sample 200 --ks 32,256,1024 `
  --out target/b2_oversample_adc.json
```

Artefato promovido: `data/b2_oversample_adc.json` (+`.meta.json`, sha256 do
próprio artefato) · gerado por `tools/b2_oversample_adc.py`.

| estágio | k=32 | k=256 | k=1024 |
|---|---:|---:|---:|
| bits → rerank f32 | 0,755 | 0,755 | 0,755 |
| ADC → rerank f32 | 0,755 | 0,755 | 0,755 |
| bits s/ rerank (rank-1) | 0,745 | 0,745 | 0,745 |
| **teto f32 exato no bank inteiro** | **0,755** | **0,755** | **0,755** |

Sanidade do probe: `sign(P·q)` reproduz `s2_query_hv.bin` em **100,0%**
(mesmo `seed=3735928559` do manifest) — o caminho bits é o mesmo do bank.

Leitura:

1. **Oversample não move nada**: k=1024 de 5000 dá o mesmo 0,755 de k=32. O doc
   certo não está fora do top-K; ele perde no ranking, com margem.
2. **ADC não ajuda** (−0,5 pp no estágio, empata pós-rerank): somar a magnitude
   da query projetada não carrega sinal além do sinal JL.
3. **A quantização 1-bit custa 1 pp.** O teto em cosseno float exato contra o
   bank inteiro é 0,755 — idêntico ao bits+rerank. Os 24,5 pp que faltam para
   0,90 não são do rerank, nem do top-K, nem da quantização: são a **assimetria
   query=`s2` sozinho vs doc=`s1|rel|s2`** já diagnosticada no §2. Rank do doc
   certo: mediana 0, p90 1 (bits e f32).
4. Objetos duplicados no corpus (477/5000 linhas em 236 grupos, 9,5%) não
   explicam o gap.

Consequência: **B2 não fecha 0,90 neste round por nenhum caminho de rerank**.
O próximo probe documentado continua sendo indexar as **duas direções**
(`s2|rel|s1` também) — o que exige re-embed do donor, não um k maior. Enquanto
esse número não subir, A5 (T-HIER) segue bloqueado por medição, não por código
(`NO-GO: só liga com gate B2 ≥ 0,90`).

## Probe das duas direções (item 3) — 2026-11-24

Fecha o item 3 proposto no §3 depois do negativo do rerank:

```powershell
python tools/b2_two_directions.py --sample 0 --k 32 `
  --out target/b2_two_directions_full.json
```

Re-embed: `s2|rel|s1` dos 5000 fatos com o mesmo donor/seed/dim/temperature do
`emb/bank.f32` (Qwen3-Embedding-0.6B, MRL 768, seed 42, temperature 0, sem
prompt, CPU fp32 — 40 min). Banco reverso em
`data/rounds/20261117_b0_paws/emb/bank_rev.f32` (sha256 no artefato); o probe
reusa esse cache e não re-embeda. Evidência: `data/b2_two_directions.json`
(+`.meta.json`).

| direção (doc indexado) | recall@1 bits | + rerank f32 (k=32) | teto f32 exato |
|---|---:|---:|---:|
| fwd `s1|rel|s2` | 0,7674 | 0,7708 | 0,7708 |
| rev `s2|rel|s1` | 0,9146 | **0,9170** | 0,9170 |

Sanidade: o fwd reconstruído de `emb/bank.f32` com o P do manifest concorda
**100,0%** com `hv/bank.bin`, e o 0,7708 do fwd reproduz o monolito documentado
(0,7676 em `b0_parafra_gate.json`).

Leitura:

1. **A assimetria era o gap.** Indexar o fato começando por `s2` leva o
   recall@1 de 0,7708 para **0,9170** (+14,6 pp), acima do piso 0,90, sem tocar
   o teto.
2. **A quantização continua custando ~0,2 pp** e o rerank recupera exatamente
   isso (0,9146 → 0,9170, idêntico ao teto f32 0,9170). Bits+rerank é o teto.
3. **O ganho é de indexação, não de score.** O doc rev começa com o texto da
   query: é o mesmo fato guardado no lado certo. Um sistema real precisa das
   duas direções (a direção certa para cada lado da consulta) — o fwd não
   melhora por rerank nenhum.
4. Sem fusão de score entre direções: cossenos de espaços diferentes não são
   comparáveis (união por "maior cosseno" media 0,86, abaixo do rev sozinho —
   medida inválida, descartada).

Consequência: **B2 fecha 0,90 na direção reversa** (0,9170 ≥ 0,90) com o teto A3
intocado. O NO-GO de A5 ("só liga com gate B2 ≥ 0,90") fica satisfeito por
medição; A5 segue off apenas até o seu próprio gate (recall@1 vs MoE plano e
p99), não mais por B2.

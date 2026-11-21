# Playbook de Doação — ETL, transplante e nova rodada de aprendizado

> Documento operacional. Objetivo: **clonar** o processo para uma nova rodada
> sem reinventar arquitetura.  
> Spec técnica: [`INTAKE_PLAN.md`](INTAKE_PLAN.md) · Números da 1ª rodada: [`TRANSPLANT_HV.md`](TRANSPLANT_HV.md)  
> CLAIM fórum: MSG **0229** / **0244** / **0245** · Tópico `INTAKE_TRANSPLANT`  
> Pack → MoE (analista Python): [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md).

---

## 0. Em uma frase

**Doação** = importar um **mapa** (geometria) e, opcionalmente, um **escriba**
(triplas), **nunca** uma mente (pesos/tokenizer). O resultado é um banco HV de
**10.240 bits/átomo** + fatos no SGDB, com `provenance=Transplanted` e
`authority` em quarentena **60–120** (teto duro ≤240; 255=HITL).
O organismo a 146 µs **não** muda.

```text
corpus ──► ETL (offline) ──► data/ (HV + manifest + facts)
                │
                ├─► MAPA (embedder)  → embeddings → JL+sign → bank.bin
                └─► ESCRIBA (LLM)    → triplas    → SGDB + shards FHRR
```

---

## 1. Vocabulário (não misturar)

| Termo | Significa | Não significa |
|-------|-----------|---------------|
| **Doador** | Par mapa+escriba (modelos HF) | O AETHER em runtime |
| **Mapa** | Embedder (ex. Qwen3-0.6B) | LLM de 7B/70B |
| **Escriba** | Extrator de triplas (ex. Qwen2.5-7B) | Embedder |
| **Doação / transplante** | Geometria densos → HV binários | Copiar pesos |
| **ETL** | Lote offline que gera `data/` | Hot path 60 Hz |
| **HvBank** | View materializada de bits | Fonte de verdade |
| **SGDB** | Cartório (texto, autoridade, citação) | Índice de similaridade rápida |
| **Certificado** | Factibilidade + evidência | Verdade do mundo |

**Regra de ouro (derivado vs fonte):**

- Se dá para recalcular de `(SGDB + modelo@rev + seed)` → **cache** (apagável):
  `bank.bin`, `bank.idx`, embeds, shards.
- Se não dá → **registro** (durável): SGDB + `manifest.json` (+ whitening se usado).

---

## 2. Papéis e licenças (decidir antes de baixar)

| Papel | Modelo padrão | Licença | Uso |
|-------|---------------|---------|-----|
| Mapa | `Qwen/Qwen3-Embedding-0.6B` | Apache-2.0 | **Produto** |
| Mapa (alt.) | `nomic-ai/modernbert-embed-base` / Arctic-Embed-M-v1.5 | Apache-2.0 | Retrieval EN; não “conserta” PAWS |
| Mapa (PT-BR lab) | `tardellirs/colibri-embed-ptbr` | Gemma | Só se produto permitir |
| Extrator | `fastino/gliner2.5-base-v1` (JointIE) | Apache-2.0 | **Preferido** para triplas tipadas |
| Escriba | `Qwen/Qwen2.5-7B-Instruct` | Apache-2.0 | Batch offline (relações livres) |
| Escriba barato | `mohar07/qwen3-0.6b-kg-triplets` | MIT | Ontologia 20 rel.; quarentena |
| — | REBEL (código NC) / `victorlxh/ICKG-v3.2` | NC | **Proibido** |

Catálogo completo + ranking: [`DONOR_CATALOG.md`](DONOR_CATALOG.md).

- Dimensão operacional do mapa: **`dim_in = 768`**.
  Qwen3-Embedding-0.6B é **nativo 1024** com MRL (32…1024); em produção pedir
  `dimensions=768` / `truncate_dim=768` (ou truncar primeiros 768 + L2). Apache-2.0.
- HV: **`dim_hv = 10240`** → **160 × u64** = **1,28 KB/átomo**.
- Capacidade E1B: **≤ 4096** átomos/shard (`0,4 × D`); 100k ⇒ ~25 shards.
- Authority: ETL grava **60–120** (`authority_max_transplant=120` no harvest);
  teto duro ≤**240**; **255 = HITL exclusivo**. Docs antigos que dizem só “≤240”
  referem o teto, não o default de geração.
- **B-G0 (errata 0567):** cosine donor real AUC **≈0,6909** (0,4945 era bug de
  parser). E1 n=5k ≈0,5983 **abaixo** do donor; D1 MaxSim real em curso (0581).

---

## 3. Onde cada coisa mora

| Camada | Local | Regenerável? |
|--------|-------|--------------|
| Fonte de verdade | **neural-sgdb** (externo) | Não |
| Manifesto da rodada | `data/manifest.json` | Não (assinatura da rodada) |
| Whitening ZCA | `data/emb/whitening.bin` | Não (se a rodada usou) |
| Embeddings cache | `data/emb/bank.f32` | Sim (do mapa + corpus) |
| HV binário | `data/hv/bank.bin` + `bank.idx` | Sim (embeds + seed + P) |
| Fatos / conflitos | `data/facts.jsonl` | Sim (do escriba + corpus) |
| Ledger mock (CI) | `data/sgdb_mock_ledger.json` | Sim |
| Gates | `target/aether_transplant_hv.json` | Descartável |

**Núcleo Rust** (`src/core/hv.rs`): só lê bits/ids — **zero `String` no sinal**.

**Latência:**

| Faixa | Orçamento | Quem |
|-------|-----------|------|
| T0 | 146 µs | organismo / solver — **intocado** |
| T1 | ~1 ms | `HvBank::top_k_into` (~10k átomos) |
| T2 | 10–50 ms | SGDB (citação, autoridade, conflito) |

---

## 4. Processo ETL — 6 estágios (o que o script faz)

Ferramenta: [`tools/harvest_transplant.py`](../tools/harvest_transplant.py)

### Estágio 1 — Coleta e tiering

Cada fonte entra com: `authority`, `wall_ms`, `url/hash`, `lang`.

| Fonte | Authority |
|-------|-----------|
| HITL | **255** |
| Norma / doc oficial | 200–240 |
| LLM / escriba | 60–120 |
| Web aberta | 30–80 |

### Estágio 2 — Extração (escriba)

- **Dry-run (CI/lab sem rede):** corpus sintético PT-BR + conflitos plantados.
- **Produção:** GLiNER+REBEL (vazão) → Qwen2.5-7B JSON estrito (precisão).
  **2026-11-18:** tool própria `tools/scribe_extract.py` (Qwen3-8B default,
  IFStruct 79,75; JSON estrito, T=0, NFKC, dedup, quarentena 60–120). REBEL é
  CC BY-NC — evitado (ver [`RESEARCH_DOADORAS_2026.md`](RESEARCH_DOADORAS_2026.md)).
- Emite: triplas `(sujeito, relação, objeto, confiança, citação)` e, se houver
  regra verificável, cláusulas CNF (futuro Logos).

### Estágio 3 — Dedup + branqueamento

1. Normalizar NFC (preservar caixa de entidades).
2. Dedup por hash do trio / rótulo de átomo.
3. Só conceitos únicos passam pelo embedder.
4. Opcional ZCA: `x' = W(x − μ)` + L2; gravar `whitening.bin` e hash no manifest.
5. **Ablação obrigatória:** rodar com e sem `--no-whiten`; reportar os dois no RELATORIO.

### Estágio 4 — Projeção JL + binarização

```text
x (f32[dim_in], L2)  →  y = Pᵀ x  →  bit = (y ≥ 0)  →  160 × u64 LE
```

- `P` gaussiana semeada (`--seed`); **nunca** `thread_rng`.
- Header AETHV1 registra `dim_in`, `seed`, `flags.whitened`, `sha256_payload`.
- Margem JL cai com `n` átomos: se `n > 50k`, **suba D** ou declare ε no manifest.

### Estágio 5 — Shards FHRR (fatos)

- Particionar por **relação**; **≤ 48 fatos/shard** (teto medido ~64 @ D=160).
- Keys `u128` = SHA-256 do rótulo normalizado (16 bytes LE).
- **Não** meter o vocabulário 50k–320k no FHRR-160.

### Estágio 6 — Empacotamento + cartório

- Escreve `bank.bin` / `bank.idx` / `manifest.json` / `facts.jsonl`.
- Strings e citações → SGDB (`remember` + `audit_checkpoint`).
- Conflito → `contradicts` (**nunca** sobrescreve o fato anterior).

---

## 5. Como clonar — nova rodada de aprendizado

Trate cada rodada como um **lote versionado**. Não misture seeds/manifests.

### 5.1 Checklist antes de começar

1. [ ] Abrir `CLAIM` no fórum (`INTAKE_TRANSPLANT` ou tópico da rodada).
2. [ ] Fixar doador (mapa + escriba) e anotar **revision/commit HF**.
3. [ ] Escolher `seed` novo (u64) e `dim_in` (768 padrão).
4. [ ] Escolher diretório de saída: `data/rounds/YYYYMMDD_nome/` (não sobrescrever a rodada anterior às cegas).
5. [ ] Confirmar licença Apache (ou waiver Gemma se colibri).
6. [ ] Corpus da rodada: path + hash (ou dry-run sintético).

### 5.2 Comandos — rodada lab (dry-run, sem rede)

```powershell
# A partir da raiz do repo AETHER
$ROUND = "data/rounds/$(Get-Date -Format 'yyyyMMdd')_lab"
New-Item -ItemType Directory -Force -Path $ROUND | Out-Null

# 1) ETL
python tools/harvest_transplant.py `
  --limit 200 `
  --dry-run `
  --out $ROUND `
  --seed 0xDEADBEEF `
  --dim-in 768 `
  --no-whiten

# 2) Ablação whitening (opcional mas recomendada)
python tools/harvest_transplant.py `
  --limit 200 --dry-run --out "${ROUND}_whiten" --seed 0xDEADBEEF

# 3) Gates
# (probe lê data/ por padrão — copie ou use junction)
Copy-Item -Recurse -Force "$ROUND\*" data\
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
.\scripts\cargo.ps1 test --release --test transplant_hv_gate

# 4) Arquivar resultado
Copy-Item target/aether_transplant_hv.json "$ROUND/aether_transplant_hv.json"
```

### 5.3 Comandos — rodada produção (mapa real via sidecar)

Pesos HF **nunca** entram no crate. Sidecar Python → `emb/bank.f32`; harvest só JL.

```powershell
$ROUND = "data/rounds/$(Get-Date -Format 'yyyyMMdd')_prod_qwen3"
New-Item -ItemType Directory -Force -Path $ROUND | Out-Null
# 0) Partir de facts (dry-run ou escriba)
python tools/harvest_transplant.py --limit 200 --dry-run --out $ROUND --no-whiten
# 1) Mapa real MRL 768 (Apache) — requer sentence-transformers
python tools/donor_map_embed.py --out $ROUND --donor Qwen/Qwen3-Embedding-0.6B --dim 768
# 2) JL-only (sem re-sintetizar embeds)
python tools/harvest_transplant.py --jl-only --out $ROUND --seed <NOVO> --revision <hf_rev> --no-whiten --donor Qwen/Qwen3-Embedding-0.6B
# 3) Apontar data/ → rodada; probe + gate + uso
Copy-Item -Recurse -Force "$ROUND\*" data\
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
python tools/transplant_uso.py
# 4) (opcional) remember SGDB real + RELATORIO
```

**Contrato do artefato não muda** entre dry-run e produção: só mudam
`manifest.donor`, `revision` e a qualidade dos embeds/triplas.

### 5.4 O que mudar a cada rodada (e o que não)

| Mudar | Não mudar |
|-------|-----------|
| `seed` | `dim_hv = 10240` (salvo decisão explícita de subir D) |
| corpus / `--limit` | Formato AETHV1 (magic/header) |
| `donor` + `revision` | T0 / `step_into` |
| authority por fonte | Cap 240 / HITL 255 |
| whitening on/off | Proibição de `String` no núcleo |
| pasta `data/rounds/...` | Claims alheios (`transplant.rs`, etc.) |

### 5.5 Critério de “rodada aceita”

1. `manifest.json` gravado com `sha256_bank`, `seed`, `donor`, `count`.
2. `target/aether_transplant_hv.json` (ou cópia na pasta da rodada) com `"pass": true`.
3. Oito gates (ver §6) verdes.
4. `RELATORIO` no fórum com os números + path da rodada.
5. SGDB: 100% dos `key` do `bank.idx` têm registro (citação + authority).

Se falhar Spearman/JL: **não** baixe o gate — ajuste corpus/seed/D ou declare ε.

---

## 6. Gates (obrigatórios)

| Gate | Alvo |
|------|------|
| Spearman (1−cos ↔ Hamming), convenção E3 | ≥ 0,95 |
| Recall@10 | ≥ 0,90 |
| Distorção JL média | ≤ 12% |
| Determinismo (reload) | bytes idênticos |
| Latência top-k @ ≤10k | ≤ 1,5 ms |
| Alloc no caminho armado | **0 B** |
| Conflitos detectados / plantados | ≥ 95% |
| Rebuild plano de shards | idêntico ao manifest |

Leitor: `examples/transplant_hv_probe.rs` · CI: `tests/transplant_hv_gate.rs` · API: `src/core/hv.rs`.

---

## 7. Fluxo de consulta (após a doação)

```text
pergunta (borda)
   │
   ├─► embedder (T1/T2, fora do 60 Hz)
   ├─► JL+sign → query HV
   ├─► HvBank::top_k_into  → ids
   ├─► SGDB.resolve(ids)   → texto, citação, authority
   └─► Certificate + Evidence(Hypervector, authority≤240)
         └─► decide_with_evidence → Act/Cite/Propose/Abstain
```

O organismo **não** espera o HV no `step_into`. Pedido assíncrono (ring) se
integrado ao loop; senão, só tools/API de lab.

---

## 8. Arquivos canônicos (mapa mental)

```text
tools/harvest_transplant.py          # ETL
tools/scribe_extract.py              # escriba: triplas LLM → facts.jsonl (quarentena 60–120)
tools/transplant_uso.py              # U1–U6 → target/aether_transplant_uso.json
examples/transplant_hv_probe.rs      # medição + JSON
tests/transplant_hv_gate.rs          # CI
src/core/hv.rs                       # HvBank + LSH (>10k)
src/core/certificate.rs              # Evidence (append)
src/knowledge/sgdb_bridge.rs         # mock + loader
src/dashboard/transplant_view.rs     # hub: origem/destino/gates/uso
data/manifest.json                   # assinatura da rodada ativa
data/hv/bank.bin | bank.idx
data/emb/bank.f32 | whitening.bin
data/facts.jsonl
target/aether_transplant_hv.json     # gates probe
target/aether_transplant_uso.json    # usabilidade
docs/DOACAO_PLAYBOOK.md              # este arquivo
docs/INTAKE_PLAN.md                  # schema binário
docs/TRANSPLANT_HV.md                # números da 1ª rodada
```

---

## 9. Anti-padrões (já pagos)

1. Copiar pesos / tokenizer para o crate.
2. Meter HV no ciclo de 146 µs.
3. Usar Colibri em produto sem ler licença Gemma.
4. Guardar 10.240 bits dentro do SGDB (infla sem ganho).
5. Empilhar >48 fatos no mesmo shard FHRR.
6. Declarar gate verde sem JSON numérico.
7. `Set-Content -Encoding UTF8` em `.rs`/`.md` (duplo-encode).
8. Sobrescrever `data/` de uma rodada sem arquivar o `manifest` anterior.

---

## 10. Template de RELATORIO (nova rodada)

```text
RELATORIO:INTAKE_TRANSPLANT
rodada=<YYYYMMDD_nome>
donor=<modelo>@<revision>
seed=<u64>
count=<n>
whitened=<true|false>
spearman=...
recall10=...
jl_dist=...
lat_ms=...
alloc=...
conflicts=...
pass=true|false
path=data/rounds/<nome>/
json=.../aether_transplant_hv.json
```

---

## 11. Próxima rodada — receita mínima

```powershell
# 1. CLAIM no fórum
# 2. Nova pasta + novo seed
# 3. ETL
python tools/harvest_transplant.py --limit 200 --dry-run --out data/rounds/NOVA --seed <SEED>
# 4. Apontar data/ → rodada (ou copiar)
# 5. Probe + gate
.\scripts\cargo.ps1 run --release --example transplant_hv_probe --features m1
.\scripts\cargo.ps1 test --release --test transplant_hv_gate
# 6. Se pass: RELATORIO + (opcional) remember no SGDB real
# 7. Se fail: não publicar; ajustar corpus/seed; repetir
```

Isso é tudo que um agente novo precisa para **clonar o aprendizado** sem tocar no organismo.

---

## 12. Origem (doação) — o que entra

| Campo | 1ª rodada lab (2026-11-16) | Notas |
|-------|----------------------------|-------|
| Doador (mapa) | `Qwen/Qwen3-Embedding-0.6B#dry-run-synthetic` | Produção: revision HF real |
| Escriba | sintético PT-BR (dry-run) | Prod: Qwen2.5-7B Instruct |
| Licença mapa | Apache-2.0 | Obrigatório em produto |
| `dim_in` | 768 | MRL truncate se 1024 |
| `dim_hv` / words | 10240 / 160 | 1,28 KB/átomo |
| `seed` | `0xDEADBEEF` (3735928559) | Novo a cada rodada |
| `count` / `facts` | 205 / 205 | `--limit 200` + conflitos |
| Whitening | **off** (`--no-whiten`) | Ablação recomendada |
| `sha256_bank` | `9b8bfadd…c06f8f` | Assinatura da rodada |
| `sha256_embeds` | `815e2fa5…2ffa1d` | Cache regenerável |
| Conflitos | 5 plantados / 5 detectados | Cartório `contradicts` |
| Authority máx. | 240 | 255 = HITL |
| CLAIM | MSG 0229 / 0244 / 0245 | Tópico `INTAKE_TRANSPLANT` |
| ETL | `tools/harvest_transplant.py` · 6 estágios | Offline; nunca T0 |

Shards FHRR (manifest): `capital`20 · `é_um`41 · `é`20 · `é_uma`48+14 · `orbita`21 · `usa`21 · `minimiza`20.

---

## 13. Destino (transplante) — o que o AETHER recebe

| Peça | Path / API | Papel |
|------|------------|-------|
| `bank.bin` + `bank.idx` | `data/hv/` | Bits + keys u128 |
| `HvBank` | `src/core/hv.rs` | `load` · `top_k_into` · LSH se >10k |
| SGDB bridge | `src/knowledge/sgdb_bridge.rs` | mock ledger + `try_load_hv_bank` |
| Evidence | `certificate.rs` | `EvidenceSource::Hypervector` · auth≤240 |
| Decisão | `decide_with_evidence` | Act / Cite / Propose / Abstain |
| Hub | painel **Doação · ETL · Transplante** | `transplant_view.rs` |
| Job | **Probe doação HV** | harvest (se faltar) + probe + gate |

**Invariantes de destino:** T0 / `step_into` intocados · zero `String` no sinal · HV só T1/T2.

---

## 14. Estatísticas e usabilidade (medidas reais)

### 14.1 Gates INTAKE (`target/aether_transplant_hv.json`)

| Métrica | Valor | Alvo | Status |
|---------|------:|------|--------|
| Spearman | 0,9539 | ≥ 0,95 | PASS |
| Recall@10 | 1,0000 | ≥ 0,90 | PASS |
| JL distorção | 0,0084 | ≤ 0,12 | PASS |
| Latência top-k | 0,006 ms | ≤ 1,5 ms | PASS |
| Allocs query | 0 | 0 | PASS |
| Conflitos | 5/5 | ≥ 95% | PASS |
| Determinismo / rebuild | true | idêntico | PASS |
| **pass** | **true** | | |

### 14.2 E3 geometria (`target/aether_e3.json`)

| Métrica | Valor |
|---------|------:|
| Spearman | 0,9528 |
| Relação R@1 | 0,9990 |
| Analogia R@1 / R@5 | 0,9549 / 1,0000 |
| Falsos confiáveis | 0,00% |
| Verificador | 0,004 ms · 0 alloc |

### 14.3 Usabilidade U1–U6 (`target/aether_transplant_uso.json`)

| ID | Cenário | Resultado |
|----|---------|-----------|
| U1 | Auto-recuperação top-1 (5 queries) | **5/5** |
| U2 | Resolve SGDB (fato+cite+auth≤240) | **9/9** |
| U3 | Vizinhança por relação (Rust→Python) | **PASS** |
| U4 | Conflitos → Abstain/Propose | **5/5** |
| U5 | Evidence Hypervector Act | **5/5** |
| U6 | Latência Rust HvBank | **0,006 ms** |
| CI | `transplant_hv_gate` 5 · `e3` 6 · `e3b` 5 · `f1` 4 | **PASS** |

### 14.4 Hub

```powershell
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Painel central: origem · destino · gates · E3 · U1–U6. Hover → coluna **Detalhe**.
Job **Probe doação HV** regenera gates; `Recarregar` relê JSONs.

---

## 15. Fechamento de ciclo lab (2026-11-17 → 2026-11-18)

Lab dry-run + hub + Onda 0 + B0 mínimo (`--jl-only` + sidecar) **fechados**.
Documento canônico: [`CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md`](CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md).
Doadoras: [`DONOR_CATALOG.md`](DONOR_CATALOG.md) · fórum MSG **0522**.

| ID | Status | Nota |
|----|--------|------|
| B0 sidecar / `--jl-only` | **fechado** | Qwen3-Emb-0.6B @768 |
| B1 LSH ≥10k | **fechado** (lab) | round `20261117_b1` |
| JL-V4 | **fechado** | tiled `act` |
| B-G0 cosine PAWS | **donor ~0,69** | errata parser 0567; não era acaso |
| B-G0′ / E1 PAWS | **abaixo do donor** | n=5k AUC≈0,5983; alvo D1 ≥0,69 (claim 0581) |
| B2 shards ≤4k | **em curso** | lei E1B 4096 (`hv.rs`) |
| Escriba-0 | **aberto** | GLiNER2.5 → `facts.jsonl` (scaffold `tools/escriba_gliner.py`) |
| SGDB-remember ETL | **aberto** | mock ≠ cartório real |
| B4 produção (doação) | **aberto** | ≠ B4-NysHD do plano arxiv |
| FFT-G2 / MCTS | board ortogonal | não bloqueia doação |

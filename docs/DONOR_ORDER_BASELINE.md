# Doadoras de conhecimento — a régua real e a errata do 0,4945

> **BUFFY** · MSG **0567** · 2026-11-18 · complementa `DONOR_CATALOG.md` (CURSOR) e
> `RESEARCH_DOADORAS_2026.md` (OPENCODE). T0 intocado; nada aqui entra no crate:
> é régua de ETL offline (sidecar).
> Artefatos: `target/b0_order_probe.json` · `target/b0_parafra_gate.json` (re-rodado)

---

## 0. Resumo em três linhas

1. A AUC **0,4945** do B-G0, que motivou o redesenho do B4 (MSG 0539), é **bug de
   parser** no gate — não propriedade do donor. Alinhada, a mesma rodada dá
   **0,6909** (n=5000).
2. Consequência dura: a régua de qualquer mecanismo novo (E1, MaxSim, NysHD) é
   **0,69**, não 0,51. A E1 scaffold (**0,6054**) está **abaixo** do donor puro.
3. Com classes balanceadas, a acurácia de **qualquer** regra de limiar é **≤ AUC**.
   Gate que exige `acc ≥ 0,70` com `AUC = 0,6909` é insatisfazível por construção.

---

## 1. Errata 0,4945 → 0,6909 (causa raiz)

`tests/b0_parafra_gate.rs::json_parse` fatia os arrays como texto:

```text
"cosines": [0.9801689676569367, 0.9643286585960595, ...]
            ^ json.dump deixa um espaço depois dos dois-pontos
```

`cos_part` começa em `" [0.98…"`. `trim_start_matches('[')` **não** remove o
colchete porque o primeiro caractere é espaço; o primeiro token vira
`" [0.9801689676569367"` e `parse::<f64>()` falha. O `filter_map(parse().ok())`
**descarta em silêncio**, e o `zip` com 5000 rótulos trunca em **4999** →
**todos os rótulos deslocados em 1**.

Prova numérica (independente do Rust):

| cenário | n | AUC |
|---|---:|---:|
| alinhado (correto) | 5000 | **0,6909** |
| alinhado, prefixo | 2000 | 0,6921 |
| alinhado, prefixo | 400 | 0,7135 |
| deslocado em 1 (`zip` 4999) | 4999 | **0,4943** ← reproduz o gate (0,4945) |

O `assert_eq!(cosines.len(), is_label1.len())` do teste **não podia** pegar:
ele compara depois do `zip`, que já truncou.

**Fix aplicado** (sem afrouxar teto): `.trim()` antes do `trim_start_matches('[')`
nos dois arrays; `parse::<f64>().expect(...)` no lugar do descarte silencioso;
`assert_eq!` de comprimento **antes** do `zip`.

---

## 2. O que o donor entrega de verdade (medido)

Sonda própria (`tools/b0_order_probe_buffy.py`, CPU fp32, mesmos pares PAWS-x,
seed 7, n=400, nenhum download novo):

| codificação | AUC | margem cos (par − não par) | leitura |
|---|---:|---:|---|
| `A_last` (pool do donor) | 0,7239 | +0,0317 | baseline do mapa |
| `A_mean` (média por token) | 0,7296 | +0,0165 | empata com `A_last` |
| `B_pos` — `Σ ρ^{k·t}(x_t)` | **0,6274** | +0,0853 | **ρ sobre tokens pooled PIORA** |
| `C_bigram` — `Σ ρ¹(x_t)·ρ²(x_{t+1})` | **0,7347** | +0,0384 | melhor, mas +0,01 |

Duas rotas independentes (sonda própria e `pair_cosines` arquivado) convergem:
**o mapa Qwen3-Embedding-0.6B separa PAWS-x em ~0,69–0,73**, não no acaso.
Bate com a literatura (MSG 0540): BOW ~0,48–0,55, BERT afinado ~0,88 → o donor
está no meio, como esperado de embedding pooled.

### Leitura técnica

- **O donor não é cego; ele é cego a *ordem* na faixa fina.** 0,69 vem de
  sobreposição lexical, não de estrutura.
- **ρ-bind ingênuo (permutação de coordenadas no vetor do donor) piora** (0,63).
  Permutar o vetor denso do donor destrói a geometria que ele carrega. Ordem
  precisa entrar como **papel/filler** (`rho`/`bind`/`tri_sim` de
  `phase_language.rs`) sobre átomos próprios — não como rotação do embed do donor.
- **Bigrama melhora +0,01**: existe sinal de ordem local, pequeno. É o teto
  barato antes de qualquer coisa nova.
- **`AUC_MAXSIM_LITE = 0,4996` não falsifica ColBERT**: aquele probe hasheia
  token em `N_ENT=64` átomos (MSG 0548, CURSOR). É MaxSim sobre hash, não sobre
  vetor de token do donor.

### Teto aritmético (usar em todo gate de limiar)

Classes balanceadas ⇒ Youden ≤ 2·AUC−1 ⇒ melhor acurácia = (1+Youden)/2 ≤ **AUC**.
Para AUC 0,6909 o máximo é ≈0,69; `acc_covered` medido com a banda atual: **0,6672**
(cobertura 0,761). Gate futuro deve assertar `acc ≤ AUC + ε`, nunca `acc ≥ k`.

---

## 3. Lacuna no catálogo: doadora **por token** (late interaction)

O catálogo atual lista mapas **pooled** (Qwen3, harrier, EmbeddingGemma, Nemotron,
bge-m3). Todos colapsam a frase em **um** vetor — e é exatamente o pooling que
apaga a ordem. Papel que falta: doadora que entregue **vetores por token**.

| repo | licença | saída | contexto | línguas | evidência |
|---|---|---|---|---|---|
| `Qwen/Qwen3-Embedding-0.6B` (**já em cache**) | Apache-2.0 | hidden states por token (decoder), 1024 | 32k | 100+ | sonda §2: por token já disponível, **zero download** |
| `antoinelouis/colbert-xm` | **MIT** | 128-d/token, MaxSim | 256 | 81 (inclui **pt**; mMARCO pt MRR@10 27,6) | retrieval; sem número PAWS |
| `lightonai/LateOn` | **Apache-2.0** | 128-d/token | 300 | EN | BEIR 57,22 |
| `answerdotai/answerai-colbert-small-v1` | **Apache-2.0** | 96-d/token, 33M params | 300/512 | EN | BEIR 53,79 — barato p/ CPU |
| `jinaai/jina-colbert-v2` | **CC-BY-NC-4.0** | 128-d/token | — | multi | **PROIBIDO** (mesma doutrina do REBEL) |

**Fato que a pesquisa não conseguiu derrubar:** não há checkpoint Apache/MIT
*certificado* order-sensitive por cosine pooled; os três permissivos acima dão
**vetores por token**, não garantia de separação. Ou seja:

> A doadora entrega **geometria por token**; a **composição é nossa**. O diferencial
> do AETHER não é a doadora (commodity Apache), é a E1 + certificado + abstenção.

Caminho **D1** (opt-in, fora do T0): tokens do `colbert-xm` (pt nativo) ou do
Qwen3 já em cache → `bind` com papel/posição da E1 → mesmo gate `b0_parafra` →
PASS se AUC ≥ régua §4. Requisito no sidecar: `donor_map_embed.py --per-token`
(arquivo do CURSOR; não editei).

---

## 4. Régua oficial proposta para o B-G0

| mecanismo | AUC | n | observação |
|---|---:|---:|---|
| donor pooled (corrigido) | **0,6909** | 5000 | **baseline obrigatória** |
| donor + bigrama | 0,7347 | 400 | teto barato, ±0,03 |
| E1 roles (scaffold) | 0,6054 | 2000 | **−0,09 vs donor** |
| MaxSim-lite (hash 64) | 0,4996 | 2000 | inválido p/ falsificar late interaction |
| acaso | 0,5000 | — | — |

**PASS de mecanismo novo** = AUC ≥ **0,73** *e* IC bootstrap 95% não sobreposto ao
do donor. Isso **não** baixa teto (0,60 continua piso de abstain); sobe a régua para
o que a doadora já entrega. IC é obrigatório: n=400 → SE ≈ 0,03.

---

## 5. Escriba (complemento ao §2 do catálogo)

| repo | licença | porte | local |
|---|---|---|---|
| `Qwen/Qwen3-4B-Instruct-2507` | Apache-2.0 | 4B (Q4_K_M ~2,5 GB, `unsloth/…-GGUF`) | cabe na 3050 6 GB |
| `Qwen/Qwen3-1.7B` | Apache-2.0 | 1,7B (Q8 ~1,83 GB) | CPU |
| `microsoft/Phi-4-mini-instruct` | **MIT** | 3,8B, pt explícito | CPU/GPU |
| `fastino/gliner2-base-v1` (205M) | Apache-2.0 | schema/JointIE, CPU | EN |
| `knowledgator/gliner-relex-large-v1.0` | Apache-2.0 | entidades+relações | ~1,87 GB |
| `Babelscape/rebel-large` | **CC-BY-NC-SA** | triplas | **PROIBIDO** |

Regras que sobrevivem à troca de escriba: schema fechado no prompt/constrained
decoding; `temperature=0`; JSON inválido **descarta com auditoria** (nunca repara
em silêncio); confiança do LLM é heurística, **não** probabilidade calibrada;
`authority` 60–120 e `provenance=scribe:<modelo>@<rev>|src=<fonte>`.

---

## 6. Pedidos (mediação)

- **OPENCODE / CURSOR**: re-rodar `b0_parafra_gate` e **atualizar** a linha
  “B-G0 AUC≈0,49” em `DONOR_CATALOG.md` §0 e nos relatórios 0520/0539/0540/0550.
  O redesenho do B4 pode continuar válido, mas o **motivo** (“similaridade não
  separa”) não é: separa 0,69.
- **CURSOR**: `b0_e1_order_gate` — comparar a E1 contra **0,69** e reportar IC;
  hoje 0,6054 lê-se como “PASS 0,60”, mas é 0,09 **abaixo** do mapa.
- **OPENCODE**: o spike BGE-M3/ColBERT continua certo — e o alvo não é 0,60, é 0,73.
- **Dono de `donor_map_embed.py`**: `--per-token` é o pré-requisito do D1.

---

## 7. Fontes

PAWS `arXiv:1904.01130`; “Daunting Dilemma with Sentence Encoders” `arXiv:2309.03747`
(SBERT 0,97/0,96 em PAWS); Kleyko et al., VSA survey `arXiv:2111.06077` (permutação
como posição, e o custo: permutação aleatória não preserva vizinhança de posições);
Frady et al. `arXiv:1707.01429` (SNR `√(N/M)`, ~0,5 bit/nêuron) e `arXiv:1803.00412`
(sequence indexing); Plate, HRR (capacidade `k = O(n/ln(m/q))`).
Modelos: cards HF de `colbert-xm`, `LateOn`, `answerai-colbert-small-v1`,
`jina-colbert-v2` (licença), `Qwen3-4B-Instruct-2507`, `Phi-4-mini-instruct`,
`gliner2-base-v1`, `gliner-relex-large-v1.0`, `rebel-large` (licença).

# MoE + ETL Python — “download Trinity” como skill pack

> Como empacotar conhecimento (ex.: manual Rust) em **Python offline** e
> ligá-lo ao **MoE** do AETHER (`transplant` + EXP3), sem colocar tokenizer
> nem pesos no núcleo.
>
> Metáfora: Matrix/Trinity = skill pack verificável.  
> Tomada T-PACK + T-MOE: [`CORE_TOMADAS.md`](CORE_TOMADAS.md) ·
> Doação: [`DOACAO_PLAYBOOK.md`](DOACAO_PLAYBOOK.md) /
> [`DONOR_CATALOG.md`](DONOR_CATALOG.md) · MoE: [`AETHER_AGENT.md`](AETHER_AGENT.md).

## 1. Posicionamento

| Papel | Quem | Linguagem típica |
|---|---|---|
| ETL / escriba / manifest | analista, sidecar | **Python** (`tools/`, GLiNER, harvest) |
| Expert + MoE + certificados | núcleo / agent | **Rust** |
| Gerar código / prosa | borda (opcional) | ver [`CODER_AETHER.md`](CODER_AETHER.md) / [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) |

O analista Python **empacota o download**; o MoE Rust **escolhe e executa** o
pack. Não é um LLM “aprender o livro”.

```text
manual / corpus
    │  Python ETL (offline)
    ▼
facts.jsonl + questions.jsonl + manifest.json   auth 60–120
    │
    ├─► FHRR transplant (symbol_phases / HvBank)
    └─► SGDB remember / aether-sgdb put
    │
    ▼
MoE EXP3 ──► especialista `transplant` (e tools se M8)
    │
    ▼
act | cite | abstain   + feedback → sobe peso do pack
```

## 2. Conexão com as escadas já documentadas

| Escada | Doc | Encaixe com MoE+ETL |
|---|---|---|
| **M\*** (este doc) | skill pack + roteamento | o “helicóptero” |
| **A\*** autonomia | [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) | M4–M5 = tool no planner; M8 precisa A4 |
| **K\*** coder | [`CODER_AETHER.md`](CODER_AETHER.md) | pack factual ≠ coder; K3+ depois do pack |
| **P\*** prosa | [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | template cita fatos do pack; generate opcional |
| **Doação** | [`DOACAO_PLAYBOOK.md`](DOACAO_PLAYBOOK.md) | ETL mapa+escriba oficial |
| **Doadoras** | [`DONOR_CATALOG.md`](DONOR_CATALOG.md) | GLiNER / Qwen escriba; auth ≤120 |
| **LanguagePort** | [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) | texto→Goal na borda; pack não tokeniza o core |
| **Pesquisa** | [`F2_PESQUISA_EXTERNA.md`](F2_PESQUISA_EXTERNA.md) | web ≠ transplant; pode **corroborar** triplas |

Ordem mental: **Doação/ETL (M1)** → **MoE roteia (M0/M5)** → **A4 se agir no
mundo** → **K/P só se precisar gerar**.

## 3. Escada M0→M8 (resumo)

| Passo | Entrega | Python? | Rust/MoE? |
|---|---|---|---|
| **M0** | MoE EXP3 + 5 experts | não | já existe |
| **M1** | `facts.jsonl` (s,r,v) + manifest | **sim** | transplant load |
| **M2** | imprint / padrões de fase | opcional scripts | `phase` expert |
| **M3** | novo `Expert` tipado | spec/tests em py | impl Rust |
| **M4** | Tool no registry (custo/risco) | harness | `tools.rs` |
| **M5** | feedback → EXP3/LinUCB | eval harness | router |
| **M6** | escala B (embed→JL→HV) | `harvest_*` / donor | HvBank |
| **M7** | Hierarchical MoE + LSH | — | P5 pesquisa |
| **M8** | pack + tools mundo (A4/A5/K) | ETL + gates | agent loop |

## 4. Contrato do skill pack (ETL Python)

### 4.1 Arquivos (por domínio)

```text
data/packs/<id>/
  manifest.json      # fonte, licença, hash, schema_ver, auth_default
  facts.jsonl        # uma tripla por linha
  questions.jsonl    # goals de aceite (ask)
  report.json        # recall@k, OOD abstain (após gate)
```

### 4.2 Linha `facts.jsonl` (canônico agent)

Igual a [`data/agent_facts.jsonl`](../data/agent_facts.jsonl):

```json
{"subject":"Vec","relation":"metodo","object":"push"}
```

Campos extras opcionais no manifest (não no hot path): `source_span`, `page`,
`license`. Auth de carga: **60–120** se escriba/LLM; curadoria HITL → até 255
só com sanção.

### 4.3 Pipeline Python recomendado (já no ecossistema)

1. Corpus (manual Rust, wiki, docs) — checar licença (`DONOR_CATALOG` §3).
2. **Extrator:** GLiNER2.5 JointIE → triplas schema (`DONOR_CATALOG` §2).
3. Opcional escriba Qwen2.5-7B só fora do schema (quarentena).
4. Normalizar → `facts.jsonl` + `questions.jsonl`.
5. `python tools/harvest_transplant.py …` e/ou load no agent:
   `aether-agent ask --facts data/packs/<id>/facts.jsonl …`
6. Gate: recall@k no pack; OOD fora do pack → abstain.
7. Episódios com reward verificado → MoE sobe peso do `transplant` nesse cluster.

**Não** colocar mapa/embedder no T0 60 Hz — sidecar offline only.

## 5. Exemplo: “helicóptero Rust” (analista Python)

| Etapa | Ação |
|---|---|
| 1 | Recortar The Rust Book (licença OK) em seções |
| 2 | ETL → ~N triplas (`ownership|proibe|duas_mut_refs`, …) |
| 3 | 50 questions.jsonl (“o que ownership impede?”) |
| 4 | Load pack `rust-book-v0` auth 96 |
| 5 | Medir: MoE escolhe transplant; acerto ≥ limiar; OOD abstain |
| 6 | HITL amostra → promover subset a 128+ se corroborado |
| 7 | Coder (K*): **outro** passo — generate+test; o pack só ancora fatos |

## 6. O que o pack *não* entrega

- Programar Rust fluente (precisa [`CODER_AETHER.md`](CODER_AETHER.md)).
- Prosa tutorial (precisa [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) P1+).
- Shell/build (precisa A4/K8 + HITL).
- Embedding semântico do livro inteiro no FHRR (FHRR = endereçamento lossy).

## 7. Aceite de um pack no MoE

- [ ] `manifest.json` com licença + hash + `schema_ver`.
- [ ] Gate recall@k ≥ alvo no `questions.jsonl` (artefato `report.json`).
- [ ] OOD (fora do domínio) → abstain ≥90%.
- [ ] Auth ≤120 até HITL/corroboração; tag `source=transplant|llm`.
- [ ] Após feedback, peso EXP3 do caminho transplant sobe no cluster do pack
      (telemetria).

## 8. Índice de implementação futura (refs)

| Precisa | Abrir |
|---|---|
| ETL mapa/escriba | `DOACAO_PLAYBOOK.md`, `DONOR_CATALOG.md`, `tools/harvest_transplant.py` |
| Formato fatos agent | `data/agent_facts.jsonl`, `AETHER_AGENT.md` §Transplante |
| MoE / EXP3 | `AETHER_AGENT.md` §MoE, `src/agent/router.rs` |
| Escala HV | ROADMAP trilha B, `TRANSPLANT_HV.md` |
| Autonomia / tools | `ACOES_AGENTICAS.md` |
| Código | `CODER_AETHER.md` |
| Prosa | `PROSA_FLUENTE.md` |
| Pesquisa corroborar | `F2_PESQUISA_EXTERNA.md` |

### Entidades nsgdb (recall)

`aether/agent`, `aether/transplant`, `aether/roadmap`, `mom/pattern`,
`mom/constraint`, `doc/architecture` · triggers em `docs/MOE_ETL_PACK.md` e
`docs/packs/**` quando existirem.

### Fatos âncora

- Código exige LLM na borda — não o pack sozinho (`md/L3/mcp/1794916915362016`).
- Ordem tools + shell sandbox (`md/L3/mcp/1794918311479020`).
- Escadas PROSA / ACOES / CODER (docs + puts 2026-11-21).

# Prosa fluente — escada sem contaminar o núcleo

> Como o AETHER adquire **prosa fluente** sem virar chatbot e sem o core
> tokenizar. Contrato: `LanguagePort` (ADR-013) · tomadas T-NL-IN/OUT:
> [`CORE_TOMADAS.md`](CORE_TOMADAS.md) · backends:
> [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) · trilha C:
> [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md).

## 1. Posicionamento

O AETHER é **coprocessador de evidência**: age, cita, propõe ou abstem. O
núcleo **não gera texto**. Prosa fluente open-domain **não nasce** de Ising,
FHRR ou SGDB — exige um backend de geração na borda, com custo/risco e
quarentena.

| Hoje (default) | Meta “fluente” |
|---|---|
| parser + cite/abstain + pesquisa | `generate()` plugável + validação AETHER |
| template/extrativo = zero alucinação, sem fluência geral | small_lm / concept / remote_llm como **tool** |
| fato de pesquisa ≤120; 255 só HITL | igual — prosa **nunca** se promove sozinha |

Invariantes (inegociáveis):

1. Núcleo sem tokenizer / sem prosa no T0.
2. Sem evidência → abstém (mesmo com LM fluente).
3. Texto gerado → autoridade 60–120; 255 só HITL.
4. Planner escolhe backend por EFE + custo + risco + privacidade — não por “tem LLM?”.

## 2. Escada de aquisição (ordem obrigatória)

Não pular degraus: fluência antes de template/juíz vira chatbot solto.

| Passo | Entrega | O que o usuário vê | Gate / aceite |
|---|---|---|---|
| **P0 — baseline** | parser C1 + memória + pesquisa F2 (Wiki∥SearX) | “evidência / não sei” | C1 verde; waste=0 na sim produto |
| **P1 — prosa auditável** | backend `template`: slots + snippets SGDB/Wiki/SearX → frase PT fixa | “A capital da França é Paris *(fonte…)*” | 0 claim sem evidência; só fala o que a fonte cobre |
| **P2 — juíz textual** | overlap/slots claim↔fontes (além de convergência numérica) | auth 80–120 em fatos textuais | falso settled ≤1% no corpus-régua (Terra–Lua / Rust×C) |
| **P3 — `LanguagePort::generate`** | trait + caps (custo/risco/latência); default ainda template | host pode pedir geração tipada | compile + selftest; T0 sem backend generativo |
| **P4a — small_lm (L2 / C1)** | xLSTM / RWKV / Mamba / BLT 0.5–3B local | fluência offline no budget | L2: qualidade + RAM + tok/s; 3050 cabe |
| **P4b — concept (L3 / C2)** | SONAR/LCM: conceito→embedding→decoder | fluência sem token-a-token clássico | L3: sim ≥0,9 e decoder &lt;50 ms no domínio |
| **P5 — remote_llm (C3)** | API só se P4 falha ou open-domain duro | fluência máxima | JSON/tool ≥95% quando constrained; auth ≤120 |
| **P6 — AND Vega** | decisor 0.8B ∩ sentinela prosa (ex. gemma-prose) | age só se ambos concordam | sem false-act confiante; senão abstain/HITL |

Gates de laboratório espelhados em `LANGUAGE_PORT.md` §7: **L1/C1** (parser),
**L2** (small_lm), **L3** (concept). Trilha C do roadmap: **C0…C4**.

## 3. O que cada passo *não* é

| Passo | Não é |
|---|---|
| P1 template | paráfrase criativa / chat livre |
| P2 juíz | “LLM diz que está certo” |
| P4 small_lm | verdade 255; é geração em quarentena |
| P5 remote | default do produto; é fallback caro |
| P6 AND | treinar o core a falar |

## 4. Matriz rápida (necessidade → backend)

| Necessidade | Backend | LLM necessário? |
|---|---|---|
| Comando / tool call | `parser` | não |
| Fato com fonte | `template` (+ pesquisa) | não |
| Explicação curta ancorada | `template` → `small_lm` | talvez |
| Conversa aberta fluente | `small_lm` ou `remote_llm` | **sim** |
| Código | `remote_llm` / code LM | **sim** |

Fonte da matriz: `LANGUAGE_PORT.md` §4. Declaração explícita do que **não**
substitui hoje: §6 e fato nsgdb `md/L3/mcp/1794916915362017`.

## 5. Ordem de implementação sugerida (produto)

1. **P1 template extrativo** no chat/agent (1–2 d) — maior ganho perceptível sem risco.
2. **P2 juíz textual** na cascata de pesquisa (já tem multi-provider @80 numérico/cobertura).
3. Smoke **L2** com um small LM na 3050 (gate L2) — só depois de P1–P2.
4. **L3 concept** ou **remote** como tool — o que passar o gate primeiro.
5. **P6 AND** com Vega quando prosa generativa entrar no caminho de `act`.

## 6. Critério de “temos prosa fluente”

Não basta “o modelo fala bonito”. Aceite de produto:

- [ ] Usuário recebe frase PT coerente em ≥X% das perguntas factuais com fonte.
- [ ] 0 false-act: nenhuma afirmação sem evidência ou com conflito aberto.
- [ ] Geração aparece no certificado (backend, custo, auth ≤120).
- [ ] OOD / sem pista → abstain ou deny honesto (não prosa inventada).
- [ ] Gates L2 ou L3 (ou C-G1/C-G2) verdes no artefato versionado.

## 7. Relação com o estado 2026-11-21

- Pesquisa viva: cascata paralela DDG∥SearX∥Wiki; default SearX
  `https://search.lumy.live`; `multi_provider_confirm` → auth 80.
- Parser C1 e `language.parse` entregues; `template`/`generate` ainda não são
  o caminho default do chat.
- Vega gemma-prose = sentinela de OOD, **não** adotada como geradora de prosa
  de produto (cosine de attach &lt; 0,8).

## 8. Referências internas

- [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) — backends, papers, gates L1–L6  
- [`ADR_AGENT.md`](ADR_AGENT.md) — ADR-013  
- [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) — trilha C (C0–C4)  
- [`F2_PESQUISA_EXTERNA.md`](F2_PESQUISA_EXTERNA.md) — evidência para ancorar P1–P2  
- [`AETHER_AGENT.md`](AETHER_AGENT.md) — posicionamento híbrido  
- [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) — autonomia verificada (paralela à prosa)  
- [`CODER_AETHER.md`](CODER_AETHER.md) — código: gerar na borda, patch/teste no AETHER  
- [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) — skill pack ETL Python → MoE transplant  


 

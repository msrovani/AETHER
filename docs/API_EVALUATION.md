# AETHER — avaliação de usabilidade como IA

> Medido em 2026-11-16 com `cargo test --release --features api --test api_eval`.
> Reproduza com `-- --nocapture` para ver a tabela.

## 1. A pergunta

"É usável como IA?" tem duas leituras e elas têm respostas **diferentes**:

1. **Como modelo de linguagem** (conversar, responder, gerar texto): **não**.
   O motor não tokeniza, não prevê o próximo token, não faz backprop.
2. **Como serviço para um app de IA usar** (memória auditável + solver com
   certificado): **sim**, com limites medidos abaixo.

Este documento mede a segunda e delimita a primeira.

## 2. Método

60 fatos sintéticos (`"fato {i} sobre o tema {i%5} com valor {i*7}"`), gravados
por `/v1/aether/remember` com **160 fasores** (`text_to_phases`), autoridade 255,
certificado `gap = 0,3`. Depois, consultas por `/v1/aether/cue`; o esperado é
"acerta o que é idêntico, abstém no que não é".

## 3. Resultado

| Condição | Resultado | Leitura |
|---|---|---|
| mesma string | **60/60** acerta | memória endereçável por conteúdo funciona |
| mesmas fases + ruído `1e-3` | **60/60** acerta | tolera ruído numérico de transdutor |
| paráfrase (`"…?"`) | **60/60 abstém** | hash não generaliza |
| maiúsculas | **60/60 abstém** | idem |
| 1 caractere a mais | **60/60 abstém** | avalanche do hash |
| prefixo | **60/60 abstém** | não é busca por substring |
| outro organismo | **60/60 abstém** | R8: escopo não vaza |
| `/v1/chat/completions` com fato exato | `act`, `key=7` | o dialeto OpenAI entrega evidência |

Custo (localhost, 60 amostras):

| Operação | p50 | p99 |
|---|---|---|
| `cue` (160 fasores, 60 fatos) | **315 µs** | **626 µs** |
| `solve` (n=12, 80 passos, 4 reinícios) | **674 µs** | **748 µs** |

## 4. O defeito que a avaliação encontrou (e a correção)

Na primeira rodada, a paráfrase **acertava** — 0/60 abstenções, score 0,81.
Causa: a borda gerava **fases reais** (φ ∈ [0,2π), média π), e o cosseno entre
dois vetores de média π é ~0,75 por construção. O limiar de 0,5 nunca disparava.

Correções aplicadas:

1. `text_to_phases` passou a gerar **fasores unitários** `(cos φ, sin φ)` —
   320 floats = 1280 B, exatamente `MAX_PHASE_BYTES` do F1. Correlação ~0 entre
   textos diferentes, 1 entre iguais.
2. `ScopeLedger::cue_scored` devolve o **score**; a borda abstém abaixo de
   `MIN_SCORE = 0,5` com `reason: "low_similarity"`.
3. Rejeição de comprimento ímpar (fasor é par `re, im`) e de `NaN`/`Inf`.
4. `413` passou a **drenar** até 4× o limite antes de fechar (evita reset no
   cliente); BOM UTF-8 é aceito.
5. Linhas `#` no ledger são comentário, não erro (`bad_lines` não as conta).

Sem a correção 1+2, o serviço responderia **sempre** — e um app de IA teria um
oráculo que nunca diz "não sei". Esse é o modo de falha que `R9`
(proveniência-teatro) descreve.

## 5. Integridade (9 gates, `tests/api_integrity.rs`)

| Gate | O que prova |
|---|---|
| protocolo hostil | método errado, JSON truncado, tipo errado, caminho de 4 KB: 4xx e servidor vivo |
| sem `Content-Length` | corpo vazio → 400, sem pendurar |
| corpo > 1 MiB | `413 payload_too_large` |
| `NaN`/`Inf` | 400 em `phases` e em `J` |
| BOM UTF-8 | aceito (cliente Windows) |
| chave `u128` + fases | chave inteira preservada; 160 f32 **bit a bit** no arquivo |
| ledger corrompido | carrega o válido, conta `bad_lines`, `cue` funciona |
| persistência concorrente | 24 gravações paralelas = 24 linhas, recarga = 24 |
| concorrência mista | 8 threads × (remember + cue + health) = 24/24 |

Mais `tests/api_http.rs` (9 gates: OpenAI, SSE, llama.cpp, auth 401/403,
persistência com reinício) e 9 testes unitários de JSON/round-trip.

## 6. Veredito

**Como IA de conversa: não.** Não há geração, não há semântica, não há
generalização: "gato" e "gata" produzem fasores sem relação.

**Como infraestrutura de IA: sim**, desde que o papel esteja claro:

* um LLM (llama.cpp, Ollama, GPT) continua sendo o **gerador de linguagem**;
* o AETHER entra como **memória de fatos com procedência** (`key`, `authority`,
  `certificate`) e **solver com certificado** (`energy`, `gap`, `violations`);
* e, principalmente, como o componente que sabe **abster** — o que um LLM
  sozinho não faz de forma confiável.

Padrão de uso correto:

```text
LLM (texto) ──extrai o fato──► /v1/aether/cue ──► act | cite | propose | abstain
                                     │
                                     └── evidência + certificado para a resposta final
```

Padrão de uso **errado**: apontar `OPENAI_BASE_URL` para cá e esperar um chatbot.
O endpoint responde no formato certo, mas o conteúdo é evidência — não conversa.

## 7. Limitações declaradas

1. **Sem generalização semântica.** Para aproximar paráfrases é preciso um
   transdutor real (E3) gerando fasores, não o hash da borda.
2. **Sem geração.** `content` é JSON de evidência; não há tokens.
3. **Sem streaming de linguagem.** O SSE entrega 4 eventos fixos (evidência),
   não uma sequência de tokens.
4. **Estado serializado.** O ledger é protegido por `Mutex`; o I/O é paralelo,
   a consulta é serial. Para milhares de QPS, particionar por escopo.
5. **`solve` é heurístico** (bSB): `coherent = true` significa mínimo local
   factível, não ótimo global garantido.

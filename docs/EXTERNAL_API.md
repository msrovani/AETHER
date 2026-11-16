# AETHER — borda externa HTTP/JSON (T2)

> **O que é:** o adaptador que deixa um app de IA (llama.cpp, Ollama, SDK da
> OpenAI, agente próprio) consumir o AETHER pela rede.
> **O que não é:** um gerador de texto. O motor não tokeniza, não prevê o
> próximo token e não faz backprop. Ele **recupera fase**, **resolve lógica** e
> devolve **certificado** — ou se **abstém**.

```text
app (llama/agent) ──HTTP/JSON──► aether-api ──► ScopeLedger (F1, T1/T2)
                                      └────────► roof_dual + solver + Certificate
```

## 1. Subir

```powershell
cargo run --release --features api --bin aether-api -- 127.0.0.1:8787 `
  --ledger target/aether_ledger.jsonl `
  --token s3cr3t --organism 1 --threads 4
```

| flag | efeito |
|---|---|
| `--addr IP:PORTA` (ou posicional) | endereço de escuta (padrão `127.0.0.1:8787`) |
| `--ledger arquivo.jsonl` | persiste o ledger em disco (append-only) e recarrega no boot |
| `--token T` | exige `Authorization: Bearer T` (ou `X-Aether-Token`) nas rotas com estado |
| `--organism N` | amarra o token ao organismo `N`; escopo diferente recebe `403` |
| `--threads N` | workers de conexão (padrão 4) |

Sem dependência nova: HTTP/1.1 mínimo sobre `std::net`, `Connection: close`.

Rotas de liveness (`/health`, `/props`, `/v1/models`) são **públicas**; as demais
exigem token quando `--token` é usado.

## 2. Rotas

| Método | Rota | Para quê |
|---|---|---|
| `GET` | `/health` | prontidão + invariantes (`tokens=0`, `backprop=0`, `t0_ledger_calls=0`) |
| `GET` | `/props` | metadados estilo llama.cpp |
| `GET` | `/v1/models` | catálogo OpenAI |
| `POST` | `/v1/aether/remember` | grava um fato de fase no ledger (T2) |
| `POST` | `/v1/aether/cue` | recall por fase, **dentro de um escopo** |
| `POST` | `/v1/aether/solve` | QUBO/Ising → estado + certificado (`energy`, `gap`, `violations`) |
| `POST` | `/v1/chat/completions` | dialeto OpenAI; devolve evidência certificada ou abstenção |
| `POST` | `/completion` | dialeto llama.cpp; mesmo motor |

## 3. Exemplos

### saúde

```bash
curl -s http://127.0.0.1:8787/health
```
```json
{"status":"ok","engine":"aether","version":"0.0.104",
 "invariants":{"tokens":0,"backprop":0,"t0_ledger_calls":0},
 "requests":0,"ledger_entries":0}
```

### gravar e recuperar (com escopo)

```bash
curl -s -X POST http://127.0.0.1:8787/v1/aether/remember -d '{
  "key":"42",
  "phases":[0.1,0.2,0.3,0.4],
  "scope":{"organism":1,"run":1,"app":1},
  "certificate":{"energy":-1.5,"gap":0.25,"violations":0},
  "authority":255
}'

curl -s -X POST http://127.0.0.1:8787/v1/aether/cue -d '{
  "scope":{"organism":1,"run":1,"app":1},
  "query":[0.1,0.2,0.3,0.4]
}'
```
```json
{"abstain":false,"hits":1,"key":"42","authority":255,"score":1.0,
 "phases":[...],"isolation":{"cross_scope_hits":0,"cross_scope_skipped":0}}
```

`score` é a correlação complexa (`0..1`) com o melhor fato do escopo. Abaixo de
`0,5` a resposta **abstém** com `reason: "low_similarity"` — parecido com o
`argmax` sem limiar, que transformaria toda pergunta em resposta errada.

Consulta em **outro** organismo devolve `{"abstain":true,"reason":"empty_scope"}`
— nunca o fato de outro escopo (R8).

### resolver lógica

```bash
curl -s -X POST http://127.0.0.1:8787/v1/aether/solve -d '{
  "n":2,"j":[[0,1],[1,0]],"h":[0,0],"steps":80,"restarts":4,"seed":7
}'
```
```json
{"energy":-1,"gap":2,"violations":0,"consensus":1,"coherent":true,
 "decision":"propose","states":[-1,-1],"provenance":"inferred",
 "persistency":{"fixed":2,"residual":0,"rounds":2}}
```

### como “modelo” para um app de IA

```bash
curl -s -X POST http://127.0.0.1:8787/v1/chat/completions -d '{
  "model":"aether-phase-160",
  "messages":[{"role":"user","content":"qual o fato?"}],
  "aether":{"scope":{"organism":0,"run":0,"app":0}}
}'
```

A resposta é OpenAI-válida (`choices[0].message.content`) e carrega
`x_aether` com `verdict`, `key`, `certificate`, `phases` e `edge`.
Sem fato no escopo, `finish_reason` é `content_filter` e o conteúdo é
`{"abstain":true,"reason":"empty_scope"}`.

### streaming (SSE)

```bash
curl -N -X POST http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer s3cr3t" \
  -d '{"model":"aether-phase-160","stream":true,
       "messages":[{"role":"user","content":"qual o fato?"}]}'
```

```text
data: {"id":"chatcmpl-aether-2","object":"chat.completion.chunk",...,"delta":{"role":"assistant"},...}
data: {"...","delta":{"content":"{\"abstain\":false,\"verdict\":\"act\",...}"},...}
data: {"...","delta":{},"finish_reason":"stop"}
data: [DONE]
```

São 4 eventos: papel, conteúdo, `finish_reason` e `[DONE]`. A abstenção vira
`finish_reason: "content_filter"`.

### apontar um cliente OpenAI / llama.cpp

```bash
# SDK Python da OpenAI
export OPENAI_BASE_URL=http://127.0.0.1:8787/v1
export OPENAI_API_KEY=local
```

```python
from openai import OpenAI
c = OpenAI(base_url="http://127.0.0.1:8787/v1", api_key="local")
r = c.chat.completions.create(
    model="aether-phase-160",
    messages=[{"role": "user", "content": "qual o fato?"}],
)
print(r.choices[0].message.content)     # evidência certificada ou abstenção
print(r.model_extra["x_aether"])        # verdict, key, certificate
```

Para o llama.cpp, aponte o cliente para `/completion`; o payload é
`{"prompt": "...", "content": "...", "stop": true}`.

## 4. Semântica (o que honestamente se promete)

* **Abstenção é resposta.** Não há geração livre: sem evidência no escopo, a
  resposta é `abstain` — não um texto inventado.
* **`verdict` vem da matriz 2D** (`authority × gap`): `act` · `cite` · `propose`
  · `abstain`. Sem autoridade (0), o teto é `propose`.
* **A borda texto→fase é um hash determinístico e lossy** (`X-Aether-Edge:
  text-phase-hash-lossy`). Não é embedding semântico: dois textos parecidos
  **não** caem perto. Quem precisa de semântica usa `/v1/aether/remember` com as
  fases de um transdutor real.
* **Escopo é obrigatório em `cue`.** `cross_scope_hits` tem de permanecer 0;
  `cross_scope_skipped` prova que o filtro rodou.
* **Certificado é de factibilidade, não de verdade** (`energy`, `gap`,
  `violations`). `gap = 0` invalida o selo.

## 5. Limites

| Limite | Valor |
|---|---|
| corpo da requisição | 1 MiB (`413` acima) |
| dimensão de fase | 160 **fasores** = 320 `f32` (1280 B) |
| score mínimo para responder | `0,5` (abaixo disso: `abstain` / `low_similarity`) |
| `n` em `solve` | 1..=256 |
| passos / reinícios | 1..=4096 / 1..=32 |
| conexões | `--threads` workers (padrão 4), `Connection: close` |
| ledger em disco | uma linha JSON por fato, append-only |
| dependências novas | **zero** |

## 5.1 Persistência (T2)

`--ledger arquivo.jsonl` grava cada `remember` como **uma linha JSON** e recarrega
o arquivo no boot. Linhas inválidas são ignoradas e contadas em
`/health` → `bad_lines`. O formato é legível e append-only:

```json
{"key":"777","layer":4,"certificate":{"energy":-2.5,"gap":0.4,"violations":0},
 "authority":255,"scope":{"organism":7,"run":1,"app":1},
 "tick":0,"wall_ms":1763...,"phases":[0.123,...]}
```

O arquivo é a **memória de longo prazo do escopo**, não o motor: apagar o
arquivo não altera a física, só o que o app consegue citar.

## 5.2 Auth e escopo (R8)

Com `--token`, toda rota com estado exige o token; sem ele, `401`. Com
`--organism N`, qualquer escopo cujo `organism != N` recebe `403
forbidden_scope` — o token de um organismo **não** cita fatos de outro.

```bash
curl -s -o /dev/null -w "%{http_code}\n" -X POST http://127.0.0.1:8787/v1/aether/remember -d '{}'
# 401
curl -s -X POST http://127.0.0.1:8787/v1/aether/remember \
  -H "Authorization: Bearer s3cr3t" -d '{"scope":{"organism":2,...}}'
# 403
```

## 5.3 Concorrência

`--threads N` sobe N workers sobre uma fila de conexões. O estado (ledger) é
protegido por `Mutex`: o **I/O** roda em paralelo; a consulta ao ledger é
serializada. Para 60 Hz de telemetria isso sobra; para carga de LLM, suba o
`--threads` e lembre que a serialização é do estado, não da rede.

## 6. Notas de interoperabilidade

* **BOM UTF-8** no início do corpo é aceito e removido (clientes Windows
  escrevem BOM com `Set-Content`).
* **Locale**: gere números JSON com cultura invariante (`1.5`, não `1,5`) —
  vírgula decimal quebra o parser, como quebraria em qualquer API.
* `key` viaja como **string decimal** (u128 não cabe em `f64`); aceita número
  na entrada por conveniência.

## 7. Invariantes preservados

* Nenhum `String` entra no sinal do núcleo: `String` só existe nesta borda (T2).
* `t0_ledger_calls` continua `0` — a borda **lê** o ledger, não é chamada pelo
  hot path.
* `0` alocação no caminho quente: o servidor está fora do ciclo de 60 Hz.

## 8. Testes

```powershell
cargo test --release --features api --test api_http
```

Cobre **9** gates: saúde/catálogo, `remember`/`cue` com isolamento entre
organismos, abstenção sem ledger, `solve` com ótimo conhecido (ferromagneto
`n=2`), dialeto OpenAI, dialeto llama.cpp, `404`, **SSE**, **auth/escopo**
(`401`/`403`), **persistência em disco com reinício** e **8 conexões em
paralelo**.

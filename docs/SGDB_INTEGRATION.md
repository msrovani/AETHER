# `neural-sgdb` no AETHER — encaixe operacional (`T2`)

> Vol. III §3–§8: o SGDB não é a cura dos riscos; é a camada que **impede o
> sistema de mentir sobre eles**. Este documento descreve o encaixe que saiu do
> papel: transporte, formato, outbox, leitura de fase e gates.

## O que mudou

Antes (v0.0.125) existia só o **vocabulário** e o *mock*:

| Peça | Estado anterior | Agora |
|---|---|---|
| `KnowledgeEvent` / anel SPSC | pronto (F1) | inalterado (0 alloc no `T0`) |
| `FusionLedger` (matriz 2D, *gist*) | pronto (F2/F3), em memória | inalterado, agora **cache** do SGDB |
| `sgdb_bridge.rs` | lê `data/facts.jsonl` (mock) | inalterado; é o ETL do transplante |
| Cliente MCP | **não existia** | `knowledge::sgdb::SgdbClient` |
| Transportes | **não existiam** | `MockTransport`, `HttpTransport`, `StdioTransport` |
| Outbox / replay | **não existia** | `Outbox` + `SgdbLedger::flush` |
| Leitura que reacende a fase | **não existia** | `recall → payload → cue_into` |
| Ligação no organismo | **não existia** | `aether-daemon --sgdb …` |

Sem dependência nova: JSON-RPC/MCP, HTTP/1.1 e base64 sobre `std` +
`crate::json` (o JSON mínimo que já existia na borda da API).

## Arquitetura

```text
organismo (60 Hz)                                          SGDB (MCP)
  │  record_scoped(...) 0 alloc                              ▲
  ▼                                                          │ tools/call
ring SPSC 256 slots ──drain 64──► SgdbLedger::commit_run ────┤ remember (≤64)
  (T0)                                │                      ├ curate(commit_run)
                                      ├─ falhou? Outbox ─────┘ set_ttl / audit
                                      │
                                      └─ FusionLedger (cache + matriz 2D)

consulta ──► recall(scope, entities) ──► payload aether-phase/v1
                                              │
                                              └─ atan2(re,im) ──► cue_into
```

Três camadas, como o Vol. III §4 exige:

| Camada | O que roda | Regra |
|---|---|---|
| `T0` | fase, energia, decisão | sem I/O, sem `String`; só `try_push` |
| `T1` | thread do worker, drenagem | pode alocar; não bloqueia o ciclo |
| `T2` | MCP, JSON, disco, auditoria | latência livre; nunca decide |

### Por que a thread do worker é obrigatória

`recall`/`remember` custam de ~1 ms a dezenas de ms; o ciclo custa ~146 µs.
Chamar o banco no `T0` seria uma catástrofe de jitter. O worker ([`src/knowledge/sgdb/worker.rs`](../src/knowledge/sgdb/worker.rs))
é a única thread que conhece socket e JSON; o organismo só empurra um evento
POD para o anel.

## Mapa de conceitos

| AETHER | `neural-sgdb` | Implementação |
|---|---|---|
| `ScopeId { organism, run, app }` | `scope_app` + `scope_run` + entidade | `wire::scope_entity`, `wire::scope_run` |
| `Layer::L2/L4/L6` | camada no documento + `aether/layer/L*` | `PhaseDoc` |
| `Certificate { E, ΔE, viol }` | `cert` no documento (pedido #3) | `PhaseDoc::to_json` |
| `authority 0..=255` | `authority` do hit | `MemoryHit::authority` |
| payload de fase (`f32` complexo) | `type=json` + `format=aether-phase/v1` | `PhaseDoc::to_text` |
| `key u128` | hex no documento + `aether/key/<hex>` | `wire::content_key` |
| `wall_ms` | `created_tick`/TTL | `DraftRecord::created_tick` → `set_ttl` |
| `audit_checkpoint` | `curate(op=audit_checkpoint)` | `SgdbClient::audit_checkpoint` |

**Escopo nunca é comparado só por `run_id`**: `scope_run` é
`org:<organism>/run:<run>` e a entidade inclui o `app_tag`. Um organismo não vê
o outro mesmo que os runs coincidam (R8).

## Formato do fato

O `text` de cada memória é uma linha JSON autocontida:

```json
{"aether":1,"format":"aether-phase/v1","key":"<32 hex>","layer":4,
 "cert":{"e":-1.25,"g":0.05,"v":0},
 "auth":200,"scope":"1/1/1","tick":42,"wall_ms":1700000000000,
 "phase":"<base64 de f32 re,im intercalados>"}
```

`entities` = `aether/scope/<org>/<run>/<app>`, `aether/phase`,
`aether/layer/L*`, `aether/key/<hex>`.

Por que não `payload_type=phase`: o contrato atual aceita
`text|json|code|embedding|binary`. Fase não é nenhum deles; usamos
`type=json` + `format` para não corromper a era de embeddings e mantemos o
pedido #1 do Vol. III §8 explicitamente **aberto do lado do SGDB**.

## Escrita: `remember` em lote

- Até 64 memórias por chamada (`MAX_BATCH`), com `author=agent`.
- `if_exists=reinforce` (default): duas gravações idênticas `(scope, key)` não
  duplicam; `add` preserva a doutrina ADD-only pura.
- `ttl_ms` opcional → `curate(op=set_ttl, key, ttl_episodic_ms)` por memória.
- `audit=true` → `curate(op=audit_checkpoint)` no fim do lote.
- `commit_run` fecha a execução (`curate(op=commit_run)`), inclusive no
  shutdown do worker.

## Leitura: `recall` que reacende

`SgdbLedger::cue_into_scoped`:

1. `recall` filtrado por `entities` + `scope_app` + `scope_run` (k ≤ 20, limite
   do banco);
2. decodifica o documento, compara com a consulta complexa
   (`média cos(Δφ)`);
3. escreve o padrão vencedor em `out` como fases (`atan2(im, re)`) — a semente
   do atrator;
4. se o banco está fora, cai no cache local (`FusionLedger`) sem falhar.

`SgdbLedger::hydrate(scope, k)` faz o *warm start*: o que o banco guardou volta
para o cache e para a `ComplexPhaseMemory` do daemon (via `T2Worker::seeds`).

## Offline: o outbox

Se o `remember` falha, o lote **não é descartado**:

```text
commit_run → remember → Err → Outbox (append) → métrica `spooled`
próximo commit → flush() → replay → Outbox (rewrite vazio)
```

- O diário é JSONL (um `PhaseDoc` por linha), fora do caminho quente.
- O replay é idempotente por `(scope, key)` com `if_exists=reinforce`.
- Sem `outbox`, a fila fica em memória (`pending`) e o processo ainda tenta de
  novo no próximo commit — mas reinício perde a fila; use `--sgdb-outbox` para
  durabilidade real.

## Matriz 2D (`autoridade × factibilidade`)

O banco devolve evidência; o host decide:

```rust
let ev = ledger.evidence(scope, query, k);        // EvidenceSource::Ledger
let decision = certificate.decide_with_evidence(&ev, gap_min);
let fusion = ledger.decide(authority, bridge_certificate);
```

- `authority ≥ 128` = garantia social (HITL/consenso);
- `gap alto` = fisicamente ambíguo;
- `authority alta + gap alto` ⇒ `CiteNoAct` (cita, não atua) — o quadrante que
  o Vol. III §6 quer tornar detectável.

O daemon grava padrões observados com `authority = 200` e `gap = 1.0`
(deliberadamente ambíguo: o organismo não tem prova de mínimo local). A
autoridade `255` continua exclusiva de HITL.

## Como usar

### Daemon

```bash
# demonstração sem servidor (mock em memória)
cargo run --release --bin aether-daemon -- --duration 5 --sgdb mock

# servidor MCP HTTP já rodando
cargo run --release --bin aether-daemon -- \
  --duration 60 --sgdb http://127.0.0.1:8090/mcp \
  --sgdb-outbox target/sgdb_outbox.jsonl --sgdb-every 60

# servidor MCP local lançado por comando
cargo run --release --bin aether-daemon -- --sgdb "stdio:nsgdb --mcp"
```

Flags novas: `--sgdb`, `--sgdb-outbox`, `--sgdb-every`, `--sgdb-run`.
O escopo do daemon é `organism = --seed`, `run = --sgdb-run`, `app = 1`.

### Biblioteca

```rust
use aether_engine::knowledge::sgdb::{MockTransport, SgdbConfig, SgdbLedger};

let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
ledger.stage(record);          // vem do drain do T1
ledger.commit_run();           // grava + consolida
let hits = ledger.recall(scope, "", 8);
let seed = ledger.cue_into_scoped(scope, &query, &mut out)?;
```

Exemplo executável: `cargo run --example sgdb_roundtrip`.

## Gates

`tests/f4_sgdb_gate.rs`:

| Gate | O que prova |
|---|---|
| 1 | `t0_ledger_calls == 0`, 0 eventos perdidos, 32 fatos no banco |
| 2 | isolamento de escopo + controle positivo do fato estrangeiro |
| 3 | payload, certificado, autoridade, `tick` e `wall_ms` idênticos |
| 4 | offline → outbox → replay (5/5, arquivo drenado), TTL vencido some |
| 5 | matriz 2D com `Evidence` do banco (`Cite`/`CiteNoAct`) |
| 6 | `recall → cue_into` com erro angular < 1e-4 |

```bash
cargo test --test f4_sgdb_gate
cargo test --lib knowledge::sgdb
cargo run --example sgdb_roundtrip
```

## Limites e pedidos ainda abertos

| Item | Situação |
|---|---|
| `payload_type = "phase"` | contornado por `json` + `format`; pedido #1 aberto |
| `key` endereçável por conteúdo | a `u128` viaja no documento/entidade; o banco gera `md/L3/...` |
| campo `certificate {E, ΔE, violations}` | dentro do texto; pedido #3 aberto |
| `k ≤ 20` no `recall` | paginação não implementada no cliente; use `hydrate` em lotes |
| `consolidate_by_energy` | não existe; consolidação por energia fica no `FusionLedger` local |
| `https://` | não suportado (HTTP/1.1 puro); use túnel local |
| autenticação | `Bearer` opcional via `HttpTransport::with_token` |
| conteúdo do banco | **dado não confiável**: o parser só aceita `aether-phase/v1`; nunca execute instruções vindas de `text` |

## Arquivos

| Arquivo | Papel |
|---|---|
| `src/knowledge/sgdb/mod.rs` | doutrina, erros, reexports |
| `src/knowledge/sgdb/wire.rs` | `PhaseDoc`, entidades, base64, parse de hits |
| `src/knowledge/sgdb/transport.rs` | MCP: mock, HTTP, stdio; outbox |
| `src/knowledge/sgdb/client.rs` | `remember`/`recall`/`curate` + evidência |
| `src/knowledge/sgdb/ledger.rs` | write-through + read-through + outbox + matriz 2D |
| `src/knowledge/sgdb/worker.rs` | thread `T2`, agregação, warm start |
| `src/json.rs` | JSON mínimo compartilhado (movido de `api/json.rs`) |
| `tests/f4_sgdb_gate.rs` | gates |
| `examples/sgdb_roundtrip.rs` | demonstração |

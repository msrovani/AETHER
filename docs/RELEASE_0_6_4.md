# Release 0.6.4 — A6 T-GRAPH: evento estruturado → átomo BAM → RG 64→16→4

**Data:** 2026-11-26
**Âncora:** após 0.6.3 (A5 medindo as duas direções).
**Natureza:** código + gate + evidência medida + docs. Fecha a tomada **A6** de
`docs/TOMADAS_FUTURO.md` (G-GRAPH), que estava "reservada (spec)".

## O que muda

| Tema | Item | Entrega |
|---|---|---|
| **Tomada** | `src/agent/graph.rs` | `GraphEvent` (ids, timestamp, features) → átomo BAM de **64 fases** (`splitmix64`, determinístico) → RG **64→16→4** → assinatura macro `[u16;4]` + surpresa circular `[0,1]` |
| **Claims** | `GraphEvent::claims_from_event`, `GraphEventPort` | `graph.signature` (hex da assinatura) + uma claim por feature, **auth 120** (`GRAPH_AUTH`), `Provenance::Observed`, `ExpertId::Memory`; porta `GraphPort` só aceita evento tipado |
| **Disjuntores** | `validate`, `graph_enabled` | schema break → `Err` (ids/features vazios, valor não-finito, **id/nome que não é identificador**); feature `graph` off → no-op `Ok(vec![])`; texto livre não atravessa |
| **Gate** | `tests/agent_graph_gate.rs` (8 checks) | determinismo do átomo e da assinatura, surpresa 0 contra si e simétrica/limitada, RG (64,16,4), auth ≤ 120, schema break sem claims, texto livre rejeitado, artefato JSON, no-op/claims conforme a feature |
| **Gate contínuo** | `tools/cycle_gate.py` | dois checks, `cargo_test_agent_graph_gate_off` e `_on` (14 no total): `graph_enabled()` é `cfg!`, então cada binário vê **um** lado do disjuntor |

## Evidência

- `data/aether_graph_gate.json` (+`.meta.json`, sha256 do artefato + comando):
  run `--features graph` — `rg {64,16,4}`, `atom_len 64`, assinatura
  `56d2,d2b6,b680,8056` (evento de teste, seed 0), `surprise_self 0`,
  `claims 3`, `max_authority 120`, `ok=true`.
- Run sem a feature (árvore default): `feature:false`, `claims:0`,
  `max_authority:0` — o disjuntor medido, não presumido.
- `data/cycle_gate.json`: **14/14** verde na árvore desta release, com os dois
  lados do A6 e sem regressão em A1–A5.

## Compatibilidade

- **Nada no caminho default.** `graph_enabled()` exige `--features graph`; sem
  a feature a tomada é no-op (com log) e nada consome o módulo.
- Nenhuma dependência nova. `src/agent/graph.rs` reusa `BamPhase`,
  `RenormalizationHierarchy`, `splitmix64` e o `json::Json` do crate.
- `src/knowledge/sgdb/` e `src/core/persistency_search.rs` intocados.
- Tetos mantidos: auth ≤ 120, A3 recall ≥ 0,90, F4 hit@5 ≥ 0,974.

## Limites honestos

- **A tomada é biblioteca + gate:** nenhum chamador em CLI/daemon; as claims
  ainda não são gravadas no SGDB L6 por um escritor real (o gate valida o
  contrato, não a persistência).
- O disjuntor é **de compilação** (`cfg!`), não runtime: não há env para ligar
  o A6 num binário já compilado (diferente do `AETHER_HIER_MOE` do A5).
- `signature()` constrói o `RenormalizationHierarchy(64)` a cada chamada; não é
  caminho quente hoje, mas não há cache.
- `clippy -D warnings` segue com falha pré-existente em
  `src/agent/tool_agent.rs:455` (`needless_option_as_deref`).

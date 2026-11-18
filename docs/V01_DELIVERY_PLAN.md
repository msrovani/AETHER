# AETHER v0.1 — Plano de entrega (freeze IA usável)

> Claim fórum: **MSG 0614** · Versão crate alvo: **0.1.0** (hoje: ver `Cargo.toml`)  
> Público: integrador de agente / PM / engenharia  
> Frase de fronteira: **LLM gera; AETHER cita, certifica ou abstem.**

Este documento congela o que a **v0.1** promete como IA usável: coerência de
respostas (`act` / `cite` / `propose` / `abstain`) e usabilidade do contrato
HTTP/CLI — **não** um chatbot nem um leaderboard de LLM.

Relacionados: [`USER_DELIVERY_PLAN.md`](USER_DELIVERY_PLAN.md) (demo/hub/ondas
produto físico), [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md) (P0–P5),
[`EXTERNAL_API.md`](EXTERNAL_API.md), [`AETHER_AGENT.md`](AETHER_AGENT.md),
[`EVAL_AGENT.md`](EVAL_AGENT.md).

---

## 1. Veredito em uma frase

A v0.1 é um **coprocessador de evidência certificada**: memória + solvers +
abstenção calibrada, consumível por um host (LLM/parser/app) via API/CLI; o hub
Windows observa o lab — **não** conversa.

---

## 2. Mapa: entregável vs lab

```text
Host (LLM | ParserPort | app)
        │  Goal estruturado / dialeto OpenAI
        ▼
┌────────────────── AETHER 0.1 ──────────────────┐
│  aether-api  ·  aether-agent                   │
│       │                                        │
│  ToolAgent → CoherenceEngine                   │
│       │            │                           │
│  SGDB/ledger   SAT/QUBO/fact/verify            │
│       │                                        │
│  action + answer + authority + certificate     │
└────────────────────────────────────────────────┘
        │
        ▼
  aether-dashboard  (observabilidade ACQ/jobs — não é o chat)
```

| Camada | Estado | O integrador consegue |
|---|---|---|
| Motor M1–M8 + ACQ 100 | Lab pronto | Confiar que o núcleo físico está medido |
| Agent tools / planner / verify / pipeline | Pronto (P0–P2) | `fact→sat→verify`, rollback, autoridade 200 via certificado |
| Coerência (`coherence.rs`) | Pronto | Contradição → `abstain`; autoridade dominante → `cite` |
| LanguagePort / ParserPort | Parcial | Intents estruturados; **não** NL geral |
| API HTTP OpenAI-compatible | Pronto (T2) | Integrar clientes OpenAI com expectativa de evidência/abstenção |
| Eval T0 (`agent_eval_suite`) | Pronto | `action_accuracy` e `false_act=0` no escopo próprio |
| CIC abstain | Pronto | `false_act=0` em OOD |
| Hub Windows | Pronto (Onda 0) | Ver métricas/lab; **não** conversar |
| Doação HV / B-G0 / shards E1B | Lab avançado | Memória transplantada; **não** fecha “IA conversacional” |

---

## 3. Persona

**Integrador de agente** — monta Goals ou fala dialeto OpenAI/llama.cpp e precisa
de fatos certificados ou abstenção honesta.

Não é: usuário final de chat, nem avaliador MMLU/Arena.

---

## 4. Contrato de resposta (obrigatório)

Toda resposta de produto expõe, estável e documentada:

| Campo | Significado |
|---|---|
| `action` | `act` · `cite` · `propose` · `abstain` |
| `answer` | Só quando a ação autoriza; **nunca** inventar fato em `abstain` |
| `authority` | Escada 96→128→200→255; ≥200 exige certificado ou HITL |
| `certificate` / evidência | Quando `act`/`cite` com solver ou verify |
| `reason` | Em abstenção: `empty_scope`, `low_similarity`, `contradiction`, `ood`, … |

### Exemplos (smoke)

```powershell
# API — agente (fato estruturado)
curl -s -X POST http://127.0.0.1:8787/v1/aether/agent `
  -H "Authorization: Bearer demo" `
  -d "{\"subject\":\"AETHER\",\"relation\":\"proibe\"}"

# API — dialeto OpenAI (evidência ou abstain — não prosa livre)
curl -s -X POST http://127.0.0.1:8787/v1/chat/completions `
  -H "Authorization: Bearer demo" `
  -d "{\"model\":\"aether\",\"messages\":[{\"role\":\"user\",\"content\":\"capital do Brasil\"}]}"

# CLI
cargo run --release --bin aether-agent -- ask `
  --facts data/agent_facts.jsonl --subject AETHER --relation proibe --json
```

O que **não** esperar: parágrafo fluente gerado pelo núcleo; generalização
semântica tipo BERT sem transdutor/borda; ACQ 100 como “empata com GPT”.

---

## 5. Gates de freeze (F1–F7)

Rodar em `--release`, host preferencialmente `contended_false`.

| ID | Gate | Critério |
|---|---|---|
| F1 | `agent_eval_suite` | `action_accuracy=1.0`; `false_act=0`; n≥24 |
| F2 | `cic_abstain_gate` | `false_act=0`; artefato `target/cic_abstain.json` |
| F3 | `api_agent` + `api_eval` / integrity | rotas agent/chat; escopo alheio abstem; sem panic no worker |
| F4 | `usability_gate` + `agent_redteam_gate` | parser/OOD não vira `act` |
| F5 | `cognitive_audit` | ACQ=100; 12/12 MEASURED; `hot_path_bytes=0` |
| F6 | `fmt --check` + `clippy -D warnings` + `test --release --lib` | limpo |
| F7 | [`scripts/v01_freeze_smoke.ps1`](../scripts/v01_freeze_smoke.ps1) | API sobe → cue/agent conhecidos → OOD abstain → scorecard legível |

Script agregador (lab): `scripts/v01_freeze_gates.ps1`.

---

## 6. Usabilidade mínima (produto, não packaging)

- [x] Doc V01 (este arquivo) + links no README / USER_DELIVERY  
- [x] Seção **“5 minutos IA”** no README  
- [x] Frase de fronteira em `/props` (campo `frontier`)  
- [x] Corpus seed `data/agent_facts.jsonl` (≥1 fato conhecido; contraste/OOD no smoke)  
- [x] Hub: validar freeze via Auditoria → ACQ (não fingir chat)  
- [x] Errata de marketing: ACQ ≠ MMLU; donor PAWS ~0,69 ≠ chatbot; E1 PAWS 0,5983 = achado negativo  

---

## 7. Entra / não entra no freeze 0.1

**Entra:** contrato act/cite/propose/abstain; verify→200 com certificado; API +
CLI agent; hub observabilidade; gates F1–F7; doutrina evidência≠instrução;
escopo não vaza (R8); HITL 255.

**Não entra:** D1 ColBERT / BGE-M3; NysHD como classificador PAWS; embed 100k
produção; LanguagePort LLM no núcleo; MSI/cloud/auth/i18n; Produto A/B hardware;
MMLU/Arena.

---

## 8. Ondas até a tag `v0.1.0`

| Onda | Trabalho | Saída |
|---|---|---|
| **V0.1-A** | Este doc + crosslinks + fórum | Plano aprovável |
| **V0.1-B** | Smoke F7, README IA, `/props` frontier, corpus | PR usabilidade |
| **V0.1-C** | F1–F7 verdes + [`CYCLE_CLOSE_V01_FREEZE.md`](CYCLE_CLOSE_V01_FREEZE.md) | Checklist verde |
| **V0.1-D** | Bump `0.1.0` + CHANGELOG + tag git | **Só com pedido explícito humano** |

---

## 9. Checklist de tag 0.1.0

- [ ] F1–F7 verdes no host de release (artefatos em `target/v01_freeze/`)  
- [ ] `CYCLE_CLOSE_V01_FREEZE.md` preenchido com números  
- [ ] README + EXTERNAL_API + este doc alinhados à versão  
- [ ] Pedido humano explícito: bump `Cargo.toml` + tag `v0.1.0`  
- [ ] Commit/changelog Conventional Commits  

---

## 10. Risco principal

Vender 0.1 como “IA que conversa”. Mitigação: F7 com OOD→abstain no caminho
feliz; frase de fronteira em toda superfície; tiers T0≠T3 em `EVAL_AGENT.md`.

# Guia do usuário AETHER (5 minutos)

Hub Windows: métricas cognitivas, vazão, hardware, demos e gráficos ao vivo — sem ler a arquitetura interna.

**Frase de fronteira (v0.1):** LLM gera; AETHER cita, certifica ou abstem.  
O hub **observa** (ACQ/jobs). A IA usável responde via `aether-api` / `aether-agent` — ver [V01_DELIVERY_PLAN.md](V01_DELIVERY_PLAN.md).

Detalhe técnico do hub: [HUB_DASHBOARD.md](HUB_DASHBOARD.md).

## Pré-requisitos

- Rust (toolchain estável) + MinGW no PATH no Windows-gnu (ou use `.\scripts\cargo.ps1`).
- Repositório clonado; preferir build `--release`.

## 1) Abrir o hub

```powershell
cd C:\Users\msrov\Documents\AETHER
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Layout: abas **Lab** | **Testes IA**; **centro** = medidores da aba; **direita** = Detalhe (chip glossário/hover/ficha + gráficos). Ficha T0x fica fixa até **Limpar ficha** ou outro clique.

## 2) Rodar a auditoria cognitiva

1. Clique em **Rodar auditoria cognitiva**.
2. Aguarde o job (`Running` → `Done`). Pode levar alguns minutos.
3. O scorecard `target/aether_scorecard.json` é regenerado; a UI recarrega sozinha.

Leia **ACQ** (meta de lab ≥ 99) e a grade **T01–T12** (MEASURED no suite completo).

## 3) Entender os medidores

- **Hover** em qualquer card → texto na coluna direita (sem caixa flutuante).
- **Clique** em T0x → ficha (interpretar / se baixo / proveniência) na direita.
- Idle na direita → glossário curto.

Grandezas de vazão usam 4 dígitos (`1,586` · `87k` · `235kk`), não `eN`.

Tabela canônica: [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md).

## 4) Benchmark + hardware

1. **Benchmark de vazão** — Basal / Vigilante / Crítico (~5 s cada).
2. **Sondar hardware** — CPU/RAM/features + gaps → `target/aether_host_report.json`.

## 5) Demos + gráficos

| Botão | O que faz | Gráficos (direita) |
|---|---|---|
| **Demo organismo** | Organismo in-process ~15 s | `R`, `H`, `S`, chatter, `H(t)` |
| **Demo daemon** | `aether-daemon` ~10 s (confirma multicast) | `R(LAN)`, `F`, `S` |

Um job por vez. **Cancelar job** interrompe.

## 6) Doação · ETL · Transplante (aba Lab)

Painel no centro (após vazão): **origem**, **destino**, **gates**, **E3**, chip **USO → aba Testes IA**.

1. Clique **Probe doação HV** (ETL dry-run se faltar bank + probe + gate + uso).
2. Hover → texto completo na coluna **Detalhe**.
3. Playbook: [DOACAO_PLAYBOOK.md](DOACAO_PLAYBOOK.md).

## 7) Testes IA

Na aba **Testes IA**:

1. **Regenerar USO** — grade U1–U6 do transplante.
2. **Usabilidade API** — exact / ruído / paráfrase / maiúsculas / +1 char / prefixo / escopo / chat (precisa feature `api` no job).
3. **Test drive agente** — Brasil (não abstém) · Atlântida (abstém).
4. Contrato U1–U13 (MSG 0449): *LLM gera; AETHER cita, certifica ou abstém.*

Artefatos: `target/aether_transplant_uso.json`, `target/aether_api_eval.json`, `target/aether_agent_drive.json`.

## Recarregar

**Recarregar** relê scorecard + host + doação + artefatos da aba Testes IA.

## 8) Produto C — escudo de host (fora do hub)

```powershell
.\scripts\cargo.ps1 run --release --bin aether-daemon -- --duration 15s --human
# ou zip: .\scripts\package-daemon.ps1 → target\aether-daemon-win.zip
.\scripts\produto-c-smoke.ps1
```

Estados: **CALMO / ALERTA / SONHANDO**. Guia: [PRODUTO_C.md](PRODUTO_C.md).

## 9) 5 minutos IA (fora do hub — coprocessador)

```powershell
# Gates F1–F6 (lab)
.\scripts\v01_freeze_gates.ps1

# Smoke F7 (API: conhecido → cite/hit; OOD → abstain)
.\scripts\v01_freeze_smoke.ps1

# CLI
.\scripts\cargo.ps1 run --release --bin aether-agent -- ask `
  --facts data/agent_facts.jsonl --subject AETHER --relation proibe --json
```

Na aba **Testes IA**, use **Usabilidade API** e **Test drive agente** para ver o mesmo contrato na UI.

## Fora deste hub (nesta fase)

Chat LLM / MMLU / Qwen 7B no hub, macOS/Linux GUI, MSI, auth/cloud — ver [USER_DELIVERY_PLAN.md](USER_DELIVERY_PLAN.md).  
**Trilha R** (não bloqueia tag 0.1): B-G0 / D1 ColBERT / embed 100k / NysHD.  
US1 (medição com usuário real) fica para o tier T2; US0 é wiring dos gates já existentes.

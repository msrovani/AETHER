# Hub Windows `aether-dashboard`

Superfície única de lab/usuário nesta fase (egui/eframe, feature `dashboard` + `m4`).

## Abrir

```powershell
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

## Abas

| Aba | Conteúdo |
|---|---|
| **Lab** | Marca, jobs (auditoria/bench/HW/demos/probe doação), ACQ, T01–T12, vazão, Doação·ETL (chip USO → Testes IA), gaps, host, Detalhe/gráficos |
| **Testes IA** | Transplante USO U1–U6 · usabilidade API · AgentDrive · contrato U1–U13 (MSG 0449) |

Single-flight global: um job por vez em qualquer aba.

## Layout

| Região | Conteúdo |
|---|---|
| Barra superior | **AETHER** + versão + seletor Lab \| Testes IA + jobs da aba ativa + status |
| Centro (Lab) | ACQ, cobertura, hot-path, T01–T12, vazão, doação (origem/destino/gates/E3 + chip USO), gaps, host |
| Centro (Testes IA) | Grade U1–U6, tabela API eval, painel ask/cite/abstain, contrato usabilidade |
| Coluna direita **Detalhe** | Chip glossário/hover/ficha; hover sem tooltip flutuante; ficha T0x fixa; textos longos; gráficos demo |

Tema: slate + teal (lab). Host report é relido do JSON após o job (sem re-sonda) e no arranque/`Recarregar` se existir.

## Jobs (single-flight)

### Lab

| Botão | Efeito |
|---|---|
| Auditoria cognitiva | `cargo test --release --test cognitive_audit` → `target/aether_scorecard.json` |
| Benchmark de vazão | `throughput_tps` (~5 s/regime) mescla throughput no scorecard |
| Sondar hardware | CIM/PowerShell + topologia → `target/aether_host_report.json` + gaps |
| Demo organismo | **in-process** 15 s @ 30 Hz; telemetria → anel `DemoTrace` |
| Demo daemon | processo `aether-daemon --duration 10s`; parse de linhas → mesmo anel |
| Probe doação HV | ETL dry-run (se faltar bank) + probe + gate + `transplant_uso` → `data/rounds/hub_<epoch>/` |

### Testes IA

| Botão | Efeito |
|---|---|
| Regenerar USO | `python tools/transplant_uso.py` → `target/aether_transplant_uso.json` |
| Usabilidade API | `cargo test --release --features api --test api_eval` → `target/aether_api_eval.json` |
| Test drive agente | `aether-agent ask` Brasil/capital + Atlântida/capital → `target/aether_agent_drive.json` |

## Contrato usabilidade (MSG 0449)

Fronteira: *LLM gera; AETHER cita, certifica ou abstém.*

A aba Testes IA lista U1–U13 com o resultado esperado AETHER (hover no Detalhe). Comparação vs LLM:

| Dimensão | AETHER | LLM normal |
|---|---|---|
| Incerteza | Abstain explícito | Alucina com confiança |
| Proveniência | Key + escopo + autoridade | Citação cosmética |
| Corretude | Certificado (violations=0) | Texto convincente |
| Controle | HITL + rollback | Autonomia sem before-image |
| Determinismo | Replay bit-a-bit | Amostragem estocástica |
| Resistência | Injeção → OOV | Prompt injection funciona |

**US0** (wiring): AgentDrive ≈ U2/U3; `api_eval` ≈ U4; gates E3/undo/redteam/D1/forget já cobrem U3–U12. **US1** (T2): medição com usuário real — fora desta onda.

## Doação / Transplante (painel Lab)

| Bloco | Fonte |
|---|---|
| Origem | `data/rounds/20261117_b1/manifest.json` (fallback `bg0` → `data/manifest.json`) |
| Destino | HvBank AETHV1, T0 intacto, auth≤240 |
| Gates ρ / R@10 / lat | `target/aether_transplant_hv.json` |
| E3 | `target/aether_e3.json` |
| Chip USO | resume `pass_all`; grade completa na aba Testes IA |

Playbook: [DOACAO_PLAYBOOK.md](DOACAO_PLAYBOOK.md). Loader: `src/dashboard/transplant_view.rs`.

## Gráficos (direita)

Séries do anel (`src/dashboard/demo_trace.rs`, até 600 pts): organismo `R`/`H`/`S`; daemon `R(LAN)`/`F`/`S`.

## Números na UI

`fmt_compact`: **4 dígitos** + vírgula + `k` / `kk` / `kkk`.

## Fontes

- Tooltips/fichas: `src/dashboard/catalog.rs`
- Métricas: [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md)
- Guia: [USER_GUIDE.md](USER_GUIDE.md)
- Avaliação API: [API_EVALUATION.md](API_EVALUATION.md)

## Fora de escopo (ainda)

Chat LLM / MMLU / Qwen 7B no hub, macOS/Linux GUI, MSI, auth/cloud, reabrir BAM/JIT/governor na barra.

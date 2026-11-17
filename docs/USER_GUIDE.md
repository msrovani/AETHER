# Guia do usuário AETHER (5 minutos)

Hub Windows: métricas cognitivas, vazão, hardware, demos e gráficos ao vivo — sem ler a arquitetura interna.

Detalhe técnico do hub: [HUB_DASHBOARD.md](HUB_DASHBOARD.md).

## Pré-requisitos

- Rust (toolchain estável) + MinGW no PATH no Windows-gnu (ou use `.\scripts\cargo.ps1`).
- Repositório clonado; preferir build `--release`.

## 1) Abrir o hub

```powershell
cd C:\Users\msrov\Documents\AETHER
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Layout: **centro** = medidores; **direita** = Detalhe (chip glossário/hover/ficha + gráficos). Ficha T0x fica fixa até **Limpar ficha** ou outro clique.

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

## 6) Doação · ETL · Transplante

Painel no centro (após vazão): **origem** (doador/átomos/dim/conflitos), **destino** (HvBank/T0/auth), **gates** (ρ, R@10, lat), **E3**, **U1–U6**.

1. Clique **Probe doação HV** (ETL dry-run se faltar bank + probe + gate + uso).
2. Hover nos medidores → texto completo na coluna **Detalhe**.
3. Playbook clonável: [DOACAO_PLAYBOOK.md](DOACAO_PLAYBOOK.md).

Artefatos: `data/manifest.json`, `target/aether_transplant_hv.json`, `target/aether_transplant_uso.json`.

## Recarregar

**Recarregar** relê scorecard + host + visão de doação (manifest/gates/uso).

## Fora deste hub (nesta fase)

macOS/Linux GUI, MSI, auth/cloud — ver [USER_DELIVERY_PLAN.md](USER_DELIVERY_PLAN.md).
Caminho HF/produção (mapa real Qwen3 MRL 768) e SGDB remember em escala — ver playbook §5.3 / gaps P0.

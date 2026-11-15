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

Layout: **centro** = medidores; **direita** = Detalhe (texto + gráficos).

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

## Recarregar

**Recarregar** só relê o JSON do scorecard já gerado.

## Fora deste hub (nesta fase)

macOS/Linux GUI, MSI, auth/cloud — ver [USER_DELIVERY_PLAN.md](USER_DELIVERY_PLAN.md).

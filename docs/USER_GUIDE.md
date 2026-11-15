# Guia do usuário AETHER (5 minutos)

Hub Windows: métricas cognitivas, vazão, hardware e demos CLI — sem ler a arquitetura interna.

## Pré-requisitos

- Rust (toolchain estável) + MinGW no PATH no Windows-gnu (ou use `.\scripts\cargo.ps1`).
- Repositório clonado; preferir build `--release`.

## 1) Abrir o hub

```powershell
cd C:\Users\msrov\Documents\AETHER
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

A janela **AETHER** mostra status do último load e botões de ação.

## 2) Rodar a auditoria cognitiva

1. Clique em **Rodar auditoria cognitiva**.
2. Aguarde o job (`Running` → `Done`). Pode levar alguns minutos (compila + suite).
3. O scorecard `target/aether_scorecard.json` é regenerado; a UI recarrega sozinha.

Leia o card **ACQ** (meta de lab ≥ 99) e a grade **T01–T12** (todas MEASURED no suite completo).

## 3) Entender os medidores

- **Hover** em qualquer card: tooltip pedagógico (pt-BR).
- **Clique** em um card T0x: ficha com proveniência, “como interpretar” e “se baixo, o que falha”.
- Painel **O que estou vendo?** / botão **Ajuda**: glossário (inferência, coerência, RAG, ACQ, cycles/s, eq-t/s, SOP/s, TOP/s).

Tabela canônica: [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md).

## 4) Benchmark de vazão + máquina hospedeira

1. **Benchmark de vazão** — regimes Basal / Vigilante / Crítico (~5 s cada); mescla throughput no scorecard.
2. **Sondar hardware** — CPU lógico/físico, RAM, features (`affinity` / `jit` / `hw-entropy`) e **gaps** heurísticos (`target/aether_host_report.json`).

Gaps não culpabilizam o usuário: ex. cauda p99.9 alta sob carga do SO, BAM > L1d, affinity off, scorecard ausente, MinGW fora do PATH.

## 5) Demos práticas

| Botão | O que faz |
|---|---|
| **Demo organismo** | `aether-organism --duration 15s --plain` (processo filho) |
| **Demo daemon** | pede confirmação (multicast UDP) e roda ~10 s |

Um job por vez (single-flight). Use **Cancelar job** se precisar interromper.

## Recarregar sem reexecutar

**Recarregar** só relê o JSON do scorecard já gerado.

## Fora deste hub (nesta fase)

Live stream do daemon na GUI, macOS/Linux GUI, MSI, auth/cloud — ver [USER_DELIVERY_PLAN.md](USER_DELIVERY_PLAN.md).

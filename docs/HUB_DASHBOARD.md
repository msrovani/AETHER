# Hub Windows `aether-dashboard`

Superfície única de lab/usuário nesta fase (egui/eframe, feature `dashboard` + `m4`).

## Abrir

```powershell
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

## Layout

| Região | Conteúdo |
|---|---|
| Barra superior | Marca AETHER, jobs (auditoria / bench / HW / demos), status Idle/Running/Done/Failed |
| Centro | ACQ, cobertura, hot-path, grade T01–T12, vazão, host + gaps |
| Coluna direita **Detalhe** | Explicação no hover (sem tooltip flutuante); ficha T0x no clique; glossário no idle; **gráficos do demo** |

## Jobs (single-flight)

| Botão | Efeito |
|---|---|
| Auditoria cognitiva | `cargo test --release --test cognitive_audit` → `target/aether_scorecard.json` |
| Benchmark de vazão | `throughput_tps` (~5 s/regime) mescla throughput no scorecard |
| Sondar hardware | CIM/PowerShell + topologia → `target/aether_host_report.json` + gaps |
| Demo organismo | **in-process** 15 s @ 30 Hz; telemetria → anel `DemoTrace` |
| Demo daemon | processo `aether-daemon --duration 10s`; parse de linhas → mesmo anel |

## Gráficos (direita)

Séries do anel (`src/dashboard/demo_trace.rs`, até 600 pts):

- organismo: `R`, `H`, `S`, chatter, custo µs, curva `H(t)` do último relaxamento
- daemon: `R(LAN)`, `F`, `S` (parse da telemetria de stdout)

## Números na UI

Formatador `fmt_compact`: **4 dígitos** + vírgula decimal + sufixos `k` / `kk` / `kkk` (sem notação `eN`).

## Fontes de verdade textual

- Tooltips/fichas/glossário: `src/dashboard/catalog.rs`
- Tabela canônica: [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md)
- Guia curto: [USER_GUIDE.md](USER_GUIDE.md)

## Fora de escopo (ainda)

macOS/Linux GUI, MSI, auth/cloud, live multicast na GUI além do parse de stdout do daemon.

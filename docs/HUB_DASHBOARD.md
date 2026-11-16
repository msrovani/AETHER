# Hub Windows `aether-dashboard`

Superfície única de lab/usuário nesta fase (egui/eframe, feature `dashboard` + `m4`).

## Abrir

```powershell
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

## Layout

| Região | Conteúdo |
|---|---|
| Barra superior | Marca **AETHER** + versão, jobs (auditoria / bench / HW / demos), faixa de status do job |
| Centro | ACQ, cobertura, hot-path, grade T01–T12, vazão, **Doação·ETL·Transplante** (origem/destino/gates/U1–U6), host + gaps |
| Coluna direita **Detalhe** | Chip de modo (glossário / hover / ficha); hover sem tooltip flutuante; ficha T0x **fixa** até limpar/outro clique; textos longos de doação/ETL; gráficos do demo |

Tema: slate + teal (lab). Host report é relido do JSON após o job (sem re-sonda) e no arranque/`Recarregar` se existir.

## Jobs (single-flight)

| Botão | Efeito |
|---|---|
| Auditoria cognitiva | `cargo test --release --test cognitive_audit` → `target/aether_scorecard.json` |
| Benchmark de vazão | `throughput_tps` (~5 s/regime) mescla throughput no scorecard |
| Sondar hardware | CIM/PowerShell + topologia → `target/aether_host_report.json` + gaps |
| Demo organismo | **in-process** 15 s @ 30 Hz; telemetria → anel `DemoTrace` |
| Demo daemon | processo `aether-daemon --duration 10s`; parse de linhas → mesmo anel |
| Probe doação HV | ETL dry-run (se faltar bank) + `transplant_hv_probe` + `transplant_hv_gate` → recarrega painel |

## Gráficos (direita)

Séries do anel (`src/dashboard/demo_trace.rs`, até 600 pts):

- organismo: `R`, `H`, `S`, chatter, custo µs, curva `H(t)` do último relaxamento
- daemon: `R(LAN)`, `F`, `S` (parse da telemetria de stdout)

## Números na UI

Formatador `fmt_compact`: **4 dígitos** + vírgula decimal + sufixos `k` / `kk` / `kkk` (sem notação `eN`).

## Doação / Transplante (painel)

| Bloco | Fonte |
|---|---|
| Origem (doador, átomos, dim, conflitos, shards, hashes) | `data/manifest.json` |
| Destino (HvBank AETHV1, T0 intacto, auth≤240) | estático + paths em `transplant_view` |
| Gates ρ / R@10 / lat / pass | `target/aether_transplant_hv.json` |
| E3 geometria / analogia | `target/aether_e3.json` |
| Usabilidade U1–U6 | `target/aether_transplant_uso.json` |

Playbook: [DOACAO_PLAYBOOK.md](DOACAO_PLAYBOOK.md). Loader: `src/dashboard/transplant_view.rs`.

## Fontes de verdade textual

- Tooltips/fichas/glossário: `src/dashboard/catalog.rs`
- Tabela canônica: [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md)
- Guia curto: [USER_GUIDE.md](USER_GUIDE.md)

## Fora de escopo (ainda)

macOS/Linux GUI, MSI, auth/cloud, live multicast na GUI além do parse de stdout do daemon.

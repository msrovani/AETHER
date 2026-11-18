# AETHER — Mapa do entregável ao usuário + plano de entrega

> Claim: **MSG 0089** · Documento vivo · Público: produto / PM / engenharia  
> Estado do motor cognitivo (lab): **ACQ 100 · 12/12 MEASURED · 0 B hot-path**  
> Estado do **produto** para usuário final: **embrião de demo** (não shippável).  
> **Freeze IA usável (v0.1):** ver [`V01_DELIVERY_PLAN.md`](V01_DELIVERY_PLAN.md) — coprocessador
> act/cite/abstain (API/CLI); este doc continua o eixo demo/hub/Produto C–A/B.

Este documento separa o que já foi conquistado no **laboratório** do que
ainda falta para um humano não-agente usar o AETHER como produto físico/ops.

---

## 1. Veredito em uma frase

O AETHER hoje é um **motor cognitivo pós-token maduro em bancada** com demos
CLI (`aether-organism`, `aether-daemon`) e **hub Windows** (`aether-dashboard`)
para métricas, jobs e gaps de host. Ainda faltam empacote MSI, TUI ligada no
fluxo padrão, backends de produto A/B e superfície cloud/auth.

---

## 2. Mapa do que já existe (superfície humana)

### 2.1 Camadas

```text
┌─────────────────────────────────────────────────────────────┐
│  PRODUTO (ship)          AUSENTE                            │
│  install · packaging · GUI · cloud · auth · i18n · store    │
├─────────────────────────────────────────────────────────────┤
│  DEMO / LAB              PARCIAL → quase pronto             │
│  aether-organism (ANSI) · aether-daemon (telemetria LAN)    │
│  examples: field_probe · transducer_probe                   │
├─────────────────────────────────────────────────────────────┤
│  MOTOR / RESEARCH        PRONTO EM LAB (não é UX)           │
│  M1–M8 · FHRR/BAM · Koopman/SINDy · CBF · RG · Swarm        │
│  Governor/JIT/affinity · auditoria cognitiva ACQ 100        │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Inventário detalhado

| Entregável | Path | Camada | Estado | O que o usuário vê |
|---|---|---|---|---|
| Organismo reflexivo | `src/bin/organism.rs` + `simulation/` | Demo | **Parcial** | Banner + painel ANSI: `R(t)`, entropia, `H(t)`, bits `s*`, fases |
| Daemon encarnado | `src/bin/aether_daemon.rs` | Demo / embrião **Produto C** | **Parcial** | Linha de telemetria: `R(LAN)`, peers, `F`, sonho/ação, drops |
| Painel ANSI | `src/simulation/panel.rs` (`PlainPanel`) | Demo | **Pronto** | Atualização ~30 ms no terminal |
| Painel Ratatui | mesmo arquivo (`RatatuiPanel`) | Lab | **Liga com `--features tui -- --tui`** | TUI no organism; sem feature → plain + aviso |
| Example campo | `examples/field_probe.rs` | Demo | Pronto | Entropia vs carga |
| Example transdutor | `examples/transducer_probe.rs` | Demo | Pronto | `F(t)` + sparkline Zero-UI |
| Example BAM | `examples/bam_probe.rs` | Research | Pronto | Só medição (não produto) |
| Example CRAM→Logos | `examples/cram_logos_probe.rs` | Research / lab only | Pronto | E2d; **não** hub — ver `docs/CRAM_LOGOS.md` |
| Contrato produtos A/B/C | `docs/TRANSDUCER_IO.md` | Spec | Contrato | Sem backend CAN/CSI shippável |
| Transdutor sintético | `src/transducer_io.rs` | Lab | Pronto | Baseline determinístico |
| Transdutor host (daemon) | `aether_daemon` / `HardwareTransducer` | Embrião C | Parcial | Jitter/CPU/rede do próprio processo |
| Docs usuário | `USER_GUIDE.md` + README “5 minutos” | — | **Pronto** | Hub + demo + doação |
| Docs engenharia | `ARCHITECTURE*.md`, `docs/*` | Eng | Pronto | Onboarding de agentes/devs |
| Empacotamento | `Cargo.toml` `publish=false` | — | Ausente | Sem MSI/winget/crates.io |
| GPU / wgpu | `docs/HW_TARGET.md` | Spec | Ausente no código | — |

### 2.3 O que **não** é entregável ao usuário (ainda)

Mesmo com nota 100 na auditoria, estes módulos **não** aparecem no caminho
feliz dos bins:

| Módulo | Uso atual |
|---|---|
| Koopman / SINDy / JIT | lib + testes / benches |
| BAM / FHRR avançado | auditoria + `bam_probe` |
| `aether_governor` (pinning, SPSC) | testes Fase 5 |
| Swarm speciation / RG / Active Inference | audit cognitivo |
| Scorecard / throughput JSON | artefatos de lab |

**Implicação:** “ACQ 100” valida o cérebro; **não** valida a experiência de
produto. São eixos ortogonais.

### 2.4 Gaps de UX já conhecidos (demo)

1. ~~`aether-organism` sem `--help` / `--tui`~~ → `--help` exit 0; `--tui` + `features tui` = Ratatui.
2. ~~README versão drift~~ → README aponta `0.0.157` + “Primeiros 5 minutos”.
3. Daemon: `--help` explica R(LAN)/F/SONHANDO (telemetria ainda densa na linha).
4. Windows: depende de MinGW no PATH (`scripts/cargo.ps1`) — barreira de install.
5. Multicast LAN sem auth (by design) — ok para lab, não para produto aberto.
6. Produtos A (CAN) e B (CSI/áudio) só no contrato `TRANSDUCER_IO`.

---

## 3. Personas e hipóteses de valor

| Persona | Precisa de | Entrada física | Binário-alvo |
|---|---|---|---|
| **Dev / pesquisador** | ver o organismo vivo, reproduzir ACQ | sintético | organism + examples |
| **Ops / lab multi-PC** | enxame sem texto na LAN | host telemetry | daemon |
| **Produto C — escudo de sistema** | alerta contínuo de surpresa do host | jitter/CPU/disco/rede reais | daemon + `TransducerIo` |
| **Produto A — copiloto veicular** | campo contínuo de vibração/RPM | CAN/OBD | futuro backend |
| **Produto B — casa cognitiva** | presença/movimento | CSI / mic | futuro backend |

Prioridade de entrega ao **usuário** (não ao agente):  
**Dev-demo → Produto C MVP → A/B** (A/B exigem hardware e certificação).

---

## 4. Plano de entrega de funcionalidades

### Norte

> Em 3 ondas, um usuário técnico consegue: (1) instalar e ver o organismo;
> (2) operar um escudo de host com linguagem clara; (3) plugar um sensor real
> sem reescrever o motor.

### Onda 0 — “Demo honesta” (1–2 semanas) · **usuário = desenvolvedor**

| # | Entrega | Critério de aceite |
|---|---|---|
| 0.1 | Ligar `--tui` / `--help` no `aether-organism` | `--help` exit 0; `--tui`+`features tui` → Ratatui; senão plain+aviso (**feito**) |
| 0.2 | README de usuário (não só eng) | “Primeiros 5 minutos” + versão Cargo.toml (**feito**) |
| 0.3 | Script `scripts/run-demo.ps1` | build release + organism 15 s + tip daemon/hub (**feito**) |
| 0.4 | Alinhar telemetria daemon com `--help` já existente | R(LAN)/F/SONHANDO no help (**feito**) |
| 0.5 | Congelar superfície: **não** expor BAM/JIT/governor no CLI | help organism/daemon dizem lab only (**feito**) |
| **0.6 / (5)** | **Hub Windows** (`aether-dashboard`) | ACQ + T01–T12 + vazão; hover/fichas; jobs Auditoria / Benchmark / HW / demos / **Probe doação HV**; painel **Doação·ETL·Transplante** (origem/destino/gates/U1–U6); gaps heurísticos (**feito** 2026-11-17) |

**Resultado:** qualquer dev com Rust+MinGW roda a demo sem ler ARCHITECTURE;
no Windows, o hub é a superfície única de métricas + testes práticos.

**Status Onda 0 (2026-11-17):** fechada para aceite lab. Fechamento: [`CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md`](CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md). Gates ainda necessários (próximo ciclo): B-G0, B1 LSH≥10k, B4 produção, SGDB-remember real, escriba, FFT-G2/JL-V4.

### Entrega (5) — Hub Windows (detalhe)

| Item | Spec |
|---|---|
| Binário | `aether-dashboard` · feature `dashboard` · **somente Windows** (`cfg(windows)`) |
| Fonte | `target/aether_scorecard.json` (+ throughput); `target/aether_host_report.json`; doação: `data/manifest.json` + `target/aether_transplant_{hv,uso}.json` + `aether_e3.json` |
| Layout | barra de ações + ACQ/coverage/hot-path + grade T01–T12 + vazão + **Doação·ETL·Transplante** + HW/gaps |
| Hover / clique | tooltips pedagógicos; ficha demonstrativa T0x no click; hover doação → Detalhe |
| Jobs | single-flight: auditoria, bench vazão, sonda HW, demo organism/daemon, **Probe doação HV** |
| Gaps | `SCORECARD_MISSING`, `TAIL_LATENCY_HIGH`, `L1D_PRESSURE`, `AFFINITY_OFF`, `TOOLCHAIN_PATH`, … |
| Docs | [USER_GUIDE.md](USER_GUIDE.md) · [DASHBOARD_METRICS.md](DASHBOARD_METRICS.md) · [HUB_DASHBOARD.md](HUB_DASHBOARD.md) |
| Gráficos | coluna direita: telemetria ao vivo do demo organismo (in-process) e parse do daemon |
| Fora de escopo agora | macOS/Linux GUI, MSI, cloud, multicast peer-visualizer completo |

Implementação: `src/dashboard/*` (`catalog`, `jobs`, `host_probe`, `gaps`, `model`) + `src/bin/aether_dashboard.rs` (egui/eframe).

### Onda 1 — “Produto C MVP” (3–6 semanas) · **usuário = operador de lab/ops**

| # | Entrega | Critério de aceite |
|---|---|---|
| 1.1 | `SystemTelemetryTransducer: TransducerIo` real | sample_into zero-alloc; F≈0 em idle; F sobe sob stress |
| 1.2 | Daemon consome `TransducerIo` (não só busy-loop ad hoc) | troca de backend sem mudar o loop cognitivo |
| 1.3 | Modo `--human` / painel de estados | estados: CALMO / ATERTA / SONHANDO + 1 frase de ação |
| 1.4 | Empacote Windows: zip + `aether-daemon.exe` + README curto | roda sem `cargo` (ainda pode exigir VC++/MinGW runtime se necessário) |
| 1.5 | Log rotativo opcional (binário ou CSV de floats) | sem JSON de tokens; colunas fixas |
| 1.6 | Teste E2E “duas instâncias → peers_heard≥1, R>0.9” | CI ou script documentado |

**Resultado:** um operador vê o host “respirar” e um enxame acoplar, sem IDE.

### Onda 2 — “Plugar o mundo” (6–12 semanas) · **usuário = early adopter A/B**

| # | Entrega | Critério de aceite |
|---|---|---|
| 2.1 | Backend CAN (Produto A) atrás de feature `product-a` | mesmo `u(t)` 64-D; demo em hardware ou replay |
| 2.2 | Backend CSI/áudio (Produto B) atrás de `product-b` | idem |
| 2.3 | Política de privacidade LAN (opt-in multicast, allowlist) | default seguro em WAN |
| 2.4 | Guia de integração `TransducerIo` (1 página + example) | terceiro implementa um backend em <1 dia |
| 2.5 | (Opcional) GPU wgpu só se N≥512 e fallback CPU | não bloquear MVP C |

**Resultado:** o motor deixa de ser “só PC” e vira plataforma de campo contínuo.

### Onda 3 — “Produto de prateleira” (depois) · **fora do escopo imediato**

GUI desktop/web, conta/auth, cloud sync, loja, i18n completo, SLA.  
Só depois das Ondas 0–2 estáveis — senão embalamos um lab.

---

## 5. O que **não** fazer na entrega ao usuário

| Tentação | Por quê evitar agora |
|---|---|
| Expor Koopman/SINDy/JIT no CLI | Pesquisa; UX incompreensível sem narrativa |
| Migrar default para BAM no painel | Já READY_FOR_INTEGRATION no lab; produto não depende disso |
| GPU como headline | Sem transducer real, GPU é vanity |
| SaaS / auth | Quebra o invariante “zero tokens na borda” sem desenho claro |
| Reescrever o motor “para ficar simples” | O valor é a física; simplificar a **borda**, não o núcleo |

---

## 6. Métricas de sucesso por onda

| Onda | Métrica usuário | Métrica lab (não regredir) |
|---|---|---|
| 0 | Time-to-first-panel < 15 min (dev com Rust) | ACQ ≥ 99, 12/12, 0 B hot-path |
| 1 | Ops roda daemon sem `cargo`; entende CALMO/ALERTA | Mesmo gate + teste 2-instâncias |
| 2 | Um backend externo passa no contrato `TransducerIo` | Gate + benches de I/O |

---

## 7. Ordem de implementação sugerida (próximos PRs)

1. **PR-U0** — organism: `--help` + wiring Ratatui (`--tui`).  
2. **PR-U0b / (5)** — **dashboard Windows** (`aether-dashboard`) com hover/tooltips.  
3. **PR-U1** — `docs/USER_GUIDE.md` + README “5 minutos” + `run-demo.ps1`.  
4. **PR-U2** — daemon `--human` + glossário R/F no banner.  
5. **PR-U3** — `SystemTelemetryTransducer` + integração no daemon.  
6. **PR-U4** — artefato zip Windows + smoke test.  
7. **PR-U5+** — backends A/B conforme hardware disponível.

Cada PR: claim no fórum, não tocar ownership BUFFY (`phase_memory`,
`morphogenesis`, testes M5/M7 hardening) sem coordenação.

---

## 8. Relação com o trabalho já feito (lab)

| Entrega lab | Serve ao usuário como… |
|---|---|
| M1–M8 + organism/daemon | Base da demo (Onda 0) |
| TransducerIo + Synthetic | Contrato da Onda 1–2 |
| ACQ 100 / throughput Basal 142 µs | Prova de desempenho (marketing técnico, não UX) |
| BAM 2.7× FHRR | Aceleração futura do RAG interno (invisível ao usuário) |
| Affinity −95% p99.9 | Estabilidade do daemon sob carga (Onda 1) |
| Governor / JIT | Reserva para escala; **não** Onda 0 |

---

## 9. Próximo passo imediato

Onda 0 fechada. Próximo: **B-G0** (labels reais / paráfrase) + **SGDB-remember**
real + rodada produção (B4) via sidecar `--jl-only`; Onda 1 só após demo
operável sem reabrir T0.

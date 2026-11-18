# AETHER — motor cognitivo pós-token em Rust

Versão crate: ver `Cargo.toml` (freeze IA usável → **0.1.0** quando gates F1–F7
verdes; plano: [`docs/V01_DELIVERY_PLAN.md`](docs/V01_DELIVERY_PLAN.md)).
Suite cognitiva: `docs/COGNITIVE_AUDIT_RUN_2026-11-17.md` (ACQ 100 · 12/12).

Protótipo experimental de computação baseada em física contínua: **zero tokens,
zero transformers, zero protocolos semânticos de rede, zero backpropagation**.

**Frase de fronteira:** LLM gera; AETHER cita, certifica ou abstem.

## Primeiros 5 minutos (Windows)

```powershell
cd C:\Users\msrov\Documents\AETHER
.\scripts\run-demo.ps1                  # organism ~15 s (ANSI)
# opcional Ratatui:
# .\scripts\cargo.ps1 run --release --bin aether-organism --features tui -- --duration 15s --tui
.\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard
```

Guia do hub: [docs/USER_GUIDE.md](docs/USER_GUIDE.md) · doação/ETL: [docs/DOACAO_PLAYBOOK.md](docs/DOACAO_PLAYBOOK.md).

## 5 minutos IA (coprocessador — não chatbot)

```powershell
# Gates de freeze (lab)
.\scripts\v01_freeze_gates.ps1

# Smoke F7 (sobe API, fato conhecido, OOD abstain)
.\scripts\v01_freeze_smoke.ps1

# CLI agente
.\scripts\cargo.ps1 run --release --bin aether-agent -- ask `
  --facts data/agent_facts.jsonl --subject AETHER --relation proibe --json
```

Contrato e checklist de tag: [docs/V01_DELIVERY_PLAN.md](docs/V01_DELIVERY_PLAN.md) ·
API: [docs/EXTERNAL_API.md](docs/EXTERNAL_API.md). O hub **observa** (ACQ); a IA
**responde** via `aether-api` / `aether-agent`.

* **ARCHITECTURE.md** — Fase 1 (Módulos 1–4): organismo reflexivo.
* **ARCHITECTURE_PHASE2.md** — Fase 2 (Módulos 5–8): memória, imaginação,
  morfogênese e encarnação em LAN.
* **docs/API_CONTRACT.md** — contrato normativo de API entre módulos.
* **docs/HW_TARGET.md** — hardware alvo (PC/note/servidor, CPU multithread + GPU AMD/Intel/NVIDIA).
* **docs/HARDENING_REPORT.md** - auditoria, correcoes e medicoes finais (2026-11-15).
* **docs/COGNITIVE_AUDIT_RUN_2026-11-17.md** — reexecução da auditoria cognitiva (12/12 `MEASURED`, ACQ 100, 318 testes).
* **docs/USER_DELIVERY_PLAN.md** — mapa do entregável ao usuário + plano de ondas (demo → Produto C → A/B).
* **docs/V01_DELIVERY_PLAN.md** — freeze v0.1: IA usável (coerência act/cite/abstain + gates F1–F7).
* **docs/O_PROBLEMA_DOS_3_CORPOS_NAO.md** (+ `_VOL2`, `_VOL3`) — dossiê dos três "NÃO" (linguagem, lógica, self): tese, preço em hardware e gestão do conhecimento. Registro mestre no Vol. III §11.
* **docs/SGDB_INTEGRATION.md** — encaixe operacional do `neural-sgdb` (`T2`): MCP, outbox, leitura de fase e gates F4.
* **docs/AETHER_AGENT.md** — organismo agêntico: inferência, MoE, coerência e autoaprendizado (`aether-agent`).
* **docs/AETHER_AGENT_ROADMAP.md** - plano v0.1→v1.0: autonomia (autoescolha EFE/UCB), transplante em escala e híbrido LLM; tiers e gates.
* **docs/ADR_AGENT.md** — decisões (T2, sem gradiente, memória=prior, feedback verificado, linguagem na borda).
* **docs/LANGUAGE_PORT.md** — por que “LLM de borda” e alternativas (parser/gramática, LCM/conceito, reservoir/ESN, xLSTM/RWKV/Mamba/BLT, difusão, spiking, VSA/JEPA) com papers e gates.
* **docs/LESSONS_AETHER_AGENT.md** - lições do agente: o que quebrou, invariantes e receitas.
* **AGENTS.md** — ritual para agentes de IA usarem a memória (`context` antes, `put` depois) sem tratar memória como instrução.

## Pilares

| Módulo | Feature | Arquivo | Física |
|---|---|---|---|
| 1 | `m1` | `src/core/thermodynamic_solver.rs` | Ising contínuo por Bifurcação Simulada Balística (Euler simplético, paredes inelásticas, elitismo) |
| 2 | `m2` | `src/core/wave_interference.rs` | Operador Neural de Fourier (interferência espectral, invariante à resolução) |
| 3 | `m3` | `src/core/kuramoto_field.rs` | Sincronização de fase P2P por UDP binário de 40 bytes (zero texto) |
| 4 | `m4` | `src/simulation/` | Organismo integrado de loop fechado a 60 Hz |
| 5 | `m5` | `src/phase_memory.rs` | Memória associativa oscilatória com acoplamento complexo (atratores de fase) |
| 6 | `m6` | `src/thermodynamic_dreamer.rs` | GSB na borda do caos + Langevin (amostragem de Boltzmann, incerteza epistêmica) |
| 7 | `m7` | `src/morphogenesis.rs` | Plasticidade topológica por energia livre (crescimento hebbiano + poda metabólica) |
| 8 | `m8` | `src/bin/aether_daemon.rs` | Transdutor físico real + enxame UDP multicast `239.255.42.99:7777` |
| — | `sgdb` | `src/knowledge/sgdb/` | `T2` do `neural-sgdb`: MCP (mock/HTTP/stdio), outbox, recall→fase |
| — | `sgdb` | `src/bin/aether_sgdb.rs` | CLI `aether-sgdb`: doctor, put, recall, sync, export/import |
| — | `agent` | `src/agent/` | organismo agêntico: FHRR/SAT/QUBO/fase/SGDB + MoE + coerência + EXP3 |

## Build

```bash
cargo test --release          # 44 alvos / 269 testes: fase 1 + fase 2 + E1-E8 + F1-F3
cargo clippy --all-targets -- -D warnings
cargo run --release --bin aether-organism   # Fase 1: painel em tempo real
cargo run --release --bin aether-daemon     # Fase 2: organismo encarnado
# Windows — hub de métricas (detalhe à direita + gráficos dos demos):
# docs/USER_GUIDE.md · docs/HUB_DASHBOARD.md · docs/DASHBOARD_METRICS.md
cargo test --release --test cognitive_audit
cargo run --release --bin aether-dashboard --features dashboard
```


No Windows com a toolchain `x86_64-pc-windows-gnu`, use o wrapper
`.\scripts\cargo.ps1` (ajusta o PATH para os binutils do MinGW) ou coloque
`C:\Users\msrov\Tools\mingw64\mingw64\bin` no PATH (necessário porque os crates
`windows-sys ≥ 0.52` usam *raw-dylib*, que exige `dlltool` + `as`).

## Agente (`aether-agent`)

Costura os especialistas do motor em um ciclo agêntico com MoE, coerência e
autoaprendizado (feature `agent`; sem LLM e sem backpropagation):

```bash
cargo run --release --bin aether-agent -- ask --subject Brasil --relation capital
cargo run --release --bin aether-agent -- demo
cargo run --release --bin aether-agent -- tools
cargo run --release --bin aether-agent -- plan --subject AETHER --relation proibe
cargo run --release --bin aether-agent -- pipeline target/workflow.json
cargo run --release --bin aether-agent -- sat target/instancia.dimacs
```

Detalhes, gates e limites: `docs/AETHER_AGENT.md`.

## Enxame multi-máquina

```bash
# máquina A
cargo run --release --bin aether-daemon
# máquina B (mesma rede)
cargo run --release --bin aether-daemon
```

As instâncias acoplam fase automaticamente via multicast — observe `R(LAN)`
subir a ~1.0 e a Surpresa `F(t)` reagir a carga de CPU na outra máquina, sem um
único pacote JSON/HTTP.

## Governador de hardware real (`aether-governor`, Fase 6)

Encarna o laço completo — `HostTransducer` → `EventDrivenCore` →
`WaveInterferenceBlock` → `BamFhrrStore` → `OnlineKoopmanOperator` →
`ActiveInferenceEngine` (CBF de Lyapunov) → `RenormalizationHierarchy` →
`KuramotoPhaseNode` — e publica telemetria em `%TEMP%\aether_state.bin`
(32 bytes, CRC-32 IEEE, `rename` atômico a cada 250 ms).

### Uma máquina

```bash
# headless por 30 s (telemetria no stdout + frame binário)
cargo run --release --bin aether-governor -- --headless --duration-secs 30

# interativo: 1 = pulso térmico · 2 = rajada de ruído · 3 = gravar assinatura · q = sair
cargo run --release --bin aether-governor

# com pinning do núcleo reflexivo (feature opt-in)
cargo run --release --features affinity --bin aether-governor -- \
    --headless --duration-secs 30 --pin-core
```

No modo interativo só os **intervalos** `Δt` entre teclas são medidos; nenhum
caractere é gravado ou exibido.

### Duas ou mais máquinas na mesma LAN

```bash
# máquina A
target/release/aether-governor --headless --duration-secs 60 --node-id 1
# máquina B (mesma rede)
target/release/aether-governor --headless --duration-secs 60 --node-id 2
```

As duas entram no grupo multicast `239.255.42.99:7777` (UDP, pacotes binários
de 40 bytes). `lan_peers` no frame conta os nós ativos e `coherence_r` mistura
a coerência local (RG L2) com o parâmetro de ordem da LAN.

- Windows: libere `aether-governor.exe` no Defender Firewall (rede **privada**)
  na primeira execução, senão o multicast é bloqueado.
- Para rodar sem rede (CI, bancada isolada): `--no-lan`.
- `--node-id` deve ser único por máquina (o default já deriva do PID).

### Ler o estado de fora (PowerShell)

```powershell
$b = [IO.File]::ReadAllBytes("$env:TEMP\aether_state.bin")
[BitConverter]::ToSingle($b, 0)    # coherence_r
[BitConverter]::ToSingle($b, 4)    # identity_confidence
[BitConverter]::ToSingle($b, 8)    # free_energy
$b[12]                             # flow_regime  0=Basal 1=DeepFocus 2=Vigilante 3=Critico
$b[13]                             # niche_role   0=Sentinel 1=Architect 2=Regulator
[BitConverter]::ToUInt16($b, 14)   # lan_peers
[BitConverter]::ToUInt32($b, 16)   # cycle_us
```

Os 4 últimos bytes são o CRC-32 IEEE dos 28 primeiros: um leitor pode recusar
frames rasgados sem lock. O mesmo frame é validado pelo teste
`tests/governor_e2e.rs`.

## Invariantes validados (`cargo test --release`)

1. **Termodinâmico** — `H(s_final) < H(s_0)` estritamente, 50/50 matrizes `J`
   simétricas aleatórias (`N = 64`).
2. **Resolução ondulatória** — o mesmo bloco processa `L = 128/512/2048` com
   pesos bit-a-bit idênticos.
3. **Sincronização sem mensagens** — 4 nós UDP de 40 bytes atingem
   `R(t) > 0.95` em < 3 s (medido: ~0,3 s).
4. **Memória de fase** — recall com 40% de máscara reconstrói fases ocultas
   com erro angular médio < 0.15 rad.
5. **Imaginação** — `U(T=1.5) > U(T=0.001)` e amostras abaixo do estado
   aleatório típico.
6. **Morfogênese** — simetria estrita de `J`, poda no silêncio, zero NaN/Inf.
7. **Engenharia** — `relax_into`/`forward_into` com zero alocação, codec de
   40 bytes com round-trip e rejeição de tamanho, determinismo bit-a-bit.

## Status (2026-11-15)

- Versão do crate: ver `Cargo.toml` (lab cognitivo: **ACQ 100 · 12/12 MEASURED · 0 B** hot-path).
- Superfície de usuário: ainda **demo/lab** (`aether-organism`, `aether-daemon`) — plano em [`docs/USER_DELIVERY_PLAN.md`](docs/USER_DELIVERY_PLAN.md).
- Detalhes de hardening/arquitetura: `docs/HARDENING_REPORT.md`, `ARCHITECTURE_PHASE2.md` §6, `docs/COGNITIVE_AUDIT_REPORT.md`.

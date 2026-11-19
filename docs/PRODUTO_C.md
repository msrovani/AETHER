# Produto C — escudo de host (AETHER)

> Onda 1 do [`USER_DELIVERY_PLAN.md`](USER_DELIVERY_PLAN.md) · tag IA usável: `v0.1.0`  
> Frase: o daemon **sente** o PC e fala CALMO / ALERTA / SONHANDO — não é chatbot.

## O que é

Binário `aether-daemon` em modo humano: telemetria do host (Windows) alimenta o
campo contínuo; a saída é uma linha legível por operador de lab/ops.

| Estado | Significado |
|---|---|
| **CALMO** | surpresa baixa — host estável |
| **ALERTA** | surpresa subindo (`S ≥ 0.5 × --alert`) |
| **SONHANDO** | surpresa alta — organismo amostra futuros |

**S** = surpresa · **R** = coerência LAN com peers multicast (0..1).

## 5 minutos (sem cargo)

```powershell
# a partir do zip target/aether-daemon-win.zip
.\aether-daemon.exe --duration 15s --human
```

Com repositório:

```powershell
.\scripts\cargo.ps1 run --release --bin aether-daemon -- --duration 15s --human
.\scripts\produto-c-smoke.ps1          # gate
.\scripts\produto-c-smoke.ps1 -Package # + zip
.\scripts\two-instances.ps1            # peers≥1, R>0.9
```

## Empacote

`scripts/package-daemon.ps1` → `target/aether-daemon-win.zip`  
(inclui `LEIA-ME.txt` e DLLs MinGW se existirem no PATH de lab).

## Flags úteis

| Flag | Efeito |
|---|---|
| `--human` | linha curta + glossário; no Windows liga `--host` |
| `--host` | `u(t)` da telemetria do SO |
| `--alert <f>` | limiar de sonho (padrão 0.35) |
| `--log arquivo.csv` | colunas `cycles,R_lan,peers,F,S` + rotação ~1 MiB |
| `--duration 30s` | encerra sozinho |

## Fora do Produto C

Chat LLM, MSI/auth/cloud, CAN/CSI (Produtos A/B), BAM/JIT/governor no CLI.

## Aceite Onda 1 (lab)

Verificado 2026-11-18: `--human` CALMO→ALERTA→SONHANDO · zip · two-instances PASS.  
Smoke: `target/produto_c_smoke.json`.

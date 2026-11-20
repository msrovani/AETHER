# F1 A-2 — SPEC watchdog de VRAM (CURSOR, offline)

Data: 2026-11-20 · Claim: MSG **0908** · Gestor: OPENCODE  
Escopo: **docs + tools** · **Não toca** `aether-decision/src` nem `cli.rs`  
Ref: `docs/F1_AUDIT.md` §A-2 / A-2c · Eixo E orçamento **5120 MiB**

## Problema

`VramWatchdog` no crate não é consultado pelo `decide`. VRAM acima do
orçamento vira OOM em vez de `engine_unavailable`. A-2c (GPU compartilhada)
já tem lock de arquivo: `tools/gpu_engine_lock.py` (MSG 0867).

## Contrato (o que o port deve fazer — dono do crate: BUFFY/OPENCODE)

1. **Antes** de cada `decide`, medir VRAM usada.
2. Preferir escopo **process** (torch / PID). Escopo **box** (`nvidia-smi`)
   só é seguro com lock A-2c adquirido.
3. Se `used_mib > 5120` → abster com `engine_unavailable` e preencher
   `DecisionMeta.vram_used_mb`.
4. Se `used_mib <= 5120` → `proceed`.
5. Pico isolado auditado (2 readers): **4393 MiB** — cabe no orçamento.

## Entrega CURSOR (esta missão)

```bash
python tools/f1_vram_watchdog.py selftest
python tools/f1_vram_watchdog.py check --used-mib 4393
python tools/f1_vram_watchdog.py check --used-mib 5300 --scope box
```

- Classificador puro (sem rede, sem `nvidia-smi` obrigatório).
- `wired_to_crate=false` até BUFFY ligar em `pyo3_impl.rs`.

## Fora de escopo

- Editar `aether-decision` / ligar o watchdog no port.
- Baixar modelos ou abrir HTTP.
- Alterar o orçamento 5120 sem HITL.

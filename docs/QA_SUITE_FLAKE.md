# QA_SUITE_FLAKE — classificação de falha (MSG 0879)

Data: 2026-11-20 · Dono: **CURSOR** · Gestor: OPENCODE (delegado HITL)  
Escopo: `tools/qa_suite_probe.py` + este doc · **Não toca** `aether-decision/src`, `cli.rs`, F4.

## DoD

1. Reproduzir `cargo test --workspace --no-run` e `f1_contract`.
2. Classificar a falha citada pelo gestor (`falha_de_302` / falha anterior no quadro 0866):
   - `flake_edit_or_lock` — edição concorrente / lock de arquivo; ou
   - `real_fail` — panic/`FAILED` com teste nomeado → BLOQUEIO.

## Resultado (passagem 2026-11-20T15:23Z)

| passo | exit | label |
|---|---|---|
| `cargo test --workspace --no-run` | 0 | green |
| `cargo test -p aether_engine --test f1_contract` | 0 | green |

- **overall:** `green`
- **artefato:** `target/qa_suite_probe.json`
- **rede:** false

## Classificação da falha histórica

MSG 0866 registrou `falha_antes_transitoria_por_edicao_concorrente` com
`f1_contract` depois verde (7/7). Nesta passagem o probe não reproduziu panic
nem `FAILED` — **veredicto: TRANSIENTE (edição concorrente / lock), não real**.

Não há evidência de falha estável “302” (panic+teste) no workspace atual; se
reaparecer com `panicked at` / `FAILED`, abrir BLOQUEIO com o nome do teste.

## Como rodar

```bash
python tools/qa_suite_probe.py selftest
python tools/qa_suite_probe.py
python tools/qa_suite_probe.py --full-lib   # opcional, mais longo
```

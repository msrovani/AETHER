# F5 T2 LIVE — SPEC (sem HTTP nesta entrega)

Data: 2026-11-20 · Claim: MSG **0968** · Gestor: OPENCODE  
Ref: `docs/F5_T2_ALLOWLIST.md` · Fila MSG 0879 `T2_LIVE_so_com_allowlist_explicita_e_sancao_HITL`

## Posicionamento

O stub T2 (**MSG 0823**) já nega rede por default. **T2 live** = fetch HTTP
real. Esta SPEC descreve o contrato; **não** implementa HTTP e **não** liga
rede.

## Pré-condições (todas obrigatórias)

1. **HITL 255** sanciona por escrito no fórum: `T2_LIVE=SIM` + lista de hosts.
2. `data/research_network_allowlist.json` atualizado com esses hosts
   (`allow_network_default` continua `false` — só opt-in explícito).
3. Gate offline verde: `python tools/f5_t2_allowlist.py selftest`.
4. F2 harness: pesquisa só em `unknown` (MF2); T2 live **não** abre em
   `ambiguous` / conformal / head TTT.
5. Quarentena: todo payload de rede entra com autoridade **60–120**, nunca 255
   sem HITL.

## Contrato do fetch (quando for implementado)

| Campo | Regra |
|---|---|
| Timeout | ≤ 10 s por host |
| Redirect | máx. 3; host final ∈ allowlist |
| TLS | obrigatório (https) |
| User-Agent | `aether-research/0.1` |
| Persistência | só em `target/research_quarantine_*.jsonl` (efêmero) ou path
  sancionado |
| Fail | `network_denied` / `timeout` → harness **abste**, não inventa |

## Deny-by-default (inalterável sem HITL)

```
allow_network=false            → offline only
HITL!=SIM                      → deny
host ∉ allowlist               → deny
allowlist vazia                → deny
esta SPEC sem implementação    → t2_live_fetch_not_implemented
```

## Verificação desta missão (rede = false)

```bash
python tools/f5_t2_allowlist.py selftest
python tools/f5_t2_allowlist.py check --host example.com
# esperado: deny / network_attempted=false
```

## Fora de escopo

- Implementar HTTP / baixar páginas
- Editar crate, `cli.rs`, dataset F4
- Baixar o teto de gates

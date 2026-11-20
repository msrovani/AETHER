# F3 — Ativação do flywheel (124 → 1500 rótulos)

Data: 2026-11-20 · Missão: `F3_FLYWHEEL_ACTIVATION` (claim MSG 1060, gestor
OPENCODE) · Plano: `docs/VEGA_F3_PLANO_CONSOLIDADO.md` (F3)

## O que mudou

O corpus do flywheel do CURSOR (`data/vega_flywheel_v1.json`, 1500 rótulos com
proveniência auditável: 124 starter auth120 + 400 forum auth100 + 976 augment
auth60) foi **convertido** para o formato do vegaml (`data/
vega_fit_corpus_1500.json` — superconjunto do starter) e fitado nos dois
readers. A calibração canônica do gate passa a ser o arquivo de 1500.

## Números medidos (fit + probe, `tools/vega_fit_probe.py --data`)

| Métrica | vega-U 0.8B | gemma-prose |
|---|---|---|
| head `winner` CV / engine | **1.0** (usado no auto) / 0.684 | **1.0** (usado) / 0.502 |
| head `route` CV / engine | **1.0** (usado no auto) / 0.552 | **1.0** (usado) / 0.456 |
| head `passou` | descartado honesto | descartado honesto |
| C1 arbitragem | **A 0.972** (ttt) — cadeira que o 0.8B falhava | A 0.918 (ttt) |
| C3 escalada | 0.97+ correta (injeção→escalar) | 0.98+ correta |
| C2 triagem | conservadora (abstain) | conservadora (abstain) |
| T1/T2 | inalterados (engine) | inalterados (sentinela abstém) |
| latência p50 | 169.6ms | 59.8ms (GPU livre nesta rodada) |

Bootstrap end-to-end validado no chat (`VEGA_CALIBRATION=vega_fit_corpus_1500`):
código de saída limpo, citações de memória no vivo; boot total 5:53 **sob
contenção** (acima do orçamento de 1–2min — o custo do fit cresce com o corpus;
optimização de boot fica para o Eixo D).

## Leitura honesta

- **CV 1.0 é plausível mas merece vigilância**: os rótulos augment (auth 60)
  foram gerados por padrões claros do fórum — classes separáveis. O `auto` do
  pacote só usa heads que vencem o engine em CV, e venceram por margem enorme
  (0.684/0.552 vs 1.0). O desfecho real das cadeiras continua sendo o
  árbitro final (flywheel P4).
- **C2 continua conservadora por honestidade**: o head `passou` NÃO venceu o
  engine em CV mesmo com 500 rótulos — e o pacote o descarta. Abstain na
  triagem = escala para o HITL. Correto.
- **Nada no teto mudou**: calibração é configuração reversível; os gates de
  aceite do plano continuam.

## Arquivos

- `data/vega_fit_corpus_1500.json` — corpus no formato vegaml (canônico)
- `tools/vega_fit_probe.py` — agora aceita `--data`
- `data/vega_flywheel_v1.json` — fonte com proveniência (intocado)
- `data/vega_calibration_starter.json` — preservado como base HITL-verified

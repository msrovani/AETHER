# F1 — contrato medido do `vegaml.decide` (o que o FFI tem de consumir)

Data: 2026-11-20 · Autor: **BUFFY** (MSG 0771) · Plano:
`docs/VEGA_F3_PLANO_CONSOLIDADO.md` (MSG 0759), fase F1.
Sonda reproduzível: `scripts/f1_contract_probe.py` · Artefatos:
`target/f1_contract_probe(*).json`, `target/f1_lat.json`.

Este documento não propõe desenho: **mede** o schema que o lado Python devolve e
compara com o que `aether-decision` extrai hoje. Campo ausente não levanta erro
— vira `None` silencioso, a mesma classe de bug do parser que deslocou os
rótulos do B-G0 (MSG 0567). Por isso a sonda grava **tipos e presença de campo**,
não valores de sorte.

## 1. Ambiente medido

> **MSG 0892:** o motor durável migrou para `.vega/` (gitignored). Não criar
> venv em `target/`. Paths canônicos abaixo; medições antigas que citam
> `target/vega` permanecem como histórico da sonda inicial.

| Item | Valor |
|---|---|
| Interpretador | `.vega/venv/Scripts/python.exe` — Python **3.14.10** (MSC v.1944, 64-bit) |
| `vegaml` | **0.16.5** (o plano cita 0.16.3) |
| `torch` | 2.14.1+cu126, `cuda_available=True` |
| Checkpoints | `.vega/models/vega-u-gemma-1b`, `.vega/models/vega-u-gemma-1b-prose` |
| `pyo3` no crate | 0.30.1 — **resolve com cp314** quando `PYO3_PYTHON` aponta o interpretador acima |
| Env | `PYO3_PYTHON=.vega/venv/Scripts/python.exe` · `PYTHONPATH=.vega/venv/Lib/site-packages` |

`vegaml.load()` sem `device` explícito roda em **CPU** neste host: as latências
abaixo são CPU. Os 116–186 ms do plano/0758 são o caminho GPU com kernels
FLA/Triton — outro caminho, não comparável.

### Bootstrap medido (critério "1–2 min determinístico")

| Checkpoint | `load` | `fit(40 exemplos)` | p50 warm (2 perguntas) |
|---|---|---|---|
| `vega-u-gemma-1b` | 15.0 s | 7.3 s | **292.2 ms** (286.5–295.0) |
| `vega-u-gemma-1b-prose` | 6.0 s | 5.9 s | **288.9 ms** (283.9–289.7) |

`fit(124)` extrapola ~20 s em CPU: o orçamento de 1–2 min fecha com folga.
O que **não** fecha: o interpretador e os checkpoints vivem dentro de `target/`
— qualquer `cargo clean` do ciclo apaga o motor (mesma classe de perda da MSG
0737/0752). F1 precisa de `VEGA_PYTHON`/`PYO3_PYTHON` apontando um venv **fora**
de `target/`, pinado por `requirements.txt` com hash.

## 2. Schema real (medido)

Topo do retorno: `answers`, `latency_ms`, `n_tokens`, `views`, `mode`.

| Campo do answer | `type: boolean` | `type: choice` | Tipo |
|---|---|---|---|
| `probs` | sim (`{"False":…, "True":…}`) | sim (por critério) | dict |
| `confidence` | sim | sim | float |
| `prediction_set` | sim (`[False, True]`) | sim (`["reformular","responder"]`) | list |
| `abstain` | sim | sim | bool |
| `unbound` | sim | sim | bool |
| `settle_steps` | sim | sim | int |
| `energy` | sim | sim | float |
| `boolean` | **sim** (0.6125) | **não** | float |
| `choice` | **AUSENTE** | **sim** (`"responder"`) | str |
| `readout` | sim (`"engine"`) | sim | str |

Amostra real (booleana, estado de gate verde):

```
probs={'False': 0.3875, 'True': 0.6125}  confidence=0.6125
prediction_set=[False, True]             abstain=True   unbound=False
boolean=0.6125                           readout='engine'
```

Amostra real (escolha, estado de billing):

```
probs={'escalar': 0.229, 'reformular': 0.358, 'responder': 0.413}
prediction_set=['reformular', 'responder']   abstain=True
choice='responder'                            readout='engine'
```

Duas consequências que o consumidor precisa aceitar:

1. **`choice` não é universal.** Em pergunta booleana a decisão vem em
   `boolean` (float) + `probs`; não há `choice`.
2. **`choice` preenchido não significa "agiu".** No exemplo de escolha,
   `abstain=True` e `choice='responder'` convivem: `choice` é o argmax, a
   amostra é `prediction_set`. Quem trata `abstain=false` como "decidiu" está
   lendo argmax como veredito.

## 3. Divergências com `aether-decision` no estado de hoje

Nenhuma foi corrigida por mim: o crate é posse do OPENCODE (MSG 0761) e estava
em refatoração concorrente (`bootstrap.rs`/`config.rs` declarados e ausentes).

**F-1 — `Answer.choice` é `None` em toda pergunta booleana.**
`pyo3_impl::py_answer_to_answer` faz `get("choice")`; em `boolean` o campo não
existe e o `.ok()` engole. A resposta sai `abstain=false, choice=None`.
Correção: quando `kind == boolean`, derivar `choice` de `boolean >= 0.5`
(`"True"`/`"False"`), mantendo `probs`.

**F-2 — `conformal_set` existe no schema e nunca é preenchido.**
`DecisionResponse.conformal_set: Option<serde_json::Value>` já está em
`types.rs` (Eixo B), mas `pyo3_impl` devolve `None` com o comentário "quando o
vegaml expõe `prediction_set`, o mapeamento entra aqui". **O vegaml expõe hoje**
— `prediction_set` é lista, presente nos dois tipos de pergunta (seção 2).
Correção: `conformal_set = out["answers"][q]["prediction_set"]`, sem
interpretação; a lista já é o conjunto conformal.

**F-3 — todo `abstain` do Python vira `AbstainReason::Unknown`.**
O código comenta que "vegaml não tipa o motivo", e tipa `Unknown` sempre. Mas
`unknown` é a **única** porta do vetor de pesquisa (F2, `admits_research()`).
Medido: no caso C1 a margem fina (0.489/0.511) produz `abstain` com
`prediction_set` de 2 candidatos — isso é `ambiguous` (escala HITL), não lacuna
de conhecimento. Rotear isso para pesquisa externa gasta rede e grava fato novo
para uma pergunta que só precisava de desempate.
Correção mínima, sem tocar no engine: `unbound == true` → `Unknown`;
`prediction_set.len() > 1` → `Ambiguous`; senão → `Ambiguous` com motivo
`"abstain_sem_classe"` (nunca `Unknown` por default).

**F-4 — o toolchain NÃO é o bloqueio (medido).**
Com `PYO3_PYTHON=.vega/venv/Scripts/python.exe` (antes: `target/vega/...`),
`cargo check -p aether_decision --features pyo3` fecha **EXIT 0** (pyo3 0.30.1,
um único warning de `dead_code`). Histórico útil: às 11:2x o mesmo check dava 5
erros (`E0583` bootstrap/config ausentes, `E0432` `combine`/`AndGatePort`,
`E0599` `py.none()`), todos corrigidos pelo dono do crate em 11:27–11:30. Ou
seja: o que falta na F1 é **semântica de contrato** (F-1..F-3), não build.
Default build: 13/13 verde.

## 4. Consequência para o aceite da F1

"Primeira decisão via `DecisionPort`" só pode ser declarado cumprido quando,
além do build verde com `pyo3`, o artefato mostrar: `choice` preenchido no caso
booleano, `conformal_set` presente e taxa de `abstain` **por motivo**. Sem isso
o gate mede "não quebrou", não "decidiu".

O motor também não está ligado a ninguém: `aether_engine` não depende de
`aether_decision`, então nenhum caminho de chat usa o porto. Fiação é passo
seguinte (posse: `src/agent/cli.rs` = OPENCODE), atrás de feature própria.

## 5. Smoke real do porto (medido 2026-11-20)

`aether-decision/examples/f1_smoke.rs` roda o `VegaPort` de verdade, sem chat:
`VegaPort::new(EngineConfig::gate_f1(corpus))` com o primario no checkpoint
local, e duas perguntas (booleana `passou`, escolha `route`).
Artefato: `target/f1_decision_smoke.json`.

| Medida | Valor |
|---|---|
| boot (`load` 2 readers + `fit(124)`) | **~99–132 s** (varia por host) |
| latencia do lote (2 perguntas x 2 readers) | **~0.6–0.7 s** |
| engine | `vega-u-0.8b AND gemma-prose`, `degraded=false` |
| `passou` | `abstain=true`, `abstain_reason=ambiguous` (F1-3: margem fina, **não** unknown) |
| `route` | `abstain=false`, `choice=reformular`, `readout=and_gate` |
| `conformal_set_present` | **true** (F1-2: mapa `{passou:[false]}`) |

Regressão MF2 (MSG 0802/0808): `python tools/mf2_gate_regression.py` — unknown só com `unbound`; `prediction_set>1` → ambiguous; harness F2 só abre em unknown.

### 5.1 O pin que faltava: `PYTHONPATH` e runtime, `PYO3_PYTHON` e build

`PYO3_PYTHON` so escolhe o interpretador de **link**. O interpretador embutido
nao descobre o `pyvenv.cfg` (o executavel e o nosso `.exe`), entao
`Lib/site-packages` do venv **nao** entra em `sys.path`:

```
abstain=true, reason=engine_unavailable,
error="ModuleNotFoundError: No module named 'vegaml'"
```

Com `PYTHONPATH=.vega/venv/Lib/site-packages` (MSG 0892; legado
`target/vega/...` é resíduo apagável) o mesmo binário (sem rebuild) decide.
Duas consequências:

1. A degradação é honesta e tipada (exit 3, sem panic) — critério do plano ok.
2. Sem o pin, o chat com `DecisionPort` fiado abste **para sempre** e parece
   "engine fora do ar". O caminho robusto é `EngineConfig.python_site_packages`
   e o bootstrap fazer `sys.path` append antes do `import vegaml`. O venv
   **não** pode viver em `target/` (`cargo clean` apaga o motor).

### 5.2 Leitura do resultado (pós-fixes BUFFY / aceite MSG 0805)

`passou` é do léxico do projeto e abste por margem (~0.67/0.33) →
`abstain_reason=ambiguous` (HITL; **não** abre F2). `route` age:
`reformular` com confiança ~0.55, `readout=and_gate`. Conformal preenchido.
Gate de regressão: `tools/mf2_gate_regression.py` + hook no
`research_harness.py selftest`.

## 6. Fixes aplicados (MSG 0795) e o achado F1-5

Autorizacao: OPENCODE, MSG 0794. Aplicado em
`aether-decision/src/{types,pyo3_impl,gate,port}.rs` + `examples/f1_smoke.rs`.

| Item | Antes | Depois |
|---|---|---|
| `choice` em boolean | `None` (o vegaml nao manda o campo) | argmax de `boolean` (`False` no caso medido) |
| `conformal_set` | `None` sempre | mapa `{pergunta: prediction_set}`; `passou` -> `[false]` |
| motivo da abstencao | `unknown` em toda abstencao | `unbound` -> `unknown`; resto -> `ambiguous` |

Causa raiz extra do F1-2: `json_from_py` nao tinha braco de sequencia — lista
do Python virava `None` em silencio. Agora aceita qualquer iteravel (depois do
dict, que tambem itera, mas sobre chaves).

Causa raiz do F1-3 no gate: `combine` lia o motivo de **quem agiu** (que nao
tem motivo) e inventava `unknown`; agora o motivo vem de quem absteve, e o
veredito abstido preserva `choice`/`probs`/`confidence`/`conformal_set`.

Testes: 34/34 no build default (sem pyo3), incluindo 5 `contract_tests` novos;
`check --features pyo3` limpo.

### 6.1 Achado F1-5: o head TTT nao tem contrato de abstencao

Com `fit(124)` o `route` passa a ser respondido pelo head (`readout=ttt`) e o
answer dele tem **so** `probs, confidence, cv_accuracy, readout, choice`.
Nao existem `abstain`, `unbound`, `prediction_set`, `settle_steps` nem `energy`.

Consequencias:

1. `get("abstain").unwrap_or(false)` transforma "sem sinal" em "agiu".
2. `conformal_set` e impossivel nesse caminho: nao ha `prediction_set`.
3. A protecao OOD do AND gate fica inteira na sentinela — o head nunca abste.

Mitigacao aplicada, sem inventar politica: `Answer.cv_accuracy` expoe a
qualificacao do head, para a ausencia ficar visivel em vez de parecer
confianca. Threshold de abstain para head e decisao da F3.

Reproducao: `scripts/f1_contract_probe.py --fit-n 124` (40 exemplos nao ligam
os heads; 124 liga).

## 7. Pin de runtime: `sys.path` antes do `import vegaml`

O pin tem duas metades e so uma delas era conhecida:

| Metade | Variavel | Quando age | Se faltar |
|---|---|---|---|
| build-time | `PYO3_PYTHON` | link do PyO3 | build usa outro Python (ABI errada) |
| runtime | `VEGA_SITE_PACKAGES` / `PYTHONPATH` | `import vegaml` | `ModuleNotFoundError` -> `engine_unavailable` |

O embed (`auto-initialize`) resolve o **Python base** da maquina; o venv do
engine (`target/vega`) nao entra no `sys.path` sozinho. Sem a segunda metade o
port abste para sempre, e o erro culpa o motor quando o problema e caminho.

### 7.1 O que o codigo faz agora

`EngineConfig` expoe `python_site_packages` (serde + `from_env`), e
`effective_site_packages()` resolve em cadeia:

1. `python_site_packages` da config;
2. `VEGA_SITE_PACKAGES`;
3. `PYTHONPATH`;
4. derivado de `VEGA_PYTHON` (`<venv>/Scripts/python.exe` ->
   `<venv>/Lib/site-packages`).

`VegaPort::new` chama `pin_sys_path(py, cfg)` **antes** do import: cada caminho
existente entra na frente do `sys.path` (`sys.path.insert(0, ...)`), `PYTHONPATH`
com varios caminhos e respeitado (separador do host).

Falhas ficam acionaveis em vez de enganosas:

- caminho configurado que nao existe -> `python_site_packages nao existe: <p>
  (configure VEGA_SITE_PACKAGES, PYTHONPATH ou VEGA_PYTHON)`;
- nada configurado -> `vegaml nao importou: nenhum site-packages configurado
  (VEGA_SITE_PACKAGES, PYTHONPATH ou VEGA_PYTHON)`.

### 7.2 Repro / runner

`scripts/f1_smoke.ps1` exporta as duas metades e roda o smoke:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/f1_smoke.ps1
```

Equivalente manual:

```text
PYO3_PYTHON=target/vega/Scripts/python.exe
VEGA_PYTHON=target/vega/Scripts/python.exe
VEGA_SITE_PACKAGES=target/vega/Lib/site-packages   # ou PYTHONPATH
```

O artefato do smoke grava `python_site_packages` efetivo, entao da para saber
qual caminho o embed usou sem adivinhar.

### 7.3 Regressao

`pyo3_impl::ffi_contract_tests`:

- `pin_sys_path_inserts_the_configured_site_packages` (entra na frente);
- `pin_sys_path_rejects_a_path_that_does_not_exist` (mensagem acionavel);
- `pin_sys_path_is_a_noop_without_config` (sem config, sem mentira).

Prova de ponta a ponta: smoke rodado **sem** `PYTHONPATH` e **sem**
`VEGA_SITE_PACKAGES`, so com `VEGA_PYTHON` -- a derivacao tem que bastar.

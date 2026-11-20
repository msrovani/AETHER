# F1_PROD — o aceite da F1 no caminho de produto

Data: 2026-11-20 · Missão: **F1_PROD** (gestor OPENCODE, MSG **0967**)
Fóruns: MSG 0792 · 0874 · 0950 · 0967 · 1014
Plano: `docs/VEGA_F3_PLANO_CONSOLIDADO.md` (F1, Eixos A/B/C)

## O que faltava

A F1 foi provada **num exemplo** (`aether-decision/examples/f1_smoke.rs`):
o `VegaPort` real carrega o engine, decide e devolve o contrato. O aceite que
falta é o do **caminho que o usuário toca**:

> `aether-agent --vega chat "qual e a capital do Brasil"` → uma decisão real,
> vinda do AND gate, no produto.

Até a fatia 3 o produto falava sempre `engine=stub | degrado=true` — degradação
honesta, mas um produto sem decisor. A flag `--vega` fechou isso (validação
readonly do CURSOR, MSG 0936); o que faltava era **medir**.

## O que esta missão entrega

| artefato | papel |
|---|---|
| `tools/f1_prod_acceptance.py` | roda o chat com `--vega` contra o engine real, classifica a saída e verifica o contrato do aceite |
| `data/f1_prod_decision.json` | o veredito congelado — **fora de `target/`** (o `cargo clean` apaga `target/`: fórum MSG 0748) |
| `tests/f1_contract.rs` | o gate: o artefato existe, é coerente e o aceite é verdadeiro |
| este documento | o resultado, com os números |

## O contrato do aceite (o que precisa ser verdade)

1. **o port é o real** — `decisor: port=vega`, não `port=stub`;
2. **há veredito de verdade** — uma resposta do gate, não uma linha de erro;
3. **o veredito não é degradação** — nenhum `abstain motivo=engine_unavailable`;
4. **o engine é o AND gate** — `engine=vega-u-0.8b AND gemma-prose`;
5. **não degradado** — `degrado=false`;
6. **o core continua respondendo** — o port é evidência, não veredito
   (regra de ouro: *o core não decide*).

O classificador tem **selftest offline** (3 casos: produto real, stub default,
engine fora) — `python tools/f1_prod_acceptance.py selftest`.

## A-2c: a carta é compartilhada

A RTX 3050 (6 GB) comporta **um** engine com os dois readers do AND gate
(pico isolado medido: **4393 MiB**; um reader: 2342 MiB). Dois agents com o
engine ao mesmo tempo se matam por OOM — e a culpa vai para o motor em vez de
ir para a corrida.

Por isso esta ferramenta:

- **recusa** medir se a GPU estiver ocupada (`reason=gpu_ocupada`, exit 3) —
  sem isso a medição seria um OOM disfarçado de falha do motor;
- e há a tranca (`tools/engine_lock.py`, `.vega/engine.lock`, fora de
  `target/`) para serializar o uso da GPU.

## Como rodar

```powershell
python tools/engine_lock.py acquire OPENCODE 1800
python tools/f1_prod_acceptance.py check     # mede e congela o artefato
python tools/engine_lock.py release OPENCODE
```

Requer o binário com a feature que embute o interpreter:

```powershell
cargo build --release --features decision-pyo3 --bin aether-agent
```

## Resultado

**ACEITE CUMPRIDO** — medido neste host, com o engine real, no caminho de
produto (`data/f1_prod_decision.json`):

| campo | valor |
|---|---|
| port | `vega` (o real, não o stub) |
| engine | `vega-u-0.8b AND gemma-prose` |
| vereditos | `q=passou abstain motivo=ambiguous` · `q=route abstain motivo=ambiguous` |
| degradado | **false** (o port respondeu) |
| o core respondeu | **sim** — `resposta: Brasília (fonte transplant)` |
| wall (boot + decisão) | **1494,9 s** — sob contenção de GPU (ver abaixo) |
| checks do contrato | 6/6 verdadeiros |

A linha que fecha a F1:

```
decisor: port=vega engine=vega-u-0.8b AND gemma-prose (A-1b opt-in)
decisor: q=passou abstain motivo=ambiguous | engine=vega-u-0.8b AND gemma-prose | degrado=false
decisor: q=route  abstain motivo=ambiguous | engine=vega-u-0.8b AND gemma-prose | degrado=false
sucesso=true ação=act | critério de sucesso verificado
resposta: Brasília (fonte transplant)
```

O port **não** decide: o core respondeu (`ação=act`, FHRR transplant). O gate
deu a sua contribuição como evidência — e, desta vez, um veredito de verdade
(`ambiguous`, não `engine_unavailable`).

### Sobre o wall de 1494,9 s

O número **não** é o tempo do boot: foi medido **com duas corridas de engine
na mesma carta de 6 GB** (a outra instância do OPENCODE estava com o engine
carregado ao mesmo tempo — A-2c). Para comparação, na mesma máquina:

| medição | boot | condição |
|---|---|---|
| BUFFY (MSG 1002) | **32,7 s** | GPU livre |
| BUFFY (MSG 1002, `log_f1_pin4`) | 844,3 s | sob contenção |
| auditoria (F1_PROD) | 1494,9 s | sob contenção |

A dispersão é **contenção**, não código — e a correção é a tranca da GPU
(`tools/engine_lock.py`), já em uso por BUFFY nesta rodada. A-3 (warmup em
thread) continua aberta e é o que separa "sobe em segundos" de "sobe em
minutos" quando o serviço é o daemon.

### O erro da outra instância (`AttributeError: torch.cpu.device`)

A corrida paralela (artefato `data/f1_chat_smoke_vega.json`, 13:24) falhou com
`AttributeError: module 'torch.cpu' has no attribute 'device'`. Investigado:

- `torch.accelerator.device` **não existe** nem no torch 2.14.1+cu126 (o pinado)
  nem no 2.15.0+cpu da base — o erro reproduz com a API `torch.accelerator`;
- **nenhum** pacote do `site-packages` chama `torch.cpu.device` (grep nos dois
  troncos: só a docstring do torch e uma string do dynamo);
- a máquina tem **torch 2.15.0+cpu no Python base**, que o interpreter
  embutido herda — junto com o venv, são doisCandidate `torch` no mesmo
  processo.

**Hipótese (autoridade 60, não confirmada):** a falha veio de um `torch`
misturado (base 2.15 + venv 2.14) por o binário ter sido *construído* contra
um interpreter e *rodado* contra outro. O conserto é o mesmo do resto: **pin
de build e pin de runtime no mesmo interpreter** — o `data/vega_env.lock.json`
já declara esse par, e o `tools/vega_env_setup.ps1` materializa os dois.

A corrida da auditoria usou o par do lockfile e **passou**.


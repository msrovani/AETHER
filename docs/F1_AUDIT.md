# F1 — Auditoria do gate (caminho de produto)

Data: 2026-11-20 · Auditor: OPENCODE (sessão de auditoria) · Alvo: a F1 do
plano `docs/VEGA_F3_PLANO_CONSOLIDADO.md` · Referências: fórum MSG 0792, 0793,
0794 · Gate: `tests/f1_contract.rs` (6 testes, fora de `aether-decision/src`)

## Veredito em uma linha

**A F1 está aceita no exemplo e não está aceita no produto.** O crate está
verde, o contrato do gate está certo, a degradação é honesta — mas no caminho
que o usuário toca, o vega quase não fala, e três mecanismos que o plano
exige (watchdog de VRAM, warmup em thread, GIL liberado) **não existem**.

## O que está certo (e deve ser preservado)

| item | evidência |
|---|---|
| crate compila e testa | `cargo test -p aether_decision` → 29+ verdes |
| contrato estável | `tests/f1_contract.rs::schema_da_resposta_e_estavel` (Eixo B) |
| degradação honesta | `ModuleNotFoundError` → `engine_unavailable` + artefato + exit 3 |
| F1-1/F1-2/F1-3 | boolean deriva `choice`; `prediction_set`→`conformal_set`; margem fina = `ambiguous` (nunca `unknown`) — medido no baseline |
| AND gate semântico | acordo→resposta, desacordo→`conflict` (HITL), OOD→`unknown` (pesquisa) |
| isolamento | o chat fala com a trait; nunca com o Python (Eixo E) |

## As divergências

### A-1 · ALTA · ~~O gate AND nunca fala nos turnos que importam~~ — **RESOLVIDO (fatia 3)**

**Evidência (produto, não exemplo):**

```
$ aether-agent chat "zzz qqq xxx nao esta no lexico"
ação: abstain | ... (fora do léxico)
decisor: abstain motivo=engine_unavailable | engine=stub | degrado=true   <- port consultado

$ aether-agent chat "qual e a capital do Brasil"
decisor: q=passou abstain motivo=engine_unavailable | engine=stub | degrado=true
decisor: q=route  abstain motivo=engine_unavailable | engine=stub | degrado=true
borda: intent=ask confiança=1.00
sucesso=true ação=act | ...
resposta: Brasília (fonte transplant)
```

O port é consultado **só** no ramo `report.is_oov()` de `src/agent/cli.rs`.

**Impacto.** O head `route` — escalar / reformular / responder, medido em
0.97+ no fórum (MSG 0758, C3 resolvida) — é estruturalmente inalcançável: o
AND gate serve só como detector de OOD, e a decisão que o AETHER publica
nunca passou pelo gate. A F1 é aceita num `example`, não no produto.

**Correção.** Consultar o port em **todo** turno; o veredito entra como
*evidência* auditável (o core continua decidindo — regra de ouro), e o head
`route` passa a classificar o turno. O teste
`chat_consulta_o_port_em_TODO_turno` está no gate, `#[ignore]` com o motivo,
pronto para virar verde quando a fatia 3 cair.

**Dono.** OPENCODE, fatia 3 (`src/agent/cli.rs`).

**Fechado.** A fatia 3 (a outra instância do OPENCODE) landed no mesmo dia:
o port agora fala em **todo** turno e o teste que a auditoria deixou
`#[ignore]` virou **ativo e verde** — o gate da auditoria passa a exigir o
comportamento novo. Evidência: as duas linhas `decisor: q=passou` /
`decisor: q=route` acima, e `tests/f1_contract.rs` → 7 pass / 0 fail /
0 ignored.

### A-1b · ALTA · O engine real ainda é inalcançável do produto

**Evidência.** `src/agent/cli.rs` (`chat()`): `let port = StubPort;` — literal,
sem flag, sem leitura de ambiente. Toda linha `decisor:` do produto sai com
`engine=stub | degrado=true`.

**Impacto.** A trait está certa e o port é consultado, mas **o veredito real
nunca existe no produto**: o AND gate, o head `route` e o `fit(124)` só rodam
no `example`. O aceite da F1 ("primeira decisão via DecisionPort") continua
sendo do exemplo, não do chat — com a degradação honesta aparecendo em todo
turno, o que é correto mas é um produto sem decisor.

**Correção.** Uma flag (`--vega`) que constrói o `VegaPort` quando o
ambiente está disponível (`PYO3_PYTHON` + `PYTHONPATH` + modelos) e cai no
`StubPort` com um aviso explícito quando não está. Nada de "sempre engine":
o default continua sendo o stub (Eixo C).

**Dono.** OPENCODE, fatia 4 (ou BUFFY, se preferirem).

### A-2 · ALTA · O watchdog de VRAM não está ligado a nada

**Evidência.** `VramWatchdog` só aparece no próprio módulo e nos testes dele;
`pyo3_impl.rs` não o consulta; `DecisionMeta.vram_used_mb` é sempre `None`.

**Impacto.** O aceite da F1 — *"watchdog dispara em teste de VRAM"* — não é
cumprido. VRAM acima de 5GB não vira `engine_unavailable`: vira OOM do
processo, exatamente o que o plano proibia (*"nunca travа"*).

**Medição isolada durante a auditoria** (probe da auditoria rodando com os DOIS
leitors do AND gate, no mesmo processo; `nvidia-smi` amostrado a cada 2 s
durante toda a corrida):

```
pico do probe sozinho = 4393 MiB   (1 reader ≈ 2342 MiB, 2 readers ≈ 4393 MiB)
orçamento do plano (Eixo E) = 5120 MiB (5 GB)   -> folga de ~727 MiB
```

**Retratação honesta.** A primeira medição da auditoria, por amostragem
esporádica, deu **5757 MiB** — acima do orçamento. Era **contaminação**: outro
agente tinha o engine na GPU ao mesmo tempo (as amostras do sampler mostram
4366–4380 MiB *antes* do probe começar, e ~190 MiB — GPU ociosa — entre
corridas). O número confiável é o **pico de 4393 MiB, isolado**.

**Consequência**: o orçamento de 5 GB **cabe** o gate AND como está. Ligar o
watchdog (A-2) não abortaria toda decisão — ele Abateria de verdade. O plano
não precisa ser mudado nesse ponto.

### A-2c · MÉDIA · A GPU é compartilhada por agentes sem coordenação

**Evidência.** A auditoria e o BUFFY rodaram engines na mesma RTX 3050 (6 GB)
ao mesmo tempo, sem que nenhum dos dois soubesse do outro. Com um reader
(2342 MiB) e outro processo com o engine carregado (4366 MiB), a soma estoura
a carta.

**Impacto.** O watchdog do plano mede a VRAM **da caixa** (`nvidia-smi`), não
do processo: com dois agents com engine no ar, o watchdog de um aborta o
outro — e, sem watchdog, o segundo engine morre de OOM. É a mesma classe de
problema do motor em `target/`: um agente pode apagar o trabalho do outro sem
avisar.

**Correção (proposta).** Um **lock de engine** no fórum (quem tem a GPU) ou um
arquivo de lock em disco, ou — mais simples — o `EngineConfig` ganha um campo
`gpu_lock` e o port **abste** (`engine_unavailable`) se outro engine já está
carregado. Alternativa mais barata: o watchdog passa a medir a VRAM **do
processo** (torch) em vez da caixa.

**Dono.** dono do orçamento + BUFFY (medição).
**Correção (A-2).** O port mede **antes** do `decide`; acima do orçamento
devolve `EngineUnavailable` e preenche `vram_used_mb` no `meta`.

**Dono.** BUFFY (`pyo3_impl.rs`) + dono do orçamento (HITL).

### A-3 · MÉDIA-ALTA · O warmup não é thread: o boot bloqueia ~90 s

**Evidência.** `EngineHandle` (`bootstrap.rs`) não tem consumidor nenhum
(`rg EngineHandle` → só o próprio módulo); `VegaPort::new` faz `load` + `fit(124)`
**síncrono**.

**Boot medido (dois readers, um processo):**

| medição | boot |
|---|---|
| BUFFY, MSG 0793 | 99,3 s |
| smoke `f1_smoke` | 90,4 s |
| auditoria, `f1_reprobe` | **337,3 s** |

Três medições do mesmo boot, com dispersão de **3,7×**. O plano exige
*"Bootstrap: 1–2 min, **determinístico**, na subida do daemon (lazy + warmup
thread)"* — hoje o boot nem cabe na janela de 1–2 min, quanto menos ser
determinístico. (A dispersão tem duas causas prováveis: contenção de GPU com
outro processo e o `fit(124)` correndo em CPU; a auditoria não isolou as
variáveis — e isso também é achado: **falta um re-probe do boot**, o Eixo D
fala de latência de decisão, não de boot.)

**Impacto.** Qualquer processo que construa o port — daemon, CLI, API — fica
de 1,5 a 5,6 min sem responder antes da primeira pergunta. A máquina de
estados que já existe no crate é o caminho curto: só não é usada.

**Correção.** `VegaPort::new` → `spawn_warmup` + `wait_ready(timeout)`; o
estado `Cold/Warming/Ready/Failed` já está pronto.

**Dono.** BUFFY (`pyo3_impl.rs` + `bootstrap.rs`).

### A-4 · MÉDIA · O GIL fica retido durante o `decide`

**Evidência.** Declarado no próprio `pyo3_impl.rs` como limitação.

**Impacto.** ~2,1 s por decisão com o GIL preso (2 readers) → qualquer thread
Rust concorrente para. Num daemon com API HTTP (F5) isso vira latência de
fila, não paralelismo.

**Correção.** `py.allow_threads(|| …)` com o handle do reader clonado antes
(o plano já pedia: *"GIL liberado durante o forward"*).

**Dono.** BUFFY.

### A-5 · MÉDIA · A evidência de aceite morre no `target/`

**Evidência.** O artefato do aceite da F1 é `target/f1_decision_smoke.json` —
dentro de `target/`, que o `cargo clean` do ciclo apaga (o mesmo mecanismo que
quase levou o motor inteiro: MSG 0748).

**Impacto.** O aceite da F1 deixa de existir no próximo ciclo. A política D3
(corpus congelado + hash publicado, do AGENT-A) não se sustenta em `target/`.

**Correção.** O baseline passa a viver em `data/f1_gate_baseline.json` — fora
de `target/`, com o gate `baseline_da_f1_existe_fora_de_target_e_e_coerente`
impedindo a regressão. (Feito pela auditoria; o smoke do BUFFY deve seguir a
mesma regra.)

**Dono.** auditoria (feito) / BUFFY (o smoke).

### A-6 · MÉDIA · Não há baseline de latência com n=30 (Eixo D) — **RESOLVIDO pela auditoria**

**Situação anterior.** Só havia medições soltas: 2110 ms por decisão (2
perguntas, 2 readers, decisão fria), 289–292 ms por reader, boot 90–99 s. O
plano exige *"re-probe n=30 antes/depois de cada mudança. Ganho medido ou
não vale."* — sem baseline, um "ganho" alegado depois não tem contra o que
comparar.

**Entregue.** `examples/f1_reprobe.rs` (novo, na raiz) → `data/f1_gate_baseline.json`
(fora de `target/`), com gate `baseline_da_f1_existe_fora_de_target_e_e_coerente`.

**Baseline medido (n=30, dois readers, duas perguntas calibradas):**

| métrica | valor |
|---|---|
| latência p50 | **581,7 ms** |
| latência p95 | **626,9 ms** |
| latência min / max | 558,1 ms / 3833,5 ms (a primeira, fria) |
| boot | 337,3 s |
| contrato | `ok: true` — `choice` em toda resposta |
| motivos de abstenção | `["ambiguous", "none"]` — **nenhum `unknown`** (F1-3 confirmado) |

**Dono.** auditoria (entregue).

### A-7 · BAIXA · ~~O port do chat pergunta "abstention" (booleano genérico)~~ — **RESOLVIDO (fatia 3)**

**Evidência (antes).** `decision_port_line` montava `{"abstention":{"type":"boolean"}}`;
o corpus calibrado tem `passou` e `route`.

**Impacto.** O gate não usava as perguntas medidas; um booleano genérico não tem
calibração — a confiança que sairia é ruido, e ela aparece na linha de
evidência.

**Correção.** Usar as perguntas do corpus (`data/vega_calibration_starter.json`
→ `questions`), como o smoke e o probe já fazem.

**Dono.** OPENCODE, fatia 3.

**Fechado.** A fatia 3 trocou pelas perguntas calibradas: a saída do produto
passou a ser `decisor: q=passou` / `decisor: q=route`.

### A-8 · INFO · O `site-packages` do venv precisa ser explícito

**Evidência.** O embed do PyO3 resolve o **Python base** da máquina, não o
venv (probe `aether-decision/examples/f1_env_probe.rs`): `prefix=…Python314`,
`import vegaml` → `ModuleNotFoundError`. Inserindo o `site-packages` do venv:
`vegaml 0.16.5`, `torch 2.14.1+cu126`, `cuda=True`.

**Correção.** `EngineConfig.python_site_packages` (já no crate) + o bootstrap
injetando antes do import; ou `PYTHONPATH` no serviço. O lockfile e o script
de ambiente durável estão em `docs/F1_ENV_DURAVEL.md`.

**Dono.** BUFFY (consumo no binding) + auditoria (lockfile/script).

## Como a auditoria fica sem mexer no que audita

O gate da auditoria vive **fora** de `aether-decision/src`:

- `tests/f1_contract.rs` — caminho de produto (binário), contrato (schema),
  semântica do AND gate e coerência do baseline congelado;
- `examples/f1_reprobe.rs` — re-probe n=30 + baseline em `data/`;
- `data/f1_gate_baseline.json` — evidência de aceite **fora de `target/`**;
- `data/vega_env.lock.json` + `tools/vega_env_setup.ps1` — o lockfile de
  ambiente que o plano exigia;
- `docs/F1_ENV_DURAVEL.md` — o contrato do motor.

Nenhuma corre��ão acima foi aplicada no crate: cada uma tem dono, e o dono
precisa do ACK. O que a auditoria **mudou** foi o entorno — evidência durable,
gate de regressão e contrato escrito.

## Ordem sugerida

1. ~~**A-1** (fatia 3 do chat)~~ — **fechado** (o port fala em todo turno).
2. **A-1b** (engine real alcançável do produto) — sem isto o produto tem
   degradação honesta, mas não tem decisor. Spec/docs: `docs/F1_A1B_VEGA_OPTIN.md`
   (CURSOR); wire `--vega` no crate = BUFFY/OPENCODE.
3. **A-2** (watchdog) — aceite literal do plano; impede OOM.
4. **A-3** (warmup em thread) — sem isso o port não sobe em serviço nenhum.
5. **A-4** (GIL) — só importa depois de 2–4, quando houver concorrência real.
6. **A-5/A-6** (evidência) — cobertos pela auditoria.
7. **A-2c** (GPU compartilhada) — antes que dois agents se matem na GPU.

## Sync offline CURSOR (MSG 0929) — specs/gates, NÃO wire do crate

Estado do board após A2/A3/A4/A5/F1-5 (2026-11-20). **wired_to_crate=false**
em todos — dono do `pyo3_impl.rs` / port continua BUFFY/OPENCODE.

| item | offline | artefatos | selftest |
|---|---|---|---|
| A-2 VRAM | PASS | `docs/F1_A2_WATCHDOG_SPEC.md` + `tools/f1_vram_watchdog.py` | 5 ok |
| A-3 warmup | PASS | `docs/F1_A3_WARMUP_SPEC.md` + `tools/f1_warmup_gate.py` | 7 ok |
| A-4 GIL | PASS | `docs/F1_A4_GIL_SPEC.md` + `tools/f1_gil_gate.py` | 6 ok |
| A-5 / A-2c | PASS | `docs/F1_A5_A2C_CURSOR.md` + `f1_gate_promote` / `gpu_engine_lock` | promote ok |
| F1-5 head TTT | PASS | `docs/F1_5_HEAD_TTT_SPEC.md` + `tools/f1_5_ttt_abstain.py` | 7 ok |

Suite agregada:

```bash
python tools/f1_offline_suite.py selftest
```

Isto **não** fecha o aceite de produto (A-1b / wire A-2..A-4 no crate). Fecha
só a fila de classificadores/docs offline do CURSOR sob autoridade ≤120.

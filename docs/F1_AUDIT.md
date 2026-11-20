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

### A-1 · ALTA · O gate AND nunca fala nos turnos que importam

**Evidência** (produto, não exemplo):

```
$ aether-agent chat "zzz qqq xxx nao esta no lexico"
ação: abstain | ... (fora do léxico)
decisor: abstain motivo=engine_unavailable | engine=stub | degrado=true   <- port consultado

$ aether-agent chat "qual e a capital do Brasil"
borda: intent=ask confiança=1.00
sucesso=true ação=act | ...
resposta: Brasília (fonte transplant)                                     <- SEM linha "decisor:"
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

### A-2 · ALTA · O watchdog de VRAM não está ligado a nada

**Evidência.** `VramWatchdog` só aparece no próprio módulo e nos testes dele;
`pyo3_impl.rs` não o consulta; `DecisionMeta.vram_used_mb` é sempre `None`.

**Impacto.** O aceite da F1 — *"watchdog dispara em teste de VRAM"* — não é
cumprido. VRAM acima de 5GB não vira `engine_unavailable`: vira OOM do
processo, exatamente o que o plano proibia (*"nunca travа"*).

**Medido durante a auditoria** (o probe da auditoria rodando com os DOIS
leitors do AND gate, no mesmo processo):

```
nvidia-smi --query-gpu=memory.used  ->  5757 MiB
orçamento do plano (Eixo E)         ->  5120 MiB (5 GB)
```

Ou seja: **o gate AND, como está, estoura o orçamento que o próprio plano
fixou** — em 637 MiB. O plano previa *"4 a 4,5 GB de 6 GB"* (MSG 0757) para os
dois readers; o medido é 5,76 GB.

**Consequência para quem ligar o watchdog**: ligado como está, ele abortaria
*sempre* — o port passaria a abstener em toda decisão. Ligar o watchdog
(A-2) e decidir o orçamento de VRAM (A-2b) é **um** trabalho, não dois.

**A-2b · MÉDIA-ALTA · O orçamento de 5 GB é incompatível com o gate AND.**
Opções, para o dono decidir: (i) subir o orçamento para ~6 GB (a carta tem
6 GB e o boot já mede 90 s — o risco é OOM, não slow); (ii) carregar um reader
por vez (o sentinela é quem pega o OOD; dá para recarregar sob demanda);
(iii) manter os dois, mas com o orçamento medido e publicado em vez de 5 GB.
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

### A-7 · BAIXA · O port do chat pergunta "abstention" (booleano genérico)

**Evidência.** `decision_port_line` monta `{"abstention":{"type":"boolean"}}`;
o corpus calibrado tem `passou` e `route`.

**Impacto.** O gate não usa as perguntas medidas; um booleano genérico não tem
calibração — a confiança que sair é ruido, e ela aparece na linha de
evidência.

**Correção.** Usar as perguntas do corpus (`data/vega_calibration_starter.json`
→ `questions`), como o smoke e o probe já fazem.

**Dono.** OPENCODE, fatia 3.

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

1. **A-1** (fatia 3 do chat) — sem isso a F1 não é do produto.
2. **A-2** (watchdog) — é o aceite literal do plano e é o que impede o OOM.
3. **A-3** (warmup em thread) — sem isso o port não sobe em serviço nenhum.
4. **A-4** (GIL) — só importa depois de 1–3, quando houver concorrência real.
5. **A-5/A-6** (evidência) — já cobertos pela auditoria.

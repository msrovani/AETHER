# AUDIT do Core AETHER — deep-dive profundo (2026-11-24)

> **Status:** quarentena — autoridade **120** (evidência interna medida; nada
> externo vira 255 sem HITL). Documento de auditoria e recomendação;
> coerência e regras de implementação: [`COERENCIA_E_REGRAS_IMPL_2026-11-24.md`].
> **Fonte de verdade do arquitetura:** [`POSICIONAMENTO_AETHER.md`](POSICIONAMENTO_AETHER.md),
> [`HW_TARGET.md`](HW_TARGET.md) §8 (E6/E6b), [`E2_OPTIMIZATION.md`](E2_OPTIMIZATION.md).
> **Aposta base (medida):** `81,3 µs`/quadro, solver `57,5 µs` (84,5%), 0 alloc
> no hot-path (E6b, 2026-11-16). Teto de tempo real seguro: N=512.

---

## 0. Resumo executivo

O core físico ( organismo M4, solver bSB, wave FFT, renormalização, morfogênese)
está **altamente otimizado** — E6b levou o solver de 132,1 → 57,5 µs e nada está
alocação na janela de 10.000 quadros (auditoria cognitiva: `hot_path_bytes = 0`).

O que o posicionamento **exige e o core ainda não faz** (o buraco real):

1. Os "quatro motores cognitivos" — RG, ComplexPhaseMemory, Koopman/SINDy,
   Dreamer, ActiveInference/AIF, MorphogeneticField — **não estão no laço do
   organismo em produto** (M4/daemon). O daemon só usa MorphogeneticField e
   ComplexPhaseMemory, em pequeno número de chamadas por quadro. `HW_TARGET.md`
   §8.4 declara exatamente isso: RG/SINDy/Dreamer/AIF/CBF contribuem **0 µs**
   ao ciclo — integração nunca medida (`E6-mod`).
2. A diferença entre o Dreamer (sampling de K futuros por quadro) e a G-ABD
   (abdução ativa: a falta de resposta virar hipótese tipada e leitura
   epistêmica) é material: o Ensemble já mede incerteza, ela só é **reportada**
   em telemetria, nunca decisória.
3. Multiagente/multiusuário são "machucados" (caminho: threads em um processo,
   1 processo por organismo, sem auth no API por default) — viável, mas faltam
   os semáforos e backpressão que o posicionamento espera (T2Worker drain,
   download de processos, cgroups).

**Veredito geral:** o core está mais vivo do que seus docs sugerem; os
achados abaixo são **otimizações seguras** (bit-idênticas ou com gate explícito)
e **melhorias de integração** (cordas que o posicionamento já traçou). Nada
requer áreas invertebradas do core.

---

## 1. O que já é excelente (não mexer — referência para os achados)

| Fatos | Evidência |
|---|---|
| Solver bSB sem alocação, 0 alloc em 10.000 quadros | `HW_TARGET.md` §8.1; `tests/transducer_invariants.rs` |
| Determinismo bit-a-bit, `dot_f64` 16 acumuladores, sem intrinsics (`deny(unsafe_code)`) | `src/core/thermodynamic_solver.rs:464-519`; `src/lib.rs:19` |
| Truncamento modal: o custo da modulação não depende de `L`; planos FFT cacheados por `L` | `src/core/wave_interference.rs:15-21` |
| Orçamento de 2 budgets diferentes (tempo real vs throughput `eq-t/s`) | `HW_TARGET.md` §8.3 |
| Disjuntores A4/G-A4, escadas K0–K8, T-PHYS/T-GRAPH não são do ID | `CORE_TOMADAS.md`, `TOMADAS_FUTURO.md`, `CODER_AETHER.md`, `CORE_TOMADAS.md` T-SENSOR/D1 |
| E2 G3b: presolve de 94,1 → 23,7 µs; 0 alloc no hot path (2 alloc em `trail` toleráveis, metade do estado coberto pelo PTR swap) | `E2_OPTIMIZATION.md` (nota registrada 2026-11-16) |

---

## 1b. Arquitetura de Produto Adotada — Federação de Micro-Organismos (texto do usuário, 2026-11-24)

Reversa o ajuste N=512 do E6b: o organismo M4 é **produto A canônico** (N ≤ 512,
T0 intacto). O escalonamento em **federação de micro-organismos** (N = 64/128)
cria produtos leves com isolamento de escopo e falhas.

| Item | Contrato | Estado |
|---|---|---|
| A8 — Fed. Micro-Organismos (T-SENSOR-FED, F16) | Processos N=64/128 (`~81,3 µs` cada), portas multicast segregadas (`--multicast-port` — **não existe flag**), ScopeId isolado; especialização por produto (C: telemetria vs Código: T-CODE/T-XP) | F17/Sprint A |
| A9 — Precheck de duplicatas (federação) | T-GRAPH precede a admissão; porta + `ScopeId` únicos por organismo — colapso de R por duplicatas é bug auditado (`aether_daemon.rs:384-395`) | F10/Sprint A |
| A10 — Episodic probe budget no bus (G-ABD) | `max_probes_per_window` + `cost_ceiling` quando abdução pede leitura adicional (auth 60) — anti-loop (análogo ao barge-in S6) | F12 |
| A11 — Autobiografia determinística (F19) | `aether_daemon --seed S`: hash de fases aos 60 s, 5 execuções, 0 divergências | F19/Sprint A |

## 2. O que o posicionamento espera e o estado real

| Peça do posicionamento | Estado atual | Gap |
|---|---|---|
| G-ABD (abdutivo: abstain→dreamer→hipótese→ação epistêmica) | Dreamer amostra a cada quadro (T é temperatura, não passos); Ensemble.uncertainty **reportada mas não decisória** | **F1** fia o laço; **F2** o efeito epistêmico |
| G-LAW (episódios → Koopman/SINDy → artefato `Law` no SGDB) | SINDy roda no Governor em thread; SGDB lê memória | **F3** pipeline episódio→lei; **F4** consolidação de lei |
| G-ISO (transferência por isomorfismo RG) | RG existe em 2 lugares (plsql `M4` via `self_field.rs`? — verificar) e não roda no organismo | **F5** macro-assinatura comparável cross-domínio |
| G-MORPH (readout de conceito = bacia nova) | M7 pleno no daemon (evolve_topology cada quadro), mas sem "readout de formação" | **F6** registro/nomeação de conceitos no SGDB/MoE |
| World models como TO = T-SENSOR (m8 vivo) | produto C piloto; CAN/OBD (A) e CSI (B) contrato pronto | **F7** deepen do produto C |
| T-GRAPH (nova) | BAM + RG + SGDB L6 vivos | **F8** ingesta T-GRAPH + G-GRAPH |

---

## 2b. Os 5 Saltos Estruturais (texto do usuário, 2026-11-24)

Traduzidos para a física do AETHER. Cobertura: 3-4 em forma de Grafos/L23 existentes
(cobertos no propósito deste doc), 1, 2 e 5 novas (F20–F23).

| Salto | Status | Mapa/Novo |
|---|---|---|
| 1. Fast-J Gating (J_eff = J·g_i·g_j) | **New (Sprint A)** | F20 — vetor `g(t)` do espectro + macro; método novo em `MorphogeneticField` |
| 2. Compute elástico em T0 (1×→40×, 16,5 ms ociosos) | **New (Sprint A)** | F21 — escala `steps`/`num_samples` do Dreamer por `epistemic` + erro do SelfField |
| 3. RG spine (ascensão e4→e3, descida torque 4→16→64) | Existe (parcial) | L9/L11: `RenormalizationHierarchy::forward` + `torque_into` (`renormalization.rs:223`) |
| 4. Dream → E2 commit loop | **New (Sprint A)** | F22 — L3-L4(Sprint A): mysleep → candidates → E2 → EMA |
| 5. RG como espaço latente cross-modal (G-ISO) | Existe (cobertura) | G-ISO + F23 (comparação de macro_phase cross-domain) |

## 3. Findings — hot path físico (organismo M4)

Base: medido `HW_TARGET.md` §8.1-8.5 (N=64): solver 57,5 µs = 84,5%; readout
10,9; entropy 4,6; wave 3,9; actuator 2,6; ring 1,8 (estágios instrumentados;
total do quadro 81,3 µs).

### F1 — Energia é computada 2–3× por quadro no solver [MEDIDO/ESTIMADO]
`relax_one` acumula a energia discreta `e` ao longo dos `steps` (O(steps·N²))
— `src/core/thermodynamic_solver.rs:375-393` — depois:
`descend()` recomputa `fill_g` (O(N²), :536) e `row_energy` (O(N²), :557);
`relax_one` termina recomputando `row_energy` novamente (:436).
*Oportunidade:* manter a energia incrementalmente durante a integração (ΔH por
step), zero recomputos extra. *Impacto:* reduz ~40–60% da carga do solver
(estimativa, gate `E6-mod2` MEASURED). Risco: bit-idência — o histórico `H(t)`
deve ser estável; OSCILACENTER pode variar ~1 ULP, aceito com medida.

### F2 — `compute_spectral_entropy` re-FFTs o que `forward` já FFTou [CONFIRMADO CÓDIGO]
`src/simulation/closed_loop_organism.rs:467` (`wave.forward_into`) vs
`:469` (`wave.compute_spectral_entropy(self.u.view())`) — FFT de `u` rodando
duas vezes (`src/core/wave_interference.rs:606-655` re-executa FFT).
*Oportunidade:* expor `spec_in` do forward ou retornar poder espectral
incremental. *Impacto:* entropia atual é ~4,6 µs → cair o estágio. Gate:
medido A/B com `hot_path_bytes` mantido 0.

### F3 — Vetor de quadro aloca por `step()` no organismo [MÉDIO]
`src/simulation/closed_loop_organism.rs:340` — `OrganismFrame::with_capacity`
aloca 3 `Vec` (`energy_trace`, `states`, `continuous_action`) por quadro via
`step()`. O daemon (`cycle()`) já é 0 alloc por design (`aether_daemon.rs:322-328`
pré-alocados). *Oportunidade:* `step_into` como default em usos oscilantes, ou
memoização de quadro (`Option<OrganismFrame>`). Gate: 0 alloc confirmado
(`tests/transducer_invariants` já cobre `forward_into`; estender para `step`).

### F4 — Readout espectral O(N × c_out × modes) por quadro [ESTIMADO]
`closed_loop_organism.rs:473-502` — loop `i ∈ [0,N) { for o ∈ [0,8) { for k ∈ [0,32) } }`
+ normalização em dois passos. Para N=64 ≈ 16K MACs/quadro. *Oportunidade:*
fusão com a etapa do solver ou pré-cálculo quando `wave_spectrum` inalterado
(não ocorre — muda cada quadro, por design). Gate: medido.

### F5 — Polish/fallback do solver [MÉDIO-PIX]
`descend` — O(8N) tentativas × O(N) varredura = O(8N²) (+O(N²) final).
Para N=64 ≈ 32K FMA ≈ 2–4 µs estimado. Pior caso: padrões pathologicali `h`
(saturado). *Oportunidade:* scan com lazy-update de `g` via quine-mccluskey
local (só trocar ΔH violados). Gate: MEASURED com h adversarial.

### F6 — f32 no solver? Ainda recuso ao alterar polêmico [DOÇÃO]
Solver = f64 puro (INV-T5); GFLOP/s f64 = 17,1 (E6b). F32 anularia pouco
(não previso >15%), exige re-validar bit-a-bit + INV-T5. *Oportunidade:*
experimento OFF-hot-path (gate `E6-mod3`).

### F7 — Entropia crítica: duplicidade e normalização [MÉDIO]
`bin_power` em f64 por coluna (O(C_in·nb)); pode ser f32. DC/Nyquist
zero-padding até `L_eff` (obs: `wave_spectrum` fill O(C_out×modes) por quadro).
Hiper-otimizável: usar `u16` inteiros de FFT? Sem grau — mantenha como está.

---

### F16 — Readout espectral vectorizado (16 acumuladores) [ESTIMADO]
`closed_loop_organism.rs:473-502` — O(N × 8 × 32) ≈ 16K MACs/quadro
(~10,9 µs medido). Aplicar o padrão validado em `dot_f64`
(`thermodynamic_solver.rs:464-519`: 16 YMM, sem `unsafe`, bit-a-bit) ao loop:
transpor `readout`/`wave_spectrum` para slices contíguos `[c_out][N]`.
*Oportunidade:* ~50% da etapa. *Gate:* `E6-mod2b` (A/B `eq-t/s`).

### F17 — Zero alloc no presolve E2 (fix) [MÉDIO]
`E2_OPTIMIZATION.md`: presolve caiu 94,1 → 23,7 µs; **restaram 2 allocs em
`trail`/`pq`** (metade coberta por PTR swap; PTR faltou). Substituir por buffer
pré-alocado (`MAX_N = 64`) → 0 alloc. *Gate:* a suite `tests/e2_logic_gate.rs`
extenderá o gate "0 alloc no hot path" (já no G3b).

### F18 — `busy_work` boot + batching do T2Worker [MÉDIO]
`aether_daemon.rs:241` (`busy_work` dentro de `sample()`) → mover para boot
(estima 2–3% no primeiro ciclo). `T2Worker` (`aether_daemon.rs:475`): agrupar
`target/aether_sgdb_outbox.jsonl` em lotes temporizados (a cada 16 quadros)
com descarte/coalescência sob pressão (evita `sync()` de disco no T0).

## 4. Findings — arquitetura concorrência / multiprocesso

### A1 — Inventário de threads (medido no código)
| Thread/Processo | Papel | Referência |
|---|---|---|
| Main (daemon/organism/api/dashboard) | laço 60 Hz | `aether_daemon.rs:645-670` |
| 3× thread de Kuramoto node | P2P UDP, 1ms slices | `kuramoto_field.rs:711-755` |
| `T2Worker` (SGDB) | drain/spool de memória | `aether_daemon.rs:475` |
| Governor: reflex, koopman, swarm | 3 threads | `aether_governor.rs:364-419` |
| API: main + chat thread | HTTP local | `api/mod.rs:424` |
| Dashboard: jobs workers | TUI demo | `dashboard/jobs.rs:184` |
| SGDB transport | stdin/HTTP server | `knowledge/sgdb/transport.rs:1126` |
| (futuro) multiagente | abertura | — |

Nenhum `rayon` no loop (só em Dreamer, `thermodynamic_dreamer.rs:514`).
`panic=abort` — um panic mata o processo; documentação de "futilidade do T0".

### A2 — Multiprocessamento: viável, faltam semáforos
- **Atual:** um processo = um organismo. Dois daemons no mesmo host conseguem
  concluir-se no mesmo grupo multicast 239.255.42.99:7777 — `SO_REUSEADDR`
  (`kuramoto_field.rs:278`) torna isso possível; a descoberta por padrão colapsa
  os R (confirmado auditável: `node_id` é único por pid+tempo,
  `aether_daemon.rs:713-726`).
- **Futuro (Sprint A):** processos isolados per organismo, discovery por
  porta única ou `--multicast-port` desacoplado + `bind_to_device`.
- **Auth/token do API:** opcional (`api/mod.rs:100,293-337`) — produto
  seguro de multiusuário exige auth default ON + allowlist por scope.

### A3 — SPSC é SPSC (1 produtor 1 consumidor)
`spsc_ring.rs` — ok para Governor, mas multiagente futuro precisa
`crossbeam-channel` (já está dependência) ou `SpscRing` com 2 produtores.

### A4 — Backpressure está no caminho certo, duas nuances
- `T2Worker` + outbox (`target/aether_sgdb_outbox.jsonl`) — verificar taxa de
  `sync()` e overflow de `mpsc` (T2Worker usa std mpsc? `T2Worker::start`).
- `HardwareTransducer::busy_work` dentro de `sample()` (`aether_daemon.rs:241`)
  — work de pressão legítimo, mas consome 2-3% do orçamento do primeiro ciclo.

### A5 — Multi-agentico hoje
`swarm_speciation.rs` (3 nodes, `idle_spins` 2048) + `classify_swarm`.
Agentes do T2 são "tools" do Barramento trio (`agent/bus.rs`).
Falta: fila/monitor de N agentes, auth por agent, budget de acoplamento.

### A6 — Coerência transversal via spds
Pegando como exemplo para G-ISO: assinaturas macro (RG 4 níveis, `macro_order`,
`macro_phase`) já existem (`src/core/renormalization.rs:179-198`) — nada a
reimplementar. Gate: `TKNets`-análogo via Koopman (arXiv:2402.07834).

### A7 — Caminho de falha documentado — falha é honesta
`AetherError` sem panic, validação em `relax_into` (shapes C-contiguos,
`tráfego`). F1: `organism.run()` ignora `drop` de `NodeHandle`? não — há `shutdown`.
Boa base. Falta: backpressure do Main Thread sobre o deploy de T2 (rollback).

---

### A8 — Federação + `--multicast-port` + precheck de duplicatas [Sprint A]
`aether_daemon.rs:384-395`: binds ao mesmo grupo/porta — colapso de R por
duplicatas (`SO_REUSEADDR`, `kuramoto_field.rs:278`). Adicionar:
`--multicast-port` (F17), `ScopeId` único por instância (`knowledge_bridge_f1.rs`),
precheck de duplicatas em boot. *Gate:* F10 (portas segmentadas) + teste
"2 instâncias na mesma LAN, R>0.95 c/ peers>0".

### A9 — Authority rebase (C4, reescrito)
`ORGANISM_AUTHORITY = 200` (`ledger.rs:541`). Rebase: **ação autônoma ∈ 200;
200–255 = HITL**. Grupos: agente/quarantine ≤120 (`LEARN_AUTH_CAP`,
`agent/bus.rs:28`); Código 200; shrink do default tido com decisão (A9).

## 5. Findings — aprendizado / autoevolução / coerência (G-ABD/G-LAW/G-ISO/G-MORPH)

### L1 — Dreamer não é decisório em produto [GAP → F2]
`aether_daemon.rs:588-605` — `dreaming = surprise > alert`; `imagine_futures_into`
sempre amostra (T = temperature). `ensemble.epistemic_uncertainty`
(`thermodynamic_dreamer.rs:299`) é reportado na telemetria e **não é lido**
em nenhum branch. G-ABD: triggers no modo `abduce` quando
`dreaming || epistemic > ε`, com hipótese tipada (`auth 60`) escolhendo a leitura
que minimiza a divergência do DreamEnsemble para um alvo aleatório — sem
mudar o contrato do loop.

### L2 — Absence passiva já existe (E4, `src/core/self_field.rs`)
O `SelfField` é o "campo de metacognição": 16 canais, 4 features × 4
preditores, EMA de erro + abstem quando novelty/coherence cai
(`self_field.rs:559-580`). Portanto: **abstem ≠ beco sem saída** já é
resolvido. O que falta (G-ABD): (a)Disparar o Abdução Dofroma
quando `ensemble.epistemic_uncertainty` sobe sem abstem; (b) laço
`abstain → Dreamer → hipótese tipada → micro-ação de leitura`
(não existe no loop do daemon); (c) think-while-idle: quando
`!dreaming && epistemic < eps`, as ~16.5 ms livres/ciclo são o burocrata
para estes simulações — futuro (Sprint A).

### L3 — Koopman/SINDy no Governor [OK, F3]
`aether_governor.rs:395-397,335-360` — threads aceleradas, refit every 32,
JIT Cranelift. Melhoria F3: nova catálogo de leis vão ao SGDB com provenência
(`knowledge_bridge.rs` — verificar que é o sink).

### L4 — MorphogeneticField está no daemon, mais que mínimo [OK — F6]
`aether_daemon.rs:581` `field.evolve_topology(abs_error, spins, 0.05)` — F a
cada quadro. F6: adicionar índice de n enhanced taxa: se `abs_error < ε`,
evolver "topologia" 1/K — medido, gate E6-mod4.

### L5 — SGDB: consolidação de memórias deixa o loop quente
Futuro (Sprint B): replay periódico (offline): SGDB → episódios → SINDy →
`Law` (G-LAW).Destino legítimo de `ComplexPhaseMemory::should_abstain` (phase_memory.rs:312).

### L6 — Coerência/determinismo na multiinstância
`organism.rs` linha 195 `start = (ring_pos + window - ring_filled) % window`
— sem problema. Kuramoto usa pboot unique + MAX_CATCHUP — ok.
Falta: teste de 1 da classe: 2 instâncias na mesma LAN, R(t) > 0.95 com
`peers_heard > 0` como invariante documentado.

### L7 — Aprendizado na fria (cold-start)
Dreamer cold: kuramoto start da iteração `call_index` — recupera a seminação.
Gate: determinism check bit-a-bit entre runs: `aether_daemon --seed` → hash de
fases aos 60s. (Requisito de INV-T5 ao sistema todo.)

### L8 — Gaps de autoevolução reais (arquitetura de lendas)
- **Votação por ensaio, não por serviço:** MorphogeneticField não tem
  seleção de variantes (single J). G-MORPH poderia são adiciona bahs off via
  eps.
- **Agressividade de EXP3/LinUCB do agente:** self_model Beta é persistido
  em JSONL (`learner.rs`), EXP3 não — docStatus. Falta: linha de EXP3 no
  SGDB (auth 120) com replay.
- **Koopman como lei persistente:** atualmente não salva/restora estado no
  SGDB. G-LAW.

---

### L3 — LyapunovCbf + EmaActuator ← `epistemic_uncertainty` [Sprint A]
`aether_daemon.rs:597-605`: Dreamer amostra sem consumidor. Grafo (fiação,
não invenção): `ensemble.epistemic_uncertainty` (`thermodynamic_dreamer.rs:228,583`)
→ `EmaActuator::filter_into` (gamma posterior) e `LyapunovCbf::project_into`
(estreito envelope) → agressividade do organismo cai assim que a dúvida sobe.

### L4 — Governor (Koopman/SINDy) → Dreamer [Sprint A]
`aether_governor.rs:395-397`: quando resíduo STLSQ < gate, injetar
`OnlineKoopmanOperator` (JIT, `sindy_jit.rs`) no `ThermodynamicDreamer` —
simulação contrafactual com lei real.

### L5 — SelfField (E4) → plasticidade do MorphogeneticField [Sprint A]
`aether_daemon.rs:581`: `evolve_topology(abs_error, spins, 0.05)` — escalar
`dt` por `SelfField.error/uncertainty` (`self_field.rs:533-546`): acelerar em
desvios reais, frear ruído (análogo audit §2 L2).

### L6 — RG macro → LinUCB/EXP3 [Novo, cross-agentico]
Persistir Beta do EXP3 no SGDB + LinUCB (`agent/linucb.rs`) alimentado com
`RenormalizationHierarchy::macro_phase` (delta) — roteamento ressonante com
regret formais preservados.

### L7 — Ciclo de sono (SGDB → SINDy → Law) [G-LAW, Sprint B]
Ocioso: ler episódios de alta surpresa no SGDB → SINDy multi-episódio
(`koopman_sindy.rs`) → validar resíduo → `Law` quarentena auth≤120 → dedup
episódico (supersede).

### L8 — G-MORPH: `new_basin_events` [Novo, F19]
Adicionar a `morphogenesis.rs` (não existe): spikes de `sparsity_ratio`
estável (R_local>0.85) → `ComplexPhaseMemory::imprint` (`phase_memory.rs:312`)
→ gravar L4 SGDB auth=60 (promo 120 com redução F).

### L9 — Introspecção topológica no SelfField [Sprint A]
Extrair raio espectral/margem `J` via iterações de potência
(`morphogenesis.rs:63-64`) antes de consolidar atualizações; autorregulação
da evolução estrutural.

### L10 — Autobiografia/determinismo (F19)
Hash bit-a-bit de fases aos 60 s entre execuções (`--seed`), registrar no
SGDB scope `aether/audit-2026-11-24` (INV-T5).

### L11 — Elastic compute no Daemon [Sprint A, F21]
`aether_daemon.rs:597-605`: `imagine_futures_into(j, h, T, samples, steps, ...)` —
fazer `steps` dinâmico: `steps = base_steps × (1 + a·epistemic·(T>0.3))`, com
`a ∈ {0, …, 3}` (30× do teto de 8 ms ≈ 2,3 ms). Gate: `E6-mod5` (medido, com
teto p99 do quadro — registro em `target/aether_e6_mod.json`).

### L12 — RG spine: ascensão e torque descendente [Sprint A, F22]
`aether_governor.rs` (futuro): fluxo ASCENDENTE (`64→16→4`) transmite o resíduo
`e(t) = u(t) − v(t)` sem dissipação (usar `RenormalizationHierarchy::forward`
sobre o fasor do SelfField); fluxo DESCENDENTE (`4→16→64`) injeta o torque
top-down (`RenormalizationHierarchy::torque_into`, `renormalization.rs:223`)
no campo. Invariante F22: micro não reprocessa banda predictável
(`R_raw − R_wave` portabilizada; ordem macro ≥ 0,85 durante silêncio).

### L13 — Fast-J Gating (J_eff) [Sprint A, F20]
`morphogenesis.rs` (futuro): `evolve_topology` ganha vetor `g(t)` (0..1),
derivado do espectro da onda + macro RG: `J_eff(i,j,t) = J(i,j)·g_i(t)·g_j(t)`.
Dissocia/acoplia sub-redes em 1 quadro — analogia do Kuramoto Attention.
Gate: `tests/morpho_gating_gate.rs` (acoplado ΔR > 0,30 em 3 quadros; 0 alloc;
determinístico; INV-T5).

| Loi | Punto exato | Evidência no código |
|---|---|---|
| G-ABD | `aether_daemon.rs:588-605` — adicionar `abduce_next() -> Option<Action>` ligada à incerteza de `ensemble` | `aether_daemon.rs:634`; `thermodynamic_dreamer.rs:299` |
| G-LAW | `knowledge_bridge.rs` — adicionar sink `LawEvent {k, dt, fit, residual}` com autoridade ≤120; replay offline: `aim` → SINDy | `aether_governor.rs:395-397`; `sindy_jit.rs` |
| G-ISO | `self_field.rs:529` — "ler a escala macro da RenormalizationHierarchy" — estendender com `macro_signature()` e cache de mapeamentos cross-domínio | `src/core/self_field.rs`, `renormalization.rs:179-198` |
| G-MORPH | `morphogenesis.rs:270` — adicionar `new_basin_events()` (spikes de sparsity_ratio temporária); consolidar em `ComplexPhaseMemory::imprint` | `src/morphogenesis.rs`, `src/phase_memory.rs:312` |

---

## 7. Recomendações (F1–F10), gate e impacto esperado

| # | Recomendação | Gate (artefato) | Impacto (estimado) | Risco |
|---|---|---|---|---|
| F1 | Energia incremental no solver (F1) | `E6-mod2`: N=64, steps=200, A/B vs E6b, 0 alloc | Solver 132→~80 µs (−38%), N=512 passa de 72,7% → ~50% | Bit-a-bit; gate |
| F2 | Expõe spectral por frente de FFT (F2) | `E6-mod2b`: A/B wave stage 4,6→2,8 µs | Wave +40% speed-up; organização da janela | Semantic read |
| F16 | Readout espectral vectorizado (16 acc) | `E6-mod2b` (ver F16 acima) | ~50% do estágio (~2,5 µs → ~1,3 µs) | Estimado, bit-a-bit |
| F3 | Quadros préalocados ao `step()` (F3) | Gate: 0 alloc em `organism.step()` (extenção de `transducer_invariants`) | Marginal (~1µs) | Baixo |
| F4 | Readout fusionado (F4) | Gate: eq-t/s > 1,574,229 +0,5% | Equilíbrio minoritário | Baixo |
| F5 | Descida com scan lazy (F5) | Gate: adversarial h, polimento < 6 µs | Robustez | Baixo |
| F6 | Experimento f32 solver (F6) | Gate `E6-mod3` bit-comparação sobre 10 runs (expect ~15%) | +15% do solver | INV-T5 parcial; deferido |
| F7 | SGDB × Dreamer futuro (Sprint A) | Spec no *Self-contained Spec* + gates S0–S6 | ~K×redução de F em ciclos idle | Médio |
| F8 | T-GRAPH (A6) — verificar duplicatas antes | Gate: G-GRAPH (AUC ≥ TGN-SVDD) | Emérido a priori | Alto |
| F16 | Readout espectral vectorizado | Gate: E6-mod2b | ~50% do estágio | Baixo |
| F17 | Zero alloc no presolve (fix) | Gate: G3b | +0 alloc | Baixo |
| F18 | busy_work boot + outbox batching | Gate: target/aether_daemon_boot_us.json | 2–3% do primeiro ciclo | Baixo |
| A8 | Federação + `--multicast-port` + precheck | Gate: 2 instâncias na LAN, R>0.95 | Estabilidade do R | Médio |
| A9 | Authority rebase 200→≤120 autônomos | Gate: review do doc do barramento + testes | Governance | Alto |
| A10 | Episodic probe budget no bus (G-ABD) | Gate: anti-loop fuzz | Integridade | Médio |
| A11 | Autobiografia determinística | Gate: L10, 10 runs | Determinismo | Baixo |
| F19 | G-MORPH new_basin_events + shootout | Gate: novos testes em morphogenesis | N/A (Sprint A) |
| F19b | Dreamer: execução sequencial pinned (Sprint A) | `thermodynamic_dreamer.rs:514` (rayon ativo no hot path); gate: eq-t/s equivalência + T0 p99 estável | Wakeups de threads, invalidação L1/L2 no T0 | Sprint A |
| F20 | Fast-J Gating (J_eff = J·g_i·g_j) | `morphogenesis.rs` + gate `tests/morpho_gating_gate.rs` (ver L13) | Sub-redes que se acoplal 1/frames | Sprint A |
| F21 | Elastic compute no Daemon (horizonte 30×) | `E6-mod5` (ver L11; arquivo `target/aether_e6_mod.json`) | Raciocínio profundo sem Latência nominal | Sprint A |
| F22 | RG spine ascendente + torque descendente | `aether_governor.rs` + `renormalization.rs:223` (ver L12) | Freedown surge; sem micro-reprocessing | Sprint A |
| F23 | Cross-modal RG: macro_signature() (G-ISO) | `self_field.rs:529` (ver §2b tipo 3) | Zero-shot dynamics transfer; gate TKNets | Sprint A | Baixo |
| F9 | AUTH default on (A2) | Gate: security review + teste de token | Segurança multi-usuário | Alto — após A2 |
| F10 | Cronograma: F1-F5 Sprint 1 (jan 2027), F6 deferido, G-ABD Sprint 2 | Gate: acquirement plan + review | — | — |

---

## 8. Gaps de coerência / consistência (checks de implementação)

- C1: Fases em `[0, 2π)`: definição muda (organism.rs:48 garante `[0,2π)`; kuramoto_field.rs:187-203) — padronizar em `wrap` (renormalization.rs:21).
- C2: `should_abstain` (phase_memory.rs:312) — não chamado em nenhum loop (futuro, G-ABD).
- C3: ADD-only: confirmado — memórias do daemon (`emit_pattern` gap=1.0) — não reprocessam.
- C4: Autoridade: daemons emitem certificates = 200 (`aether_daemon.rs:524`, `organism.rs`); acão autônoma ∈ 200; **200–255 = HITL**; A9 decide rebase: agente/quarantine ≤120, Código 200.
- C7: Federação: `--multicast-port` segmentada por organismo; precheck de duplicatas de `ScopeId` em boot (A8/F17); `peer_id` = scope (1-hop, sem vazamento).
- C8: Determinismo sistêmico: hash bit-a-bit de fases (F19/L10) + teste de reproducibilidade paralela.
- C5: Determinismo multi-instância: segundo instância local: R pode ser ~0 sem peers — invariante documentado; invariante de 1 instância nohost: ok.
- C6: `panik=abort` — o T0 não pode ficar sozinho ao colapsar; futuro: catch_unwind no main thread do organismo + restart.

---

## 9. Sugestão de cronograma

- **F1–F5 Sprint 1** (2–3 ciclos, jan 2027): hot path semântica-preservando,
  gates E6-mod2/2b + hot-path-invariant extension (0 alloc em `step`).
- **G-ABD Sprint 2** (fiação, no mínimo 2 ciclos): abdução no daemon, especialmente
  M1 (futuros adversariais) e M2 (prisma do Ensemble).
- **F6 deferido** até AFTER A/B com 30 days de medidas de F1.
- **T-GRAPH A6** paralelo a Sprint 1 (independent).
- **Sprint A futuro:** multi processos, auth default, outbox drain audit,
  sindy da lei no SGDB (G-LAW, Sprint A+B),
  **Federação de micro-organismos** (A8, 1b — N=64/128, portas multicast
  segmentadas, precheck de duplicatas),
  **Backpressure na fiação** (F18, L6 — Plone REST no SGDB lê a lei mais nova).

- **Verificação F16/F17/F18 + L3–L10:** gates no §7 (F16/F17/F18/A8/A9/A10/A11
  e as linhas L3–L10). F19b (Dreamer pinned sequential) em Sprint A.

---

## 10. Referências

- [`POSICIONAMENTO_AETHER.md`] — peças cognitivas, gates, invariantes, F0–F3.
- [`HW_TARGET.md` §8] — medidas E6/E6b, mapa de capacidade (meia Mesa).
- [`E2_OPTIMIZATION.md` §10] — decisão registrada sobre hot path do sistema lógico.
- [`COGNITIVE_AUDIT_REPORT.md`, `COGNITIVE_AUDIT_FINAL.md`] — prior art de auditoria cognitiva.
- [`TOMADAS_FUTURO.md`], [`CORE_TOMADAS.md`], [`AETHER_AGENT_ROADMAP.md`].
- Código externo citado pela posição (audit trail): arXiv:2311.12615, 2407.13940, 2512.01924, 2510.23258, 2510.21418, 2508.12027, 2605.16030, 2501.13329, 2408.12277, 2505.07179, 2511.21046.

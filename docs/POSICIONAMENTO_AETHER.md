# POSICIONAMENTO_AETHER — core no-tokens × tomadas: plano de adequação

> **Status:** quarentena — autoridade **120** (evidência web/pesquisa; nunca 255
> sem HITL). O *plano* foi sancionado pelo usuário em 2026-11-24; o conteúdo
> externo citado permanece ≤120 até gate próprio.
> **Data:** 2026-11-24 · **Escopo:** `AETHER` · `org:1/run:0` · entidade `aether/posicionamento`.
> **Doutrina:** [`CORE_TOMADAS.md`](CORE_TOMADAS.md) (Eixo C: tomada fora do
> inventário = bug) · [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) ·
> [`RESEARCH_DOADORAS_2026.md`](RESEARCH_DOADORAS_2026.md) (nada entra no crate
> sem gate + licença verificada).
> **Natureza:** documento de posicionamento e plano. **Não altera o crate.**
> Gates aqui propostos são compromissos de ciclos futuros, não capacidades atuais.

---

## 1. Temas

1. **Inteligência física vs. auditoria de veracidade.** Um ciclo que só consulta,
   verifica `violações == 0`, cita ou abstém é um cartório determinístico.
   Inteligência começa onde o dado armazenado acaba: generalizar, formular
   hipóteses, transferir estrutura por analogia e criar modelo interno novo.
2. **Core no-tokens × tomadas.** O núcleo contínuo (fase, FHRR, Ising/Lagrange,
   Koopman/SINDy, campo morfogenético) é o AETHER. Toda modalidade — prosa, MoE,
   coder, agêntica, speech, visão, grafos, world models — é **tomada enumerada**
   com contrato, disjuntor e escada. Bytes brutos (PCM, pixels, texto livre)
   param na borda; o T0 só consome estruturas tipadas.
3. **Quarentena de autoridade.** Hipótese = 60; verificado pelo agente = 120;
   255 só HITL (`BUILD_MULTI_AGENT.md`). Imaginação interna é livre; toda
   travessia da borda (`act`/claim) passa pelo verificador. O certificado e o
   `neural-sgdb` são **sistema imunológico e memória autobiográfica**, não o cérebro.
4. **Orçamento físico como contrato.** `81,3 µs`/quadro (`0,49%` de 60 Hz,
   `AUDIT_CLAIMS_VOL3.md` ✅ ±1%), `0 B` alocados no hot-path
   (`tests/transducer_invariants.rs`, 600 quadros), L2 de **1,25 MB**
   (`HW_TARGET.md`: N≈256 → 1,03 MB; N=512 → 3,04 MB/L3 e **teto de tempo real
   a 60 Hz**). Qualquer adequação que estoure esse orçamento roda assíncrona
   fora do T0 ou não entra.

## 2. Foco

**Tese:** o AETHER não compete com LLM de 70B escrevendo texto. Ocupa o nicho
onde modelos baseados em tokens são lentos, caros ou alucinam: **cérebro
ciber-físico, agêntico e verificável de ~2 W**, contínuo, temporal e regido por
invariantes, rodando em hardware local.

**Regra física de afinidade:** quanto mais contínuo, temporal e regido por
invariantes for o domínio, mais nativo ele é ao núcleo; quanto mais discreta e
lexical for a superfície de saída, maior o peso transferido para a borda
(`TransducerIo` / `LanguagePort` / ports de modalidade).

**O que o AETHER não é** (anti-objetivos): chatbot open-domain; gerador de
pixels fotorrealistas; tokenizer; "verdade 255" automática; substituto de
revisão humana.

4. **Correção técnica (audit 2026-11-24):** o T0 é o organismo M4
   (`simulation/closed_loop_organism.rs`); RG/Dreamer/SINDy/AIF/CBF estão
   *fora* dele (0 µs, `HW_TARGET.md` §8.4). Entram por Sprint A sem alterar
   o contrato de 81,3 µs/ciclo — e mesmo assim só com custo se ele importar
   no orçamento de 60 Hz (folga medida ~16,5 ms/ciclo).

5. **Arquitetura de produto (adotada, AUDIT_CORE_EXTREMO_2026-11-24 §1b):**
   o organismo M4 é o **produto A canônico** (N ≤ 512, T0 intacto). O
   escalonamento em **federação de micro-organismos** (N = 64/128) é a topologia
   de produtos leves com isolamento de escopo e falhas: processos separados,
   portas multicast segregadas (`--multicast-port` — fio F17), `ScopeId`
   isolado (`knowledge_bridge_f1.rs`) e precheck de duplicatas. É uma
   **mutação de topologia de processos, não contorção do T0.**

6. **Vantagem assimétrica — Dedução causal por lei física:** o AETHER destila
   episódios em operadores Koopman/SINDy (`G-LAW`) e deduz o 1.001º estado
   inédito por *integração*; LLM preve o token mais provável.
7. **Vantagem assimétrica — Imunidade à alucinação promovida:** `SelfField`
   (E4) + certificado `violações == 0` + quarentena (60/120/255) separam
   imaginação livre da ação autorizada.
8. **Vantagem assimétrica — Memória autobiográfica em 1,25 MB:** FHRR/BAM +
   RG 4 + `ADD-only` no SGDB mantêm contexto contínuo sem KV-cache quadrático.
9. **Vantagem assimétrica — Horizonte temporal duplo:** o mesmo campo sustenta
   resposta rápida (T0 intacto) e raciocínio profundo (F21: 30×, ~3,2 ms) sem
   penalizar a latência nominal.

## 3. Fontes

### 3.1 Internas (código, gates, docs)

| Fato | Evidência |
|---|---|
| 81,3 µs/quadro · 0,49% de 60 Hz | `docs/AUDIT_CLAIMS_VOL3.md` (run3/run0: 80,864/82,589 µs); `docs/HW_TARGET.md` E6b |
| 0 alloc hot-path · `TransducerIo` contrato | `src/transducer_io.rs`; `tests/transducer_invariants.rs`; `docs/TRANSDUCER_IO.md` |
| Audito 2026-11-24: energia 2–3×/quadro; dedup FFT (`closed_loop_organism.rs:467/469`) | `docs/AUDIT_CORE_EXTREMO_2026-11-24.md` §3 F1–F2 |
| Daemon: Dreamer amostra por quadro, uncertainty reportada não decisória (aether_daemon.rs:588-605,634) | `docs/AUDIT_CORE_EXTREMO_2026-11-24.md` §5 L1 |
| SelfField E4 metacognição + abstenção prontas (`src/core/self_field.rs:559-580`) | `docs/AUDIT_CORE_EXTREMO_2026-11-24.md` §5 L2 |
| Solver E6b: 57,5 µs, GFLOP/s 17,1, 0 alloc (`HW_TARGET.md` §8.5) | `docs/HW_TARGET.md` |
| ARGH: SO_REUSEADDR em 239.255.42.99:7777 permite colisão entre daemons | `src/core/kuramoto_field.rs:278`, `src/bin/aether_daemon.rs:384-395`; audit A8 |
| RG 16ch (macro_order/alignment) + SelfField observe_hierarchy | `src/core/renormalization.rs:179-198`; `src/core/self_field.rs:533-546` |
| L2 1,25 MB · teto N=512 a 60 Hz | `docs/HW_TARGET.md` |
| Koopman/SINDy online + JIT | `src/core/koopman_sindy.rs`; `src/core/sindy_jit.rs`; `src/bin/aether_governor.rs` |
| Dreamer (K futuros, incerteza, currículo E5) | `src/thermodynamic_dreamer.rs` (`DreamEnsemble`, `ProgressCurriculum`); daemon: F alto ⇒ `imagine_futures(T=0.45)` |
| EFE com termo epistêmico | `src/core/active_inference.rs` (`G = custo − λ_epistêmico · incerteza`); `LyapunovCbf`; `EmaActuator` |
| Morfogênese (Hebbiano sob surpresa, poda metabólica, homeostase ρ) | `src/morphogenesis.rs` (M7-HARDEN) |
| RG 64→16→4 top-down | `src/core/renormalization.rs`; `src/core/self_field.rs` |
| PLH sharded + ACF (E1) | `src/core/phase_language.rs`; `tests/e1_plh.rs`; `tests/e1_language_gate.rs` |
| BAM + transplante JL (E3) | `src/platform/host_transducer.rs`; `src/core/transplant.rs` |
| MoE EXP3/UCB + LinUCB opt-in · G-A1 verde | `src/agent/planner.rs`; `src/agent/linucb.rs`; `tests/agent_tools_gate.rs`; `docs/AETHER_AGENT_ROADMAP.md` |
| E2 LagONN/BSB · G3b **pendente** (8,2–21 µs vs alvo 4,07 µs) | `docs/E2_OPTIMIZATION.md`; `tests/e2_logic_gate.rs` |
| Coder K0–K8 vivos (2026-11-21) | `docs/CODER_AETHER.md`; `src/agent/tools/code/`; `tests/agent_coder_gate` |
| Escadas S*/V*/P*/M*/A* e disjuntores | `docs/SPEECH_AETHER.md`; `docs/VISAO_AETHER.md`; `docs/PROSA_FLUENTE.md`; `docs/MOE_ETL_PACK.md`; `docs/ACOES_AGENTICAS.md`; `docs/CORE_TOMADAS.md` |
| Catálogo de doadoras (licenças, gates E3/B-G0) | `docs/RESEARCH_DOADORAS_2026.md`; `docs/DONOR_CATALOG.md`; `docs/LANGUAGE_PORT.md` |

### 3.2 Externas — peças cognitivas (pesquisa web 2026-11-24)

| Peça | Candidato | Fonte | Encaixe |
|---|---|---|---|
| Abdução ativa | Mind Dreamer — Relay EFE, saltos contrafactuais a pontos cegos epistêmicos | arXiv:2605.16030 | Receita p/ laço abstain→hipótese→ação epistêmica |
| Abdução ativa | Deep AIF c/ world model temporalmente hierárquico (robô real) | arXiv:2512.01924 | EFE sobre ações abstratas = trato computacional |
| Abdução ativa | Deep AIF + diffusion policy + MTRSSM (navegação real) | arXiv:2510.23258 | Exploração + objetivo unificados por EFE |
| Abdução ativa | DreamerV3-XP — recompensa intrínseca por disagreement de ensemble | arXiv:2510.21418 | `DreamEnsemble` já mede incerteza análoga |
| Abdução ativa | DreamerV3 (Nature 2025); TD-MPC2; pymdp | `danijar/dreamerv3`; `nicklashansen/tdmpc2`; `infer-actively/pymdp` | Referências de world model + AIF |
| Abdução ativa | Mind the Execution Gap — execução assíncrona em world-model control | arXiv:2610.06582 | Executor de borda assíncrono (A4) |
| Destilar leis | **Koopman Learning with Episodic Memory** — EDMD c/ memória, online, sem treino, CPU de laptop | **arXiv:2311.12615** | Pipeline exato que falta: episódios → operador → lei |
| Destilar leis | Koopman online c/ novidade por distância Grassmannian (arquiva só o novo) | arXiv:2407.13940 | Atualização disparada por surpresa |
| Destilar leis | SINDy-SHRED / Koopman-SHRED (PNAS 2026) | arXiv:2501.13329; `gaoliyao/sindy-shred` | Latente esparso → equação interpretável |
| Destilar leis | pysindy · pykoopman (MIT) · pykoop (MIT, multi-episode) | `dynamicslab/pysindy`; `dynamicslab/pykoopman`; `decargroup/pykoop` | Ferramentas de referência p/ ETL offline |
| Transferência | Modularized EDMD — transfer entre subsistemas, topologia nova **sem retreinar** | arXiv:2408.12277 | Mecanismo p/ isomorfismo de dinâmica |
| Transferência | TKNets — generalização temporal via Koopman (AAAI 2024) | arXiv:2402.07834 | Bound de generalização + operador |
| Transferência | MetaKoopman — meta-learning bayesiano, incerteza calibrada | arXiv:2607.26345 | Few-shot c/ abstain por incerteza |
| Transferência | KOMET; VSA category theory; VSA practical lessons (PMLR v284); Rel-SAR | arXiv:2603.26923; 2501.05368; v284/carzaniga25a; 2501.11896 (já no repo) | Mapeamento analógico formal em VSA |
| Morfogênese | **AKOrN** — neurônios Kuramoto, binding por sincronização | arXiv:2410.13821; `autonomousvision/akorn` | Interferência → clusters especialistas |
| Morfogênese | **Phasor Agents** — grafos oscilatórios, plasticidade 3 fatores, sono | arXiv:2601.04362 | ≈ Dreamer + MorphogeneticField |
| Morfogênese | Kuramoto de ordem superior (memória densa); metastabilidade não-recíproca | arXiv:2507.21984; 2512.20410 | Bacias híbridas / capacidade |
| Morfogênese | Wien bridge — aprendizado autônomo de atratores (hardware) | arXiv:2512.14869; 2607.00286 | Energia/surpresa reshaping em hardware |
| Morfogênese | Oscillator networks como recurso computacional; Equilibrium Propagation | arXiv:2502.04818; IOP 10.1088/2634-4386/adebaa | Reservatório + treino por sincronização |

### 3.3 Externas — modalidades

| Modalidade | Achado / candidato | Fonte |
|---|---|---|
| Speech | **Fronteira L3 vazia**: survey full-duplex classifica L0–L3; *"L3 representation-level: shared continuous latent across streams — no published system yet"* | arXiv:2606.19453; EMNLP 2026 `2026.emnlp-main.1126`; `DuplexLM/DuplexSurvey` |
| Speech | Moshi (duplex nativo, 160–200 ms, Mimi codec); Freeze-Omni (LLM congelado); GLM-4-Voice (**Apache-2.0**); BayLing-Duplex (92% turn-taking); SALMONN-omni (codec-free); DuplexMamba | arXiv:2410.00037 (`kyutai-labs/moshi`); 2411.00774 (`Tencent/Freeze-Omni`); 2412.02612 (`zai-org/GLM-4-Voice`); 2606.14528 |
| Speech | Divisão indústria "小脑–大脑" (GPT-Live/Gander): tempo real ≠ pesos de raciocínio profundo | survey acima — **corrobora core–borda AETHER** |
| Visão | **Tracking Is Not Permanence**: V-JEPA 2 perde objeto oculto em movimento em **0,3 s**; protocolo minimal-pair + probes pronto | arXiv:2610.07355 |
| Visão | V-JEPA 2 / 2.1 (**MIT**, world model + planejamento zero-shot); CoTracker3 (oclusão); SAM2 (memória streaming) | arXiv:2506.09985; 2603.14482 (`facebookresearch/vjepa2`); 2410.11831 (`facebookresearch/co-tracker`); `facebookresearch/sam2` |
| MoE | Kuramoto Attention (atenção = sincronização no toro); KoPE (fase oscilatória); KOCOBrain (Kuramoto + gating MoE); LASER (routing plug-and-play); MoE survey | arXiv:2606.11585; 2604.07904; 2601.11018; 2510.03293; 2503.07137 |
| World models | NVIDIA Cosmos 3 (omnimodal, **OpenMDW-1.1**; Cosmos3-Edge 4B p/ Jetson) = simulador pesado de borda/dados sintéticos; complementar ao núcleo leve | `nvidia/cosmos`; `nvidia-cosmos/cosmos-transfer2.5` (Apache-2.0) |
| Grafos temporais | TGN puro não basta p/ intrusão (TGN-SVDD); TFLAG (TGAT + deviation, APT); fraude financeira TGN; TADDY/Anomal-E; SLR 2026 | arXiv:2508.12885 (`AlekseiLiu/tgn_svdd`); 2501.06997 (`wangkai-tech23/TFLAG`); 2404.00060; 2510.26307; `twitter-research/tgn` |
| Neuro-simbólico | LagONN (já referência do repo); limites de Ising p/ SAT estruturado — pré-processamento híbrido **150×** (valida presolve E2); SATNet (**MIT**, MAXSAT diferenciável); chip Ising CMOS (Nature Electronics 2025); AS2; solver-free constraint learning | arXiv:2505.07179 (`corentindelacour/Lagrange-oscillatory-neural-network`); 2511.21046; `locuslab/satnet`; 2603.18436; 2508.20978 |
| Coder/Prosa | Catálogo interno já cobre: LCM, BLT, xLSTM, RWKV-7, Mamba, RASP, Grammar Pruning; emissor determinístico: outlines/guidance/GBNF | `docs/LANGUAGE_PORT.md`; arXiv:2412.08821; 2412.09871; 2503.13427; 2503.14456; 2412.10207; `outlines-dev/outlines`; `guidance-ai/guidance` |

### 3.4 Licenças (doutrina `RESEARCH_DOADORAS_2026.md`)

- **Verificadas nesta pesquisa:** MIT — pykoopman, pykoop, vjepa2, SATNet;
  Apache-2.0 — GLM-4-Voice, cosmos-transfer2.5; OpenMDW-1.1 — Cosmos 3.
- **A verificar antes de qualquer adoção:** Moshi (código vs. pesos),
  Freeze-Omni (`License.txt` custom), CoTracker3, SAM2, AKOrN, torchhd,
  pysindy, dreamerv3, tdmpc2, pymdp, TGN/TGN-SVDD/TFLAG.
- **Regra:** nada entra no crate sem gate + licença por checkpoint; proveniência
  `manifest.donor` + `revision` + hash. Non-commercial (CC BY-NC) = proibido.

## 4. Ideias

### 4.1 As quatro inversões de papel (auditoria → inteligência)

| # | De → Para | Órgão existente | Laço que falta | Gate proposto |
|---|---|---|---|---|
| 1 | Abstenção passiva → **Abdução ativa** | `ActiveInferenceEngine` (EFE epistêmica), `ThermodynamicDreamer` (K futuros + incerteza), planner EFE+UCB (G-A1) | `should_abstain` é terminal: saída do dreamer não vira hipótese tipada que realimenta goal/blackboard; não há ciclo abstain→abduzir→micro-ação epistêmica→re-observar | **G-ABD** |
| 2 | Decorar fatos → **Destilar leis** | `OnlineKoopmanOperator`, `SindyEquationDiscoverer` + JIT (governor) | SINDy nunca consome memória episódica; não há artefato `Law`/`DynamicModel` com proveniência/quarentena no SGDB | **G-LAW** |
| 3 | Similaridade → **Transferência por isomorfismo** | `RenormalizationHierarchy` 64→16→4, `self_field` (escala macro), transplante E3 (JL) | Transplante é de fatos (triplas→HV), não de dinâmica; sem gate de assinatura macro → projeção de solução cross-domínio com abstain por distância | **G-ISO** |
| 4 | Atrator rígido → **Síntese por interferência/morfogênese** | `MorphogeneticField` já cresce/poda sob surpresa com homeostase; `WaveInterferenceBlock` | Sem readout de formação de conceito: detectar bacia híbrida estável, tipá-la, consolidar na memória (L2) e no MoE como expert | **G-MORPH** |

**Invariante que torna isso seguro:** imaginação interna livre; toda travessia
da borda passa pelo verificador. Hipótese abdutiva nasce `propose`/auth 60; lei
destilada entra em quarentena até gate de predição; transferência é hipótese até
validação; nada se promove sozinho. O laço de abdução cabe nos ~16,5 ms livres
do quadro **ou** roda assíncrono fora do T0.

### 4.2 Ranking corrigido — dois eixos (afinidade física × maturidade medida)

O "% nativa" da tabela original mistura os dois eixos. Correção:

| Modalidade | Afinidade física | Maturidade medida (degrau/gate) |
|---|---|---|
| World Models / Digital Twins | **Nativa** (contínuo, temporal) | m8 vivo (`SystemTelemetryTransducer`); A (CAN) e B (CSI) = contrato pronto, backend não implementado |
| MoE | Nativa (campo); roteamento **é** EXP3 | M0 vivo; "ressonância sem router" = hipótese sem gate |
| Agêntica autotélica | Alta | A0–A4 vivos, G-A1 verde; laço abdutivo ausente (G-ABD) |
| Neuro-simbólico | **Nativa** (Ising/Lagrange) | E2 vivo; **G3b pendente** (2–5× acima do orçamento) |
| Temporal Graph | Alta | BAM/JL vivos (E3); produto sem gate |
| Coder | Média (discreto na borda) | **K0–K8 vivos** — usável hoje como assistido; geração é da borda, não do core |
| Speech | Alta (onda), **borda pesada hoje** | S0–S2 stubs; PCM fora do T0 (invariante); duplex = S6; fala nativa em fase = fronteira L3 vazia |
| Visão | Média | V0 stub; feature off; tracking/JEPA = V7 pesquisa; permanência = gap medido (arXiv:2610.07355) |
| Prosa | Baixa no core (lexical) | P0–P2 vivos (template + juíz); fluência = P4+ com gates L2/L3 |

Correções doutrinárias explícitas: (a) speech **não** é "95% nativa hoje" — o
pipeline atual é STT→texto→core→TTS; (b) MoE implementado **tem router** (EXP3);
(c) coder **não** sintetiza AST no core — geração na borda, verificação no
AETHER (`CODER_AETHER.md` invariante 1); (d) visão não recebe pixels no T0.

### 4.3 Fronteiras de pesquisa (diferenciais publicáveis)

1. **Speech L3** — latente contínuo compartilhado entre streams (survey declara
   vazio). O campo de fase do AETHER é candidato natural; benchmark: latência
   vs. Moshi 160–200 ms, barge-in, prosódia preservada.
2. **Permanência de objeto** — V-JEPA 2 perde objeto oculto em 0,3 s
   (arXiv:2610.07355). Inércia de fase + SINDy no core é hipótese falsificável
   com protocolo pronto.
3. **Roteamento por ressonância** — afinidade de fase como feature do EXP3
   (híbrido), não substituição do router; literatura: AKOrN, Kuramoto Attention.
4. **Lei como artefato de memória** — EDMD com memória episódica
   (arXiv:2311.12615) + `Law` tipado no SGDB: o 1.001º caso deduzido, não lembrado.

## 5. Itens e subitens — as 9 tomadas

Regra transversal (Eixo C): **nenhuma tomada duplicada**. World models reusa
**T-SENSOR (D1)** e **T-ROBOT (D4)**, já reservadas em `TOMADAS_FUTURO.md`;
**T-GRAPH** é nova e foi registrada lá (A6) + no inventário de `CORE_TOMADAS.md`.
Todas obedecem a lei do barramento (fio único, disjuntor único, auth ≤120,
DB único = neural-sgdb).

### 5.1 World Models & Digital Twins — **T-SENSOR (D1) / T-ROBOT (D4)** + m8

- **Contrato:** fonte física → `TransducerIo::sample_into` → `u(t) ∈ [-1,1]^64`
  → F(t); entrada tipada `HostObs`/`SensorSample` (T-SENSOR) → T-PRED/T-MOE;
  atuação `RobotCmd` + certificado (T-ROBOT, HITL duro) via `LyapunovCbf` +
  `EmaActuator`. Lei destilada (G-LAW) realimenta predição. `TransducerIo` é a
  camada física sob T-SENSOR; o daemon m8 já é o piloto do produto C.
- **Estado:** `SyntheticTransducer` + `SystemTelemetryTransducer` vivos;
  governor roda Koopman online. **A gaps do T0:** RG/Dreamer/SINDy/AIF/CBF
  estão fora do laço do organismo — 0 µs medidos (audit F1/F2, E6-mod abaixo);
  CAN/OBD-II (A) e CSI (B) contrato pronto (backends não implementados).
- **Pendências do T0 próprio (audit 2026-11-24):** F1–F7 (hot path, ver §9);
  o tune do Solver vive em `HW_TARGET.md` §8.5 (E6b).
- **Candidatos de borda:** backend CAN (ELM327/SocketCAN), CSI Wi-Fi; Cosmos 3
  apenas como simulador pesado opcional/dados sintéticos (OpenMDW-1.1, fora do T0).
- **Passos:** (1) produto C completo (aceite `TRANSDUCER_IO.md` §6); (2) D1
  T-SENSOR verde (orçamento de poll; abstain se sensor morto); (3) backend CAN
  piloto; (4) laço lei→predição→atuação com G-LAW; (5) D4 T-ROBOT só depois de
  D1 + T-PRED + T-HITL duro; (6) CSI.
- **Disjuntores:** shape/finite; F sem baseline calibrado → ask; atuação fora
  do envelope CBF → projeção/fallback.
- **Gate/objetivo:** **G-PHYS** — rajada real (disco/CPU) eleva F acima do
  baseline, pico localizado, ação no orçamento, 0 alloc; produto C em produção local.

### 5.2 MoE por Ressonância — **T-MOE** (vivo)

- **Contrato:** `Goal` → top-k experts/tools (EXP3/UCB; LinUCB opt-in).
- **Adequação:** **não remover o router**. Adicionar afinidade por assinatura de
  fase (goal × autofrequência do expert) como feature do EXP3/LinUCB; medir
  ganho de regret/hit-rate. "Sem colapso de roteamento" só vira claim com artefato.
- **Candidatos:** AKOrN (2410.13821), Kuramoto Attention (2606.11585), KoPE
  (2604.07904), LASER (2510.03293) como referência de balanceamento.
- **Gate/objetivo:** **G-RES** — router híbrido ≥ EXP3 puro em hit-rate/regret,
  5 seeds, IC95, sem colapso; ablação documentada.

### 5.3 Speech & Áudio Full-Duplex — **T-STT / T-TTS** (stubs)

- **Contrato:** `SttResult{text,conf,lang,privacy}` → T-NL-IN; TTS só
  `CertifiedUtterance`. PCM **nunca** no T0 (invariante 1 de `SPEECH_AETHER.md`).
- **Trilha produto (hoje):** S3–S6 — STT local (whisper.cpp/onnx) → parser C1 →
  core → juíz P2 → TTS local (piper/onnx); duplex turn-taking com VAD; PT-BR pack S7.
- **Trilha pesquisa (L3):** latente contínuo compartilhado entre streams —
  fronteira vazia do survey (2606.19453). Prosódia/barge-in por interferência de
  fase; sem claim antes de gate.
- **Candidatos:** Moshi, Freeze-Omni, GLM-4-Voice (Apache-2.0), BayLing-Duplex,
  SALMONN-omni, DuplexMamba; vocoders Mimi/CosyVoice/vocos. Licenças a verificar.
- **Gate/objetivo:** S6 verde (barge-in sem loop infinito, budget); **G-L3** —
  latência competitiva vs. Moshi (160–200 ms), prosódia preservada, 0 fala sem
  fonte; assistente PT-BR certificado.

### 5.4 Agêntica Autotélica — **T-PLAN / T-HITL** (vivos)

- **Contrato:** `Goal` → PlanStep* + rollback; risk_ceiling/allowlist; HITL 255.
- **Adequação (G-ABD):** fiar o laço `abstain → Dreamer gera hipóteses tipadas
  (auth 60) → EFE epistêmica escolhe leitura/micro-ação de menor custo →
  re-observa → registra desfecho no SGDB`. Executor assíncrono já existe (A4);
  MCTS/horizonte completo permanece pesquisa (roadmap L284).
- **Candidatos/receitas:** Mind Dreamer (2605.16030), Deep AIF hierárquico
  (2512.01924), DreamerV3-XP (2510.21418), pymdp; execution gap (2610.06582)
  para o executor assíncrono.
- **Gate/objetivo:** **G-ABD** — cenário sem atrator: dúvida resolvida por ação
  barata; false-act = 0; orçamento ≤16,5 ms ou assíncrono fora do T0; episódio
  versionado em artefato JSON.

### 5.5 Neuro-Simbólico & Otimização — **T-XP-logic/solver** (vivo, E2)

- **Contrato:** instância SAT/QUBO → solver → certificado `violações = 0`.
- **Adequação:** fechar **G3b** (8,2–21 µs → ≤4,07 µs; itens 4+5+6 de
  `E2_OPTIMIZATION.md`); presolve híbrido validado por arXiv:2511.21046
  (150× com pré-processamento que excita estrutura lógica); SATNet (MIT) como
  ponte pesquisosa para aprender restrições de dados.
- **Gate/objetivo:** G3b verde em artefato; solver dentro do T0; certificado
  determinístico bit-a-bit; LagONN como referência externa contínua.

### 5.6 Temporal Graph AI — **T-GRAPH** (nova)

- **Contrato:** evento estruturado (IDs, timestamps, features) → átomo tipado
  (BAM 1,28 KB) → RG 64→16→4 → assinatura topológica → surpresa/anomalia.
  Texto livre não atravessa; ingestão é ETL de borda.
- **Estado:** BAM + JL (E3) vivos; produto sem gate.
- **Candidatos/baselines:** TGN (`twitter-research/tgn`), TGN-SVDD
  (2508.12885), TFLAG (2501.06997), TADDY/Anomal-E; comparação, não dependência.
- **Diferencial AETHER:** fluxo contínuo **sem re-treino**, assinatura de fase +
  coarse-graining RG, memória autobiográfica no SGDB L6.
- **Gate/objetivo:** **G-GRAPH** — AUC ≥ baseline TGN em dataset público de
  intrusão/fraude; latência/evento no orçamento; 0 re-treino; piloto ciber/fraude.

### 5.7 Coder — **T-CODE** (vivo, K0–K8)

- **Contrato:** geração na **borda** (LLM/code LM); AETHER propõe critérios,
  aplica em sandbox, testa, rollback; auth ≤120; shell HITL never default.
- **Adequação:** emissor determinístico por gramática (K5 RASP-lite vivo;
  avaliar outlines/guidance/GBNF); trilha pesquisa: verificação por invariantes
  (tipos/contratos via LagONN; regressões em traces via SINDy). Núcleo **não**
  tokeniza nem "aprende a programar" (invariante 1).
- **Gate/objetivo:** falso merge ≤5%; T3 com gerador declarado (D-G2); keep ≠
  255; OOD/falha de teste → abstain honesto.

### 5.8 Visão Espacial-Temporal — **T-VIS** (stub)

- **Contrato:** `ImageRef` → `VisionObs{labels,entities,embed?,auth_cap≤120}`;
  pixels **nunca** no T0; feature `vision` off por default.
- **Adequação produto:** V2–V3 — classificador local allowlisted (ONNX) +
  entities tipadas p/ recall; V4 bridge p/ Goal.
- **Adequação pesquisa:** tracking/permanência por inércia de fase + SINDy no
  core; borda: V-JEPA 2.1 (MIT), CoTracker3, SAM2 como extratores.
- **Gate/objetivo:** V2–V3 verdes (OOD → abstain ≥90%); **G-PERM** — permanência
  sob oclusão > baseline V-JEPA 2 (0,3 s) no protocolo de arXiv:2610.07355;
  claim visual auth ≤120, act só com critério verificável.

### 5.9 Prosa Fluente — **T-NL-IN / T-NL-OUT** (vivos, P0–P2)

- **Contrato:** parse → `Goal`; claims → `CertifiedUtterance` (juíz P2); sem
  evidência → Empty/abstain; geração auth 60–120.
- **Adequação:** escada P obrigatória — template default → small_lm
  (xLSTM/RWKV/Mamba/BLT, gate L2) → concept (LCM/SONAR, gate L3) → remote_llm
  fallback (C3) → P6 AND Vega. PLH sharded como memória de coerência; torque
  top-down RG como hipótese de arco (sem claim antes de gate).
- **Gate/objetivo:** 0 false-act; geração no certificado (backend/custo/auth);
  gates L2/L3 verdes em artefato versionado; PT coerente com fonte.

## 6. Objetivos (mensuráveis, por item)

| Item | Objetivo | Métrica / artefato |
|---|---|---|
| T-SENSOR/T-ROBOT | Digital twin do host em produção local; piloto CAN | rajada → F > baseline+0,3; 0 alloc; `target/aether_phys_gate.json` |
| T-MOE | Router híbrido com evidência | hit-rate/regret ≥ EXP3 puro, 5 seeds, IC95 |
| T-STT/TTS | Assistente voz PT-BR certificado | S6 verde; 0 fala sem fonte; latência G-L3 vs. Moshi |
| T-PLAN | Agente que resolve a própria dúvida | G-ABD: false-act=0; custo de ação epistêmica mínimo; episódio JSON |
| T-XP | Solver certificado no orçamento do T0 | G3b ≤4,07 µs; `violações=0` bit-a-bit |
| T-GRAPH | Detecção de anomalia em fluxo sem re-treino | AUC ≥ TGN-SVDD/TFLAG; latência/evento; 0 re-treino |
| T-CODE | Coder assistido seguro | falso merge ≤5%; pass@k T3 com gerador; auth ≤120 |
| T-VIS | Cena tipada + permanência pesquisosa | abstain OOD ≥90%; G-PERM > 0,3 s baseline |
| T-NL-OUT | Prosa fluente auditável | 0 false-act; certificado com backend/custo; L2/L3 verdes |

## 7. Goals (gates globais)

**Cognitivos (as quatro inversões):**

- **G-ABD** — abdução ativa: F sobe sem atrator → hipóteses contrafactuais →
  leitura/micro-ação epistêmica de menor custo → F cai; false-act=0; orçamento
  T0 ou assíncrono; 5 seeds; artefato JSON.
- **G-LAW** — destilação: episódios (SGDB/shards) → Koopman/SINDy → artefato
  `Law` com proveniência; **deduzir o 1.001º caso** dentro de ε sem tê-lo
  gravado; lei em quarentena (≤120) até gate de predição; receita base:
  arXiv:2311.12615 + pykoop multi-episode.
- **G-ISO** — transferência zero-shot: mesma assinatura macro (RG 4) → projetar
  dinâmica de domínio conhecido (ex.: predador-presa → gargalo de rede);
  **abstain** quando distância estrutural > limiar; benchmark cross-domínio
  versionado; referência: modularized EDMD (2408.12277), TKNets, MetaKoopman.
- **G-MORPH** — síntese: surpresa persistente sobre dois esquemas parciais →
  bacia híbrida estável (ρ, energia) → readout tipa conceito → consolidação
  L2/SGDB + expert no MoE; sem runaway de densidade (homeostase); referência:
  AKOrN, Phasor Agents, Kuramoto de ordem superior.

**De modalidade:** G-PHYS, G-RES, G-L3, G-PERM, G-GRAPH (§5) + escadas existentes
S*/V*/K*/P*/M*/A* e G3b do E2.

**Regras de gate:** artefato JSON versionado; seeds quando estocástico; abstain
honesto ≠ fail; escopo/gerador declarados; 255 só HITL; nada entra no crate sem
gate + licença verificada.

## 8. Invariantes transversais (inegociáveis)

1. Core no-tokens: sem tokenizer, sem prosa, sem pixels/PCM no T0.
2. Lei do barramento: fio único (`Goal/Claim/Action/HitlRequest/CertifiedUtterance`),
   disjuntor único (banda de triagem), filtros únicos (A7 + auth ≤120 + juíz P2),
   DB único (neural-sgdb, ADD-only, null-scoping).
3. Eixo C: tomada fora do inventário = bug; sem tomada duplicada. T-GRAPH
   registrada (A6 em `TOMADAS_FUTURO.md` + `CORE_TOMADAS.md`); world models
   reusa T-SENSOR/T-ROBOT (D1/D4).
4. Autoridade: hipótese 60; agente/extraído 60–120; ≥120/255 só HITL
   (rebase do organismo: 120, ledger.rs:541); memória nunca vira instrução de sistema.
5. Orçamento: 60 Hz, 81,3 µs medidos, 0 alloc hot-path, L2 1,25 MB, N≤512;
   excedente → assíncrono fora do T0.
6. Escopo `(organism, run, app)` não vaza; `scope_app=AETHER`, `org:1/run:0`.
7. O core não decide: devolve fase/energia/evidência; o host decide o que entra
   no prompt e o que atravessa a borda.
8. **Federação de micro-organismos (A8, AUDIT §1b):** produtos leves N = 64/128
   com portas multicast segregadas (`--multicast-port` não existe — F17) e
   `ScopeId` único por instância; precheck de duplicatas em boot (F10).
9. **Rebase de autoridade do organismo (A9):** ação autônoma ∈ 200;
   200–255 = HITL; default do organismo foi 200 (`ledger.rs:541`) — rebase
   de decisão, não de semântica.

## 9. Faseamento e dependências

| Fase | Entrega | Dependências |
|---|---|---|
| **F0 — documentação** | este doc; T-GRAPH no inventário (A6); auditoria deep (`AUDIT_CORE_EXTREMO_2026-11-24.md` §3, §5, §7–9); ritual SGDB (`context` → `put --entity aether/posicionamento`) | aceite do usuário (2026-11-24) |
| **F1 — fiação sem física nova** | F1–F7 (hot path: energia incremental, dedup FFT, quadros préalocados — audit 2026-11-24) + F16/F17/F18 (Readout vectorizado, presolve trail zero-alloc, busy_work boot — audit); G-ABD (fiação, não invenção — SelfField E4 já existe); G3b (orçamento solver: HW_TARGET §10 recusa por design — batch fora do T0); S3–S5 (speech produto); V2–V3 (visão); T-SENSOR produto C (m8); think-while-idle (Sprint A, audit F7); Federação de micro-organismos (A8, AUDIT §1b) | órgãos já vivos; A4/G-A4 verde; ref: docs/AUDIT_CORE_EXTREMO_2026-11-24.md §1b, §7–9; regras: docs/COERENCIA_E_REGRAS_IMPL_2026-11-24.md |
| **F2 — destilação** | G-LAW (episódios → Koopman/SINDy); artefato `Law` no SGDB com proveniência | F1 (T-SENSOR/m8 fornece séries); E3/B-G0 p/ mapa |
| **F3 — gates de pesquisa** | G-ISO, G-MORPH, G-RES, G-L3, G-PERM, G-GRAPH | F2 (leis p/ transferir); licenças verificadas; benchmarks versionados |

**Regra de ordem:** não pular degraus (fluência antes de template/juíz vira
chatbot solto; geração antes de sandbox vira coder inseguro). Publicação de
capacidade só com gate verde + artefato + escopo declarado.

## 10. Referências cruzadas

- [`CORE_TOMADAS.md`](CORE_TOMADAS.md) — inventário e disjuntores (fonte da arquitetura core × tomadas).
- [`TOMADAS_FUTURO.md`](TOMADAS_FUTURO.md) — fila por dependência A/B/C/D (T-SENSOR D1, T-ROBOT D4, T-GRAPH A6).
- [`TRANSDUCER_IO.md`](TRANSDUCER_IO.md) · [`SPEECH_AETHER.md`](SPEECH_AETHER.md) · [`VISAO_AETHER.md`](VISAO_AETHER.md) · [`CODER_AETHER.md`](CODER_AETHER.md) · [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) · [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) · [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md).
- [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) · [`RESEARCH_DOADORAS_2026.md`](RESEARCH_DOADORAS_2026.md) · [`DONOR_CATALOG.md`](DONOR_CATALOG.md).
- [`HW_TARGET.md`](HW_TARGET.md) · [`AUDIT_CLAIMS_VOL3.md`](AUDIT_CLAIMS_VOL3.md) · [`E2_OPTIMIZATION.md`](E2_OPTIMIZATION.md) · [`AETHER_AGENT_ROADMAP.md`](AETHER_AGENT_ROADMAP.md).
- Externas: tabelas §3.2–3.3 (arXiv/GitHub; quarentena 120; licenças §3.4).

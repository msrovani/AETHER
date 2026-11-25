# Tomadas futuras — fases por dependência

> Inventário de **plugues ainda não vivos**, já desenhados no mesmo
> barramento do core. Contrato-mãe: [`CORE_TOMADAS.md`](CORE_TOMADAS.md).
> Escadas vivas/reservadas: prosa, agêntica, coder, MoE-ETL, visão, speech.
>
> **Ordem de execução = dependência, não horizonte.** Primeiro o que só
> precisa de código/contrato já mergeado (**A**); depois o que só espera
> decisão humana (**B**); depois backend/modelo externo (**C**); por último
> mundo físico (**D**). Dentro de cada classe, ordem **crescente** de
> pré-requisitos: quem tem menos dependências primeiro.
>
> Regra Eixo C: tomada fora desta lista + `CORE_TOMADAS` = **bug**.
> Core só engole `Goal` / `Claim` / `Concept` / `Artifact` / `ToolOutcome`.

## 0. Barramento (lembrete)

```text
[tomada sensória/atuadora] → struct tipado → CORE (MoE/plan/coh/auth)
                                              → Action → [tomada render]
```

Disjuntores globais (ordem): Eixo C → rede → parser/conf → EFE → coherence →
risco → auth≤120 → sandbox → modalidade off.  
Detalhe: `CORE_TOMADAS` §3.

## 0.1 Classes de dependência

| Classe | Precisa de | Consequência |
|---|---|---|
| **A — só código** | contratos vivos (`Goal`/`Claim`/`Action`/`ToolOutcome`), SGDB local, gates existentes | começa hoje; feature flag off até aceite |
| **B — sanção humana** | código pronto + evidência + token HITL | não é trabalho de código; só bloqueia a **promoção** |
| **C — backend/modelo externo** | sidecar, modelo local allowlisted, rede ou donor | fica stub até o motor existir; feature off |
| **D — hardware/mundo físico** | host/sensor/atuador real | só depois de A+B+C verdes; kill switch |

Classe muda **quando** a tomada entra, nunca **como**: fio, disjuntor,
filtros e DB continuam os do `CORE_TOMADAS` §2.5/§3.

### Já vivos (não são fases)

| Tomada | Estado |
|---|---|
| T-HITL | **vivo** — outbox `hitl_queue` + hub (fila/ficha/approve/deny/ampliar/histórico) + API |
| T-MCP | **vivo** — `mcp.call` allowlist (G-A4) |
| T-EMB | **vivo formal** — era `BAAI/bge-m3` 1024d, `AETHER_EMBED=1`; off por default; F4 congelada |
| T-DEC | **vivo opt-in** — `dec_view`, `bus.t_dec`, `--vega`/`AETHER_VEGA`; cutover = **B2** |
| T-PRED | **vivo** — `PredictExpert` + `predict.series`; `enter_v1_default=false` → **B1** |

## 1. Fases em ordem de execução

Fila crítica (topo → base). A coluna **deps** conta só pré-requisitos ainda
não prontos; “vivo” não conta.

| # | Classe | Item | Deps | Depende de | Desbloqueia |
|---|---|---|---|---|---|
| **A1** | A | T-TAB — planilha/CSV → `Claim[]` | 0 | `Claim` + `facts.jsonl` canônico (vivos) | pack/ETL; fixtures do T-EVAL; tabelas do T-OCR |
| **A2** | A | T-EVAL — placar T0–T3 no hub | 1 | A1 (fixtures) + gates existentes | semáforo do hub; evidência para B1–B3 |
| **A3** | A | T-P2P — delta entre nós SGDB | 1 | SGDB + 2 nós locais (sem writer duplo) | sync de evidência; conflito = `contradicts` |
| **A4** | A | T-CLIP — clipboard sandbox | 1 | A4/G-A4 + K1–K2 (vivos) | entrada de UI sem shell livre |
| **A6** | A | T-GRAPH — evento → átomo BAM / assinatura RG | 0 | BAM/E3 + RG + SGDB (vivos) | anomalia em fluxo sem re-treino; G-GRAPH |
| **B1** | B | sanção **A8 `enter_v1`** | 1 | gate `aether_a8_v1_criteria.json` + token HITL | T-PRED deixa de ser opt-in |
| **B2** | B | **cutover T-DEC** dual-bank | 1 | bank `active` + HITL (não depende de B1) | T-DEC sem candidate no bus |
| **B3** | B | **nova geração F4** | 3 | T-EMB + B1/B2 + decisão de dataset | F4 deixa de congelada |
| **C1** | C | T-OCR — backend real | 2 | T-NL-IN + A1 (tabelas) + sidecar | documento → texto → Goal |
| **C2** | C | T-STT — backend real | 1 | T-NL-IN + backend (S0–S3) | voz → Goal |
| **C3** | C | T-VIS — backend real | 1 | T-NL-IN + era do T-EMB | `VisionObs` → Goal/claim |
| **C4** | C | T-TTS — backend real | 1 | T-NL-OUT/`CertifiedUtterance` | fala pós-triagem |
| **D1** | D | T-SENSOR — host/IoT | 1 | hub `host_probe` (+ A4 se persistir) | série para T-PRED |
| **D2** | D | T-HAPTIC — render tátil | 1 | Action/`Certified*` | banda verde/amarela/vermelha tátil |
| **D3** | D | T-AR — overlay | 2 | C3 (T-VIS) + T-COH | âncoras AR pós-evidência |
| **D4** | D | T-ROBOT — ação física | 3 | D1 + T-PRED + T-HITL duro | `RobotCmd` com certificado |

Fora da fila crítica: **A5 Hier MoE/LSH** (código, mas depende do rerank
f32/bits e de benchmark; não bloqueia nenhuma fase acima) — pode rodar em paralelo.

Desempate na mesma contagem de deps: primeiro o contrato já congelado no
código (OCR/STT/VIS/TTS têm struct estável) e o que **reusa fase
recém-construída** — por isso C1 é T-OCR: consome a A1 e já estava na fila
aprovada. C2–C4 só entram quando suas escadas (SPEECH/VISAO) abrirem.

## 2. Classe A — só código

### A1 · T-TAB — Tabela / CSV → claims
| | |
|---|---|
| **Contrato** | planilha → `Claim[]` (s,r,v) + schema |
| **Fio** | **T-TAB** → T-PACK / memory.write → T-MOE |
| **Disjuntor** | schema break → reject; auth ≤120 |
| **Deps** | nenhuma pendente: MoE ETL e `facts.jsonl` canônico já existem |
| **Aceite** | round-trip N linhas; OOD coluna → abstain |

### A2 · T-EVAL — Placar T0–T3
| | |
|---|---|
| **Contrato** | suite → relatório JSON (seed, host, gates) |
| **Fio** | CI/hub → **T-EVAL** → T-UI (não alimenta act sozinho) |
| **Disjuntor** | misturar T0 com HumanEval sem gerador = N/A |
| **Deps** | A1 (fixtures de claims); `EVAL_AGENT.md`; CODER K7; PROSA L2 |
| **Aceite** | artefato reproduzível; false_act=0 na suite própria |

### A3 · T-P2P — Telepathy / sync evidência
| | |
|---|---|
| **Contrato** | 2+ SGDB nós → delta hits (sem fundir writers) |
| **Fio** | T-MCP/nsgdb → **T-P2P** → T-COH (conflito = contradicts) |
| **Disjuntor** | mesmo FileStorage com 2 writers = proibido |
| **Deps** | SGDB local + 2 nós (nenhum serviço externo) |
| **Aceite** | conflito preservado; 0 overwrite silencioso |

### A4 · T-CLIP — Clipboard sandbox
| | |
|---|---|
| **Contrato** | in: texto/diff → Goal/patch; out: CertifiedUtterance/texto |
| **Fio** | UI → **T-CLIP** → T-NL-IN ou T-CODE; saída pós T-COH |
| **Disjuntor** | path escape; segredo detectado → redact |
| **Deps** | A4; `CODER_AETHER.md` K1–K2 (vivos) |
| **Aceite** | 0 exfil fora do sandbox |

### A6 · T-GRAPH — Eventos → átomo / assinatura topológica
| | |
|---|---|
| **Contrato** | evento estruturado (IDs, timestamp, features) → átomo tipado (BAM) → RG 64→16→4 → assinatura/surpresa |
| **Fio** | **T-GRAPH** → T-MOE / T-PRED → T-COH → Action; claims → SGDB L6 |
| **Disjuntor** | schema break → reject; auth ≤120; feature `graph` off; texto livre não atravessa |
| **Deps** | nenhuma pendente: BAM (`host_transducer`), transplante E3 (JL), RG e SGDB já vivos |
| **Aceite** | **G-GRAPH** — AUC ≥ baseline TGN em dataset público; latência/evento no orçamento; 0 re-treino (`POSICIONAMENTO_AETHER.md` §5.6) |

### A5 · Hier MoE/LSH (paralelo, pós-rerank f32/bits)
| | |
|---|---|
| **Contrato** | roteamento hierárquico/LSH sobre o mesmo `Goal` → experts |
| **Disjuntor** | score < min_score → abstain (igual ao MoE plano) |
| **Deps** | T-EMB + rerank f32/bits (`B2_BITS_F32_RERANK.md`); benchmark próprio |
| **Aceite** | recall ≥ MoE plano com custo menor; 0 regressão no gate |

## 3. Classe B — decisão humana (código pronto)

### B1 · sanção A8 `enter_v1`
| | |
|---|---|
| **Contrato** | `PredictExpert` deixa de ser opt-in (`enter_v1_default=true`) |
| **Deps** | `aether_a8_v1_criteria.json` verde + token HITL |
| **Aceite** | false-act ≤ limiar do gate; audit trail do token |

### B2 · cutover T-DEC dual-bank
| | |
|---|---|
| **Contrato** | só bank `active` no bus; candidate nunca roteia |
| **Disjuntor** | `engine_unavailable` → stub/abstain tipado; nunca 255 |
| **Deps** | bank `active` promovida + HITL (independente de B1) |
| **Aceite** | 0 candidate no bus; degraded honesto |

### B3 · nova geração F4
| | |
|---|---|
| **Contrato** | dataset/era nova substitui a F4 congelada |
| **Deps** | T-EMB (era) + B1/B2 + decisão de dataset (não reescrever o congelado) |
| **Aceite** | 0 mistura de eras; gate F4 conforme doc |

## 4. Classe C — backend/modelo externo

### C1 · T-OCR — Documento → texto
| | |
|---|---|
| **Contrato** | `DocRef` → texto UTF-8 + conf + spans |
| **Fio** | **T-OCR** → T-NL-IN e/ou T-PACK (ETL) → core |
| **Disjuntor** | conf baixa → ask; PII deny; feature `ocr` off |
| **Estado hoje** | **vivo stub** — contrato `DocRef`→texto em `ocr.rs`; falta backend real atrás de `feature ocr` (off) |
| **Deps** | sidecar Python/Tesseract **ou** ramo de T-VIS (V2+); A1 para tabelas; não duplicar pixels no core |
| **Aceite** | mesmos gates de STT conf → não chutar Goal |

### C2 · T-STT — speech → text
| | |
|---|---|
| **Contrato** | áudio-ref → texto + conf + lang + privacy |
| **Disjuntor** | conf baixa → ask; feature `speech` off; PCM fora do T0 |
| **Deps** | backend local allowlisted; escada S0–S3 (`SPEECH_AETHER.md`) |
| **Aceite** | mesma regra de conf do OCR; 0 áudio no T0 |

### C3 · T-VIS — vision → struct
| | |
|---|---|
| **Contrato** | `ImageRef` → `VisionObs` (labels, entities, embed opcional, auth≤120) |
| **Disjuntor** | conf baixa → ask; feature `vision` off; pixels fora do T0 |
| **Deps** | backend de visão; `health(view=era)` do T-EMB se usar embed |
| **Aceite** | 0 mistura de dims/eras; 0 pixel no MoE |

### C4 · T-TTS — text → speech
| | |
|---|---|
| **Contrato** | só `CertifiedUtterance` → áudio |
| **Disjuntor** | abstain/sem fonte → “não sei”, nunca narração inventada |
| **Deps** | T-NL-OUT P2 (vivo) + backend TTS; escada S0–S2 |
| **Aceite** | paridade com banda verde/amarela/vermelha |

## 5. Classe D — hardware / mundo físico

### D1 · T-SENSOR — Host / IoT / CIM
| | |
|---|---|
| **Contrato** | `HostObs` / `SensorSample` tipado |
| **Fio** | **T-SENSOR** → T-PRED / T-MOE → Action → T-UI |
| **Disjuntor** | sampling storm; PII; sem allowlist de métricas |
| **Deps** | `host_probe` do hub (vivo); A4 se persistir |
| **Aceite** | orçamento de poll; abstain se sensor morto |

### D2 · T-HAPTIC — Render tátil
| | |
|---|---|
| **Contrato** | Action → padrão háptico (como TTS) |
| **Fio** | T-COH → **T-HAPTIC** (paralelo a T-TTS/T-UI) |
| **Disjuntor** | abstain factual → padrão “não sei”, não “sucesso” |
| **Deps** | mesma regra Certified* |
| **Aceite** | parity com banda verde/amarela/vermelha |

### D3 · T-AR — Overlay / espacial (opcional)
| | |
|---|---|
| **Contrato** | Action + pose → âncoras AR |
| **Fio** | T-VIS + T-COH → **T-AR** |
| **Disjuntor** | sem `VisionObs` → off |
| **Deps** | C3 (T-VIS) + T-COH |
| **Aceite** | não inventa âncora sem evidência |

### D4 · T-ROBOT — Ação física tipada
| | |
|---|---|
| **Contrato** | `RobotCmd` → `RobotFeedback` + certificado de execução |
| **Fio** | T-PLAN → **T-ROBOT** → T-COH; HITL se risco ≥ médio |
| **Disjuntor** | sem HITL em risco alto; kill switch |
| **Deps** | D1 (sensor) + T-PRED + T-HITL duro; nunca shell genérico |
| **Aceite** | 0 cmd sem `SuccessCriteria`; rollback ou safe-stop |

## 6. Mapa de dependências (profundidade crescente)

```text
A1 T-TAB ──► A2 T-EVAL
   └──────► C1 T-OCR ──► pack/ETL
A3 T-P2P   (SGDB + 2 nós)
A4 T-CLIP  (A4 + K1–K2)
A5 Hier    (T-EMB + rerank f32/bits) · paralelo

B1 A8      (gate + token) ────► B3 F4
B2 T-DEC   (bank active + HITL) ─┘

C2 T-STT ─┐
C3 T-VIS ─┼─► T-NL-IN ──► T-MOE ──► Action
C4 T-TTS ◄─ T-NL-OUT (vivo) ◄─ Action triada

D1 SENSOR ──► T-PRED ──► D4 ROBOT (HITL duro)
D2 HAPTIC ◄─ Action
C3 VIS ────► D3 AR ◄─ T-COH
```

Não pular: **A1** antes de C1 (tabelas de documento); **B1/B2** antes de B3;
**C3** antes de D3; **D1 + T-PRED + T-HITL** antes de D4.

## 7. Non-goals (não viram tomada)

- Tokenizer / next-token no T0  
- Dois writers no mesmo FileStorage  
- Shell/POST livres  
- Câmera/mic sempre-on sem privacy  
- Auto-255 por convergência de modelo  
- Tomada sem struct tipado (“bytes pro MoE”)  
- Pular classe por pressa: externo antes de código pronto, hardware antes de backend

## 8. Checklist ao promover uma tomada futura → viva

1. Entrar em [`CORE_TOMADAS.md`](CORE_TOMADAS.md) inventário (estado).  
2. Escada própria ou seção na escada mãe (P/A/K/M/V/S).  
3. Feature flag default **off**.  
4. Disjuntor + teste vermelho (OOD/abstain).  
5. Fato nsgdb + trigger path.  
6. Hub: cor na banda de triagem se afetar Action.  

## 9. Índice cruzado

| Tomada | Classe | Fase | Doc âncora |
|---|---|---|---|
| T-TAB | A (só código) | A1 | [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) |
| T-EVAL | A (só código) | A2 | [`EVAL_AGENT.md`](EVAL_AGENT.md), CODER K7, PROSA L2 |
| T-P2P | A (só código) | A3 | [`SGDB_INTEGRATION.md`](SGDB_INTEGRATION.md) |
| T-CLIP | A (só código) | A4 | [`CODER_AETHER.md`](CODER_AETHER.md) K1–K2 |
| T-GRAPH | A (só código) | A6 | [`POSICIONAMENTO_AETHER.md`](POSICIONAMENTO_AETHER.md) §5.6 |
| Hier MoE/LSH | A (só código, paralelo) | A5 | [`B2_BITS_F32_RERANK.md`](B2_BITS_F32_RERANK.md) |
| A8 `enter_v1` | B (sanção) | B1 | [`F4_NOVA_GERACAO_SANCAO_255.md`](F4_NOVA_GERACAO_SANCAO_255.md) |
| T-DEC cutover | B (sanção) | B2 | [`T_DEC_UPGRADE.md`](T_DEC_UPGRADE.md) |
| F4 nova geração | B (sanção) | B3 | [`F4_CLOSE_V3_HITL.md`](F4_CLOSE_V3_HITL.md) |
| T-OCR | C (externo) | C1 | [`VISAO_AETHER.md`](VISAO_AETHER.md) V2+ |
| T-STT, T-TTS | C (externo) | C2, C4 | [`SPEECH_AETHER.md`](SPEECH_AETHER.md) |
| T-VIS | C (externo) | C3 | [`VISAO_AETHER.md`](VISAO_AETHER.md) |
| T-SENSOR, T-ROBOT | D (hardware) | D1, D4 | hub probe, [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) A7 |
| T-HAPTIC, T-AR | D (hardware) | D2, D3 | SPEECH (parity), VISAO V4+ |

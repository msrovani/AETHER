# Tomadas futuras — próximo · médio · distante

> Inventário de **plugues ainda não vivos**, já desenhados no mesmo
> barramento do core. Contrato-mãe: [`CORE_TOMADAS.md`](CORE_TOMADAS.md).
> Escadas vivas/reservadas: prosa, agêntica, coder, MoE-ETL, visão, speech.
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

Horizontes:

| Horizonte | Significado |
|---|---|
| **Próximo** | fecha fiação do que já existe pela metade; desbloqueia A4/P1/packs |
| **Médio** | novo valor de produto; feature flag; escada curta própria |
| **Distante** | pesquisa / hardware / mundo físico; só após médio verde |

---

## 1. Próximo — fechar o quadro atual

### T-DEC — DecisionPort / Vega
| | |
|---|---|
| **Contrato** | `Goal`/pergunta → veredito abstain/route/choice + conformal |
| **Fio** | T-NL-IN → **T-DEC** → T-MOE/T-PLAN (AND com experts) → T-COH → T-UI/TTS |
| **Disjuntor** | `engine_unavailable` → stub/abstain tipado (nunca panic) |
| **Estado hoje** | **vivo** — inventariado em CORE_TOMADAS; hub `dec_view`; `--vega` / `AETHER_VEGA` |
| **Deps** | F1 DecisionPort; não bloqueia T0 sem feature |
| **Aceite** | degraded honesto; false-act confiante = 0 nos fixtures |

### T-EMB — Embedder (era declarada)
| | |
|---|---|
| **Contrato** | texto|visão → `Vec<f32>` + `model_id` / dim |
| **Fio** | T-NL-IN / T-VIS / T-PACK → **T-EMB** → SGDB L4 / recall hybrid → T-MOE |
| **Disjuntor** | dim/era nova sem `health(view=era)` → lexical only |
| **Estado hoje** | **formalizado** — `src/agent/embed.rs` (`EmbedPort` off); F4 **congelada** |
| **Deps** | ADR-0008; `DONOR_CATALOG` mapa |
| **Aceite** | 0 mistura de eras; gate B-G0/F4 conforme doc |

### T-MCP — Tools MCP allowlist
| | |
|---|---|
| **Contrato** | discover schema → call tipada → `ToolOutcome` |
| **Fio** | T-PLAN → **T-MCP** → (nsgdb / peers) → T-COH |
| **Disjuntor** | nome fora allowlist; args inválidos |
| **Estado hoje** | **vivo** — `mcp.call` tipado (`memory.recall`/`write`/`sgdb.context`); G-A4 verde |
| **Deps** | [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) A4 / G-A4 |
| **Aceite** | 0 call fora da lista; trace_id em toda call |

### T-HITL — Canal humano (approve / deny / ampliar)
| | |
|---|---|
| **Contrato** | `HitlRequest` → `HitlDecision` (auth, ampliar pesquisa, merge) |
| **Fio** | T-AUTH / T-NET / T-CODE → **T-HITL** → promove 255 ou aborta |
| **Disjuntor** | timeout → abstain (não auto-255) |
| **Estado hoje** | **vivo** — `HitlRequest`/`HitlDecision` em `src/agent/hitl.rs`; timeout→abstain; 255 só com token |
| **Deps** | F2 ampliar; ADR autoridade |
| **Aceite** | 0 promoção silenciosa; audit trail |

**Ordem próximo sugerida:** T-HITL inventário → T-MCP/A4 → T-EMB formal → T-DEC no diagrama hub.

---

## 2. Médio — novas capacidades de produto

### T-PRED — Expert preditivo (série / mundo)
| | |
|---|---|
| **Contrato** | série tipada → forecast + intervalo + certificado |
| **Fio** | sensor/API → **T-PRED** (expert MoE) → T-COH → T-NL-OUT/TTS |
| **Disjuntor** | OOD série → abstain; sem cert → cite only |
| **Escada** | reservatório / Koopman (ROADMAP P5); E8 já medido em lab |
| **Deps** | T-MOE M3 expert novo; [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) |
| **Aceite** | gate NRMSE/calibração no artefato; false-act ≤ limiar |

### T-OCR — Documento → texto
| | |
|---|---|
| **Contrato** | `DocRef` → texto UTF-8 + conf + spans |
| **Fio** | **T-OCR** → T-NL-IN e/ou T-PACK (ETL) → core |
| **Disjuntor** | conf baixa → ask; PII deny |
| **Nota** | pode ser ramo de T-VIS (V*); não duplicar pixels no core |
| **Deps** | [`VISAO_AETHER.md`](VISAO_AETHER.md) V2+ ou sidecar Python |
| **Aceite** | mesmo gates de STT conf → não chutar Goal |

### T-TAB — Tabela / CSV → claims
| | |
|---|---|
| **Contrato** | planilha → `Claim[]` (s,r,v) + schema |
| **Fio** | **T-TAB** → T-PACK / memory.write → T-MOE |
| **Disjuntor** | schema break → reject; auth ≤120 |
| **Deps** | MoE ETL; `facts.jsonl` canônico |
| **Aceite** | round-trip N linhas; OOD coluna → abstain |

### T-P2P — Telepathy / sync evidência
| | |
|---|---|
| **Contrato** | 2+ SGDB nós → delta hits (sem fundir writers) |
| **Fio** | T-MCP/nsgdb → **T-P2P** → T-COH (conflito = contradicts) |
| **Disjuntor** | mesmo FileStorage 2 writers = proibido |
| **Deps** | doctrine nsgdb p2p; escopo nunca vaza |
| **Aceite** | conflito preservado; 0 overwrite silencioso |

### T-EVAL — Placar T0–T3
| | |
|---|---|
| **Contrato** | suite → relatório JSON (seed, host, gates) |
| **Fio** | CI/hub → **T-EVAL** → T-UI (não alimenta act sozinho) |
| **Disjuntor** | misturar T0 com HumanEval sem gerador = N/A |
| **Deps** | [`EVAL_AGENT.md`](EVAL_AGENT.md); CODER K7; PROSA L2 |
| **Aceite** | artefato reproduzível; false_act=0 na suite própria |

### T-CLIP — Clipboard sandbox
| | |
|---|---|
| **Contrato** | in: texto/diff → Goal/patch; out: CertifiedUtterance/texto |
| **Fio** | UI → **T-CLIP** → T-NL-IN ou T-CODE; saída pós T-COH |
| **Disjuntor** | path escape; segredo detectado → redact |
| **Deps** | A4; [`CODER_AETHER.md`](CODER_AETHER.md) K1–K2 |
| **Aceite** | 0 exfil fora sandbox |

**Ordem médio sugerida:** T-TAB/OCR (pack) → T-PRED expert → T-EVAL hub → T-P2P → T-CLIP.

---

## 3. Distante — mundo físico e render exótico

### T-SENSOR — Host / IoT / CIM
| | |
|---|---|
| **Contrato** | `HostObs` / `SensorSample` tipado (já há cheiro no hub probe) |
| **Fio** | **T-SENSOR** → T-PRED / T-MOE → Action → T-UI |
| **Disjuntor** | sampling storm; PII; sem allowlist de métricas |
| **Deps** | dashboard host_probe; A4 se persistir |
| **Aceite** | orçamento de poll; abstain se sensor morto |

### T-ROBOT — Ação física tipada
| | |
|---|---|
| **Contrato** | `RobotCmd` → `RobotFeedback` + certificado de execução |
| **Fio** | T-PLAN → **T-ROBOT** → T-COH; HITL se risco ≥ médio |
| **Disjuntor** | sem HITL em risco alto; kill switch |
| **Deps** | A7; nunca shell genérico |
| **Aceite** | 0 cmd sem SuccessCriteria; rollback ou safe-stop |

### T-HAPTIC — Render tátil
| | |
|---|---|
| **Contrato** | Action → padrão háptico (como TTS) |
| **Fio** | T-COH → **T-HAPTIC** (paralelo a T-TTS/T-UI) |
| **Disjuntor** | abstain factual → padrão “não sei”, não “sucesso” |
| **Deps** | mesma regra Certified* |
| **Aceite** | parity com banda verde/amarelo/vermelho |

### T-AR — Overlay / espacial (opcional)
| | |
|---|---|
| **Contrato** | Action + pose → âncoras AR |
| **Fio** | T-VIS + T-COH → **T-AR** |
| **Disjuntor** | sem VisionObs → off |
| **Deps** | VISAO V4+; distante |
| **Aceite** | não inventa âncora sem evidência |

**Ordem distante:** T-SENSOR (já parcial) → T-HAPTIC (barato) → T-ROBOT → T-AR.

---

## 4. Mapa de dependências (visão única)

```text
PRÓXIMO                         MÉDIO                         DISTANTE
T-HITL ─┐                       T-OCR ──┐                     T-SENSOR ─┐
T-MCP  ─┼─ A4 / G-A4            T-TAB ──┼─ T-PACK / MoE       T-HAPTIC ─┼─ render
T-EMB  ─┼─ recall/era           T-PRED ─┼─ T-MOE expert       T-ROBOT ──┼─ HITL duro
T-DEC  ─┘  AND Vega             T-EVAL ─┼─ T-UI               T-AR ─────┘  VIS+
                                T-P2P  ─┼─ SGDB
                                T-CLIP ─┘  CODE/UI

ENTRADAS já reservadas: T-STT T-TTS (SPEECH) · T-VIS (VISAO)
SAÍDAS vivas: T-UI · (T-NL-OUT parcial)
```

Não pular: **médio OCR/TAB** alimenta pack **antes** de robot; **T-HITL**
antes de T-ROBOT; **T-EMB** antes de hybrid VIS/texto.

---

## 5. Non-goals (não viram tomada)

- Tokenizer / next-token no T0  
- Dois writers no mesmo FileStorage  
- Shell/POST livres  
- Câmera/mic sempre-on sem privacy  
- Auto-255 por convergência de modelo  
- Tomada sem struct tipado (“bytes pro MoE”)  

---

## 6. Checklist ao promover uma tomada futura → viva

1. Entrar em [`CORE_TOMADAS.md`](CORE_TOMADAS.md) inventário (estado).  
2. Escada própria ou seção na escada mãe (P/A/K/M/V/S).  
3. Feature flag default **off**.  
4. Disjuntor + teste vermelho (OOD/abstain).  
5. Fato nsgdb + trigger path.  
6. Hub: cor na banda de triagem se afetar Action.  

---

## 7. Índice cruzado

| Tomada | Horizonte | Doc âncora |
|---|---|---|
| T-DEC, T-EMB, T-MCP, T-HITL | próximo | CORE_TOMADAS, F1, F4, ACOES, F2 |
| T-PRED, T-OCR, T-TAB, T-P2P, T-EVAL, T-CLIP | médio | MOE_ETL, VISAO, EVAL, CODER |
| T-SENSOR, T-ROBOT, T-HAPTIC, T-AR | distante | hub probe, ACOES A7 |
| T-STT, T-TTS | reservada (escada) | [`SPEECH_AETHER.md`](SPEECH_AETHER.md) |
| T-VIS | reservada (escada) | [`VISAO_AETHER.md`](VISAO_AETHER.md) |

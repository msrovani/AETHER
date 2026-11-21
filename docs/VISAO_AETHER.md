# Visão — escada até `T-VIS` (sem pixels no core)

> Como o AETHER adquire **visão** sem o núcleo processar imagem.
> Contrato: tomada **T-VIS** em [`CORE_TOMADAS.md`](CORE_TOMADAS.md) —
> `ImageRef` → `VisionObs` tipado → `Goal` / `Claim` / entities.
> JEPA/world-model: pistas em [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) §3.9.

## 1. Posicionamento

| Core **faz** | Borda **T-VIS** faz |
|---|---|
| Roteia MoE, certifica auth, abstem | Decodifica pixels → struct |
| Recall por entities / pack | Labels, boxes, embed opcional |
| act/cite com evidência | Nunca escreve Action sozinha |

```text
câmera / arquivo
    │  T-VIS (feature vision, default OFF)
    ▼
VisionObs { labels, entities, embed?, auth_cap≤120 }
    │
    ├─► T-NL-IN / Goal tipado     (ex.: "que objeto é?")
    ├─► T-MOE → transplant/memory (entities = pack)
    ├─► T-NET                     (corroborar rótulo na web, opt-in)
    └─► T-COH → Action
              │
              ├─► T-NL-OUT / P*   (descrever com fonte)
              └─► T-TTS           (só CertifiedUtterance)
```

**Invariantes**

1. Pixels / tensors de visão **não** entram no T0 (60 Hz).
2. `VisionObs.auth_cap` ≤ 120; 255 só HITL.
3. Sem `VisionPort` → T-VIS no-op (disjuntor modalidade).
4. Embed de visão não mistura dims/era do SGDB sem `health(view=era)`.
5. Visão não libera `act` sozinha — precisa A\* / critério verificável.

## 2. Escada V0→V8

| Passo | Entrega | Ligação às tomadas | Gate / aceite |
|---|---|---|---|
| **V0** | Spec `VisionPort` / `VisionObs` + feature `vision` off | T-VIS reservada | compile; default off |
| **V1** | Stub: ImageRef → labels fixos de fixture | T-VIS → Goal de teste | selftest offline |
| **V2** | Classificador local allowlisted (ex. ONNX) → labels+conf | T-VIS → T-MOE/memory | conf baixa → abstain |
| **V3** | Entities tipadas (`kind/name`) p/ recall_entities | T-VIS → T-PACK/SGDB | 1-hop strings idênticas |
| **V4** | Bridge VisionObs → `language.parse` / Goal schema | T-VIS → T-NL-IN | OOV visual = abstain |
| **V5** | Corroborar rótulo via T-NET (opt-in) | T-VIS + T-NET → auth 80 se multi-fonte | waste=0 |
| **V6** | Embed visão (era dedicada) + recall hybrid | T-VIS embed ≠ texto sem era_report | health(era) ok |
| **V7** | World-model / JEPA-like (prever latente) | T-VIS → Concept; T-NL-OUT sob demanda | L3-visão: latência+calibração |
| **V8** | Visão no planner (tool `sense.vision`) | T-PLAN + A4 se gravar frame | G-A4; risco HITL |

Dependências: V4 usa P0 parser; V5 usa A5/T-NET; V8 usa A1–A4; descrever cena usa P1+.

## 3. Conexão com as outras escadas

| Escada | Papel com visão |
|---|---|
| [`CORE_TOMADAS.md`](CORE_TOMADAS.md) | inventário T-VIS + disjuntores |
| [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) | tool `sense.vision` no registry (V8); risk_ceiling |
| [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) | pack de **rótulos/objetos** (ETL) ancora entities |
| [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | P1 template: “vejo X *(fonte vision+…)*” |
| [`CODER_AETHER.md`](CODER_AETHER.md) | visão de UI/screenshot só via sandbox (K/A4) |
| [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) | VL-JEPA / V-JEPA como candidato V7 |
| Speech ([`SPEECH_AETHER.md`](SPEECH_AETHER.md)) | STT pede “o que é isto?”; TTS narra só pós-triagem |

Ordem mental: **V0–V3 struct** → **V4 Goal** → **MoE** → (NET/pack) → **Action** → prosa/TTS.

## 4. Contrato `VisionObs` (estável)

```rust
pub struct ImageRef {
    pub path_or_id: String,     // não bytes no core
    pub mime: Option<String>,
}

pub struct VisionObs {
    pub labels: Vec<(String, f32)>,
    pub entities: Vec<String>,  // "obj/caneca", "scene/cozinha"
    pub embed: Option<Vec<f32>>,
    pub model_id: Option<String>,
    pub auth_cap: u8,           // clamp ≤120
    pub privacy: PrivacyClass,  // local | redact | deny
}
```

Persistência: claims de visão → SGDB com `source=vision` (ou llm se captioner),
auth ≤120. Pack MoE pode listar entities canônicas do domínio.

## 5. Disjuntores específicos de visão

```text
feature vision off?           → no-op
ImageRef inválido / PII deny? → abstain
conf label < limiar?          → ask / abstain (não chutar Goal)
era embed conflito?           → lexical only + health(era)
Action sem outra evidência?   → cite vision only, auth baixa; act se critério ok
```

## 6. O que cada passo *não* é

| Passo | Não é |
|---|---|
| V1 stub | percepção real |
| V2 classificador | OCR/prosa livre de cena |
| V5 + web | verdade 255 |
| V7 JEPA | LLM multimodal no core |
| V8 tool | câmera sempre ligada / gravação sem HITL |

## 7. Ordem de implementação sugerida

1. **V0** — trait + feature flag em `CORE_TOMADAS` / crate (zero runtime).  
2. **V1** — fixture smoke no agent (Goal sintético).  
3. **V2–V3** — um modelo local + entities alinhadas a um pack MoE piloto.  
4. **V4** — fio até T-NL-IN / planner.  
5. **V5** — só com rede opt-in.  
6. **V6–V7** — pesquisa; não bloqueia produto.  
7. **V8** — depois de A4.

## 8. Critério de “temos visão”

- [ ] Feature `vision` off por default; on explícito.
- [ ] Core nunca recebe pixmap; só `VisionObs`.
- [ ] OOD / baixa conf → abstain ≥90% nos fixtures.
- [ ] Claim visual auth ≤120; multi-fonte (V5) pode 80–120.
- [ ] TTS/UI só descrevem após T-COH (mesma regra de CertifiedUtterance).

## 9. Refs

- [`CORE_TOMADAS.md`](CORE_TOMADAS.md) §5.3 T-VIS  
- V-JEPA 2 / VL-JEPA — `LANGUAGE_PORT.md` §3.9  
- Escadas A/P/M/K — aderência §3 deste doc  
- Doação de rótulos — `MOE_ETL_PACK.md` / `DONOR_CATALOG.md`  

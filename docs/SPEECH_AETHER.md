# Speech (STT ↔ TTS) — escada até `T-STT` / `T-TTS`

> Como o AETHER adquire **fala** sem o núcleo processar PCM.
> Contratos: tomadas **T-STT** e **T-TTS** em [`CORE_TOMADAS.md`](CORE_TOMADAS.md).
> Mesmo barramento: áudio → texto/Goal → core → Action → texto certificado → áudio.

## 1. Posicionamento

| Core **faz** | Borda speech **faz** |
|---|---|
| Parse Goal, MoE, certifica, abstem | STT: PCM → texto / conf |
| Decide act/cite/abstain | TTS: só `CertifiedUtterance` → áudio |
| Nunca “ouve” nem “fala” no T0 | Feature `speech` **off** por default |

```text
microfone
    │  T-STT (feature speech)
    ▼
SttResult { text, conf, lang, privacy }
    │
    └─► T-NL-IN (parser) ──► T-MOE / T-PLAN / T-NET / T-PACK …
                                    │
                                    ▼
                              T-COH → Action
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
               T-NL-OUT         T-UI            T-TTS
               (prosa P*)                      CertifiedUtterance
                                                    │
                                                    ▼
                                                 alto-falante
```

**Invariantes**

1. PCM **não** entra no core / T0.
2. STT com `conf` baixa → ask/repetir, **não** chutar Goal.
3. TTS **nunca** narra abstain factual como se soubesse; sem fontes → silêncio/beep.
4. Auth de claims gerados/pesquisa continua ≤120; fala não promove 255.
5. Privacy: `local | redact | deny` — deny corta a tomada.

## 2. Escada S0→S8

| Passo | Entrega | Tomadas | Gate / aceite |
|---|---|---|---|
| **S0** | Spec `SttPort` / `TtsPort` + feature `speech` off | T-STT, T-TTS reservadas | compile; default off |
| **S1** | Stub STT: fixture WAV → texto fixo | T-STT → T-NL-IN | selftest offline |
| **S2** | Stub TTS: CertifiedUtterance → WAV silêncio/beep | T-TTS | abstain → beep “não sei” |
| **S3** | STT local allowlisted (ex. whisper.cpp / onnx) | T-STT | conf &lt; limiar → ask |
| **S4** | Fio STT→parser C1→MoE (pedido por voz) | T-STT → T-NL-IN → T-MOE | OOV igual ao texto |
| **S5** | TTS local (ex. piper / onnx) só pós-triagem | T-COH → T-NL-OUT → T-TTS | 0 fala sem fonte em claim |
| **S6** | Duplex turn-taking (barge-in, VAD) | STT+TTS + UI | sem loop infinito; budget |
| **S7** | Idioma / PT-BR pack + privacy redact | STT lang + pack MoE | PII deny test |
| **S8** | Tool `sense.stt` / `render.tts` no planner | T-PLAN + A\* | risk_ceiling; HITL se gravar |

Dependências: S4 precisa P0/C1; S5 precisa P1 (template) ou T-NL-OUT mínimo; S8 precisa A1+.

## 3. Conexão com as demais escadas / tomadas

| Escada / doc | Papel |
|---|---|
| [`CORE_TOMADAS.md`](CORE_TOMADAS.md) | inventário + disjuntores §5.1–5.2 |
| [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) | T-NL-OUT gera o texto que o TTS pode falar (P1+) |
| [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) | voz = outra borda para D1; act ainda verificada |
| [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) | vocabulário/domínio melhora STT+transplant |
| [`VISAO_AETHER.md`](VISAO_AETHER.md) | “o que é isto?” (STT) + frame (VIS) → mesmo Goal |
| [`CODER_AETHER.md`](CODER_AETHER.md) | TTS não aplica patch; só pode **anunciar** resultado pós-K |
| [`F2_PESQUISA_EXTERNA.md`](F2_PESQUISA_EXTERNA.md) | pedido falado → T-NET; resposta falada só com evidência |
| [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md) | parse/generate na cadeia STT→…→TTS |

Ordem mental: **STT → NL-IN → core → NL-OUT → TTS**. Visão e speech são
paralelos na entrada; **uma** Action na saída.

## 4. Contratos estáveis

### 4.1 STT

```rust
pub struct SttResult {
    pub text: String,
    pub conf: f32,
    pub lang: Option<String>,
    pub privacy: PrivacyClass,  // local | redact | deny
}
```

### 4.2 TTS

```rust
pub struct CertifiedUtterance {
    pub text: String,           // de T-NL-OUT pós-triagem
    pub action: ActionKind,     // act | cite | propose | abstain
    pub auth: u8,
    pub sources: Vec<SourceId>,
}
```

Regra TTS:

| `action` | Comportamento |
|---|---|
| `act` / `cite` com sources | pode falar |
| `propose` | falar com marcador “hipótese” / auth baixa |
| `abstain` / deny | **não** afirma fato; prompt curto ou tom “não sei” |

## 5. Disjuntores (banda speech)

```text
feature speech off?              → no-op
privacy == deny?                 → corta STT
STT conf < limiar?               → ask (repetir), não Goal
parser OOV?                      → abstain (igual texto)
Action abstain + pedido factual? → TTS não inventa
rede off + precisava T-NET?      → deny honesto falado como “sem evidência”
```

Triagem hub (alinhada a CORE_TOMADAS): verde fala claim; amarelo “em
quarentena”; vermelho silêncio/beep.

## 6. O que cada passo *não* é

| Passo | Não é |
|---|---|
| S1–S2 stub | produto de voz |
| S3 STT | NLU — só texto |
| S5 TTS | chatbot falante sem evidência |
| S6 duplex | assistente sempre-a-escutar sem HITL/privacy |
| S8 tools | gravar conversas sem política |

## 7. Ordem de implementação sugerida

1. **S0** — traits + feature `speech` (zero runtime).  
2. **S1–S2** — stubs + teste: abstain → beep.  
3. **S3–S4** — STT local → parser (mesmo gates C1).  
4. **S5** — TTS só com CertifiedUtterance (depois P1).  
5. **S6–S7** — UX + PT-BR + privacy.  
6. **S8** — tools no agent após A1.

## 8. Critério de “temos speech”

- [ ] `speech` off por default.
- [ ] Core sem PCM.
- [ ] STT baixa conf → ask ≥ limiar nos fixtures.
- [ ] 0 narrações de fato sem `sources` (teste vermelho).
- [ ] Mesma Action serve UI texto e TTS (uma triagem).

## 9. Refs

- [`CORE_TOMADAS.md`](CORE_TOMADAS.md) §5.1–5.2  
- [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) — texto certificado  
- [`VISAO_AETHER.md`](VISAO_AETHER.md) — entrada multimodal paralela  
- [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) — autonomia não muda com voz  

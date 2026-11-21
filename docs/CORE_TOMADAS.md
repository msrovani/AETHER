# Core AETHER × tomadas (prosa, MoE, coder, agêntica, visão, speech)

> O **núcleo** não fala, não vê, não ouve e não programa. Ele **certifica,
> roteia, lembra e abstem**. Tudo que é modalidade ou geração entra por
> **tomada enumerada** (Eixo C: socket fora do inventário = bug).
>
> Escadas de produto: [`PROSA_FLUENTE.md`](PROSA_FLUENTE.md) ·
> [`ACOES_AGENTICAS.md`](ACOES_AGENTICAS.md) · [`CODER_AETHER.md`](CODER_AETHER.md) ·
> [`MOE_ETL_PACK.md`](MOE_ETL_PACK.md) · [`VISAO_AETHER.md`](VISAO_AETHER.md) ·
> [`SPEECH_AETHER.md`](SPEECH_AETHER.md). Linguagem: [`LANGUAGE_PORT.md`](LANGUAGE_PORT.md).
> Fila próximo/médio/distante: **[`TOMADAS_FUTURO.md`](TOMADAS_FUTURO.md)**.

## 1. O que é o core (e o que não é)

| Core **é** | Core **não é** |
|---|---|
| FHRR / fase / SAT / QUBO / Ising | Tokenizer / next-token |
| MoE EXP3 (rotear experts) | “Cérebro único” generativo |
| Coherence + auth + certificado | Verdade 255 sem HITL |
| SGDB hits tipados (evidência) | Prompt system a partir de memória |
| Planner/tools com allowlist | Shell/visão/áudio no hot path T0 |

```text
                    ┌──────────────────────────────────────┐
   modalidades      │         BORDAS / TOMADAS             │
   (texto, voz,     │  parse · generate · see · hear · …   │
    visão, ETL)     └──────────────┬───────────────────────┘
                                   │  Goal / Concept / Artifact tipados
                                   ▼
                    ┌──────────────────────────────────────┐
                    │              CORE AETHER               │
                    │  MoE → experts → coherence → Action    │
                    │  act | cite | propose | abstain        │
                    └──────────────┬───────────────────────┘
                                   │  claims + cert + auth≤120
                                   ▼
                    ┌──────────────────────────────────────┐
                    │     TOMADAS DE SAÍDA (render)         │
                    │  template · TTS · UI hub · patch …    │
                    └──────────────────────────────────────┘
```

**Regra:** o core só consome **estruturas** (`Goal`, `Claim`, `Concept`,
`Artifact`, `ToolOutcome`). Bytes brutos de áudio/imagem/texto livre
**param** na tomada, não atravessam o T0.

## 2. Inventário de tomadas (fiação)

Cada tomada tem: **ID**, direção, contrato, disjuntor (quando desarma),
escada de produto, estado.

### 2.1 Entrada (sensório / intenção)

| ID | Nome | Contrato | Disjuntor | Escada | Estado |
|---|---|---|---|---|---|
| **T-NL-IN** | `LanguagePort::parse` | texto → `Goal` + confidence | OOV / conf &lt;0.50 → abstain | P0, A0–A1 | **vivo** (C1) |
| **T-PACK** | ETL doação (Python) | corpus → `facts.jsonl` + manifest | licença / auth&gt;120 sem HITL | M1–M6 | **vivo** (playbook) |
| **T-NET** | `research.query` (F2/A5) | pergunta → claims tieradas auth≤120 | rede off; waste=0 | A5, P0/P2 | **vivo** (planner + chat; opt-in) |
| **T-STT** | speech→text | áudio → texto UTF-8 **ou** `Goal` | sem motor STT; PII; offline policy | — | **reservada** |
| **T-VIS** | vision→struct | imagem/frame → `VisionObs` tipado | sem motor visão; sem pixels no core | — | **reservada** |

### 2.2 Núcleo (sempre presentes)

| ID | Nome | Contrato | Disjuntor | Escada | Estado |
|---|---|---|---|---|---|
| **T-MOE** | `MoERouter` EXP3/UCB | `Goal` → top-k experts/tools | score &lt; min_score → abstain | M0, M5, A1–A3 | **vivo** |
| **T-XP-*** | experts (transplant, phase, logic, solver, memory) | task → `ExpertOutput` | evidência fraca → None/abstain | M1–M3 | **vivo** |
| **T-DEC** | DecisionPort / Vega | Goal → route/abstain + conformal | `engine_unavailable` → stub | F1/P6 | **vivo** (opt-in; hub `dec_view`) |
| **T-EMB** | EmbedPort | texto → `Vec<f32>`+era | dim/era mismatch; off→lexical | F4 | **vivo formal** (off; F4 congelada) |
| **T-PLAN** | planner + executor | Goal → PlanStep* + rollback | risk_ceiling / allowlist | A0–A7 | **vivo** (mundo A4) |
| **T-COH** | coherence | claims → Action | contradição → abstain/cite | — | **vivo** |
| **T-AUTH** | autoridade / HITL | claim auth 60…255 | ≥200/255 só HITL; LLM≤120 | todas | **vivo** |
| **T-HITL** | `HitlRequest`→`HitlDecision` | promote/deny/ampliar + audit | timeout→abstain; 255 sem token | A7 | **vivo** (`hitl.rs`) |

### 2.3 Saída (atuação / render)

| ID | Nome | Contrato | Disjuntor | Escada | Estado |
|---|---|---|---|---|---|
| **T-NL-OUT** | `LanguagePort::generate` / template | Concept/claims → texto + cite auth | sem evidência → Empty | P1–P6 | **vivo (P1)** |
| **T-CODE** | `code.*` …/lm/eval + `shell.exec` | Goal→sandbox→verify/patch; T3; shell HITL | falso merge; G-A4; auth≤120; shell never default | K0–K8 | **vivo K1–K8** (`agent_coder_gate`) |
| **T-FS** | `fs.read` / `fs.write` | path relativo → bytes sob `sandbox_root` | traversal / escape | A4 | **vivo** (`tests/agent_safety_gate`) |
| **T-HTTP** | `http.get` | URL → body/claim; GET only | host fora allowlist; POST | A4 | **vivo** (fetch opt-in) |
| **T-MCP** | `mcp.call` | nome allowlist → `ToolOutcome` | nome fora; args inválidos | A4 | **vivo** (proxy tipado) |
| **T-TTS** | text→speech | texto certificado → áudio | falar claim sem fonte; auth&lt;limiar | — | **reservada** |
| **T-UI** | hub / API | Action → tela/SSE | — | produto hub | **vivo** |

## 3. Disjuntores (banda de triagem)

Ordem de corte — o primeiro que desarma vence:

```text
1. Eixo C     tomada não inventariada?     → BUG (não executa)
2. Rede       AETHER_RESEARCH_NETWORK≠1? → T-NET off
3. Parser     OOV / conf baixa?            → abstain / ask
4. MoE/EFE    score < min_score?           → abstain
5. Coherence  contradição / empate auth? → abstain | cite
6. Risco      > risk_ceiling?              → HITL (não call)
7. Auth       gerado/pesquisa              → teto 120 (nunca 255 auto)
8. Sandbox    path/tool fora allowlist?  → deny (G-A4)
9. Modalidade STT/VIS/TTS sem backend?   → tomada no-op + log
```

**Banda de triagem (produto):**

| Faixa | Cor (hub) | Significado |
|---|---|---|
| Verde | act / cite com fonte | evidência ok, risco baixo |
| Amarelo | propose / auth 60–120 | quarentena; humano pode promover |
| Vermelho | abstain / deny / HITL | OOD, risco, contradição, tomada off |

## 4. Interconexão das escadas (aderência)

```text
     T-STT ──┐
     T-VIS ──┼──► T-NL-IN (ou Goal direto) ──► T-MOE ──► T-XP-* / T-PLAN
     T-NL-IN─┘         ▲                         │
                       │                         ├─► T-NET (A5) ──► claims
     T-PACK (M*) ──────┘                         ├─► T-CODE (K*) ──► patch
                                                 ├─► T-FS/HTTP (A4)
                                                 └─► T-COH ──► Action
                                                          │
                          T-NL-OUT (P*) ◄─────────────────┤
                          T-TTS      ◄─────────────────────┤  (só texto já triado)
                          T-UI       ◄─────────────────────┘
```

Dependências duras (não pular):

| Quer | Precisa antes |
|---|---|
| P1 prosa útil | pack e/ou T-NET com evidência |
| K0 coder | A4 (T-FS) |
| K3 generate | T-NL-OUT/code LM + K0 |
| M8 pack age no mundo | A4 + M4 tool |
| T-TTS falar | Action cite/act com fonte (nunca abstain→fala inventada) |
| T-VIS no planner | `VisionObs` tipado → Goal/Claim; pixels não entram no MoE |

## 5. Visão e speech — tomadas reservadas (prontas para plugar)

Sem implementar motores agora; **contrato estável** para não refazer a fiação.

### 5.1 `T-STT` (speech → text)

Escada de produto S0→S8: **[`SPEECH_AETHER.md`](SPEECH_AETHER.md)**.

```rust
// contrato alvo (borda)
pub struct SttPort {
    // audio_pcm_or_path → Result<SttResult>
}
pub struct SttResult {
    pub text: String,           // entra em T-NL-IN
    pub conf: f32,
    pub lang: Option<String>,
    pub privacy: PrivacyClass,  // local | redact | deny
}
```

Disjuntor: `conf` baixa → pedir repetir (ask), não chutar Goal.  
Default produto: **off** até existir backend local allowlisted.

### 5.2 `T-TTS` (text → speech)

Mesma escada: **[`SPEECH_AETHER.md`](SPEECH_AETHER.md)** (S2/S5).

```rust
pub struct TtsPort {
    // CertifiedUtterance → audio
}
pub struct CertifiedUtterance {
    pub text: String,           // só de T-NL-OUT pós-triagem
    pub action: ActionKind,     // act|cite|… — abstain não fala fato
    pub auth: u8,
    pub sources: Vec<SourceId>,
}
```

Disjuntor: `action == abstain` ou `sources.is_empty()` em claim factual →
silêncio ou beep “não sei”, **nunca** narração inventada.

### 5.3 `T-VIS` (vision → struct)

Escada de produto V0→V8: **[`VISAO_AETHER.md`](VISAO_AETHER.md)**.

```rust
pub struct VisionPort {
    // ImageRef → Result<VisionObs>
}
pub struct VisionObs {
    pub labels: Vec<(String, f32)>,
    pub entities: Vec<EntityRef>,  // para recall_entities / Goal
    pub embed: Option<Vec<f32>>, // só se era embedder allowlisted
    pub auth_cap: u8,            // ≤120
}
```

Disjuntor: sem VisionPort → T-VIS no-op. Embeddings de visão **não** misturam
dims com era do SGDB sem `health(view=era)`.

## 6. Mapa mental “um fio por função”

| Capacidade humana | Tomada | Core faz |
|---|---|---|
| Entender pedido | T-NL-IN / T-STT | valida Goal |
| Saber domínio (Trinity) | T-PACK → transplant | MoE roteia |
| Pesquisar | T-NET | certifica auth |
| Decidir/agir | T-MOE + T-PLAN | act/abstain |
| Explicar | T-NL-OUT | template/generate |
| Falar | T-TTS | render pós-triagem |
| Ver | T-VIS | struct → Goal/claim |
| Programar | T-CODE | verify+sandbox |

## 7. Checklist de aderência (docs)

- [x] Prosa não libera `act` sozinha → ACOES  
- [x] Coder exige A4 → ACOES  
- [x] Pack ≠ coder → MOE_ETL + CODER  
- [x] Pack ancora prosa P1 → MOE + PROSA  
- [x] Pesquisa opt-in → T-NET / F2  
- [x] Visão/speech = tomadas reservadas, mesmo barramento Goal/Action  
- [x] Core sem token / sem pixel / sem PCM no T0  

## 8. Próximos plugues (ordem)

1. Manter T-NL-IN + T-MOE + T-NET estáveis (já medidos).  
2. ~~A4 / G-A4~~ **fechado** (`fs`/`http.get`/`mcp.call` + `agent_safety_gate`).  
3. P1 template → T-NL-OUT mínimo.  
4. Pack piloto (`MOE_ETL`) → alimenta transplant.  
5. Speech / visão — escadas reservadas; **zero** no default.  
6. Fila restante (T-HITL, T-EMB, T-PRED, robot…): [`TOMADAS_FUTURO.md`](TOMADAS_FUTURO.md).

## 9. Memória / recall

Entities: `aether/agent`, `doc/architecture`, `mom/constraint`, `mom/pattern`.  
Trigger sugerido: `docs/CORE_TOMADAS.md`, `src/agent/**`.

# Plano FINAL — otimização (arxiv × AETHER)

**Status:** TRAVADO pós-votação · ciclo **0.0.171** (A0–A5 CURSOR; A2 GPU / A3 OPENCODE em curso)  
**Forum:** MSG 0470 (mapa) · 0471 (voto OPENCODE) · 0474 (voto CURSOR) · 0473/0475/0476 (ACK final sem veto)  
**Doutrina:** *LLM gera; AETHER cita, certifica ou abstém.* Nada entra no crate sem gate. T3 N/A sem gerador.

## Quórum P1–P10

| Item | OPENCODE 0471 | CURSOR 0474 | Decisão |
|---|---|---|---|
| P1 Hopfield cleanup | SIM | SIM (onda B) | **SIM** |
| P2 NysHD kernel edge | SIM_PRIMEIRO | SIM (onda B pós A3) | **SIM** (opt-in) |
| P3 GHRR | SIM_DEPOIS | DEPOIS | **DEPOIS** |
| P4 VaCoAL BAM | SIM_DEPOIS | DEPOIS | **DEPOIS** |
| P5 SRMU relevance | SIM | SIM (onda B tarde) | **SIM** |
| P6 ToolTree poda | SIM_BARATO | SIM_BARATO (A5) | **SIM** |
| P7 Outcome/SelfModel | SIM | SIM (onda B) | **SIM** |
| P8 CIC abstain | SIM_PRIMEIRO | SIM_PRIMEIRO | **SIM** (OPENCODE 1º) |
| P9 Verify→200 | SIM | SIM (pós CIC) | **SIM** |
| P10 CASEL magnitude | DEPOIS | DEPOIS | **DEPOIS** |

Extras (CURSOR, ACK OPENCODE): **A1 embed · A2 GPU+seed/T0 · A3 B-G0 · A4 LSH/shards · C1 FFT V4** = SIM.

## Veredito (melhor ROI)

1. Destravar B2/B-G0 pelo **embed** (GPU + shards + seed/T0).  
2. **Abstain certificado** (P8 CIC) — OPENCODE já inicia.  
3. **Multi-probe LSH** + shards antes de 100k monolítico.  
4. **Poda de pré-condição** no planner (P6 leve).  
5. NysHD/Hopfield/SelfModel em **onda B** com gate próprio.  
Anti: destilar LLM · baixar teto PAWS · MCTS sem Δ≥10pp · claim alheio sem ACK.

---

## Onda A — destravar escala (CURSOR)

### A0. Runtime embed L1/L2 — CONCLUÍDO (0494/0495)
- **Dono:** CURSOR · adotado OPENCODE  
- **Ganho:** 1,56 → **10,82 txt/s** (~6,9×) · ETA 100k **2,57 h** < 4 h  
- **Código:** `tools/embed_runtime.py` — CPU fp32/B16 · GPU bf16/B64+FA2  
- **Lit:** ST efficiency — bf16 no CPU é anti-otimização  

### A1. Taxa embed limpa
- **Dono:** CURSOR · OPENCODE recalcula ETA  
- **Fazer:** `scripts/embed_rate_1k.py` ocioso → `target/embed_rate_1k.json`  
- **Gate:** textos/s + ETA_100k_h; ETA>4h → GPU batch obrigatório  
- **Aceite:** número limpo no forum

### A2. Sidecar embed GPU (3050) + fallback CPU
- **Dono:** CURSOR  
- **Contrato 0473/0476:** `seed` fixo + `temperatura=0` no embed (manifest SHA256 reprodutível / proveniência k6)  
- **Gate:** 1k GPU vs CPU; RSS≤6GB folga; razão ≥2× **ou** abstain + shard path  
- **Aceite:** artefato + post forum

### A3. B-G0 gate (OPENCODE) + round embedado
- **Dono gate:** OPENCODE (`tests/b0_parafra_gate.rs`)  
- **Dono embed round:** CURSOR/OPENCODE após A1/A2  
- **Dados:** `20261117_b0_paws` sha `58f9e5a6`  
- **Gate (0458):** recall@1 par ≥0,9 **e** rejeição não-par ≥0,9  
- **Lit:** PAWS [1904.01130](https://arxiv.org/abs/1904.01130)  
- **Aceite:** gate verde **ou** abstain honesto (não baixar teto)  
- **Nota 0476:** gate fica de pé até o round embedado landar

### A4. Multi-probe LSH + shards ≤4k (E1B) → B2
- **Dono:** CURSOR `HvBank` / `b2_100k_gate`  
- **Lit:** [1809.07471](https://arxiv.org/abs/1809.07471), [1310.4136](https://arxiv.org/abs/1310.4136)  
- **Lei E1B (OPENCODE 0520):** capacidade ≈ 0,4 × D → **4096** @ 10240 bits; 100k = **25×~4k**  
- **Gate:** n≥100k efetivo (união de shards ≤cap), LSH on no monolito/latência, p99≤5ms, RSS≤256MiB  
- **Piso recall@10≥0,90:** só dentro da capacidade; over-cap reporta sem inventar 0,90  
- **Aceite intermediário:** shard ≤4096 medido (`data/rounds/b2_shards/4k/`)

### A5. Planner — poda por pré-condição (P6)
- **Dono:** CURSOR se DEEPSEEK não claimar em 1 rodada (0473)  
- **Fazer:** excluir candidatos com pré-condição falsa dados os claims  
- **Gate:** `agent_mcts_experiment` — menos passos médios; budget 3 = 8/8; p95 select ≤+10%  
- **Lit:** NaviAgent / GAP / ToolTree

---

## Onda B — certificação e memória

### B1. CIC / abstain certificado (P8) — OPENCODE AGORA
- **Dono:** OPENCODE · arquivo próprio `tests/cic_abstain_gate.rs`  
- **Método (0476):** OOD + in-schema reais do b1/borda language; Wilson IC  
- **Invariante:** false_act = 0; false_abstain com IC declarado  
- **Artefato:** `target/cic_abstain.json` (n, taxas, IC, tetos)  
- **Nota:** tetos 0,49/0,20 são a medida atual; donos trocam pela regra CIC depois  
- **Lit:** [2607.04430](https://arxiv.org/abs/2607.04430)

### B2. SelfModel por passo (P7)
- **Dono:** DEEPSEEK (se claim)  
- **Gate:** `agent_self_model_gate` ECE por passo · EpisodeTrace  
- **Lit:** [2610.18614](https://arxiv.org/abs/2610.18614)

### B3. Hopfield / cleanup graduado (P1)
- **Dono:** M5 phase memory  
- **Gate:** e1b capacity + false-abstain  
- **Lit:** [2606.24948](https://arxiv.org/abs/2606.24948)

### B4. NysHD na borda (P2) — redirecionado (OPENCODE 0539 / CURSOR ACK)
- **Dono:** edge · OPENCODE+CURSOR  
- **Contrato fixado:** header `X-Aether-Edge` ∈ {`hash` (default), `kernel` (opt-in)}  
- **Papel:** recuperação de paráfrase (C-G0 recall), **não** discriminação de label PAWS  
- **Porquê:** AUC cosine≈0,49 (achado A3) — qualquer edge que preserve similaridade herda o sinal nulo; PAWS [1904.01130] falsifica overlap superficial  
- **Gate:** C-G0 paraphrase-recall; se <0,9/0,9 → abstain, hash permanece  
- **Label PAWS:** caminho E1 (`e1_paws_consistency`, OPENCODE) + fato `parafraseia` no corpus  
- **Lit:** [2608.06860](https://arxiv.org/abs/2608.06860)

### B5. Verify → promoção 200 (P9)
- **Após** B1 CIC · wire verify_tool (item 0409)  
- **Gate:** api_agent 14/14 estendido

### B6. SRMU relevance (P5) — tarde na onda B
- **Dono:** M5/M7 · gate capacidade stream longo

---

## Onda C — FFT V4 e DEPOIS

### C1. FFT V4 (OPENCODE)
- **Dono:** OPENCODE protótipo fora do repo / example próprio  
- **CURSOR:** merge só se ≥2× (critério 0459) senão **G2 ABSTAIN final** com números  
- **Invariante:** CPU rustfft = laço 60 Hz

### DEPOIS (sem lease agora)
- P3 GHRR · P4 VaCoAL · P10 CASEL — só com gate NC / HITL existente

---

## Divisão TRAVADA (0476)

| Item | CURSOR | OPENCODE | DEEPSEEK |
|---|---|---|---|
| A1 embed limpo | **act** | ETA | — |
| A2 GPU + seed/T0 | **act** | crosscheck | — |
| A3 B-G0 gate | embed round | **act** gate | — |
| A4 LSH/shards | **act** | revalida `target/open` | — |
| A5 poda | **act** se sem claim | estende exp. | claim 1 rodada |
| B1 CIC | cite | **act AGORA** | — |
| B2 SelfModel | — | — | **act** se claim |
| B3/B4 Hopfield/NysHD | co-design | gate próprio | — |
| B5 Verify 200 | cite | gate | — |
| C1 FFT V4 | merge se ≥2× | **act** protótipo | — |

**Cronograma:** OPENCODE = B1 agora; CURSOR = A1→A2→A4→A5 após ACK humano da Onda A; A3 gate espera round embedado; cada land CURSOR → OPENCODE revalida em `target/open`.

## Anti-objetivos

- Destilar LLM / GGUF no crate  
- Relaxar tetos PAWS para “ficar verde”  
- MCTS em árvore sem delta≥10pp medido  
- Editar claim alheio sem ACK  
- Embed sem seed/T0 (quebra proveniência)

## Aceite

1. [x] Voto OPENCODE 0471  
2. [x] Voto CURSOR 0474  
3. [x] ACK final OPENCODE 0476 (sem veto)  
4. [x] ACK humano Onda A (`continue` 2026-11-17)  
5. [x] OPENCODE B1 CIC (0478) · C1 FFT ABSTAIN (0481)  
6. [x] CURSOR A1 limpo · A2 path seed/T0 · A4 multiprobe + E1B shards≤4k · A5 poda  
7. [x] OPENCODE 0520: A3 AUC~acaso → E1 ordem; capacidade 0,4×D → B2=25×4k (CURSOR ACK 0521 / REL 0524)  
8. [ ] A2 GPU medida na 3050 (OPENCODE sequência 0487)  
9. [ ] A3/E1 parafrase após embed ordenado (PAWS) — não baixar teto

**Frase de fronteira:** LLM gera; AETHER cita, certifica ou abstém.

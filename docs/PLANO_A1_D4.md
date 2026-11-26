# Plano A1 → D4 — AETHER 0.5.6 → 0.6.x

Fila de [`TOMADAS_FUTURO.md`](TOMADAS_FUTURO.md) mantida (A→B→C→D, dependências
crescentes). Teto intocado: auth ≤ 120, A3 recall ≥ 0,90, F4 hit@5 ≥ 0,974.
Classe B só promove com token HITL. Nenhuma fase toca `src/knowledge/sgdb/` nem
`src/core/persistency_search.rs`.

**Regra por fase** — 1 contrato tipado + 1 gate vermelho→verde + 1 artefato em
`target/` + feature flag off. Sem exceção; sem "quase pronto".

## 0. Pesquisa — decisões travadas

| Assunto | Decisão | Por quê |
|---|---|---|
| Tabela→KG | topologia→schema→triplas, **sem modelo na A1** | SemTab/TabFact: o gargalo é schema, não geração |
| Eval | `run_id`, hash do dataset, tier, seed, host | linhagem estilo lm-evaluation-harness |
| Sync | oplog append-only Merkle-CRDT + vector clock | OrbitDB OPLOG, silk-graph, crdt-sync |
| OCR | PP-OCRv6 (Apache-2.0, CPU) → PaddleOCR-VL-1.6 (0,96B, GPU opt-in); Tesseract 5.5 fallback | spans+conf, roda offline |
| ASR | `nvidia/parakeet-tdt-0.6b-v3` (CC-BY-4.0, pt, int8 18–30×RT via sherpa-onnx); whisper.cpp turbo (MIT) fallback | melhor WER/pt em CPU |
| TTS | `hexgrad/Kokoro-82M` (Apache-2.0, pt-BR, ONNX ~300 MB); Piper só como subprocesso | engine GPL não linka no core |
| Visão | RF-DETR + Grounding DINO (Apache-2.0); SigLIP2 em era própria | YOLO26 (AGPL) e DINOv3 (termos Meta) fora |
| Embed/F4 | Qwen3-Embedding-0.6B (Apache-2.0, dims até 1024 = forma do bank bge-m3) | era candidata, sem tocar a v1 |
| Robô/AR/tátil | ISO 10218-1/2:2025 (reduced ≤250 mm/s, e-stop, single-point-of-control), ISO/TS 15066; OpenXR 1.1 §11.7 + Spatial Entities | contrato antes de hardware |

**Orçamento de dependências** — `arboard 3.7` já está no `Cargo.lock` (via egui),
então A4 não baixa nada; `csv/calamine/sha2/sysinfo` estão **ausentes**, por isso
A1 parseia CSV à mão e A3 ganha `src/core/sha256.rs` próprio. O gate roda
`cargo check --all-targets --offline`: dependência nova é custo real, não detalhe.

## 1. Classe A — só código

Cada ficha segue: **Tema** (tomada) · **Item/Subitem** (o que existe no código) ·
**Objetivo** · **GO** (medida que promove) · **NO-GO** (que reprova) · **Goal**
(artefato entregue).

### A1 · T-TAB — planilha/CSV → `Claim[]`
- **Tema:** T-TAB (classe A, dep 0) · **Item:** `src/agent/tab.rs` (vivo)
- **Subitem:** parser CSV/TSV RFC 4180 próprio (BOM, aspas duplas, CRLF, quebra interna, limite de 32 KiB/registro); `TabSchema` header→(subject,relation,value) + hash FNV-1a 64; coluna sem header/desconhecida→`Unmapped`; `claims_from_tab` emite `Claim` auth **96**, `source=Memory`, `provenance=Inferred`; `facts_jsonl` canônico (`object`←`value`); tool `tab` na sandbox; CLI `aether-agent tab`.
- **Objetivo:** transformar planilha em evidência tipada sem inventar linha.
- **GO:** round-trip de 1000 linhas idêntico, OOD de coluna→abstain 100%, 0 panic em 5 CSVs malformados, auth ≤ 120.
- **NO-GO:** linha descartada em silêncio, dependência nova, auth > 120.
- **Goal:** `tests/agent_tab_gate.rs` verde + `target/aether_tab_gate.json` no `tools/cycle_gate.py`.

### A2 · T-EVAL — placar T0–T3 reproduzível
- **Tema:** T-EVAL (classe A, dep 1) · **Item:** `src/agent/scorecard.rs` + `src/agent/eval.rs` + hub
- **Subitem:** runner de suite JSONL com `dataset_sha256` (SHA-256 próprio em `src/core/sha256.rs`), `run_id` determinístico, seed e host; nenhuma média cruza tiers (`T0SelfSuite` ≠ `T3External`); fixtures de claims alimentadas pela A1 (n ≥ 32); a aba Testes IA do hub lê `target/aether_agent_scorecard.json` (o placar cognitivo `aether_scorecard.json` fica intocado).
- **Objetivo:** qualquer número do placar é reproduzível por hash + comando.
- **GO:** 2 runs ⇒ taxas idênticas; `false_act=0`; T3 = N/A explícito sem gerador.
- **NO-GO:** média cruzando tiers, suite sem hash, relatório sem `run_id`.
- **Goal:** `aether-agent eval --suite bench/agent/suite.jsonl --report …` + `tests/agent_eval_report_gate.rs`.

### A3 · T-P2P — delta de evidência entre 2 nós
- **Tema:** T-P2P (classe A, dep 1) · **Item:** `src/core/sha256.rs` + `src/agent/p2p.rs`
- **Subitem:** SHA-256 próprio com vetores de teste conhecidos; `SgdbDelta{node,seq,vclock,entries}` append-only (Merkle-CRDT); `aether-sgdb sync --peer` aplica idempotente por `(scope,key)`; divergência de valor vira `contradicts`, nunca LWW.
- **Objetivo:** dois nós convergem sem fundir writers.
- **GO:** 2 nós convergem; conflito preservado 100%; 0 overwrite silencioso.
- **NO-GO:** mesmo `FileStorage` com 2 writers, LWW em conflito.
- **Goal:** `tests/agent_p2p_gate.rs` + `target/aether_p2p.json`.

### A4 · T-CLIP — entrada por clipboard sem shell livre
- **Tema:** T-CLIP (classe A, dep 1) · **Item:** `src/agent/clip.rs`
- **Subitem:** `ClipBackend` com stdin como default; OS só atrás de feature `clip` + `arboard 3.7`; redact de segredo forte (PEM, `sk-`, `ghp_`, `AKIA`, JWT) → deny do lote; hex32 **só com contexto** (`token=…`) e aí é redigido, não negado — md5/uuid colados num diff não podem custar o paste; diff→`code.patch` via `resolve_sandbox_path`; saída só pós-COH (`CertifiedUtterance`).
- **Objetivo:** colar código/dado sem abrir execução arbitrária.
- **GO:** 0 escape de path, 100% dos segredos redigidos/negados, 0 shell.
- **NO-GO:** clipboard direto no core, saída sem certificado.
- **Goal:** `tests/agent_clip_gate.rs`.

### A5 · Hier MoE/LSH (paralelo, pós-B2)
- **Tema:** T-HIER (classe A, dep T-EMB + rerank f32/bits) · **Item:** `src/agent/hier_moe.rs` (vivo)
- **Subitem:** LSH sobre o bank HV + re-rank f32 top-32 sobre os mesmos `Goal` → experts; `min_score` igual ao MoE plano.
- **Objetivo:** rotear mais barato sem perder recall.
- **GO:** recall@1 ≥ MoE plano com p99 menor (medido, não estimado).
- **NO-GO:** recall < plano; só liga com gate B2 ≥ 0,90.
- **Goal:** `target/aether_hier_moe_status.json` atualizado + gate.
- **Pré-condição do B2 cumprida (2026-11-24):** o probe das duas direções
  fechou o piso — doc `s2|rel|s1` dá **0,9170** de recall@1 (n=5000) contra
  0,7708 do `s1|rel|s2`, com bits+rerank igualando o teto f32 exato (0,9170).
  O gap era a assimetria de indexação, não o rerank
  (`docs/GATE_B2_BITS_F32_RERANK.md`, `data/b2_two_directions.json`). A5 segue
  off até o **próprio** gate medir recall@1 vs MoE plano e p99 — o bloqueio
  deixou de ser o B2.
- **Gate próprio medido (2026-11-24):** `tests/agent_hier_moe_gate.rs` roda LSH
  multi-probe + re-rank f32 top-32 contra o MoE plano no mesmo bank e na mesma
  verdade: **recall@1 0,7680 vs 0,7680** e **p99 174 µs vs 455 µs** (release,
  n=500, config **8×10×16**, pool ~988 átomos contra 5000). GO cumprido sem
  baixar teto. A config default do bank (4×16×8) **perde** recall (0,658) e não
  serve para rotear. A5 segue **off** no default: `may_enable` exige a feature
  `hier_moe` + `AETHER_HIER_MOE=1`, e `default_path` é sempre `false`.
  Artefato: `data/aether_hier_moe_status.json` (+`.meta.json`).
- **Duas direções medidas (2026-11-26):** `tools/a5_rev_round.py` projeta
  `emb/bank_rev.f32` com o **mesmo** `P` do manifest para `hv/bank_rev.bin` (autoverificação:
  reconstrói o bank fwd e exige igualdade bit a bit com `hv/bank.bin`, sha
  `20cd525e…`) e escreve `target/a5_rev_round/`; o mesmo gate mede as duas
  direções sobre a mesma verdade (n=5000, release, config **8×8×8**):
  fwd **0,7708 vs 0,7708** com p99 **335 µs vs 462 µs**; rev **0,9170 vs
  0,9170** com p99 **370 µs vs 477 µs**. O rev do A5 reproduz o probe do B2
  (0,9170) exatamente, então o número que faltava está medido no *caminho
  real*, não só no probe Python. `both_directions_ok=true`; `may_enable`
  continua `false` (feature/env) e `default_path` segue `false`: a medição
  autoriza, o flip do default é decisão separada.
- **Nota de honestidade (n=5000):** a config 8×10×16 perde 1 query no fwd
  (0,7706 < 0,7708) e é reprovada pelo `best_passing`; o gate cai para 8×8×8,
  que empata o plano. Em n=500 a 8×10×16 ainda passava (0,7680 = 0,7680) — o
  número antigo não foi baixado, foi substituído pela amostra maior.

## 2. Classe B — decisão humana (código pronto, token HITL)

### B1 · A8 `enter_v1`
- **Tema:** sanção A8 (classe B, dep `aether_a8_v1_criteria.json` verde + token) · **Item:** `src/agent/a8_v1.rs`
- **Subitem:** evidência dos 5 critérios (`agent_a8_gate`); `goal.propose` aprovado por HITL grava fato auth **255** `source=hitl`; flag `enter_v1_default=true` + revert de 1 comando.
- **Objetivo:** `PredictExpert` deixa de ser opt-in sem virar autoridade automática.
- **GO:** 5/5 critérios, `false_act ≤ gate`, audit trail do token no artefato.
- **NO-GO:** 255 automático, HITL sem fato, `hitl_sanction_255=false` no critério.
- **Goal:** `target/aether_a8_v1_criteria.json` verde.

### B2 · Cutover T-DEC dual-bank
- **Tema:** T-DEC (classe B, dep bank `active` + HITL) · **Item:** `src/agent/dec_upgrade.rs`
- **Subitem:** cutover amarra `scribe_path` ao sentinel; teste afirma 0 candidate no bus; `previous/` = rollback de 1 clique; `engine_unavailable`→stub/abstain tipado.
- **Objetivo:** só o bank `active` roteia.
- **GO:** smoke com 4 lâmpadas verdes, `degraded=false`, 0 candidate no bus.
- **NO-GO:** cutover com 1 carimbo, candidate roteando, 255 em falha de engine.
- **Goal:** `target/aether_dec_upgrade.json` + lamp smoke.

### B3 · F4 era nova
- **Tema:** F4 (classe B, dep dataset v2 congelado) · **Item:** `data/embed_eval_set_v2.jsonl` (865 linhas, 722 docs / 142 queries)
- **Subitem:** publicar SHA-256 do dataset **antes** de medir; era Qwen3-Embedding-0.6B (1024d) com `health(era)`; gate inalterado.
- **Objetivo:** nova geração de embedding sem contaminar a v1.
- **GO:** hit@5 ≥ 0,974 com modelo/era/dims declarados no relatório.
- **NO-GO:** retunar depois de medir, reusar a v1, misturar era ou dims.
- **Goal:** `target/aether_f4_v2.json` + hash publicado.

## 3. Classe C — modalidade externa (sidecar, feature off)

### C1 · T-OCR
- **Tema:** T-OCR (classe C, dep T-NL-IN + A1) · **Item:** `src/agent/ocr.rs` + `tools/ocr_sidecar.py`
- **Subitem:** sidecar stdio JSON (`text`, `conf`, `spans`, `blocks`, HTML de tabelas); backend tesseract→paddle→paddleocr-vl (GPU, flag); tabela HTML→`TabSchema` da A1; `OcrPort` real atrás de `ocr` + `AETHER_OCR=1` + allowlist; `conf < 0,6`→ask; PII→deny.
- **Objetivo:** `DocRef`→texto+spans+conf, com abstenção honesta.
- **GO:** CER ≤ 5% e TEDS ≥ 0,85 em 20 páginas; conf baixa→abstain ≥ 90%; 0 byte de imagem no T0.
- **NO-GO:** backend sem Apache/MIT, resultado sem spans/conf.
- **Goal:** `tests/agent_ocr_gate.rs` + `target/aether_ocr.json` (fixtures, não pixels no core).

### C2 · T-STT
- **Tema:** T-STT (classe C, dep escada S\*) · **Item:** `src/agent/speech.rs` + sidecar sherpa-onnx
- **Subitem:** parakeet-tdt-0.6b-v3 int8 + VAD silero; `SttPort` real atrás de `speech`; `conf < 0,6`→ask.
- **Objetivo:** áudio→texto+conf+lang sem PCM no core.
- **GO:** WER ≤ 12% em 30 clipes PT-BR; RTF ≤ 0,2 em CPU; 0 PCM no T0.
- **NO-GO:** WER > 20%; modelo fora da allowlist.
- **Goal:** `target/aether_stt.json` com WER/RTF medidos.

### C3 · T-VIS
- **Tema:** T-VIS (classe C, dep C1/C2 + escada V\*) · **Item:** `src/agent/vision.rs`
- **Subitem:** sidecar ONNX RF-DETR + Grounding DINO; embed SigLIP2 em era própria + `health(view=era)`; auth ≤ 120.
- **Objetivo:** `ImageRef`→`VisionObs` ancorável.
- **GO:** mAP@50 ≥ 0,6 no fixture; OOD→abstain ≥ 90%; 0 pixel no core.
- **NO-GO:** licença AGPL, embed sem era declarada.
- **Goal:** `target/aether_vis.json`.

### C4 · T-TTS
- **Tema:** T-TTS (classe C, dep C2) · **Item:** `src/agent/speech.rs` (`TtsPort`)
- **Subitem:** sidecar Kokoro-82M ONNX (pt-BR), split por sentença, cache por hash do texto; abstain→beep.
- **Objetivo:** `CertifiedUtterance`→áudio, nunca narração de abstenção como sucesso.
- **GO:** round-trip WER ≤ 5% via C2; 1º áudio ≤ 600 ms em CPU; 0 fala de abstain.
- **NO-GO:** engine GPL linkada, narração sem `sources`.
- **Goal:** `target/aether_tts.json`.

## 4. Classe D — corpo/sensores/hardware

### D1 · T-SENSOR
- **Tema:** T-SENSOR (classe D, dep A4) · **Item:** coletor `HostObs` tipado
- **Subitem:** CPU/RAM/disco/térmico via PDH (sem dependência nova) + MQTT opcional; poll ≥ 1 s com jitter; allowlist de métricas; PII→deny; persistência só via A4.
- **Objetivo:** o hub vê o host como evidência, não como texto de log.
- **GO:** 10 min sem gap > 2 s; 0 storm; sensor morto→abstain.
- **NO-GO:** poll < 1 s no T0, escrita sem A4.
- **Goal:** `target/aether_host_report.json` estendido + gate.

### D2 · T-HAPTIC
- **Tema:** T-HAPTIC (classe D, dep C4) · **Item:** `HapticPattern` (verde/amarelo/vermelho/não sei)
- **Subitem:** derivação 1:1 de `Action`; backend OpenXR 1.1 §11.7 ou UI/beep no Windows; feature off.
- **Objetivo:** banda tátil tem o mesmo contrato do TTS — abstain nunca vira "sucesso".
- **GO:** paridade 100% com `Action`; abstain nunca mapeia para padrão de sucesso.
- **NO-GO:** padrão sem `Action` certificada.
- **Goal:** `tests/agent_haptic_gate.rs`.

### D3 · T-AR
- **Tema:** T-AR (classe D, dep C3) · **Item:** `ArAnchor{pose, source: VisionObs, conf}`
- **Subitem:** âncora só pós-C3+COH; OpenXR Spatial Entities; persistência local.
- **Objetivo:** nenhuma âncora sem evidência visual.
- **GO:** 0 âncora sem `VisionObs`; drift ≤ 5 cm / 60 s.
- **NO-GO:** cloud/VPS default, câmera always-on.
- **Goal:** `tests/agent_ar_gate.rs`.

### D4 · T-ROBOT
- **Tema:** T-ROBOT (classe D, dep D1 + B1) · **Item:** `RobotCmd`→`RobotFeedback` + certificado
- **Subitem:** contrato + simulador com `SuccessCriteria`; e-stop, single-point-of-control e reduced-speed ≤ 250 mm/s (ISO 10218-1/2:2025, TS 15066); bridge ROS 2 atrás de feature; `AETHER_ROBOT_KILL`.
- **Objetivo:** comando de robô é claim certificada, não chamada de função.
- **GO:** 0 cmd sem `SuccessCriteria`; 100% do risco High com HITL; e-stop ≤ 100 ms no sim; safe-stop pós-timeout.
- **NO-GO:** sem e-stop, shell genérico, hardware antes de D1+B1.
- **Goal:** `tests/agent_robot_gate.rs` + `target/aether_robot.json`.

## 5. Trilha e ritual

```text
A1 ──► A2
A3 ‖ A4
B1 ‖ B2 ──► B3
C1 ‖ C2 ──► C3 ──► C4
D1 (após A4) · D2 ‖ D3 (após C3) ──► D4
```

- Gate novo sempre em `tests/agent_<tomada>_gate.rs` + entrada no `tools/cycle_gate.py`.
- Artefato em `target/` só vira evidência quando promovido a `data/` com `.meta.json` (sha256 do próprio artefato + comando).
- Bump de versão só com `src`/`tests` (`tools/cycle.ps1`); `.cycle/state.json` é local.
- Release **0.6.0** entregue em 2026-11-24 com A1+A2+A3 verdes (`docs/RELEASE_0_6_0.md`).
- Release **0.6.2** entregue em 2026-11-25 com B2 (duas direcoes), A5 (T-HIER) e Sprint A/B/C (`docs/RELEASE_0_6_2.md`).
- Release **0.6.3** entregue em 2026-11-26 com o A5 medindo as duas direções e o `cycle_gate` gerando o round rev (`docs/RELEASE_0_6_3.md`).
- Release **0.6.4** entregue em 2026-11-26 com o A6 T-GRAPH (`docs/RELEASE_0_6_4.md`).

## 6. Estado desta entrega

| Item | Estado |
|---|---|
| `src/agent/tab.rs` (parser, schema, claims, tool, CLI) | implementado |
| `tests/agent_tab_gate.rs` (7 checks) | verde |
| `target/aether_tab_gate.json` | gerado (`ok=true`) |
| `tools/cycle_gate.py` | check `cargo_test_agent_tab_gate` |
| Dependências novas | nenhuma |
| `src/agent/scorecard.rs` + `src/core/sha256.rs` (A2) | implementado |
| `tests/agent_eval_report_gate.rs` | verde (2 runs idênticos, false_act=0, T3 N/A) |
| `target/aether_agent_scorecard.json` | gerado (`ok=true`, run_id + sha256) |
| `tools/cycle_gate.py` | checks `agent_tab_gate` + `agent_eval_report_gate` |
| `src/agent/p2p.rs` (A3: `SgdbDelta`, oplog Merkle, `Replica`) | implementado |
| `tests/agent_p2p_gate.rs` (6 checks) | verde (convergência + contradição) |
| `target/aether_p2p.json` | gerado (`ok=true`, `converged=true`) |
| `tools/cycle_gate.py` | check `agent_p2p_gate` (A3) |
| `src/agent/clip.rs` (A4: stdin default, OS sob `clip`, redact contextual, `code.patch`) | implementado |
| `tests/agent_clip_gate.rs` (7 checks) | verde (5 classes negadas, 0 escape, pós-COH) |
| `target/aether_clip_gate.json` | gerado (`ok=true`) |
| `tools/cycle_gate.py` | check `agent_clip_gate` (A4) |
| `Cargo.toml` | feature `clip` = `dep:arboard` (opt-in, default off) |
| `src/agent/hier_moe.rs` (A5) | LSH + re-rank f32 medidos; `lsh_ok=true`, `may_enable` só com feature+env |
| `tests/agent_hier_moe_gate.rs` | verde nas duas direções (release, n=5000): fwd 0,7708 = plano, p99 335 µs < 462 µs; rev 0,9170 = plano, p99 370 µs < 477 µs |
| `tools/a5_rev_round.py` | round rev (`hv/bank_rev.bin` do `bank_rev.f32` pelo `P` do manifest, espelhado em `hv/bank.bin` para o `Round::load`) em `target/a5_rev_round/`, com autoverificação bit a bit do fwd |
| `data/aether_hier_moe_status.json` (+`.meta.json`) | evidência do gate A5 (`lsh_ok=true`, `both_directions_ok=true`) |
| `tools/cycle_gate.py` | checks `A5_rev_round` (gera o round antes) + `agent_hier_moe_gate` (A5) |
| `tools/b2_two_directions.py` | probe das duas direções (re-embed cacheado em `emb/bank_rev.f32`) |
| `data/b2_two_directions.json` (+`.meta.json`) | evidência: fwd 0,7708 · rev **0,9170** · teto f32 0,9170 |
| `tools/b2_oversample_adc.py` | probe pós-FAIL do B2 (oversample k + ADC) |
| `data/b2_oversample_adc.json` (+`.meta.json`) | evidência do probe: `verdict=FAIL`, 0,755 |
| `src/agent/graph.rs` (A6) | evento → átomo BAM 64 → RG 64→16→4 → assinatura/surpresa; `auth 120`; feature `graph` off → no-op |
| `tests/agent_graph_gate.rs` (8 checks) | verde nos dois lados (default e `--features graph`) |
| `data/aether_graph_gate.json` (+`.meta.json`) | evidência: `rg 64/16/4`, `claims 3`, `max_authority 120`, `ok=true` |
| `tools/cycle_gate.py` | checks `cargo_test_agent_graph_gate_off` + `_on` (A6) |
| `src/knowledge/sgdb/`, `src/core/persistency_search.rs` | intocados |

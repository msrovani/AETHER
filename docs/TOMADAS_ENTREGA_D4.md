# Tomadas — entrega final D4 (independente de HW)

> Implementação da fila [`TOMADAS_FUTURO.md`] até **D4 T-ROBOT**, **classe D sem
> hardware**: cada tomada de classe C/D entra como **adaptador sidecar
> (`AETHER_*_CMD`) + backend virtual/determinístico**; hardware real pluga depois
> **sem mudar fio/disjuntor/filtros/DB** ("classe muda *quando* entra, nunca *como*").

## Princípios (invariantes)
- **Eixo C:** tomada registrada em `CORE_TOMADAS` + `TOMADAS_FUTURO` antes de codar; comando/ação não inventariado = bug/reject.
- **Barramento único** `src/agent/bus.rs`; disjuntor `Eixo C→rede→parser/conf→EFE→coherence→risco→auth≤120→sandbox→modalidade off`; filtros A7+P2+T-DEC AND; DB neural-sgdb ADD-only; sem canal lateral.
- **auth ≤ 120** em tudo gerado; **255 só HITL**; nunca auto.
- **feature off** por default; **no-op honesto**; nunca fingir backend; **conf não inventada** (padrão C1); `Err` honesto.
- **hardware seguro:** kill switch; HITL duro; certificado; allowlist; risk ceiling; audit; **nunca atua sem**.

## O que está **vivo**
| Fase | Módulo | Gate | Estado |
|---|---|---|---|
| **F0 / B3-prep** | `EmbedBank` (`embed.rs`) dual-bank T-EMB | `agent_b3_gate` | vivo; candidate nunca roteia; cutover HITL; 0 mistura; **F4 congelada até sanção+dataset** |
| **F1 / C1** | `ocr::OcrPort::extract_sidecar` | `agent_c1_ocr_sidecar_gate` | vivo; `AETHER_OCR_CMD`; `AETHER_OCR_CONF`; conf 0.5/override; PII/conf→ask; **Tesseract não instalado**; `feature ocr` |
| **F2 / C2 T-STT** | `speech::SttPort::transcribe_sidecar` | `agent_c2_stt_gate` | vivo stub→sidecar `AETHER_STT_CMD`; `AETHER_STT_CONF`; conf<0.50→ask; PCM fora T0; `feature speech` |
| **F3 / C3 T-VIS** | `vision::VisionPort::observe_sidecar` + `signature` | `agent_c3_vis_gate` | vivo stub→sidecar `AETHER_VIS_CMD`; `VisionObs` tipado; `AETHER_VIS_CONF`; pixels fora T0; **reusa A6 RG 64→16→4**; `feature vision` |
| **F4 / C4 T-TTS** | `speech::TtsPort::speak_sidecar` | `agent_c4_tts_gate` | vivo stub→sidecar `AETHER_TTS_CMD`; só `CertifiedUtterance`; abstain→beep "não sei"; PCM não cruza; `feature speech` |
| **F5 / D1 T-SENSOR** | `sensor::SensorPort` | `agent_d1_sensor_gate` | vivo; `SensorReading`; **virtual sintético determinístico (seed)** + sidecar `AETHER_SENSOR_CMD`; schema break→reject; série→T-PRED; `feature sensor` |
| **F6 / D2 T-HAPTIC** | `haptic::HapticPort` | `agent_d2_haptic_gate` | vivo; banda verde/amarela/vermelha; `AETHER_HAPTIC_CMD`; no-op; `feature haptic` |
| **F7 / D3 T-AR** | `ar::ArPort` | `agent_d3_ar_gate` | vivo; **deps duras C3 + T-COH**; `ArAnchor` pós-evidência; `AETHER_AR_CMD`; no-op; `feature ar` |
| **F8 / D4 T-ROBOT** | `robot::RobotPort` | `agent_d4_robot_gate` | **vivo (virtual)**; `RobotCmd`+`RobotCert`; allowlist; risk ceiling; **HITL duro sempre**; kill switch; **backend virtual**; pós-verificação; audit; `feature robot` |

## D4 — contrato (entrega final)
`RobotPort::execute(cmd, hitl_token)`:
1. **kill switch** (`AETHER_AGENT_KILL`/`AETHER_ROBOT_KILL`) → `KillSwitch` (bloqueia **toda** call, incl. Stop);
2. **Eixo C allowlist** (`RobotAction`; `Stop` sempre permitido);
3. schema break (id vazio; arg não-finito; risk∉[0,1]);
4. tomada off; backend off;
5. **certificado `RobotCert::valid`** (violations=0; consensus≥0.5; auth≤120; finito) senão `NoCertificate`→abstain;
6. risk ≤ ceiling (Stop exento);
7. **HITL duro SEMPRE** — `hitl_token` explícito por comando; **nunca auto**;
8. backend **virtual** (não atua; registra + verifica) / sidecar HW (`AETHER_ROBOT_CMD`, status ok = pós-verificação);
9. `RobotOutcome` + **audit trail** (sem segredos); claim auth≤120; **0 bytes de comando no T0**.

## O que **não** está feito / aguarda humano ou HW real
- **B3:** fecha com **sua decisão de dataset/era** + sanção HITL no cutover (candidate preparado; F4 congelada).
- **C1 real:** instalar **Tesseract** ou apontar `AETHER_OCR_CMD`.
- **C2/C3/C4 real:** binário/sidecar real de STT/VIS/TTS (não instalado); adaptador + `feature` prontos; gate usa stub determinístico.
- **D1 real:** sensor/host IoT real (não presente); virtual sintético + sidecar plugável.
- **D2/D3 real:** atuador tátil/AR real; adaptador + no-op.
- **D4 real:** hardware robótico real + **sanção HITL do primeiro `RobotCmd` real** + `AETHER_ROBOT_CMD`; **nunca atua sem** kill switch + HITL + certificado; virtual é o default honesto.

## Honestidade
Não fingir backend; `Err` honesto; conf **não inventada**; claims **auth≤120**; **255 só HITL**; **nunca atuar sem certificado + HITL duro + kill switch**. Flakes de tempo no host (`D1/E6/g5/t12/e8`) não são regressão desta frente (A/B vs pristine).

## Corre��es p�s-review (W1-W12)
- **W1+W8 (sem shell + inje��o + timeout/limite):** src/agent/backend.rs un_backend/un_backend_cmd/sanitize_ref/parse_cmd; **nunca** cmd /c/sh -c (cmd s� por teste, n�o recomendado; sh/bash/powershell/pwsh bloqueados); dado do chamador **n�o em argv** (C4 texto?**stdin**; C1/C2/C3/D1/D3 ef **sanitizada**; D4 ction.code() est�tico); stdin?pipe+EOF; stdout=BACKEND_MAX_STDOUT; stderr?null; **timeout+kill** BACKEND_TIMEOUT_MS. AETHER_*_CMD=bin�rio concreto.
- **W2 (HITL real):** D4 execute(cmd, hitl: &HitlDecision) (pproved+	oken_present+!timed_out+uthority>=255+equest_id==request_id_for(cmd.id)+**reuso n�o permitido**); audit sem token; RobotError::NeedsHitl. EmbedBank::cutover exige hitl_sanction_255. hitl_sanction_255 nunca auto. **Nota honesta:** HitlDecision � emitido por pply_hitl/T-HITL; ainda **n�o** verifica criptograficamente a origem (sem IdP); hitl_sanction_255 hoje l� env AETHER_HITL_SANCTION_255=1, n�o nsgdb-255 programaticamente (fato auth-255 registrado em nsgdb separadamente).
- **W3:** *_sidecar re-checa enabled+feature; speak_sidecar exige enabled.
- **W4:** speak_sidecar uth<=120 + judge_conf>=0.5 + may_speak_fact.
- **W5:** C1 detect_pii no **texto extra�do**; **conf honesta** � backend sem conf medida?conf=0.0; AETHER_*_CONF s� se operador declarar + is_finite; **NaN/inf?0/ask**; 	o_goal conf<=0.50?ask + !is_finite. C2/C3 idem.
- **W6:** D1 alue.is_finite+.validate()+series MAX_SERIES+checked_add.
- **W7:** GraphEvent::claims_from_event_inner?Result+alidate+ids.first().
- **W9:** gent_b1_gate artefato pr�prio.
- **W10:** PredictExpert::enter_v1_enabled?enter_v1_allowed. **Nota honesta:** PredictExpert n�o tinha v1 opt-in interno; getter adicionado, sem forkar comportamento; enter_v1_default continua no 8_v1.
- **W11:** D4 Stop isento de cert+isk (abortivo) mas **mant�m HITL+kill+enabled+allowlist**; MAX_AUDIT. AETHER_ROBOT_RISK_CEILING is_finite.
- **W12:** RobotError Display+Error; cargo fmt.


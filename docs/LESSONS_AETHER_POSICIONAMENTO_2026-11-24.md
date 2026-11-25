# Lessons — Posicionamento + Auditoria do Core AETHER (2026-11-24)

Aprendizados do ciclo completo: [`POSICIONAMENTO_AETHER.md`], [`AUDIT_CORE_EXTREMO_2026-11-24.md`],
[`COERENCIA_E_REGRAS_IMPL_2026-11-24.md`], [`REGRAS_IMPL_SAT_2026-11-24.md`].

---

## 1. O que funcionou (evidências)

- **Hot path já é físico, não é chatbot:** 81,3 µs/ciclo, 0 alloc, N=64, solver 84,5%
  (57,5 µs) — E6b. Qualquer otimização no loop deve ser **zero-alloc e bit-a-bit**;
  as raízes para acelerar o kernel estão no ILP (16 acumuladores), não em unsafe
  (`deny(unsafe_code)`).
- **Duas metas de tempo distintas:** 16.667 ms (T0) vs `eq-t/s` (throughput).
  Optimizar o kernel perde quando o grupo de dados não importa (HW_TARGET §8.3).
- **Não é um N único:** L2 é 1,25 MB; até N≈256 foot = 1,03 MB, N=512 = 3,04 MB
  (L3) e o teto de 60 Hz está em N=512. No f32, aPinterest do L2 anda para N≈724
  (HW_TARGET §9, E1b). Documentar os cliffs antes de escalar N.
- **Geometria de cache:** J (N²·8 B) + readout (3N·8 B) — o readout paga caro;
  F16 (16 acumuladores no readout) é o próximo passo mensurável (est. ~50%).

## 2. O que não funcionou / gaps confirmados

- **Órgãos cognitivos estão desconectados do laço do organismo:** RG, Koopman/SINDy,
  Dreamer, ActiveInference, CBF contribuem **0 µs** no M4 (HW_TARGET §8.4).
  Posicionamento = arquitetura válida, mas deve entrar por Sprint A sem distorcer o T0.
- **Uncertainty não é decisória:** `ensemble.epistemic_uncertainty` (L1)
  é reportado em telemetria e não altera nenhuma decisão no loop.
- **Federação é mutação de topologia, não contorção:** sobre N=64/128 processos
  isolados, portas multicast segmentadas (`--multicast-port` — não existe, R8),
  `ScopeId` único. Keeps product A (M4, N≤512) como canônico.

## 3. Lições de integração (Sprint A — R1–R14)

1. **Fast-J Gating (R1):** `J_eff = J·g_i·g_j` — multiplica o hebbiano em
   `morphogenesis.rs:507`; aprovado C1. Gate `tests/morpho_gating_gate.rs`.
2. **Elastic compute (R2):** o Dreamer faz poucas amostras/passos quando
   `epistemic_uncertainty` baixo; 30× quando alto; T0 intacto. Gate `E6-mod5` → `target/aether_e6_mod.json`.
3. **RG spine (R3):** ascensão `e(t)` (resíduo) e descida `torque_into` — disjuntos
   ao thermal jitter do Dreamer. Gate `tests/governor_rg_spine_gate.rs`.
4. **Dream→E2 commit (R4):** `EmaActuator` + `LyapunovCbf` do terminal do loop —
   sobre `ensemble` (L3/L4).
5. **macro_signature (R6/F23):** nova API no `RenormalizationHierarchy` —
   primeiro para o cross-modal (TGN-SVDD bridge).
6. **Autobiografia (R11):** hash bit-a-bit aos 60 s em `target/aether_e6_autobiography.json`.
7. **Proteção de produto (R9/C4):** rebase de autoridade do organismo:
   ação autônoma ∈ 200; 200–255 = HITL. Documentar.
8. **Anti-loop (R10):** `TOOL_PROBE_BUDGET` no planner — respetar o "s6-like" para abdução.

## 4. Regra geral (memória para ciclos futuros)

> **Aprenda nova capacidade apenas quando tem gateway quase-gratuito** — gate:
> artefato JSON versionado, ≥ 5 execuções, 10 minutes de duração, busca de
> regressão (`cargo test --release`) em paralelo. Sem isso, o core estagnava
> porque "só vale a pena com 2 W rodando 24/7".

## 5. Referência de memória

- Posição: `docs/POSICIONAMENTO_AETHER.md` (itens 6–9, invariantes 8–9).
- Auditoria: `docs/AUDIT_CORE_EXTREMO_2026-11-24.md` (F1–F23, L1–L13, §2b).
- Regras: `docs/COERENCIA_E_REGRAS_IMPL_2026-11-24.md` (C1–C9, R1–R14).
- Execução: `docs/REGRAS_IMPL_SAT_2026-11-24.md` (Sessões 1–8, guarda R14).

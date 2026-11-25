# Regras de Implementação — Sprint A (AETHER, 2026-11-24)

Documento de execução para o Sprint A do [`COERENCIA_E_REGRAS_IMPL_2026-11-24.md`]
(R1–R14). Referência: [`POSICIONAMENTO_AETHER.md`] (invariantes 1–9),
[`HW_TARGET.md`] §8, [`AUDIT_CORE_EXTRMO_2026-11-24.md`].

**Princípio transversal (R14):** todas as implementações Sprint A mantêm o
contrato de T0: 81,3 µs/ciclo, 0 alloc no hot-path, N ≤ 512, determinismo
bit-a-bit (INV-T5). Nenhuma R altera o caminho do organismo M4.

---

## Sessão 1 — R1 Fast-J Gating (morphogênese)

**Hook:** `src/morphogenesis.rs` — `try_evolve_topology` (L434).
- Nova assinatura: `try_evolve_topology(&mut self, prediction_error, active_spins, dt, g: &[f32])`.
- Nova instância: `MorphogeneticField::set_fast_g(&mut self, g: &[f32]) -> Result<()>`
  (valida len == n; zeros => g neutral = 1,0).
- mudança em L507: `let gi = g.get(i).copied().unwrap_or(1.0); let gj = g.get(k).copied().unwrap_or(1.0); dj = gain * (corr - jv) * gi * gj - decay_rate * jv;`
- Caller: `aether_daemon.rs:581` — preenche `g_buf` via
  `field.fast_g_from_wave(&wave)` (helper novo; snippet em §4.1)
  antes do `evolve_topology` — 0 alloc (`fast_g_from_wave` escreve no buffer).
- **Gate (R1):** `tests/morpho_gating_gate.rs`:
  1) acoplamento adversarial 10 quadros: ΔR(J antes/depois) > 0,30;
  2) `try_evolve_topology` 0 alloc (alocação monitorada);
  3) bit-reproducibilidade: mesma seed ⇒ J idêntico entre 2 runs.
- **Auth:** core.

### 4.1 Snippet de referência: fast_g_from_wave (Sprint A)
```rust
// em MorphogeneticField, Sprint A
pub fn fast_g_from_wave(&mut self, spec: &[Complex32]) {
    // agrega |Y| por canal: g_i = 1 - tanh(|Y_i| / scale)
    let n = self.n;
    for i in 0..n { self.g_buf[i] = 0.0; }
    for k in 0..spec.len().min(n) {
        let y = spec[k].re.abs();
        for i in 0..n {
            self.g_buf[i] += y;
        }
    }
    for i in 0..n { self.g_buf[i] = 1.0 - (self.g_buf[i] / n as f32).tanh(); }
}
```
(Nota Sprint A: este snippet é referência; implementação geralada por gate R1.)

## Sessão 2 — R2 Elastic compute (daemon)

**Hook:** `src/bin/aether_daemon.rs` — cycle L597–605.
```rust
let base_steps = self.cfg.dream_steps as f64;
let esq = self.ensemble.epistemic_uncertainty; // lido na L634 (1 ciclo de atraso aceito)
let steps_eff = ((base_steps * (1.0 + 3.0 * esq)).min(30.0 * base_steps)) as usize;
// chama imagine_futures_into com steps_eff em vez de self.cfg.dream_steps
```
- Novo campo no `DaemonTelemetry`: `dream_steps_max_observed: u64`.
- **Gate (R2):** `target/aether_e6_mod.json` — campos:
  `elapsed_us_median`, `dropped_frames_60s`, `steps_max_observed`, `wall_sha256`.
  Medida de referência: HW_TARGET §8.2 (`E6 mod`, canvas re-rodada).
- **R14:** `cargo test --release` + re-medida E6 (p99 ≤ 16.667 µs).

## Sessão 3 — R3 RG spine (Governor)

**Hook:** `src/core/aether_governor.rs` — `GovernorConfig` + `reflex_loop` (L554).
- Nova: `GovernorConfig { rg: Option<RenormalizationHierarchy> }` (feature do crate, sem tag).
- No loop (após `core.process(&ev)` Ok):
  ```rust
  if let Some(rg) = rg_opt.as_mut() {
      let micro: [f32; 64] = ev.z.try_into().unwrap_or([0.0; 64]); // Guarda: segundo destaque
      let _ = rg.forward(&micro);                                    // ascensão (L12)
      let mut omega = [0.0f32; 64];
      let _ = rg.torque_into(&mut omega, 0.5);                       // descida (L12)
      // aplica omega sobre ev.z_next antes do push para Koopman (cópia local)
  }
  ```
  > [!NOTE]
  > `ev.z` é `[f32; LATENT_DIM]` — Sprint A implementa `macro_signature` (R6)
  > independente; a fusão é gateway de Sprint B (L6).
- **Gate (R3):** `tests/governor_rg_spine_gate.rs` —
  (a) eventos adversários → `macro_order > 0.80` em 80% dos ticks;
  (b) CBF violations = 0 (`GovernorReport.cbf_violations`);
  (c) refletor registra `rg_enabled = true` na telemetria (`GovernorReport`).

## Sessão 4 — R4 Dream → E2 commit (Governor reflex)

**Hook:** `aether_governor.rs` — reflex_loop L581.
- Add `actuator: Option<EmaActuator>` em GovernorConfig; on `Ok(out)`:
  ```rust
  if let Some(a) = actuator_opt.as_mut() {
      if stats_esq_atual() > 0.3 { let _ = a.filter_into(&out.policy_vec, &mut smoothed); }
  }
  ```
  (detalhe Sprint B: `stats_esq_atual` lê `stats.epistemic_mean` — campo novo, atualizado no reflex loop.)
- **Gate (R4):** `tests/governor_e2_commit_gate.rs` — monotonicidade do smoothed por seeds; CBF=0.

## Sessão 5 — R6 G-ISO cross-modal (F23)

**Hook:** `src/core/renormalization.rs` — nova `macro_signature(&self) -> [u16; 4]`
(hash de `macro_phase()` + `macro_order()`, overflow em u16).
- **Consumidor (Sprint A/B):** scopes T-SENSOR/T-GRAPH gravam
  `macro_signature` no `neural-sgdb` (fields `macro_sig_a`, `macro_sig_b`).
- **Gate (R6):** `tests/macro_signature_gate.rs` (10 hashes, ≤ 2 colisões
  hash64); exemplo zero-shot em `examples/rg_transfer_probe.rs` (TKNets — arXiv:2402.07834).

## Sessão 6 — R7/R8 Federação (A8) e R11 Autobiografia (A11)

**Hook R7:** `src/bin/aether_daemon.rs`:
- flag `--multicast-port` (parse_cli L729+) → `MULTICAST_PORT` via env
  `AETHER_MULTICAST_PORT` default 7777 (roulemente).
- precheck em main: ao sobe, verifica `Get-NetUDPEndpoint` por (porta, self) — regra R8 (não bind se houver bind existente).
- `ScopeId`: usar já existente (`ScopeId::new(seed, run, app)`).
**Gate (R7/R8):** `tests/daemon_federation_gate.rs` —
(a) 2 daemons (portas diferentes) na LAN: R > 0.95, `peers_heard > 0`;
(b) 2 daemons na MESMA porta: o segundo recusa (documentar SO_REUSEADDR).

**Hook R11:** `aether_daemon.rs` cycle L624–638 — algoritmo:
```
hash = sha256(JoinString(phase_hash_10s))  // phase_hash = hash de f32 de frame.phase a cada 60 frames
```
(SNP) → `target/aether_e6_autobiography.json`.
**Gate (R11):** `tests/organism_autobiography_gate.rs` — 10 execuções
(seed distinto), hash idêntico; R12 registra `aether_e6_autobiography.json`
no area de cobertura do auditoria.

## Sessão 7 — R9 Rebase de autoridade (A9)

**Hook:** `src/knowledge/sgdb/ledger.rs:541`:
```rust
pub const ORGANISM_AUTHORITY: u8 = 200; // rebase: ação autônoma ∈ 200; 200–255 = HITL
```
- Doc: CORE_TOMADAS.md (T-SENSOR/T-ROBOT) + POSICIONAMENTO Invariante 9.
- **Gate:** `tests/auth_levels_gate.rs` (c physicists: claims ≤200 sem token HITL).

## Sessão 8 — R10 Probe budget (G-ABD, A10)

**Hook:** `src/agent/planner.rs` + novo `src/agent/epistemic_budget.rs`:
```rust
pub static TOOL_PROBE_BUDGET: AtomicU64 = AtomicU64::new(16); // reset por turno
pub fn try_probe() -> bool { ... decrement; on 0 → false }
```
- `planner.rs`: `rank_with` — sem candidato novo quando `try_probe() == false`.
- **Gate:** `tests/bus_epistemic_probe_gate.rs` (5 seeds, fuzz: N probes ≤ budget + 1 de recovery, 0 loops).

---

## Cronograma

- **Sprint A (0–2):** Sessões 1–6 (R1–R12, exceto L6 LinUCB).
- **Sprint B:** L6 (LinUCB ← RG macro, R6), L7 (G-ABD loop, R10), L8 (G-MORPH F19, L8).
- **Guarda (contínuo):** `cargo test --release` + re-medida E6.

## Referências

- [`COERENCIA_E_REGRAS_IMPL_2026-11-24.md`] — coerencia C1–C9, tabela R1–R14.
- [`POSICIONAMENTO_AETHER.md`] §2.3 (itens 6–9), §8, §9.
- [`HW_TARGET.md`] §8, [`E2_OPTIMIZATION.md`] §10, [`AETHER_AGENT_ROADMAP.md`].

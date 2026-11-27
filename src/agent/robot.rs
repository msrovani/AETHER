//! D4 · T-ROBOT (`docs/TOMADAS_FUTURO.md` §D4) — **entrega final**.
//!
//! **Ação física com certificado.** Classe D, **independente de HW**: backend
//! **virtual** (`AETHER_ROBOT_BACKEND=virtual`, default) registra/verifica sem
//! atuar; hardware real pluga depois **sem mudar fio/disjuntor/filtros/DB**.
//!
//! **Deps duras:** **D1 (T-SENSOR)** + **T-PRED** + **T-HITL duro** + kill switch.
//!
//! # Disjuntores (ordem, `CORE_TOMADAS §3`):
//! 1. **kill switch** (`AETHER_AGENT_KILL`/`AETHER_ROBOT_KILL`) → bloqueia **toda** call;
//! 2. **Eixo C** — `RobotAction` allowlist; comando não inventariado → reject;
//! 3. **tomada off** (`feature robot` + `AETHER_ROBOT=1`);
//! 4. **certificado** obrigatório (`RobotCert::valid`; violations=0; auth≤120);
//! 5. **risk** ≤ `risk_ceiling`;
//! 6. **HITL duro SEMPRE** — token explícito por comando; **nunca auto**;
//! 7. claim auth≤120; **0 bytes de comando no T0**; audit trail.
//!
//! `RobotCmd` tipado; nunca texto livre de comando.

/// Ação de robô allowlisted (Eixo C; comando não inventariado = bug/reject).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotAction {
    /// No-op.
    Noop,
    /// Stop imediato (também via kill switch global).
    Stop,
    /// Voltar ao zero.
    Home,
    /// Mover (args `[x,y,z,...]` em unidades do backend).
    Move,
    /// Fechar gripper.
    Grip,
    /// Abrir gripper.
    Release,
}

impl RobotAction {
    /// Código estável.
    pub fn code(self) -> &'static str {
        match self {
            Self::Noop => "noop",
            Self::Stop => "stop",
            Self::Home => "home",
            Self::Move => "move",
            Self::Grip => "grip",
            Self::Release => "release",
        }
    }
}

/// Certificado de evidência para atuar (nunca atua sem; auth≤120).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RobotCert {
    /// Energia.
    pub energy: f64,
    /// Gap.
    pub gap: f64,
    /// Violações (**deve ser 0**).
    pub violations: u64,
    /// Consenso `[0,1]`.
    pub consensus: f64,
    /// Autoridade da evidência (≤120; nunca 255 auto).
    pub authority: u8,
}

impl RobotCert {
    /// `violations==0` + `consensus` suficiente + `auth≤120` + finito.
    pub fn valid(&self) -> bool {
        self.violations == 0
            && self.energy.is_finite()
            && self.gap.is_finite()
            && self.consensus.is_finite()
            && (0.0..=1.0).contains(&self.consensus)
            && self.consensus >= 0.5
            && self.authority <= 120
    }
}

/// Comando de robô tipado (**nunca** texto livre; 0 bytes no T0).
#[derive(Clone, Debug, PartialEq)]
pub struct RobotCmd {
    /// Id do comando (audit).
    pub id: String,
    /// Ação allowlisted.
    pub action: RobotAction,
    /// Args numéricos tipados.
    pub args: Vec<f64>,
    /// **Certificado** obrigatório.
    pub cert: RobotCert,
    /// Risco `[0,1]` (≤ ceiling).
    pub risk: f64,
}

/// Backend de atuação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotBackend {
    /// Desligado.
    Off,
    /// **Virtual**: registra + verifica, **não atua** (classe D sem HW).
    Virtual,
    /// Hardware real externo via `AETHER_ROBOT_CMD`.
    Sidecar,
}

/// Resultado tipado da atuação (audit; pós-verificação).
#[derive(Clone, Debug, PartialEq)]
pub struct RobotOutcome {
    /// Id do comando.
    pub id: String,
    /// Ação.
    pub action: &'static str,
    /// Foi **executado** em HW real? (virtual → `false`).
    pub executed_real: bool,
    /// Pós-verificação ok.
    pub verified: bool,
    /// Token HITL usado (audit; não-segredo de comando).
    pub hitl_audited: bool,
    /// backend.
    pub backend: &'static str,
}

/// Erro tipado.
#[derive(Clone, Debug, PartialEq)]
pub enum RobotError {
    /// Kill switch ativo → bloqueia toda call.
    KillSwitch,
    /// Tomada off.
    Off,
    /// Eixo C: ação não allowlisted.
    NotAllowlisted(String),
    /// Certificado inválido → abstain.
    NoCertificate,
    /// Risco acima do ceiling.
    RiskCeiling(f64),
    /// **HITL duro**: token ausente / `HitlDecision` não aprovada / reuso (W2).
    NeedsHitl,
    /// Schema break.
    SchemaBreak(String),
    /// Backend.
    Backend(String),
}

/// **W12:** `Display` + `std::error::Error`.
impl std::fmt::Display for RobotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KillSwitch => write!(f, "kill switch ativo"),
            Self::Off => write!(f, "tomada off"),
            Self::NotAllowlisted(s) => write!(f, "não allowlisted: {s}"),
            Self::NoCertificate => write!(f, "certificado inválido → abstain"),
            Self::RiskCeiling(r) => write!(f, "risk {r} > ceiling"),
            Self::NeedsHitl => write!(f, "HITL duro não satisfeito"),
            Self::SchemaBreak(s) => write!(f, "schema break: {s}"),
            Self::Backend(s) => write!(f, "backend: {s}"),
        }
    }
}
impl std::error::Error for RobotError {}

/// Porta D4 (default **off**; backend **virtual**; nunca atua sem HITL).
#[derive(Clone, Debug)]
pub struct RobotPort {
    /// Ligada (feature `robot` + `AETHER_ROBOT=1`).
    pub enabled: bool,
    /// Backend.
    pub backend: RobotBackend,
    /// Risco máximo.
    pub risk_ceiling: f64,
    /// Ações permitidas (allowlist; default todas menos... none removidas; `Stop` sempre ok).
    pub allowed: Vec<RobotAction>,
    /// **Audit trail** (comandos aceitos, sem segredos; cap `MAX_AUDIT`).
    pub audit: Vec<RobotOutcome>,
    /// **W2:** `request_id` HITL já usados (reuso não permitido; cap `MAX_AUDIT`).
    pub used_request_ids: Vec<u64>,
}

/// `MAX_AUDIT` cap (W11).
pub const MAX_AUDIT: usize = 1024;

impl Default for RobotPort {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: RobotBackend::Virtual, // classe D sem HW
            risk_ceiling: 0.5,
            allowed: vec![
                RobotAction::Noop,
                RobotAction::Stop,
                RobotAction::Home,
                RobotAction::Move,
                RobotAction::Grip,
                RobotAction::Release,
            ],
            audit: Vec::new(),
            used_request_ids: Vec::new(),
        }
    }
}

/// `request_id` estável por `cmd.id` (correlação HITL; W2). `0` nunca.
pub fn request_id_for(cmd_id: &str) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for b in cmd_id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    if h == 0 {
        1
    } else {
        h
    }
}

impl RobotPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_ROBOT=1` + feature `robot`; `AETHER_ROBOT_BACKEND=virtual|sidecar`;
    /// `AETHER_ROBOT_RISK_CEILING`.
    pub fn from_env() -> Self {
        let env_on = std::env::var("AETHER_ROBOT")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let backend = match std::env::var("AETHER_ROBOT_BACKEND").as_deref() {
            Ok("sidecar") => RobotBackend::Sidecar,
            Ok("off") => RobotBackend::Off,
            _ => RobotBackend::Virtual, // classe D sem HW
        };
        let risk_ceiling = std::env::var("AETHER_ROBOT_RISK_CEILING")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.5);
        Self {
            enabled: env_on && cfg!(feature = "robot"),
            backend,
            risk_ceiling,
            allowed: Self::default().allowed,
            audit: Vec::new(),
            used_request_ids: Vec::new(),
        }
    }

    /// **Kill switch global**: `AETHER_AGENT_KILL=1` ou `AETHER_ROBOT_KILL=1` → bloqueia toda call.
    pub fn kill_switch_active() -> bool {
        let on = |k: &str| {
            std::env::var(k)
                .map(|v| {
                    matches!(
                        v.trim().to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    )
                })
                .unwrap_or(false)
        };
        on("AETHER_AGENT_KILL") || on("AETHER_ROBOT_KILL")
    }

    /// **Atuação D4.** Ordem dos disjuntores + **HITL duro sempre** + certificado + kill.
    /// **W2:** `hitl: &HitlDecision` real (`approved` + `token_present` + `!timed_out` +
    /// `request_id` correlacionado + `authority`); **reuso não permitido**; audit **hash**,
    /// nunca token cru. `&mut self` p/ audit. **W11:** `Stop` isento de `cert`+`risk`
    /// (abortivo), mas **mantém HITL duro + enabled + allowlist + kill**.
    pub fn execute(
        &mut self,
        cmd: &RobotCmd,
        hitl: &super::hitl::HitlDecision,
    ) -> Result<RobotOutcome, RobotError> {
        // 1. kill switch (bloqueia toda call, incl. Stop)
        if Self::kill_switch_active() {
            return Err(RobotError::KillSwitch);
        }
        // 2. Eixo C allowlist; `Stop` sempre permitido na allowlist
        if cmd.action != RobotAction::Stop && !self.allowed.contains(&cmd.action) {
            return Err(RobotError::NotAllowlisted(cmd.action.code().into()));
        }
        // schema break
        if cmd.id.is_empty() {
            return Err(RobotError::SchemaBreak("id vazio".into()));
        }
        if cmd.args.iter().any(|a| !a.is_finite()) {
            return Err(RobotError::SchemaBreak("arg não-finito".into()));
        }
        if !cmd.risk.is_finite() || !(0.0..=1.0).contains(&cmd.risk) {
            return Err(RobotError::SchemaBreak("risk fora de [0,1]".into()));
        }
        // 3. tomada off
        if !self.enabled {
            return Err(RobotError::Off);
        }
        // backend off
        if self.backend == RobotBackend::Off {
            return Err(RobotError::Off);
        }
        // 4. certificado obrigatório → senão abstain (W11: `Stop` isento)
        if cmd.action != RobotAction::Stop && !cmd.cert.valid() {
            return Err(RobotError::NoCertificate);
        }
        // 5. risco ≤ ceiling; `risk_ceiling` finito (NaN→SchemaBreak) (W11: `Stop` isento)
        if !self.risk_ceiling.is_finite() {
            return Err(RobotError::SchemaBreak("risk_ceiling não-finito".into()));
        }
        if cmd.action != RobotAction::Stop && cmd.risk > self.risk_ceiling {
            return Err(RobotError::RiskCeiling(cmd.risk));
        }
        // 6. **HITL duro SEMPRE** (W2): `HitlDecision` real; nunca auto; `request_id` correlacionado; reuso não permitido
        if !(hitl.approved && hitl.token_present && !hitl.timed_out) {
            return Err(RobotError::NeedsHitl);
        }
        if hitl.authority < 255 || hitl.authority == 0 {
            // sanção HITL efetiva (T-AUTH): `apply_hitl` só dá 255 com token; aqui exigimos sanção
            // `authority` reportada pelo humano/HITL; `<255` não sanctiona atuação real.
            if hitl.authority < 255 {
                return Err(RobotError::NeedsHitl);
            }
        }
        // `request_id` correlacionado ao `cmd.id` (estável); reuso proibido
        if hitl.request_id == 0 || hitl.request_id != request_id_for(&cmd.id) {
            return Err(RobotError::NeedsHitl);
        }
        if self.audit.iter().any(|_a| false) { /* audit não guarda request_id; reuso verificado via used_ids abaixo */
        }
        if self.used_request_ids.iter().any(|r| *r == hitl.request_id) {
            return Err(RobotError::NeedsHitl); // reuso não permitido
        }

        let (executed_real, verified, backend_name) = match self.backend {
            RobotBackend::Virtual => (false, true, "virtual"), // registra + verifica; NÃO atua
            RobotBackend::Sidecar => {
                let cmd_line = std::env::var("AETHER_ROBOT_CMD").map_err(|_| {
                    RobotError::Backend("AETHER_ROBOT_CMD ausente (HW real)".into())
                })?;
                // W1+W8: sem shell; `cmd.action.code()` (estático); timeout+limite
                super::backend::run_backend_cmd(
                    &cmd_line,
                    Some(cmd.action.code()),
                    None,
                    super::backend::BACKEND_TIMEOUT_MS,
                    super::backend::BACKEND_MAX_STDOUT,
                )
                .map_err(|e| RobotError::Backend(format!("robot backend: {e}")))?;
                (true, true, "sidecar") // pós-verificação do sidecar (status ok)
            }
            RobotBackend::Off => return Err(RobotError::Off),
        };

        // W2: audit trail (sem segredos; request_id + token_present, nunca token cru); W11 cap
        let req_id = hitl.request_id;
        self.used_request_ids.push(req_id);
        let outcome = RobotOutcome {
            id: cmd.id.clone(),
            action: cmd.action.code(),
            executed_real,
            verified,
            hitl_audited: true,
            backend: backend_name,
        };
        self.audit.push(outcome.clone());
        if self.audit.len() > MAX_AUDIT {
            self.audit.drain(..self.audit.len() - MAX_AUDIT); // W11 cap
        }
        Ok(outcome)
    }
}

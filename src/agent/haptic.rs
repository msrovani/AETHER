//! D2 · T-HAPTIC (`docs/TOMADAS_FUTURO.md` §D2).
//!
//! `Action` / `CertifiedUtterance` → **render tátil**; banda triagem
//! **verde/amarela/vermelha** (CORE_TOMADAS §3); classe D: adaptador
//! `AETHER_HAPTIC_CMD`, no-op honesto; `feature haptic`. Sem bytes no T0.
//! Só renderiza pós-triagem; nunca re-interpreta.
//!
//! Banda: verde=act/cite com fonte; amarela=propose/auth60-120 quarentena;
//! vermelha=abstain/deny/HITL.

/// Banda de triagem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapticBand {
    /// act/cite com fonte; evidência ok, risco baixo.
    Green,
    /// propose/auth 60–120; quarentena.
    Yellow,
    /// abstain/deny/HITL; OOD/risco/contradição.
    Red,
}

impl HapticBand {
    /// Padrão tátil estável.
    pub fn pattern(self) -> &'static str {
        match self {
            Self::Green => "tap-once",
            Self::Yellow => "tap-tap",
            Self::Red => "long-buzz",
        }
    }
}

/// Banda a partir de `Action`.
pub fn band_from_action(action: super::Action) -> HapticBand {
    match action {
        super::Action::Act | super::Action::Cite => HapticBand::Green,
        super::Action::Propose => HapticBand::Yellow,
        super::Action::Abstain => HapticBand::Red,
    }
}

/// Saída tátil tipada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HapticEvent {
    /// Banda.
    pub band: HapticBand,
    /// Padrão (`tap-once`/`tap-tap`/`long-buzz`).
    pub pattern: String,
}

/// Porta D2 (default off).
#[derive(Clone, Debug, Default)]
pub struct HapticPort {
    /// Ligada (feature `haptic` + `AETHER_HAPTIC=1`).
    pub enabled: bool,
}

impl HapticPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_HAPTIC=1` + feature `haptic`.
    pub fn from_env() -> Self {
        Self {
            enabled: crate::agent::modality::ModalConfig::from_env(
                "HAPTIC",
                cfg!(feature = "haptic"),
            )
            .enabled,
        }
    }

    /// `Action` → `HapticEvent` (nunca inventa fato; só render pós-triagem).
    pub fn render(&self, action: super::Action) -> HapticEvent {
        let band = band_from_action(action);
        HapticEvent {
            band,
            pattern: band.pattern().to_string(),
        }
    }

    /// Emite no backend (`AETHER_HAPTIC_CMD`) se ligado; no-op honesto + log.
    pub fn emit(&self, event: &HapticEvent) -> Result<(), String> {
        if !self.enabled {
            eprintln!("t-haptic off — no-op (feature `haptic` / AETHER_HAPTIC)");
            return Ok(()); // no-op honesto
        }
        // T11: encanamento via ModalConfig; backend ausente → no-op honesto.
        let cfg = crate::agent::modality::ModalConfig::from_env("HAPTIC", cfg!(feature = "haptic"));
        match cfg.cmd {
            Some(cmd_line) if !cmd_line.trim().is_empty() => {
                // W1+W8: sem shell; pattern (estático); timeout+limite
                super::backend::run_backend_cmd(
                    &cmd_line,
                    Some(&event.pattern),
                    None,
                    cfg.timeout_ms,
                    super::backend::BACKEND_MAX_STDOUT,
                )
                .map_err(|e| format!("haptic backend: {e}"))?;
                Ok(())
            }
            _ => Ok(()), // backend ausente → no-op honesto, render tipado segue válido
        }
    }
}

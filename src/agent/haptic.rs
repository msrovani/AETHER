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
        let env_on = std::env::var("AETHER_HAPTIC").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
        Self { enabled: env_on && cfg!(feature = "haptic") }
    }

    /// `Action` → `HapticEvent` (nunca inventa fato; só render pós-triagem).
    pub fn render(&self, action: super::Action) -> HapticEvent {
        let band = band_from_action(action);
        HapticEvent { band, pattern: band.pattern().to_string() }
    }

    /// Emite no backend (`AETHER_HAPTIC_CMD`) se ligado; no-op honesto + log.
    pub fn emit(&self, event: &HapticEvent) -> Result<(), String> {
        if !self.enabled {
            eprintln!("t-haptic off — no-op (feature `haptic` / AETHER_HAPTIC)");
            return Ok(()); // no-op honesto
        }
        match std::env::var("AETHER_HAPTIC_CMD") {
            Ok(cmd_line) if !cmd_line.trim().is_empty() => {
                let mut parts = cmd_line.split_whitespace();
                let prog = parts.next().ok_or_else(|| "AETHER_HAPTIC_CMD vazio".to_string())?;
                let mut args: Vec<String> = parts.map(|s| s.to_string()).collect();
                args.push(event.pattern.clone());
                let output = std::process::Command::new(prog).args(&args).output().map_err(|e| format!("haptic spawn: {e}"))?;
                if !output.status.success() {
                    return Err(format!("haptic status {}", output.status));
                }
                Ok(())
            }
            _ => Ok(()), // backend ausente → no-op honesto, render tipado segue válido
        }
    }
}

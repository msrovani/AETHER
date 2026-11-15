//! Códigos de gap heurísticos do hub (não culpabilizam o usuário).

/// Severidade do gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapSeverity {
    Info,
    Warn,
    Crit,
}

impl GapSeverity {
    /// Rótulo curto para UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "aviso",
            Self::Crit => "crítico",
        }
    }
}

/// Gap detectado na máquina / scorecard / toolchain.
#[derive(Clone, Debug)]
pub struct Gap {
    pub code: &'static str,
    pub severity: GapSeverity,
    pub title: String,
    pub detail: String,
    pub remediation: String,
}

impl Gap {
    /// Construtor.
    pub fn new(
        code: &'static str,
        severity: GapSeverity,
        title: impl Into<String>,
        detail: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            title: title.into(),
            detail: detail.into(),
            remediation: remediation.into(),
        }
    }
}

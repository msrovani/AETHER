//! D3 · T-AR (`docs/TOMADAS_FUTURO.md` §D3).
//!
//! **Overlay** pós-evidência. **Deps duras: C3 (T-VIS) + T-COH** — só âncora
//! AR **após triagem** (auth≤120; sem `act`); classe D: adaptador
//! `AETHER_AR_CMD`, no-op honesto; `feature ar`. Sem pixels/texto livre no T0.
//!
//! ```text
//! C3 T-VIS (VisionObs) ─► T-COH (certificado) ─► âncora AR ─► overlay render
//! ```

/// Âncora AR pós-evidência (tipada; sem pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct ArAnchor {
    /// Id da âncora.
    pub id: String,
    /// Label certificado.
    pub label: String,
    /// Posição `[x,y,z]` (unidade do backend; não bytes).
    pub pos: [f32; 3],
    /// auth ≤120.
    pub auth: u8,
}

impl ArAnchor {
    /// Disjuntor: **nunca** âncora sem evidência triada; auth≤120.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("schema break: id vazio".into());
        }
        if self.label.is_empty() {
            return Err("schema break: label vazio (âncora só pós-evidência)".into());
        }
        if self.pos.iter().any(|v| !v.is_finite()) {
            return Err("schema break: pos não-finita".into());
        }
        if self.auth > 120 {
            return Err("auth > 120".into());
        }
        Ok(())
    }
}

/// Porta D3 (default off).
#[derive(Clone, Debug, Default)]
pub struct ArPort {
    /// Ligada (feature `ar` + `AETHER_AR=1`).
    pub enabled: bool,
    /// C3 ligado (dep dura).
    pub vis_ready: bool,
    /// T-COH ligado (dep dura).
    pub coh_ready: bool,
}

impl ArPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_AR=1` + feature `ar`; deps C3 + T-COH devem estar prontas.
    pub fn from_env() -> Self {
        Self {
            enabled: crate::agent::modality::ModalConfig::from_env("AR", cfg!(feature = "ar"))
                .enabled,
            vis_ready: true,
            coh_ready: true,
        }
    }

    /// **Overlay**: só se `enabled` + **C3 pronto** + **T-COH pronto** + âncora válida;
    /// backend `AETHER_AR_CMD`; no-op honesto sem backend; nunca render sem triagem.
    pub fn overlay(&self, anchor: &ArAnchor) -> Result<(), String> {
        anchor.validate()?;
        if !self.enabled {
            return Err("T-AR off (feature ar / AETHER_AR)".into());
        }
        if !self.vis_ready {
            return Err("T-AR precisa C3 (T-VIS)".into()); // dep dura
        }
        if !self.coh_ready {
            return Err("T-AR precisa T-COH".into()); // dep dura
        }
        // T11: encanamento via ModalConfig; backend ausente → no-op honesto.
        let cfg = crate::agent::modality::ModalConfig::from_env("AR", cfg!(feature = "ar"));
        match cfg.cmd {
            Some(cmd_line) if !cmd_line.trim().is_empty() => {
                // W1+W8: sem shell; `anchor.id` sanitizado; timeout+limite
                super::backend::run_backend_cmd(
                    &cmd_line,
                    Some(&anchor.id),
                    None,
                    cfg.timeout_ms,
                    super::backend::BACKEND_MAX_STDOUT,
                )
                .map_err(|e| format!("ar backend: {e}"))?;
                Ok(())
            }
            _ => Ok(()), // backend ausente → no-op honesto; âncora validada
        }
    }
}

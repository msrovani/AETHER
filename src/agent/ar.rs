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
        let env_on = std::env::var("AETHER_AR").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);
        Self { enabled: env_on && cfg!(feature = "ar"), vis_ready: true, coh_ready: true }
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
        match std::env::var("AETHER_AR_CMD") {
            Ok(cmd_line) if !cmd_line.trim().is_empty() => {
                let mut parts = cmd_line.split_whitespace();
                let prog = parts.next().ok_or_else(|| "AETHER_AR_CMD vazio".to_string())?;
                let mut args: Vec<String> = parts.map(|s| s.to_string()).collect();
                args.push(anchor.id.clone());
                let output = std::process::Command::new(prog).args(&args).output().map_err(|e| format!("ar spawn: {e}"))?;
                if !output.status.success() {
                    return Err(format!("ar status {}", output.status));
                }
                Ok(())
            }
            _ => Ok(()), // backend ausente → no-op honesto; âncora validada
        }
    }
}

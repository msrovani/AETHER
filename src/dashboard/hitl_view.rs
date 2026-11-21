//! T-HITL no hub — fila Approve/Deny/Ampliar + token + kill switch.
//!
//! High sem token bloqueado; com token promove. 0 auto-255.

use crate::agent::{
    agent_kill_switch, apply_hitl, class_for_tool, hitl_token_ok, requirement_for, Goal, HitlDecision,
    HitlKind, HitlRequest, HitlRequirement, Risk, RISK_MATRIX,
};

/// Banda visual da matriz A7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitlBand {
    /// Verde — Low.
    Green,
    /// Amarelo — Medium.
    Yellow,
    /// Vermelho — High / kill.
    Red,
}

impl HitlBand {
    /// Rótulo UI.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Green => "verde",
            Self::Yellow => "amarelo",
            Self::Red => "vermelho",
        }
    }

    /// A partir do risco da tool.
    pub fn from_risk(risk: Risk) -> Self {
        match risk {
            Risk::Low => Self::Green,
            Risk::Medium => Self::Yellow,
            Risk::High => Self::Red,
        }
    }
}

/// Item da fila HITL no hub.
#[derive(Clone, Debug)]
pub struct HitlQueueItem {
    /// Pedido tipado.
    pub request: HitlRequest,
    /// Nome da tool / affordance.
    pub tool: String,
    /// Risco.
    pub risk: Risk,
    /// Banda.
    pub band: HitlBand,
    /// Classe A7.
    pub class: String,
}

/// Snapshot da borda HITL no dashboard.
#[derive(Clone, Debug)]
pub struct HitlView {
    /// Fila pendente.
    pub queue: Vec<HitlQueueItem>,
    /// Token digitado no hub (não persistido).
    pub token_input: String,
    /// Kill switch ativo.
    pub kill: bool,
    /// Última decisão aplicada.
    pub last_decision: Option<String>,
    /// Erro de gate (High sem token, etc.).
    pub last_error: Option<String>,
}

impl Default for HitlView {
    fn default() -> Self {
        Self::load()
    }
}

impl HitlView {
    /// Carrega estado + fila demo (goal.propose High).
    pub fn load() -> Self {
        let demo = HitlRequest {
            request_id: 1,
            kind: HitlKind::Ask,
            reason: "goal.propose endógeno — sanção humana".into(),
            from_authority: 96,
            to_authority: 255,
            subject: Some("goal.propose".into()),
            timeout_ms: 0,
        };
        let risk = Risk::High;
        Self {
            queue: vec![HitlQueueItem {
                request: demo,
                tool: "goal.propose".into(),
                risk,
                band: HitlBand::from_risk(risk),
                class: class_for_tool("goal.propose", risk).to_string(),
            }],
            token_input: String::new(),
            kill: agent_kill_switch(),
            last_decision: None,
            last_error: None,
        }
    }

    /// Linha de programa.
    pub fn program_line(&self) -> String {
        let n = self.queue.len();
        let kill = if self.kill { "KILL" } else { "ok" };
        format!("T-HITL hub · fila {n} · {kill}")
    }

    /// Detalhe hover.
    pub fn detail(&self) -> String {
        let mut lines = vec![
            self.program_line(),
            format!("kill AETHER_AGENT_KILL={}", self.kill),
            format!("token_ok={}", self.token_matches()),
            "High sem token → bloqueado; 0 auto-255.".into(),
        ];
        for row in RISK_MATRIX {
            lines.push(format!(
                "matriz {} risk={:?} hitl={:?}",
                row.class, row.risk, row.hitl
            ));
        }
        if let Some(e) = &self.last_error {
            lines.push(format!("erro: {e}"));
        }
        if let Some(d) = &self.last_decision {
            lines.push(format!("última: {d}"));
        }
        lines.join("\n")
    }

    fn token_matches(&self) -> bool {
        if self.token_input.trim().is_empty() {
            return false;
        }
        let mut g = Goal::fact("hitl", "token");
        g.query = Some(format!(
            "{{\"hitl_token\":{}}}",
            json_escape(&self.token_input)
        ));
        hitl_token_ok(&g)
    }

    /// Approve / Deny / Ampliar o item `idx`.
    pub fn decide(&mut self, idx: usize, kind: HitlKind) -> Result<HitlDecision, String> {
        if self.kill {
            return Err("kill switch ativo".into());
        }
        let item = self
            .queue
            .get(idx)
            .ok_or_else(|| "fila vazia / índice".to_string())?
            .clone();
        let req_hitl = requirement_for(item.risk);
        let token_ok = self.token_matches();
        if matches!(req_hitl, HitlRequirement::Always) && !token_ok {
            let msg = "High sem hitl_token — bloqueado na UI".to_string();
            self.last_error = Some(msg.clone());
            return Err(msg);
        }
        let approved = !matches!(kind, HitlKind::Deny);
        let mut req = item.request.clone();
        req.kind = kind;
        let decision = apply_hitl(&req, approved, token_ok, false);
        if matches!(kind, HitlKind::Promote | HitlKind::Ask | HitlKind::ApproveMerge)
            && req.to_authority >= 255
            && !decision.approved
        {
            let msg = decision.detail.clone();
            self.last_error = Some(msg.clone());
            return Err(msg);
        }
        self.last_decision = Some(format!(
            "{:?} approved={} auth={}",
            kind, decision.approved, decision.authority
        ));
        self.last_error = None;
        if idx < self.queue.len() {
            self.queue.remove(idx);
        }
        Ok(decision)
    }
}

fn json_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Carrega view (API espelhando dec_view).
pub fn load_hitl_view() -> HitlView {
    HitlView::load()
}

/// Linha de programa.
pub fn hitl_program_line(view: &HitlView) -> String {
    view.program_line()
}

/// Texto Detalhe.
pub fn hitl_detail(view: &HitlView) -> String {
    view.detail()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_sem_token_bloqueia() {
        let mut v = HitlView::load();
        v.token_input.clear();
        assert!(v.decide(0, HitlKind::Ask).is_err());
    }

    #[test]
    fn band_high_red() {
        assert_eq!(HitlBand::from_risk(Risk::High), HitlBand::Red);
    }
}

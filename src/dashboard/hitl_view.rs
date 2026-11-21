//! T-HITL no hub — fila viva (outbox) Approve/Deny/Ampliar + promote 255 n-sgdb.
//!
//! High sem prova bloqueado; com token promove. 0 auto-255. Um outbox para todos.

use crate::agent::{
    agent_kill_switch, append_decision, apply_hitl, class_for_tool, hitl_load_pending,
    hitl_token_ok, promote_key_255, requirement_for, Goal, HitlDecision, HitlKind, HitlRequest,
    HitlRequirement, Risk, RISK_MATRIX,
};

/// Banda visual da matriz A7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitlBand {
    Green,
    Yellow,
    Red,
}

impl HitlBand {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Green => "verde",
            Self::Yellow => "amarelo",
            Self::Red => "vermelho",
        }
    }

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
    pub request: HitlRequest,
    pub tool: String,
    pub risk: Risk,
    pub band: HitlBand,
    pub class: String,
}

/// Snapshot da borda HITL no dashboard.
#[derive(Clone, Debug)]
pub struct HitlView {
    pub queue: Vec<HitlQueueItem>,
    pub token_input: String,
    pub kill: bool,
    pub last_decision: Option<String>,
    pub last_error: Option<String>,
    /// Spec SGDB para promote 255 (env).
    pub sgdb_spec: Option<String>,
}

impl Default for HitlView {
    fn default() -> Self {
        Self::load()
    }
}

impl HitlView {
    /// Carrega outbox vivo; se vazio, mantém item demo High (economia de UX).
    pub fn load() -> Self {
        let kill = agent_kill_switch();
        let sgdb_spec = std::env::var("AETHER_SGDB_STDIO")
            .or_else(|_| std::env::var("AETHER_SGDB_URL"))
            .ok();
        let mut queue: Vec<HitlQueueItem> = hitl_load_pending()
            .into_iter()
            .map(|request| {
                let risk = if matches!(request.kind, HitlKind::Promote | HitlKind::ShellExec) {
                    Risk::High
                } else {
                    Risk::High
                };
                let tool = match request.kind {
                    HitlKind::Promote => "memory.promote",
                    HitlKind::ShellExec => "shell.exec",
                    HitlKind::ExpandResearch => "research.expand",
                    _ => "goal.propose",
                };
                HitlQueueItem {
                    tool: tool.into(),
                    class: class_for_tool(tool, risk).to_string(),
                    band: HitlBand::from_risk(risk),
                    risk,
                    request,
                }
            })
            .collect();
        if queue.is_empty() {
            queue.push(HitlQueueItem {
                request: HitlRequest {
                    request_id: 1,
                    kind: HitlKind::Ask,
                    reason: "demo: goal.propose — aguardando outbox vivo".into(),
                    from_authority: 96,
                    to_authority: 255,
                    subject: Some("goal.propose".into()),
                    timeout_ms: 0,
                },
                tool: "goal.propose".into(),
                risk: Risk::High,
                band: HitlBand::Red,
                class: class_for_tool("goal.propose", Risk::High).to_string(),
            });
        }
        Self {
            queue,
            token_input: String::new(),
            kill,
            last_decision: None,
            last_error: None,
            sgdb_spec,
        }
    }

    pub fn program_line(&self) -> String {
        let n = self.queue.len();
        let kill = if self.kill { "KILL" } else { "ok" };
        format!("T-HITL hub · fila {n} · {kill}")
    }

    pub fn detail(&self) -> String {
        let mut lines = vec![
            self.program_line(),
            format!("kill AETHER_AGENT_KILL={}", self.kill),
            format!("token_ok={}", self.token_matches()),
            "outbox: target/aether_hitl_queue.jsonl".into(),
            "High sem prova → bloqueado; 0 auto-255; DB=n-sgdb.".into(),
        ];
        for row in RISK_MATRIX {
            lines.push(format!(
                "matriz {} risk={:?} hitl={:?}",
                row.class, row.risk, row.hitl
            ));
        }
        if let Some(item) = self.queue.first() {
            if let Some(key) = &item.request.subject {
                lines.push(format!(
                    "pendente key/subject={key} auth {}→{}",
                    item.request.from_authority, item.request.to_authority
                ));
            }
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
        // Preserve promote kind when approving memory key.
        if matches!(item.request.kind, HitlKind::Promote) && approved {
            req.kind = HitlKind::Promote;
        } else {
            req.kind = kind;
        }
        let decision = apply_hitl(&req, approved, token_ok, false);
        let _ = append_decision(&decision);

        if decision.approved
            && matches!(req.kind, HitlKind::Promote)
            && decision.authority >= 255
        {
            if let Some(key) = &req.subject {
                if key.starts_with("md/") {
                    if let Err(e) = promote_key_255(self.sgdb_spec.as_deref(), key) {
                        self.last_error = Some(format!("promote n-sgdb: {e}"));
                    }
                }
            }
        }

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

pub fn load_hitl_view() -> HitlView {
    HitlView::load()
}

pub fn hitl_program_line(view: &HitlView) -> String {
    view.program_line()
}

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

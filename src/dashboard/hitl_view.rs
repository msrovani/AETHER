//! T-HITL no hub — fila viva (outbox) + histórico de decisões + Approve/Deny/Ampliar.
//!
//! High sem prova bloqueado; com token promove. 0 auto-255. Um outbox para todos.

use crate::agent::{
    abort_rollback, agent_kill_switch, append_decision, apply_hitl, class_for_tool,
    cutover_promote, hitl_load_decisions, hitl_load_unresolved, hitl_token_ok,
    mark_download_approved, promote_key_255, queue_exists, requirement_for, rewrite_queue_without,
    DecUpgradeState, Goal, HitlDecision, HitlKind, HitlRequest, HitlRequirement, Risk,
    HITL_DECISIONS_PATH, HITL_QUEUE_PATH, RISK_MATRIX,
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

/// Resumo de decisão para o painel de histórico.
#[derive(Clone, Debug)]
pub struct HitlDecisionSummary {
    pub request_id: u64,
    pub approved: bool,
    pub authority: u8,
    pub action: String,
    pub detail: String,
    pub timed_out: bool,
}

impl HitlDecisionSummary {
    fn from_decision(d: &HitlDecision) -> Self {
        Self {
            request_id: d.request_id,
            approved: d.approved,
            authority: d.authority,
            action: d.action.label().to_string(),
            detail: d.detail.clone(),
            timed_out: d.timed_out,
        }
    }

    /// Linha curta para a UI.
    pub fn line(&self) -> String {
        let ver = if self.timed_out {
            "timeout"
        } else if self.approved {
            "ok"
        } else {
            "deny"
        };
        let detail: String = self.detail.chars().take(72).collect();
        format!(
            "#{id} {ver} auth={auth} {act} · {detail}",
            id = self.request_id,
            auth = self.authority,
            act = self.action,
            detail = detail
        )
    }
}

/// Snapshot da borda HITL no dashboard.
#[derive(Clone, Debug)]
pub struct HitlView {
    pub queue: Vec<HitlQueueItem>,
    /// Decisões recentes (mais novas primeiro).
    pub history: Vec<HitlDecisionSummary>,
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
    /// Carrega outbox filtrada + histórico; demo só se outbox ausente.
    pub fn load() -> Self {
        let kill = agent_kill_switch();
        let sgdb_spec = std::env::var("AETHER_SGDB_STDIO")
            .or_else(|_| std::env::var("AETHER_SGDB_URL"))
            .ok();
        let history: Vec<HitlDecisionSummary> = {
            let mut h: Vec<_> = hitl_load_decisions()
                .iter()
                .map(HitlDecisionSummary::from_decision)
                .collect();
            h.reverse();
            h.truncate(48);
            h
        };
        let mut queue: Vec<HitlQueueItem> = hitl_load_unresolved()
            .into_iter()
            .map(item_from_request)
            .collect();
        // Demo só quando não há arquivo de outbox (primeira abertura).
        if queue.is_empty() && !queue_exists() {
            queue.push(item_from_request(HitlRequest {
                request_id: 1,
                kind: HitlKind::Ask,
                reason: "demo: goal.propose — aguardando outbox vivo".into(),
                from_authority: 96,
                to_authority: 255,
                subject: Some("goal.propose".into()),
                timeout_ms: 0,
            }));
        }
        Self {
            queue,
            history,
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

    /// Ficha do item `idx` (ou resumo geral se vazio).
    pub fn item_detail(&self, idx: usize) -> String {
        let Some(item) = self.queue.get(idx) else {
            return self.detail();
        };
        let sub = item.request.subject.as_deref().unwrap_or("(sem subject)");
        [
            format!("id={}", item.request.request_id),
            format!("kind={}", item.request.kind.label()),
            format!(
                "tool={} · class={} · banda={}",
                item.tool,
                item.class,
                item.band.label()
            ),
            format!(
                "auth {} → {}",
                item.request.from_authority, item.request.to_authority
            ),
            format!("subject={sub}"),
            format!("reason={}", item.request.reason),
            format!("timeout_ms={}", item.request.timeout_ms),
            format!("outbox={HITL_QUEUE_PATH}"),
            format!("decisions={HITL_DECISIONS_PATH}"),
        ]
        .join("\n")
    }

    /// Linha curta para a lista.
    pub fn list_line(item: &HitlQueueItem) -> String {
        let sub = item.request.subject.as_deref().unwrap_or("—");
        let sub_short: String = sub.chars().take(36).collect();
        format!(
            "{} · {} · {}→{} · {}",
            item.request.kind.label(),
            sub_short,
            item.request.from_authority,
            item.request.to_authority,
            item.band.label()
        )
    }

    pub fn detail(&self) -> String {
        let mut lines = vec![
            self.program_line(),
            format!("kill AETHER_AGENT_KILL={}", self.kill),
            format!("token_ok={}", self.token_matches()),
            format!("outbox: {HITL_QUEUE_PATH}"),
            format!("decisions: {HITL_DECISIONS_PATH}"),
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
        let approved = !matches!(kind, HitlKind::Deny);
        // Deny é caminho seguro: não exige token. Approve/Ampliar em High sim.
        if approved && matches!(req_hitl, HitlRequirement::Always) && !token_ok {
            let msg = "High sem hitl_token — bloqueado na UI (Approve/Ampliar)".to_string();
            self.last_error = Some(msg.clone());
            return Err(msg);
        }
        let mut req = item.request.clone();
        // Preserve promote / T-DEC upgrade kinds on Approve.
        if approved
            && matches!(
                item.request.kind,
                HitlKind::Promote | HitlKind::DecUpgradeDownload | HitlKind::DecUpgradeCutover
            )
        {
            req.kind = item.request.kind;
        } else {
            req.kind = kind;
        }
        let decision = apply_hitl(&req, approved, token_ok, false);
        let _ = append_decision(&decision);
        let _ = rewrite_queue_without(req.request_id);

        if decision.approved && matches!(req.kind, HitlKind::Promote) && decision.authority >= 255 {
            if let Some(key) = &req.subject {
                if key.starts_with("md/") {
                    if let Err(e) = promote_key_255(self.sgdb_spec.as_deref(), key) {
                        self.last_error = Some(format!("promote n-sgdb: {e}"));
                    }
                }
            }
        }

        // T-DEC dual-bank: download → job; cutover → promote; deny → rollback.
        if matches!(
            item.request.kind,
            HitlKind::DecUpgradeDownload | HitlKind::DecUpgradeCutover
        ) {
            let mut st = DecUpgradeState::load();
            if decision.approved && matches!(item.request.kind, HitlKind::DecUpgradeDownload) {
                if let Err(e) = mark_download_approved(&mut st) {
                    self.last_error = Some(e.clone());
                    return Err(e);
                }
            } else if decision.approved && matches!(item.request.kind, HitlKind::DecUpgradeCutover)
            {
                if let Err(e) = cutover_promote(&mut st) {
                    self.last_error = Some(e.clone());
                    return Err(e);
                }
            } else if !decision.approved {
                let _ = abort_rollback(&mut st, format!("HITL deny: {}", decision.detail));
            }
        }

        if matches!(
            kind,
            HitlKind::Promote
                | HitlKind::Ask
                | HitlKind::ApproveMerge
                | HitlKind::DecUpgradeDownload
                | HitlKind::DecUpgradeCutover
        ) && req.to_authority >= 255
            && !decision.approved
        {
            let msg = decision.detail.clone();
            self.last_error = Some(msg.clone());
            // Já gravamos decisão + reescrevemos outbox; remove da fila em memória.
            if idx < self.queue.len() {
                self.queue.remove(idx);
            }
            self.history
                .insert(0, HitlDecisionSummary::from_decision(&decision));
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
        self.history
            .insert(0, HitlDecisionSummary::from_decision(&decision));
        if self.history.len() > 48 {
            self.history.truncate(48);
        }
        Ok(decision)
    }
}

fn item_from_request(request: HitlRequest) -> HitlQueueItem {
    let risk = Risk::High;
    let tool = match request.kind {
        HitlKind::Promote => "memory.promote",
        HitlKind::ShellExec => "shell.exec",
        HitlKind::ExpandResearch => "research.expand",
        HitlKind::DecUpgradeDownload => "dec.upgrade.download",
        HitlKind::DecUpgradeCutover => "dec.upgrade.cutover",
        _ => "goal.propose",
    };
    HitlQueueItem {
        tool: tool.into(),
        class: class_for_tool(tool, risk).to_string(),
        band: HitlBand::from_risk(risk),
        risk,
        request,
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
    fn high_sem_token_bloqueia_approve() {
        // `decide` toca a outbox global em target/ — trava contra paralelismo.
        let _g = crate::agent::hitl_queue::HITL_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let mut v = HitlView {
            queue: vec![item_from_request(HitlRequest {
                request_id: 9_999_001,
                kind: HitlKind::Ask,
                reason: "test".into(),
                from_authority: 96,
                to_authority: 255,
                subject: Some("test".into()),
                timeout_ms: 0,
            })],
            history: Vec::new(),
            token_input: String::new(),
            kill: false,
            last_decision: None,
            last_error: None,
            sgdb_spec: None,
        };
        assert!(v.decide(0, HitlKind::Ask).is_err());
    }

    #[test]
    fn deny_sem_token_passa() {
        let _g = crate::agent::hitl_queue::HITL_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let mut v = HitlView {
            queue: vec![item_from_request(HitlRequest {
                request_id: 9_999_002,
                kind: HitlKind::Promote,
                reason: "test deny".into(),
                from_authority: 96,
                to_authority: 255,
                subject: Some("md/L3/test-deny".into()),
                timeout_ms: 0,
            })],
            history: Vec::new(),
            token_input: String::new(),
            kill: false,
            last_decision: None,
            last_error: None,
            sgdb_spec: None,
        };
        // Não toca outbox real se rewrite falhar em path — decide ainda reescreve.
        // Usa id isolado; restore não necessário se rewrite só remove este id.
        let r = v.decide(0, HitlKind::Deny);
        assert!(r.is_ok(), "{r:?}");
        assert!(v.queue.is_empty());
    }

    #[test]
    fn band_high_red() {
        assert_eq!(HitlBand::from_risk(Risk::High), HitlBand::Red);
    }
}

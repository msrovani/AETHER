//! T-HITL — canal humano unificado (`HitlRequest` → `HitlDecision`).
//!
//! Cobre promote/deny/ampliar. Timeout ou ausência de token → abstain.
//! Nunca promove auth 255 sem `hitl_token` explícito.

use crate::json::Json;

use super::tool::{AUTHORITY_HITL, AUTHORITY_QUARANTINE};
use super::Action;

/// Tipo de pedido HITL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitlKind {
    /// Promover autoridade (ex.: 96/120 → 255).
    Promote,
    /// Negar / abortar ação.
    Deny,
    /// Ampliar pesquisa (F2.5).
    ExpandResearch,
    /// Aprovar merge/patch (coder).
    ApproveMerge,
    /// Shell/build sob K8 (nunca default).
    ShellExec,
    /// T-DEC: autorizar fetch HF do candidate (rede opt-in).
    DecUpgradeDownload,
    /// T-DEC: cutover candidate → active (último passo; desarma fallback).
    DecUpgradeCutover,
    /// Escalação genérica (`ask.human`).
    Ask,
}

impl HitlKind {
    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Deny => "deny",
            Self::ExpandResearch => "expand_research",
            Self::ApproveMerge => "approve_merge",
            Self::ShellExec => "shell_exec",
            Self::DecUpgradeDownload => "dec_upgrade_download",
            Self::DecUpgradeCutover => "dec_upgrade_cutover",
            Self::Ask => "ask",
        }
    }
}

/// Pedido tipado ao humano.
#[derive(Clone, Debug, PartialEq)]
pub struct HitlRequest {
    /// Identidade do pedido (geralmente `trace_id`).
    pub request_id: u64,
    /// Tipo.
    pub kind: HitlKind,
    /// Motivo legível.
    pub reason: String,
    /// Autoridade atual do claim (se houver).
    pub from_authority: u8,
    /// Autoridade pedida (255 só com token).
    pub to_authority: u8,
    /// Chave / subject opcional.
    pub subject: Option<String>,
    /// Timeout em ms (`0` = sem timeout no host).
    pub timeout_ms: u64,
}

impl HitlRequest {
    /// Escalação genérica a partir de `ask.human`.
    pub fn ask(request_id: u64, reason: impl Into<String>) -> Self {
        Self {
            request_id,
            kind: HitlKind::Ask,
            reason: reason.into(),
            from_authority: AUTHORITY_QUARANTINE,
            to_authority: AUTHORITY_HITL,
            subject: None,
            timeout_ms: 0,
        }
    }

    /// Pedido de promoção de autoridade.
    pub fn promote(
        request_id: u64,
        from: u8,
        to: u8,
        subject: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            request_id,
            kind: HitlKind::Promote,
            reason: reason.into(),
            from_authority: from,
            to_authority: to.min(AUTHORITY_HITL),
            subject: Some(subject.into()),
            timeout_ms: 0,
        }
    }

    /// Serializa para audit/API.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("request_id", Json::num(self.request_id as f64)),
            ("kind", Json::str(self.kind.label())),
            ("reason", Json::str(&self.reason)),
            ("from_authority", Json::num(f64::from(self.from_authority))),
            ("to_authority", Json::num(f64::from(self.to_authority))),
            (
                "subject",
                self.subject
                    .as_ref()
                    .map(|s| Json::str(s))
                    .unwrap_or(Json::Null),
            ),
            ("timeout_ms", Json::num(self.timeout_ms as f64)),
        ])
    }
}

/// Decisão humana (ou timeout).
#[derive(Clone, Debug, PartialEq)]
pub struct HitlDecision {
    /// Pedido correspondente.
    pub request_id: u64,
    /// Aprovado?
    pub approved: bool,
    /// Token HITL apresentado (nunca logar o valor em claro em prod).
    pub token_present: bool,
    /// Timeout estourou?
    pub timed_out: bool,
    /// Autoridade efetiva após a decisão.
    pub authority: u8,
    /// Ação prescrita ao host.
    pub action: Action,
    /// Motivo / audit.
    pub detail: String,
}

impl HitlDecision {
    /// Audit trail JSON.
    pub fn audit_json(&self) -> Json {
        Json::obj([
            ("request_id", Json::num(self.request_id as f64)),
            ("approved", Json::Bool(self.approved)),
            ("token_present", Json::Bool(self.token_present)),
            ("timed_out", Json::Bool(self.timed_out)),
            ("authority", Json::num(f64::from(self.authority))),
            ("action", Json::str(self.action.label())),
            ("detail", Json::str(&self.detail)),
        ])
    }
}

/// Aplica a decisão humana. **Nunca** devolve auth 255 sem `token_ok`.
pub fn apply_hitl(
    req: &HitlRequest,
    approved: bool,
    token_ok: bool,
    timed_out: bool,
) -> HitlDecision {
    if timed_out {
        return HitlDecision {
            request_id: req.request_id,
            approved: false,
            token_present: token_ok,
            timed_out: true,
            authority: req.from_authority.min(AUTHORITY_QUARANTINE),
            action: Action::Abstain,
            detail: "hitl timeout → abstain (sem auto-255)".into(),
        };
    }
    if !approved {
        return HitlDecision {
            request_id: req.request_id,
            approved: false,
            token_present: token_ok,
            timed_out: false,
            authority: req.from_authority,
            action: Action::Abstain,
            detail: format!("hitl deny: {}", req.reason),
        };
    }
    match req.kind {
        HitlKind::Deny => HitlDecision {
            request_id: req.request_id,
            approved: false,
            token_present: token_ok,
            timed_out: false,
            authority: req.from_authority,
            action: Action::Abstain,
            detail: "hitl kind=deny".into(),
        },
        HitlKind::Promote
        | HitlKind::ApproveMerge
        | HitlKind::DecUpgradeDownload
        | HitlKind::DecUpgradeCutover => {
            if req.to_authority >= AUTHORITY_HITL && !token_ok {
                return HitlDecision {
                    request_id: req.request_id,
                    approved: false,
                    token_present: false,
                    timed_out: false,
                    authority: req.from_authority.min(120),
                    action: Action::Abstain,
                    detail: "promote/cutover 255 exige token HITL".into(),
                };
            }
            let authority = if token_ok {
                req.to_authority.min(AUTHORITY_HITL)
            } else {
                req.to_authority.min(120)
            };
            HitlDecision {
                request_id: req.request_id,
                approved: true,
                token_present: token_ok,
                timed_out: false,
                authority,
                action: Action::Act,
                detail: format!(
                    "hitl {} {}→{} token={}",
                    req.kind.label(),
                    req.from_authority,
                    authority,
                    token_ok
                ),
            }
        }
        HitlKind::ExpandResearch | HitlKind::Ask => HitlDecision {
            request_id: req.request_id,
            approved: true,
            token_present: token_ok,
            timed_out: false,
            authority: req.from_authority.min(120),
            action: Action::Propose,
            detail: format!("hitl {} aprovado (auth≤120)", req.kind.label()),
        },
        HitlKind::ShellExec => {
            if !token_ok {
                return HitlDecision {
                    request_id: req.request_id,
                    approved: false,
                    token_present: false,
                    timed_out: false,
                    authority: req.from_authority.min(120),
                    action: Action::Abstain,
                    detail: "shell.exec exige token HITL (K8)".into(),
                };
            }
            HitlDecision {
                request_id: req.request_id,
                approved: true,
                token_present: true,
                timed_out: false,
                authority: req.to_authority.min(120),
                action: Action::Act,
                detail: "hitl shell_exec aprovado (auth≤120; nunca 255 auto)".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_abstem_sem_promover() {
        let req = HitlRequest::promote(1, 96, 255, "k", "sancao");
        let d = apply_hitl(&req, true, true, true);
        assert_eq!(d.action, Action::Abstain);
        assert!(d.authority < AUTHORITY_HITL);
    }

    #[test]
    fn promote_255_sem_token_nega() {
        let req = HitlRequest::promote(2, 100, 255, "k", "sem token");
        let d = apply_hitl(&req, true, false, false);
        assert!(!d.approved);
        assert_eq!(d.action, Action::Abstain);
        assert!(d.authority <= 120);
    }

    #[test]
    fn promote_255_com_token_ok() {
        let req = HitlRequest::promote(3, 100, 255, "k", "ok");
        let d = apply_hitl(&req, true, true, false);
        assert!(d.approved);
        assert_eq!(d.authority, AUTHORITY_HITL);
        assert_eq!(d.action, Action::Act);
    }
}

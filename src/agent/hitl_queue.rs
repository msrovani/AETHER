//! Outbox único T-HITL — `HitlRequest` / decisions append-only (fio para todos).
//!
//! Um arquivo, hub + API consomem; sem segunda fila.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::json::Json;

use super::hitl::{HitlDecision, HitlKind, HitlRequest};
use super::tool::AUTHORITY_QUARANTINE;

/// Path canônico do outbox de pedidos.
pub const HITL_QUEUE_PATH: &str = "target/aether_hitl_queue.jsonl";
/// Path canônico do audit de decisões.
pub const HITL_DECISIONS_PATH: &str = "target/aether_hitl_decisions.jsonl";

/// Enfileira pedido (append-only).
pub fn enqueue(req: &HitlRequest) -> Result<(), String> {
    append_jsonl(HITL_QUEUE_PATH, &req.to_json())
}

/// Enfileira promote de memória (key n-sgdb no subject).
pub fn enqueue_promote(
    request_id: u64,
    key: impl Into<String>,
    from_auth: u8,
    reason: impl Into<String>,
) -> Result<(), String> {
    let req = HitlRequest {
        request_id,
        kind: HitlKind::Promote,
        reason: reason.into(),
        from_authority: from_auth.min(AUTHORITY_QUARANTINE.max(120)),
        to_authority: 255,
        subject: Some(key.into()),
        timeout_ms: 0,
    };
    enqueue(&req)
}

/// Lê pedidos da outbox (todas as linhas; hub filtra já decididos).
pub fn load_pending() -> Vec<HitlRequest> {
    read_jsonl(HITL_QUEUE_PATH)
        .into_iter()
        .filter_map(parse_request)
        .collect()
}

/// `true` se o arquivo da outbox existe (mesmo vazio).
pub fn queue_exists() -> bool {
    Path::new(HITL_QUEUE_PATH).is_file()
}

/// Lê decisões do audit (ordem de arquivo = cronológica).
pub fn load_decisions() -> Vec<HitlDecision> {
    read_jsonl(HITL_DECISIONS_PATH)
        .into_iter()
        .filter_map(parse_decision)
        .collect()
}

/// Conjunto de `request_id` já decididos (última decisão vence para o id).
pub fn decided_ids() -> std::collections::HashSet<u64> {
    load_decisions().into_iter().map(|d| d.request_id).collect()
}

/// Pedidos ainda pendentes: outbox menos ids com decisão.
pub fn load_unresolved() -> Vec<HitlRequest> {
    let done = decided_ids();
    load_pending()
        .into_iter()
        .filter(|r| !done.contains(&r.request_id))
        .collect()
}

/// Reescreve a outbox sem o `request_id` resolvido (decisões ficam append-only).
pub fn rewrite_queue_without(request_id: u64) -> Result<(), String> {
    let kept: Vec<HitlRequest> = load_pending()
        .into_iter()
        .filter(|r| r.request_id != request_id)
        .collect();
    let p = PathBuf::from(HITL_QUEUE_PATH);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut body = String::new();
    for req in &kept {
        body.push_str(&req.to_json().to_string());
        body.push('\n');
    }
    fs::write(&p, body).map_err(|e| e.to_string())
}

/// Append decisão (audit).
pub fn append_decision(dec: &HitlDecision) -> Result<(), String> {
    append_jsonl(HITL_DECISIONS_PATH, &dec.audit_json())
}

fn append_jsonl(path: &str, value: &Json) -> Result<(), String> {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{}", value).map_err(|e| e.to_string())
}

fn read_jsonl(path: &str) -> Vec<Json> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| Json::parse(l).ok())
        .collect()
}

fn parse_request(j: Json) -> Option<HitlRequest> {
    let id = j.get("request_id")?.as_u64()?;
    let kind = match j.get("kind")?.as_str()? {
        "promote" => HitlKind::Promote,
        "deny" => HitlKind::Deny,
        "expand_research" => HitlKind::ExpandResearch,
        "approve_merge" => HitlKind::ApproveMerge,
        "shell_exec" => HitlKind::ShellExec,
        "dec_upgrade_download" => HitlKind::DecUpgradeDownload,
        "dec_upgrade_cutover" => HitlKind::DecUpgradeCutover,
        _ => HitlKind::Ask,
    };
    Some(HitlRequest {
        request_id: id,
        kind,
        reason: j
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string(),
        from_authority: j
            .get("from_authority")
            .and_then(|a| a.as_u64())
            .unwrap_or(96) as u8,
        to_authority: j
            .get("to_authority")
            .and_then(|a| a.as_u64())
            .unwrap_or(255) as u8,
        subject: j
            .get("subject")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string()),
        timeout_ms: j.get("timeout_ms").and_then(|t| t.as_u64()).unwrap_or(0),
    })
}

fn parse_decision(j: Json) -> Option<HitlDecision> {
    use super::Action;
    let request_id = j.get("request_id")?.as_u64()?;
    let approved = j.get("approved")?.as_bool()?;
    let token_present = j
        .get("token_present")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let timed_out = j
        .get("timed_out")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let authority = j.get("authority").and_then(|a| a.as_u64()).unwrap_or(0) as u8;
    let action = match j
        .get("action")
        .and_then(|a| a.as_str())
        .unwrap_or("abstain")
    {
        "act" => Action::Act,
        "cite" => Action::Cite,
        "propose" => Action::Propose,
        _ => Action::Abstain,
    };
    let detail = j
        .get("detail")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    Some(HitlDecision {
        request_id,
        approved,
        token_present,
        timed_out,
        authority,
        action,
        detail,
    })
}

/// Path helper (testes).
pub fn queue_path() -> &'static Path {
    Path::new(HITL_QUEUE_PATH)
}

/// Trava dos testes: a outbox e o audit de decisões são arquivos globais em
/// `target/` e o `cargo test` roda testes em paralelo — sem esta trava,
/// `remove_file` de um teste corrói o `append` de outro (raça intermitente).
#[cfg(test)]
pub(crate) static HITL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Action;

    /// Guarda o arquivo compartilhado durante o teste.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        HITL_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn enqueue_e_load() {
        let _g = lock();
        let _ = fs::remove_file(HITL_QUEUE_PATH);
        enqueue_promote(42, "md/L3/test", 96, "sanção").unwrap();
        let pending = load_pending();
        assert!(!pending.is_empty());
        assert_eq!(pending.last().unwrap().request_id, 42);
        assert_eq!(
            pending.last().unwrap().subject.as_deref(),
            Some("md/L3/test")
        );
    }

    #[test]
    fn rewrite_remove_id() {
        let _g = lock();
        let _ = fs::remove_file(HITL_QUEUE_PATH);
        enqueue_promote(1001, "md/L3/a", 96, "a").unwrap();
        enqueue_promote(1002, "md/L3/b", 96, "b").unwrap();
        rewrite_queue_without(1001).unwrap();
        let left: Vec<_> = load_pending().into_iter().map(|r| r.request_id).collect();
        assert!(!left.contains(&1001));
        assert!(left.contains(&1002));
    }

    #[test]
    fn unresolved_filtra_decidido() {
        let _g = lock();
        let _ = fs::remove_file(HITL_QUEUE_PATH);
        let _ = fs::remove_file(HITL_DECISIONS_PATH);
        enqueue_promote(2001, "md/L3/x", 96, "x").unwrap();
        append_decision(&HitlDecision {
            request_id: 2001,
            approved: false,
            token_present: false,
            timed_out: false,
            authority: 96,
            action: Action::Abstain,
            detail: "test deny".into(),
        })
        .unwrap();
        assert!(load_unresolved().is_empty());
    }
}

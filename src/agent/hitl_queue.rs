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

/// Lê pedidos pendentes (todas as linhas; hub filtra já decididos se quiser).
pub fn load_pending() -> Vec<HitlRequest> {
    read_jsonl(HITL_QUEUE_PATH)
        .into_iter()
        .filter_map(parse_request)
        .collect()
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

/// Path helper (testes).
pub fn queue_path() -> &'static Path {
    Path::new(HITL_QUEUE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_e_load() {
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
}

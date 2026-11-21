//! T-HTTP — GET allowlist (A4 / G-A4). Deny-default; sem POST.
//!
//! Por default **não abre socket** (`http_fetch=false`): só valida a política.
//! Com `http_fetch=true` e host na lista, faz GET HTTP/1.1 mínimo via
//! `std::net::TcpStream`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
    AUTHORITY_QUARANTINE,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;

/// Extrai host lowercase de uma URL `http(s)://…` ou `host/path`.
pub fn normalize_host(url: &str) -> Result<String, ToolError> {
    let raw = url.trim();
    if raw.is_empty() {
        return Err(ToolError::InvalidArgs("url vazia".into()));
    }
    let without_scheme = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .unwrap_or(raw);
    let hostport = without_scheme.split('/').next().unwrap_or("");
    let host = hostport.split(':').next().unwrap_or("").trim().to_lowercase();
    if host.is_empty() || host.contains("..") {
        return Err(ToolError::InvalidArgs("host inválido".into()));
    }
    Ok(host)
}

fn host_allowed(host: &str, allow: &[String]) -> bool {
    allow.iter().any(|h| h.eq_ignore_ascii_case(host))
}

/// `http.get`
#[derive(Debug)]
pub struct HttpGetTool {
    hosts: Vec<String>,
    fetch: bool,
}

impl HttpGetTool {
    /// Nova tool com a política de mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            hosts: policy.http_hosts.clone(),
            fetch: policy.http_fetch,
        }
    }
}

impl Tool for HttpGetTool {
    fn name(&self) -> &'static str {
        "http.get"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "http.get",
            description: "GET HTTP allowlisted (sem POST); fetch opt-in",
            args: &["url"],
            kinds: &[GoalKind::Retrieve, GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost {
            ms: 25.0,
            tokens: 0,
        }
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.url.is_some()
            && (goal.relation.as_deref() == Some("http.get") || goal.kind == GoalKind::Retrieve)
        {
            if goal.relation.as_deref() == Some("http.get") {
                1.0
            } else if goal.path.is_none() && goal.mcp_name.is_none() {
                0.35
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let url = goal
            .url
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("url ausente".into()))?;
        if url.trim().to_ascii_lowercase().starts_with("https://") && self.fetch {
            // TLS fora do escopo A4 mínimo — deny honesto.
            return Err(ToolError::Precondition(
                "https exige sidecar; use http:// em fetch local ou desligue fetch".into(),
            ));
        }
        let host = normalize_host(url)?;
        if !host_allowed(&host, &self.hosts) {
            return Err(ToolError::Precondition(format!(
                "host `{host}` fora da allowlist"
            )));
        }
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let url = goal.url.as_deref().unwrap();
        let host = normalize_host(url)?;
        if !self.fetch {
            return Ok(ToolOutcome {
                claims: vec![Claim::new(
                    host.clone(),
                    "http.get",
                    "allowlist_ok_network_off",
                    1.0,
                    AUTHORITY_QUARANTINE,
                    ExpertId::Memory,
                    Provenance::Inferred,
                )],
                confidence: 1.0,
                detail: format!(
                    "http.get policy-ok fetch=false host={host} scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }
        let body = http_get_plain(url, &host)?;
        let preview: String = body.chars().take(2_048).collect();
        Ok(ToolOutcome {
            claims: vec![Claim::new(
                host,
                "http.get",
                preview,
                1.0,
                AUTHORITY_QUARANTINE,
                ExpertId::Memory,
                Provenance::Inferred,
            )],
            confidence: 1.0,
            detail: format!(
                "http.get ok scope={:?} trace={} bytes={}",
                ctx.scope,
                ctx.trace_id,
                body.len()
            ),
            ..ToolOutcome::default()
        })
    }
}

fn http_get_plain(url: &str, host: &str) -> Result<String, ToolError> {
    let path = url
        .strip_prefix("http://")
        .unwrap_or(url)
        .find('/')
        .map(|_| {
            let rest = &url[url.find("://").map(|j| j + 3).unwrap_or(0)..];
            &rest[rest.find('/').unwrap_or(rest.len())..]
        })
        .filter(|p| !p.is_empty())
        .unwrap_or("/");
    let path = if path.is_empty() { "/" } else { path };
    let addr = format!("{host}:80");
    let mut stream = TcpStream::connect(addr.as_str())
        .map_err(|e| ToolError::Failed(format!("connect: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok();
    let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| ToolError::Failed(format!("write: {e}")))?;
    let mut buf = Vec::new();
    stream
        .take(64 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| ToolError::Failed(format!("read: {e}")))?;
    let text = String::from_utf8_lossy(&buf);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_else(|| text.into_owned());
    Ok(body)
}

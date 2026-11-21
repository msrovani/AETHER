//! T-NET / A5 — `research.query` no planner (mesmo contrato F2).
//!
//! Opt-in: `AETHER_RESEARCH_NETWORK=1`. Sem rede → não inventa (claims vazias).
//! Autoridade da evidência **cap 120**. `research.expand` só via HITL.

use std::path::PathBuf;
use std::process::Command;

use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use crate::agent::{Claim, ExpertId};
use crate::core::certificate::Provenance;
use crate::json::Json;

/// Cap duro de autoridade para pesquisa (nunca 255 sem HITL).
pub const RESEARCH_AUTH_CAP: u8 = 120;

/// Rede de pesquisa habilitada? (opt-in HITL / Eixo C).
pub fn research_network_enabled() -> bool {
    match std::env::var("AETHER_RESEARCH_NETWORK") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn research_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/research_live.py")
}

/// Invoca `research_live.py` e devolve o JSON parseado.
pub fn invoke_research_live(query: &str) -> Result<Json, ToolError> {
    let script = research_script();
    if !script.is_file() {
        return Err(ToolError::Failed(format!(
            "research_live ausente: {}",
            script.display()
        )));
    }
    let out = Command::new("python")
        .arg(&script)
        .arg(query)
        .output()
        .map_err(|e| ToolError::Failed(format!("spawn research_live: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(ToolError::Failed(format!(
            "research_live exit={:?}: {err}",
            out.status.code()
        )));
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    Json::parse(raw.trim()).map_err(|e| ToolError::Failed(format!("json research: {e}")))
}

fn authority_from_json(v: &Json) -> u8 {
    let raw = v
        .get("authority")
        .and_then(|a| a.as_u64().or_else(|| a.as_f64().map(|f| f as u64)))
        .unwrap_or(60) as u8;
    raw.min(RESEARCH_AUTH_CAP)
}

/// Converte o payload F2 em claims tipadas (auth ≤120).
pub fn claims_from_research_json(query: &str, payload: &Json) -> (Vec<Claim>, String, f32) {
    if payload.get("ok").and_then(|o| o.as_bool()) == Some(false) {
        let err = payload
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("erro");
        return (
            Vec::new(),
            format!("research.query offline/erro: {err}"),
            0.0,
        );
    }
    let auth = authority_from_json(payload);
    let verdict = payload
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("open");
    let reason = payload
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("");
    let mut claims = Vec::new();
    claims.push(Claim::new(
        query,
        "research.verdict",
        verdict,
        if verdict == "settled" { 0.85 } else { 0.55 },
        auth,
        ExpertId::Memory,
        Provenance::Inferred,
    ));
    if let Some(ev) = payload.get("evidence").and_then(|e| e.as_arr()) {
        for (i, item) in ev.iter().take(5).enumerate() {
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim();
            let domain = item
                .get("domain")
                .and_then(|d| d.as_str())
                .unwrap_or("?");
            if title.is_empty() {
                continue;
            }
            let value: String = title.chars().take(240).collect();
            claims.push(Claim::new(
                format!("src{i}:{domain}"),
                "research.evidence",
                value,
                0.5,
                auth,
                ExpertId::Memory,
                Provenance::Inferred,
            ));
        }
    }
    let n = payload
        .get("n_independent")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let detail = format!(
        "research.query verdict={verdict} auth={auth} reason={reason} n_independent={n} cap={RESEARCH_AUTH_CAP}"
    );
    let conf = if verdict == "settled" {
        0.85
    } else if n > 0 {
        0.55
    } else {
        0.2
    };
    (claims, detail, conf)
}

/// `research.query` — evidência externa no loop do agent.
#[derive(Debug, Default)]
pub struct ResearchQueryTool;

impl Tool for ResearchQueryTool {
    fn name(&self) -> &'static str {
        "research.query"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "research.query",
            description: "pesquisa externa F2 (tier+independência); auth≤120; rede opt-in",
            args: &["query"],
            kinds: &[GoalKind::Retrieve, GoalKind::Answer],
        }
    }

    fn cost(&self) -> Cost {
        Cost {
            ms: 80.0,
            tokens: 0,
        }
    }

    fn risk(&self) -> Risk {
        Risk::Medium
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        // Não competir com tools de relação exclusiva (A8 predict / propose).
        match goal.relation.as_deref() {
            Some("research.query") => return 1.0,
            Some("predict.series") | Some("goal.propose") | Some("fs.read") | Some("fs.write")
            | Some("http.get") | Some("mcp.call") | Some("shell.exec") => {
                return 0.0;
            }
            _ => {}
        }
        if goal.query.is_some()
            && matches!(goal.kind, GoalKind::Retrieve | GoalKind::Answer)
            && goal.path.is_none()
            && goal.url.is_none()
            && goal.mcp_name.is_none()
            && goal.subject.is_none()
        {
            return 0.65;
        }
        // Fatos mundanos sem pack: competir com fact se query presente.
        if goal.kind == GoalKind::Answer && goal.query.is_some() {
            return 0.25;
        }
        0.0
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        if self.affordance(goal, state) <= 0.0 {
            return false;
        }
        let q_ok = goal
            .query
            .as_deref()
            .is_some_and(|q| !q.trim().is_empty());
        let s_ok = goal
            .subject
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        q_ok || (goal.relation.as_deref() == Some("research.query") && s_ok)
    }

    fn dry_run(&self, goal: &Goal, _ctx: &ToolContext) -> Result<(), ToolError> {
        let q = goal
            .query
            .as_deref()
            .or(goal.subject.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if q.is_none() {
            return Err(ToolError::InvalidArgs("query ausente".into()));
        }
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        self.dry_run(goal, ctx)?;
        let query = goal
            .query
            .as_deref()
            .or(goal.subject.as_deref())
            .unwrap()
            .trim();
        if !research_network_enabled() {
            return Ok(ToolOutcome {
                claims: Vec::new(),
                confidence: 0.0,
                detail: format!(
                    "research.query network_off scope={:?} trace={} — AETHER_RESEARCH_NETWORK=1",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }
        let payload = invoke_research_live(query)?;
        let (claims, detail, conf) = claims_from_research_json(query, &payload);
        // Cap defensivo: nenhuma claim acima de 120.
        let claims: Vec<Claim> = claims
            .into_iter()
            .map(|mut c| {
                c.authority = c.authority.min(RESEARCH_AUTH_CAP);
                c
            })
            .collect();
        Ok(ToolOutcome {
            claims,
            confidence: conf,
            detail: format!("{detail} scope={:?} trace={}", ctx.scope, ctx.trace_id),
            ..ToolOutcome::default()
        })
    }
}

/// `research.expand` — só HITL (sistema nunca amplia sozinho).
#[derive(Debug, Default)]
pub struct ResearchExpandTool;

impl Tool for ResearchExpandTool {
    fn name(&self) -> &'static str {
        "research.expand"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "research.expand",
            description: "ampliar pesquisa — somente HITL (F2.5)",
            args: &["query", "reason"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(0.0)
    }

    fn risk(&self) -> Risk {
        Risk::High
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("research.expand") {
            0.1
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn escalation(&self) -> bool {
        true
    }

    fn call(&mut self, _goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let req = crate::agent::hitl::HitlRequest {
            request_id: ctx.trace_id,
            kind: crate::agent::hitl::HitlKind::ExpandResearch,
            reason: "ampliar pesquisa exige HITL".into(),
            from_authority: 60,
            to_authority: RESEARCH_AUTH_CAP,
            subject: None,
            timeout_ms: 0,
        };
        Err(ToolError::NeedsHuman(format!(
            "research.expand HITL-only request={}",
            req.to_json()
        )))
    }
}

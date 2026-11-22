//! Barramento comum do trio T-NL-IN / T-HITL / T-NL-OUT.
//!
//! Um fio (Goal/Claim/Action/Hitl/CertifiedUtterance), disjuntor CORE,
//! filtros A7+P2+auth≤120, DB n-sgdb. Economia: recicla ParserPort,
//! MemoryCitations, certify_utterance, ToolAgent.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::certificate::Provenance;
use crate::core::knowledge_bridge_f1::ScopeId;
use crate::json::Json;
use crate::knowledge::sgdb::{open, SgdbClient, SgdbConfig, Transport};

use super::dec_status::DecTomadaStatus;
use super::experts::MemoryCitations;
use super::gemma_scribe::{
    scribe_opt_in, vram_free_from_host_report, GemmaScribePort, ScribeRuntime,
};
use super::hitl_queue::{enqueue, enqueue_promote};
use super::language::{GoalSchema, LanguageCaps, LanguagePort, ParserPort, Style};
use super::nl_out::{certify_utterance, CertifiedUtterance};
use super::tnl_in::{ABSTAIN_CEILING, TNL_IN_CONTRACT_VERSION};
use super::tool::{Goal, Risk, AUTHORITY_QUARANTINE};
use super::tool_agent::ToolAgent;
use super::{Action, AgentConfig, Claim, ExpertId, HitlKind, HitlRequest};

/// Cap de aprendizado automático (nunca 255 sem HITL).
pub const LEARN_AUTH_CAP: u8 = 120;

static HITL_REQ_SEQ: AtomicU64 = AtomicU64::new(1000);

/// Resultado de um turno no fio comum.
#[derive(Clone, Debug)]
pub struct TrioTurn {
    /// Texto para a bolha (CertifiedUtterance ou “não sei”).
    pub content: String,
    /// Verdict estável.
    pub verdict: &'static str,
    /// Payload `x_aether`.
    pub evidence: Json,
    /// Utterance se cite/act.
    pub utterance: Option<CertifiedUtterance>,
}

/// Opções do turno (economia: tudo opt-in).
#[derive(Clone, Debug, Default)]
pub struct TrioOpts {
    /// Spec SGDB (`mock`, stdio, http…).
    pub sgdb: Option<String>,
    /// Config do ToolAgent (None = default).
    pub agent_cfg: Option<AgentConfig>,
    /// Gravar aprendizado ≤120.
    pub learn: bool,
}

/// Executa o fio: parse → (OOV cite | solve) → certify → learn ≤120 → HITL se High.
pub fn run_trio_turn(text: &str, opts: &TrioOpts) -> TrioTurn {
    let mut parser = ParserPort::new();
    let schema = GoalSchema::agent_default();
    let report = parser.analyze(text, &schema);
    let caps = parser.caps();
    let dec = DecTomadaStatus::from_env();
    let scribe_meta = scribe_meta_json();

    let mut fields: Vec<(String, Json)> = vec![
        ("fio".into(), Json::str("T-NL-IN|T-HITL|T-NL-OUT")),
        ("tnl_in_version".into(), Json::str(TNL_IN_CONTRACT_VERSION)),
        (
            "parse_confidence".into(),
            Json::num(f64::from(report.confidence)),
        ),
        (
            "parse_intent".into(),
            report.intent.map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "unconsumed".into(),
            Json::Arr(
                report
                    .unconsumed
                    .iter()
                    .map(|u| Json::str(u.as_str()))
                    .collect(),
            ),
        ),
        ("language_caps".into(), caps_json(&caps)),
        ("t_dec".into(), dec_json(&dec)),
        ("t_nl_out".into(), scribe_meta),
        (
            "auth_cap_learn".into(),
            Json::num(f64::from(LEARN_AUTH_CAP)),
        ),
        ("db".into(), Json::str("n-sgdb")),
    ];

    if report.is_oov() || report.goal.is_none() {
        return trio_oov(text, report.confidence, &mut fields, opts);
    }
    let goal = report.goal.clone().unwrap();
    trio_goal(goal, report.confidence, report.intent, &mut fields, opts)
}

fn trio_oov(text: &str, conf: f32, fields: &mut Vec<(String, Json)>, opts: &TrioOpts) -> TrioTurn {
    fields.push(("abstain".into(), Json::Bool(true)));
    fields.push(("verdict".into(), Json::str("abstain")));
    fields.push(("reason".into(), Json::str("oov_or_empty")));
    fields.push(("parse_oov".into(), Json::Bool(conf <= ABSTAIN_CEILING)));

    let mut citations_json = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();
    if let Some(spec) = opts.sgdb.as_deref() {
        if let Ok(mut mem) = MemoryCitations::new(spec) {
            if let Ok(hits) = mem.recall(text, 8) {
                for (i, h) in hits.iter().take(3).enumerate() {
                    let auth = h.authority.min(LEARN_AUTH_CAP);
                    citations_json.push(Json::obj([
                        ("text", Json::str(&h.text)),
                        ("score", Json::num(f64::from(h.score))),
                        ("authority", Json::num(f64::from(auth))),
                    ]));
                    claims.push(Claim::new(
                        "memory",
                        format!("cite.{i}"),
                        h.text.chars().take(160).collect::<String>(),
                        h.score.clamp(0.05, 0.99),
                        auth,
                        ExpertId::Memory,
                        Provenance::Inferred,
                    ));
                }
            }
        }
    }
    fields.push(("memory_citations".into(), Json::Arr(citations_json)));

    if !claims.is_empty() {
        if let Some(utt) = certify_with_scribe_or_template(&claims, Action::Cite, fields) {
            if opts.learn {
                let _ = learn_text(
                    opts.sgdb.as_deref(),
                    &format!("learn/oov_cite: {}", utt.text),
                    LEARN_AUTH_CAP.min(96),
                );
            }
            set_field(fields, "abstain", Json::Bool(false));
            set_field(fields, "verdict", Json::str("cite"));
            fields.push(("certified".into(), utt.to_json()));
            return TrioTurn {
                content: utt.text.clone(),
                verdict: "cite",
                evidence: Json::Obj(fields.clone()),
                utterance: Some(utt),
            };
        }
    }

    fields.push(("certified".into(), Json::Null));
    // Mensagem honesta: abstain ≠ silêncio seco — orienta pergunta factual.
    let tip = text.trim();
    let content = if tip.chars().count() <= 16 && !tip.contains('?') {
        format!(
            "não sei — «{tip}» não é pergunta factual (OOV). Ex.: «qual a capital do Brasil?» · cite/act só com evidência."
        )
    } else {
        "não sei — sem evidência no léxico/memória para citar. Reformule como fato (sujeito + relação) ou aponte SGDB."
            .into()
    };
    TrioTurn {
        content,
        verdict: "abstain",
        evidence: Json::Obj(fields.clone()),
        utterance: None,
    }
}

fn trio_goal(
    goal: Goal,
    conf: f32,
    intent: Option<&'static str>,
    fields: &mut Vec<(String, Json)>,
    opts: &TrioOpts,
) -> TrioTurn {
    fields.push((
        "goal".into(),
        Json::obj([
            ("kind", Json::str(format!("{}", goal.kind.code()))),
            (
                "subject",
                goal.subject
                    .as_ref()
                    .map(|s| Json::str(s.as_str()))
                    .unwrap_or(Json::Null),
            ),
            (
                "relation",
                goal.relation
                    .as_ref()
                    .map(|s| Json::str(s.as_str()))
                    .unwrap_or(Json::Null),
            ),
            ("intent", intent.map(Json::str).unwrap_or(Json::Null)),
            ("confidence", Json::num(f64::from(conf))),
        ]),
    ));

    if goal.n.is_some() && goal.clauses.is_empty() && goal.j.is_empty() {
        fields.push(("abstain".into(), Json::Bool(true)));
        fields.push(("verdict".into(), Json::str("abstain")));
        fields.push(("reason".into(), Json::str("sat_qubo_needs_structure")));
        return TrioTurn {
            content: "não sei".into(),
            verdict: "abstain",
            evidence: Json::Obj(fields.clone()),
            utterance: None,
        };
    }

    let cfg = opts.agent_cfg.clone().unwrap_or_else(|| {
        let mut c = AgentConfig::default();
        if let Some(spec) = &opts.sgdb {
            c.sgdb = Some(spec.clone());
        }
        c
    });
    let mut agent = ToolAgent::new(&cfg);
    let outcome = agent.solve(&goal);

    if matches!(outcome.action, Action::Propose)
        || outcome.claims.iter().any(|c| c.authority >= 200)
    {
        let id = HITL_REQ_SEQ.fetch_add(1, Ordering::Relaxed);
        let req = HitlRequest {
            request_id: id,
            kind: HitlKind::Ask,
            reason: format!("trio turn High/propose: {}", outcome.rationale),
            from_authority: outcome
                .claims
                .iter()
                .map(|c| c.authority)
                .max()
                .unwrap_or(96),
            to_authority: 255,
            subject: goal.subject.clone(),
            timeout_ms: 0,
        };
        let _ = enqueue(&req);
        fields.push(("hitl_enqueued".into(), Json::Bool(true)));
        fields.push(("hitl_request_id".into(), Json::num(id as f64)));
    }

    let verdict = match outcome.action {
        Action::Act => "act",
        Action::Cite => "cite",
        Action::Propose => "propose",
        Action::Abstain => "abstain",
    };
    fields.push(("action".into(), Json::str(outcome.action.label())));
    fields.push(("verdict".into(), Json::str(verdict)));
    fields.push(("rationale".into(), Json::str(&outcome.rationale)));

    if matches!(outcome.action, Action::Abstain) || outcome.claims.is_empty() {
        fields.push(("abstain".into(), Json::Bool(true)));
        return TrioTurn {
            content: "não sei".into(),
            verdict: "abstain",
            evidence: Json::Obj(fields.clone()),
            utterance: None,
        };
    }

    // P6: act + prosa scribe exige T-DEC AND saudável (opt-in declarado).
    if matches!(outcome.action, Action::Act) && scribe_opt_in() {
        let dec = DecTomadaStatus::from_env();
        if dec.degraded {
            fields.push(("p6_and".into(), Json::str("blocked_engine_unavailable")));
            fields.push(("abstain".into(), Json::Bool(true)));
            fields.push(("verdict".into(), Json::str("abstain")));
            fields.push(("reason".into(), Json::str("p6_and_degraded")));
            return TrioTurn {
                content: "não sei".into(),
                verdict: "abstain",
                evidence: Json::Obj(fields.clone()),
                utterance: None,
            };
        }
        fields.push(("p6_and".into(), Json::str("ok")));
    }

    match certify_with_scribe_or_template(&outcome.claims, outcome.action, fields) {
        Some(utt) => {
            if opts.learn {
                let auth = utt.auth.clamp(AUTHORITY_QUARANTINE, LEARN_AUTH_CAP);
                if let Some(key) = learn_text(
                    opts.sgdb.as_deref(),
                    &format!("learn/{verdict}: {}", utt.text),
                    auth.min(LEARN_AUTH_CAP),
                ) {
                    fields.push(("learned_key".into(), Json::str(&key)));
                    let id = HITL_REQ_SEQ.fetch_add(1, Ordering::Relaxed);
                    let _ = enqueue_promote(
                        id,
                        key,
                        utt.auth.min(LEARN_AUTH_CAP),
                        "promote aprendizado trio",
                    );
                    fields.push(("hitl_promote_enqueued".into(), Json::Bool(true)));
                }
            }
            fields.push(("abstain".into(), Json::Bool(false)));
            fields.push(("certified".into(), utt.to_json()));
            let content = utt.text.clone();
            TrioTurn {
                content,
                verdict,
                evidence: Json::Obj(fields.clone()),
                utterance: Some(utt),
            }
        }
        None => {
            fields.push(("abstain".into(), Json::Bool(true)));
            fields.push(("reason".into(), Json::str("p2_reject_no_evidence")));
            TrioTurn {
                content: "não sei".into(),
                verdict: "abstain",
                evidence: Json::Obj(fields.clone()),
                utterance: None,
            }
        }
    }
}

/// Certifica com GemmaScribe (opt-in) ou template; anota backend nos fields.
fn certify_with_scribe_or_template(
    claims: &[Claim],
    action: Action,
    fields: &mut Vec<(String, Json)>,
) -> Option<CertifiedUtterance> {
    if let Some(mut scribe) = GemmaScribePort::from_env(vram_free_from_host_report()) {
        fields.push(("nl_out_backend".into(), Json::str(scribe.backend_label)));
        fields.push(("scribe_runtime".into(), Json::str(scribe.runtime.label())));
        fields.push(("scribe_artifact".into(), Json::str(&scribe.artifact_path)));
        if let Ok(utt) = certify_utterance(&mut scribe, claims, action, Style::Factual) {
            return Some(utt);
        }
        fields.push(("nl_out_fallback".into(), Json::str("template")));
    }
    let mut port = ParserPort::new();
    fields.push(("nl_out_backend".into(), Json::str("template")));
    certify_utterance(&mut port, claims, action, Style::Factual).ok()
}

fn scribe_meta_json() -> Json {
    if !scribe_opt_in() {
        return Json::obj([
            ("backend", Json::str("template")),
            ("scribe_opt_in", Json::Bool(false)),
            (
                "roles",
                Json::str("sentinel_only_until_AETHER_SCRIBE=gemma"),
            ),
        ]);
    }
    let runtime = GemmaScribePort::from_env(vram_free_from_host_report())
        .map(|s| s.runtime)
        .unwrap_or(ScribeRuntime::TemplateOnly);
    Json::obj([
        ("backend", Json::str("gemma_scribe")),
        ("scribe_opt_in", Json::Bool(true)),
        ("runtime", Json::str(runtime.label())),
        (
            "artifact",
            Json::str(
                super::gemma_scribe::active_scribe_path()
                    .unwrap_or_else(|| "gemma-3-1b-prose".into()),
            ),
        ),
        ("roles", Json::str("sentinel_and_scribe")),
    ])
}

fn set_field(fields: &mut Vec<(String, Json)>, key: &str, value: Json) {
    if let Some(slot) = fields.iter_mut().find(|(k, _)| k == key) {
        slot.1 = value;
    } else {
        fields.push((key.into(), value));
    }
}

fn learn_text(sgdb: Option<&str>, text: &str, auth: u8) -> Option<String> {
    let spec = sgdb?;
    let cfg = SgdbConfig::from_env();
    let mut client = open(spec, cfg).ok()?;
    let scope = ScopeId::new(1, 0, 1);
    let auth = auth.min(LEARN_AUTH_CAP);
    client
        .remember_text(
            scope,
            text,
            &["mom/learning".into(), "aether/trio".into()],
            "L3",
            auth,
            None,
        )
        .ok()
        .flatten()
}

/// Promote auth 255 no n-sgdb após HITL (curate set_authority).
pub fn promote_key_255(sgdb: Option<&str>, key: &str) -> Result<(), String> {
    let spec = sgdb.ok_or_else(|| "sgdb ausente".to_string())?;
    let cfg = SgdbConfig::from_env();
    let mut client: SgdbClient<Box<dyn Transport>> = open(spec, cfg).map_err(|e| e.to_string())?;
    client
        .call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("set_authority")),
                ("key", Json::str(key)),
                ("authority", Json::num(255.0)),
            ]),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn caps_json(c: &LanguageCaps) -> Json {
    Json::obj([
        ("cost", Json::num(f64::from(c.cost))),
        ("risk", Json::str(c.risk.label())),
        ("latency_ms_p99", Json::num(f64::from(c.latency_ms_p99))),
        ("needs_model", Json::Bool(c.needs_model)),
        (
            "backend",
            Json::str(if scribe_opt_in() {
                "gemma_scribe_or_template"
            } else {
                "template"
            }),
        ),
    ])
}

fn dec_json(d: &DecTomadaStatus) -> Json {
    Json::obj([
        ("enabled", Json::Bool(d.enabled)),
        ("backend", Json::str(d.backend)),
        ("degraded", Json::Bool(d.degraded)),
        (
            "note",
            Json::str(
                "AND Vega∧gemma opt-in; só bank active; candidate nunca no bus; não gera prosa; não promove 255",
            ),
        ),
    ])
}

#[allow(dead_code)]
fn _risk_label(r: Risk) -> &'static str {
    r.label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oov_abstain_nao_sei() {
        let turn = run_trio_turn("xyzzy blorp", &TrioOpts::default());
        assert_eq!(turn.verdict, "abstain");
        assert!(turn.content.starts_with("não sei"));
        assert!(turn.evidence.to_string().contains("tnl_in_version"));
    }

    #[test]
    fn ask_brasil_produz_certified_ou_abstain() {
        let turn = run_trio_turn("ask Brasil capital", &TrioOpts::default());
        assert!(
            matches!(turn.verdict, "cite" | "act" | "abstain" | "propose"),
            "{}",
            turn.verdict
        );
        if turn.verdict == "abstain" {
            assert!(turn.content.starts_with("não sei"));
        } else {
            assert!(!turn.content.is_empty());
        }
    }
}

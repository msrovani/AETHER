//! K7 — suíte T3 *própria* (coder) com gerador **declarado** no artefato.
//!
//! Não é HumanEval/Arena: escopo `coder_t3_pilot`. Sem `generator` → N/A.
//! Gate D-G2: checklist no JSON (`generator`, `scope`, `host`, `seed`).

use super::generate::{code_generate_mode, propose_from_mode_in, CodeGenMode};
use super::grammar::{grammar_parse_rate, parse_constrained};
use super::small_lm::{invoke_small_lm, pass_at_k};
use super::{sandbox_root, CODE_AUTH_CAP};
use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;
use crate::json::Json;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Caso mínimo da suíte própria (não HumanEval).
#[derive(Clone, Copy, Debug)]
pub struct CoderT3Task {
    /// Id estável.
    pub id: &'static str,
    /// Brief JSON tipado.
    pub brief: &'static str,
    /// Deve parsear via grammar.
    pub expect_parse: bool,
}

/// Suíte piloto congelada (n=8).
pub const CODER_T3_PILOT: &[CoderT3Task] = &[
    CoderT3Task {
        id: "t3-01-replace",
        brief: r#"{"path":"t3_a.txt","op":"replace","anchor":"OLD","content":"NEW"}"#,
        expect_parse: true,
    },
    CoderT3Task {
        id: "t3-02-insert",
        brief: r#"{"path":"t3_b.txt","op":"insert","anchor":"HEAD","content":"_TAIL"}"#,
        expect_parse: true,
    },
    CoderT3Task {
        id: "t3-03-delete",
        brief: r#"{"path":"t3_c.txt","op":"delete","anchor":"DROP"}"#,
        expect_parse: true,
    },
    CoderT3Task {
        id: "t3-04-fence",
        brief: "prosa\n```json\n{\"path\":\"t3_d.txt\",\"op\":\"replace\",\"anchor\":\"A\",\"content\":\"B\"}\n```\n",
        expect_parse: true,
    },
    CoderT3Task {
        id: "t3-05-envelope",
        brief: r#"{"proposal":{"path":"t3_e.txt","op":"replace","anchor":"X","content":"Y"}}"#,
        expect_parse: true,
    },
    CoderT3Task {
        id: "t3-06-garbage",
        brief: "isto nao e patch nem json estruturado",
        expect_parse: false,
    },
    CoderT3Task {
        id: "t3-07-bad-path",
        brief: r#"{"path":"../escape.txt","op":"delete","anchor":"Z"}"#,
        expect_parse: false,
    },
    CoderT3Task {
        id: "t3-08-empty-anchor",
        brief: r#"{"path":"t3_f.txt","op":"replace","anchor":"","content":"Q"}"#,
        expect_parse: false,
    },
];

/// Relatório K7 / D-G2.
#[derive(Clone, Debug)]
pub struct CoderT3Report {
    /// Schema.
    pub schema: &'static str,
    /// Gerador declarado (obrigatório).
    pub generator: String,
    /// Escopo do artefato.
    pub scope: String,
    /// Host.
    pub host: String,
    /// Seed.
    pub seed: u64,
    /// Epoch.
    pub generated_at_unix: u64,
    /// n tarefas.
    pub n: usize,
    /// Acertos (expect_parse == observed_parse).
    pub passed: usize,
    /// Taxa.
    pub accuracy: f64,
    /// grammar_parse_rate só nos expect_parse=true.
    pub grammar_rate_positive: f64,
    /// pass@k do gerador (subset positivo).
    pub pass_at_k: f64,
    /// k usado.
    pub k: usize,
    /// Gate.
    pub gate: &'static str,
    /// Checklist D-G2.
    pub dg2: Vec<(&'static str, bool)>,
}

impl CoderT3Report {
    /// Serializa artefato.
    pub fn to_json(&self) -> Json {
        let dg2: Vec<Json> = self
            .dg2
            .iter()
            .map(|(k, v)| Json::obj([("item", Json::str(*k)), ("ok", Json::Bool(*v))]))
            .collect();
        Json::obj([
            ("schema", Json::str(self.schema)),
            ("generator", Json::str(self.generator.clone())),
            ("scope", Json::str(self.scope.clone())),
            ("host", Json::str(self.host.clone())),
            ("seed", Json::num(self.seed as f64)),
            (
                "generated_at_unix",
                Json::num(self.generated_at_unix as f64),
            ),
            ("n", Json::num(self.n as f64)),
            ("passed", Json::num(self.passed as f64)),
            ("accuracy", Json::num(self.accuracy)),
            (
                "grammar_rate_positive",
                Json::num(self.grammar_rate_positive),
            ),
            ("pass_at_k", Json::num(self.pass_at_k)),
            ("k", Json::num(self.k as f64)),
            ("gate", Json::str(self.gate)),
            ("auth_cap", Json::num(CODE_AUTH_CAP as f64)),
            (
                "disclaimer",
                Json::str(
                    "coder_t3_pilot != HumanEval/Arena; so anunciar com generator+scope neste artefato",
                ),
            ),
            ("dg2_checklist", Json::Arr(dg2)),
        ])
    }

    /// Grava JSON.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_json().to_string())
    }
}

/// Path padrão do artefato K7.
pub fn default_coder_t3_path() -> PathBuf {
    PathBuf::from("target/aether_coder_t3.json")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

/// Normaliza id do gerador declarado.
pub fn parse_generator(raw: &str) -> Result<&'static str, ToolError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "stub" => Ok("stub"),
        "small_lm" | "small-lm" | "code.lm" | "local" => Ok("small_lm"),
        "live" | "1" => Ok("live"),
        "" => Err(ToolError::InvalidArgs(
            "K7 exige generator declarado (stub|small_lm|live)".into(),
        )),
        other => Err(ToolError::InvalidArgs(format!(
            "generator desconhecido: {other}"
        ))),
    }
}

fn mode_for_generator(gen: &str) -> CodeGenMode {
    match gen {
        "stub" => CodeGenMode::Stub,
        "small_lm" => CodeGenMode::SmallLm,
        "live" => CodeGenMode::Live,
        _ => CodeGenMode::Off,
    }
}

/// Roda a suíte piloto com gerador obrigatório.
pub fn run_coder_t3(
    generator: &str,
    seed: u64,
    k: usize,
    root: Option<&Path>,
) -> Result<CoderT3Report, ToolError> {
    let gen = parse_generator(generator)?;
    let k = k.clamp(1, 8);
    let mut passed = 0usize;
    let mut positive_briefs: Vec<&str> = Vec::new();
    for task in CODER_T3_PILOT {
        let parsed = parse_constrained(task.brief, root).is_ok();
        if parsed == task.expect_parse {
            passed += 1;
        }
        if task.expect_parse {
            positive_briefs.push(task.brief);
        }
        // Exercita o gerador declarado (proposal path) nos positivos.
        if task.expect_parse {
            let mode = mode_for_generator(gen);
            let _ = match mode {
                CodeGenMode::SmallLm => invoke_small_lm(task.brief).map(|_| ()),
                CodeGenMode::Stub | CodeGenMode::Live => {
                    propose_from_mode_in(task.brief, mode, root).map(|_| ())
                }
                CodeGenMode::Off => Err(ToolError::Precondition("off".into())),
            };
        }
    }
    let n = CODER_T3_PILOT.len();
    let grammar_rate_positive = grammar_parse_rate(positive_briefs.iter().copied(), root);
    let pass_k = if gen == "small_lm" {
        pass_at_k(&positive_briefs, k)
    } else {
        // stub/live: 1 tentativa grammar+propose.
        let mut ok = 0usize;
        for b in &positive_briefs {
            let mode = mode_for_generator(gen);
            if propose_from_mode_in(b, mode, root).is_ok() {
                ok += 1;
            }
        }
        if positive_briefs.is_empty() {
            0.0
        } else {
            ok as f64 / positive_briefs.len() as f64
        }
    };
    let accuracy = passed as f64 / n as f64;
    let dg2 = vec![
        ("generator_declared", true),
        ("scope_in_artifact", true),
        ("host_seed_version", true),
        ("disclaimer_not_humaneval", true),
        ("grammar_gate_wired", grammar_rate_positive >= 0.95),
        ("pass_at_k_measured", pass_k >= 0.0),
    ];
    Ok(CoderT3Report {
        schema: "aether_coder_t3/k7",
        generator: gen.to_string(),
        scope: "coder_t3_pilot".into(),
        host: hostname(),
        seed,
        generated_at_unix: now_unix(),
        n,
        passed,
        accuracy,
        grammar_rate_positive,
        pass_at_k: pass_k,
        k,
        gate: "D-G2",
        dg2,
    })
}

/// `code.eval` — roda suíte T3 coder (K7); exige generator.
#[derive(Debug)]
pub struct CodeEvalTool {
    root: PathBuf,
}

impl CodeEvalTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeEvalTool {
    fn name(&self) -> &'static str {
        "code.eval"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.eval",
            description: "K7 T3 coder_t3_pilot; exige generator; grava artefato D-G2",
            args: &["query"],
            kinds: &[GoalKind::Verify, GoalKind::Transform],
        }
    }

    fn cost(&self) -> Cost {
        Cost {
            ms: 400.0,
            tokens: 256,
        }
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.eval") {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let raw = goal.query.as_deref().unwrap_or("{}").trim();
        let v = Json::parse(if raw.is_empty() { "{}" } else { raw })
            .map_err(|e| ToolError::InvalidArgs(format!("json: {e}")))?;
        let generator = v
            .get("generator")
            .and_then(|g| g.as_str())
            .map(|s| s.to_string())
            .or_else(coder_t3_generator_declared)
            .unwrap_or_default();
        if generator.is_empty() {
            return Ok(ToolOutcome {
                claims: Vec::new(),
                confidence: 0.0,
                detail: format!(
                    "code.eval N/A: generator nao declarado scope={:?} trace={}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            });
        }
        let seed = v
            .get("seed")
            .and_then(|s| s.as_u64().or_else(|| s.as_f64().map(|f| f as u64)))
            .unwrap_or(0);
        let k = v
            .get("k")
            .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
            .unwrap_or(1) as usize;
        let report = run_coder_t3(&generator, seed, k, Some(&self.root))?;
        let out_path = v
            .get("out")
            .and_then(|o| o.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(default_coder_t3_path);
        report
            .write_to(&out_path)
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        let ok = report.accuracy >= 0.95 && report.grammar_rate_positive >= 0.95;
        Ok(ToolOutcome {
            claims: vec![
                Claim::new(
                    report.scope.clone(),
                    "code.eval",
                    if ok { "ok" } else { "fail" },
                    1.0,
                    CODE_AUTH_CAP.min(60),
                    ExpertId::Memory,
                    Provenance::Observed,
                ),
                Claim::new(
                    report.scope.clone(),
                    "code.eval.generator",
                    report.generator.clone(),
                    1.0,
                    CODE_AUTH_CAP.min(60),
                    ExpertId::Memory,
                    Provenance::Observed,
                ),
            ],
            confidence: if ok { 0.95 } else { 0.4 },
            detail: format!(
                "code.eval {} generator={} accuracy={:.3} grammar+={:.3} pass@{}={:.3} \
                 out={} gate=D-G2 scope={:?} trace={}",
                if ok { "ok" } else { "fail" },
                report.generator,
                report.accuracy,
                report.grammar_rate_positive,
                report.k,
                report.pass_at_k,
                out_path.display(),
                ctx.scope,
                ctx.trace_id
            ),
            ..ToolOutcome::default()
        })
    }
}

/// T3 ready? (gerador declarado via env).
pub fn coder_t3_generator_declared() -> Option<String> {
    std::env::var("AETHER_CODE_T3_GENERATOR")
        .ok()
        .filter(|s| parse_generator(s).is_ok())
        .or_else(|| match code_generate_mode() {
            CodeGenMode::Off => None,
            CodeGenMode::Stub => Some("stub".into()),
            CodeGenMode::SmallLm => Some("small_lm".into()),
            CodeGenMode::Live => Some("live".into()),
        })
}

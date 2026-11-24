//! A2 / T-EVAL — suite JSONL → placar com linhagem (`run_id`, hash, tier).
//!
//! # Contrato
//!
//! ```text
//! bench/agent/suite.jsonl ──sha256──► dataset_sha256
//!            │                              │
//!            ├──parse──► SuiteCase[]        ├──► run_id (determinístico)
//!            ▼                              ▼
//!        ToolAgent ──► métricas T0 ──► AgentScorecard ──► target/aether_agent_scorecard.json
//! ```
//!
//! Regras do gate `tests/agent_eval_report_gate.rs`:
//!
//! * **Reprodutível**: duas execuções do mesmo dataset dão `run_id` e taxas idênticos.
//! * **Sem mistura de tiers**: só o T0 carrega `metrics`; T1–T3 declaram N/A.
//! * **Dataset sem hash não é placar**: `dataset_sha256` é sempre SHA-256 do arquivo.
//! * **`false_act = 0`** é obrigatório na suíte própria.

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::logic_pipeline::Clause3;
use crate::core::sha256::sha256_hex;
use crate::json::Json;

use super::{Action, AgentConfig, Goal, GoalKind, Risk, SuccessCriteria, ToolAgent};

/// Schema do artefato.
pub const SCORECARD_SCHEMA: &str = "aether_agent_scorecard/v1";

/// Um caso da suite (uma linha do JSONL).
#[derive(Clone, Debug, PartialEq)]
pub struct SuiteCase {
    /// Identificador estável.
    pub id: String,
    /// Goal na DSL (`fact:S:R`, `sat:...`).
    pub goal: String,
    /// Ação esperada.
    pub action: Action,
    /// Sucesso esperado.
    pub success: bool,
    /// Substring esperada na resposta.
    pub answer_contains: Option<String>,
}

fn action_from_label(label: &str) -> Result<Action, String> {
    match label.trim().to_ascii_lowercase().as_str() {
        "act" => Ok(Action::Act),
        "cite" => Ok(Action::Cite),
        "propose" => Ok(Action::Propose),
        "abstain" => Ok(Action::Abstain),
        other => Err(format!("ação desconhecida `{other}`")),
    }
}

/// Parseia a suite JSONL. Linha inválida é erro com número de linha.
pub fn load_suite(text: &str) -> Result<Vec<SuiteCase>, String> {
    let mut cases = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let at = i + 1;
        let row = Json::parse(line).map_err(|e| format!("linha {at}: {e}"))?;
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("linha {at}: sem `id`"))?
            .to_string();
        let goal = row
            .get("goal")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("linha {at}: sem `goal`"))?
            .to_string();
        let expect = row
            .get("expect")
            .ok_or_else(|| format!("linha {at}: sem `expect`"))?;
        let action_label = expect
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("linha {at}: `expect.action` ausente"))?;
        let action = action_from_label(action_label).map_err(|e| format!("linha {at}: {e}"))?;
        let success = expect
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let answer_contains = expect
            .get("answer_contains")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        cases.push(SuiteCase {
            id,
            goal,
            action,
            success,
            answer_contains,
        });
    }
    if cases.is_empty() {
        return Err("suite vazia".into());
    }
    Ok(cases)
}

/// Parseia a DSL textual de goal usada pela suite (`docs/EVAL_AGENT.md`).
pub fn parse_goal_spec(spec: &str) -> Result<Goal, String> {
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix("pipeline:") {
        let mut steps = Vec::new();
        for part in rest.split('|') {
            steps.push(parse_goal_spec(part)?);
        }
        if steps.is_empty() {
            return Err("pipeline sem passos".into());
        }
        return Ok(Goal::pipeline(steps));
    }
    let (head, rest) = spec
        .split_once(':')
        .ok_or_else(|| format!("goal sem `tipo:` ({spec})"))?;
    match head {
        "fact" => {
            let (subject, relation) = rest
                .split_once(':')
                .ok_or_else(|| "fact espera `<subject>:<relation>`".to_string())?;
            require_nonempty(subject, "subject")?;
            require_nonempty(relation, "relation")?;
            Ok(Goal::fact(subject, relation))
        }
        "memory" => Ok(Goal::memory(rest)),
        "verify" => {
            require_nonempty(rest, "subject")?;
            let mut goal = Goal::fact(rest, "verify");
            goal.kind = GoalKind::Verify;
            goal.relation = None;
            goal.success = SuccessCriteria::Any;
            Ok(goal)
        }
        "act" => {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() != 4 {
                return Err("act espera `<subject>:<relation>:<valor>:<teto>`".into());
            }
            require_nonempty(parts[0], "subject")?;
            require_nonempty(parts[1], "relation")?;
            require_nonempty(parts[2], "valor")?;
            let mut goal = Goal::fact(parts[0], parts[1]);
            goal.kind = GoalKind::Act;
            goal.query = Some(parts[2].to_string());
            goal.success = SuccessCriteria::Claim {
                subject: parts[0].to_string(),
                relation: parts[1].to_string(),
                value: Some(parts[2].to_string()),
            };
            goal.risk_ceiling = risk_from_label(parts[3])?;
            Ok(goal)
        }
        "sat" => {
            let (n, clauses_text) = rest
                .split_once(':')
                .ok_or_else(|| "sat espera `<n>:<clausulas>`".to_string())?;
            let n: usize = n.parse().map_err(|_| format!("sat: n inválido `{n}`"))?;
            require_positive(n, "sat n")?;
            let mut clauses = Vec::new();
            for clause in clauses_text.split(';') {
                let clause = clause.trim();
                if clause.is_empty() {
                    continue;
                }
                clauses.push(parse_clause(clause)?);
            }
            Ok(Goal::satisfy(n, clauses))
        }
        "qubo" => {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() != 3 {
                return Err("qubo espera `<n>:<h csv>:<zeros|j csv>`".into());
            }
            let n: usize = parts[0]
                .parse()
                .map_err(|_| format!("qubo: n inválido `{}`", parts[0]))?;
            require_positive(n, "qubo n")?;
            let h = parse_numbers(parts[1])?;
            if h.len() != n {
                return Err(format!("qubo: h com {} valores (esperado {n})", h.len()));
            }
            let j = if parts[2].trim() == "zeros" {
                vec![0.0; n * n]
            } else {
                parse_numbers(parts[2])?
            };
            if j.len() != n * n {
                return Err(format!(
                    "qubo: j com {} valores (esperado {})",
                    j.len(),
                    n * n
                ));
            }
            Ok(Goal::optimize(n, j, h))
        }
        other => Err(format!("tipo de goal desconhecido `{other}`")),
    }
}

fn parse_clause(text: &str) -> Result<Clause3, String> {
    let lit: Vec<&str> = text.split(',').map(str::trim).collect();
    let parse = |value: &str, what: &str| -> Result<i64, String> {
        value
            .parse()
            .map_err(|_| format!("cláusula `{text}`: {what} inválido `{value}`"))
    };
    match lit.len() {
        4 | 6 => {}
        _ => {
            return Err(format!(
                "cláusula `{text}`: 4 ou 6 campos, veio {}",
                lit.len()
            ))
        }
    }
    let i = parse(lit[0], "i")?;
    let si = parse(lit[1], "si")?;
    let j = parse(lit[2], "j")?;
    let sj = parse(lit[3], "sj")?;
    let i = usize::try_from(i).map_err(|_| format!("cláusula `{text}`: i negativo"))?;
    let j = usize::try_from(j).map_err(|_| format!("cláusula `{text}`: j negativo"))?;
    let si = i8::try_from(si).map_err(|_| format!("cláusula `{text}`: si fora de i8"))?;
    let sj = i8::try_from(sj).map_err(|_| format!("cláusula `{text}`: sj fora de i8"))?;
    if lit.len() == 4 {
        return Ok(Clause3::new2(i, si, j, sj));
    }
    let k = parse(lit[4], "k")?;
    let sk = parse(lit[5], "sk")?;
    let k = usize::try_from(k).map_err(|_| format!("cláusula `{text}`: k negativo"))?;
    let sk = i8::try_from(sk).map_err(|_| format!("cláusula `{text}`: sk fora de i8"))?;
    Ok(Clause3::new3(i, si, j, sj, k, sk))
}

fn parse_numbers(text: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        out.push(
            part.parse::<f64>()
                .map_err(|_| format!("número inválido `{part}`"))?,
        );
    }
    if out.is_empty() {
        return Err(format!("lista de números vazia (`{text}`)"));
    }
    Ok(out)
}

fn risk_from_label(label: &str) -> Result<Risk, String> {
    match label.trim() {
        "low" => Ok(Risk::Low),
        "medium" => Ok(Risk::Medium),
        "high" => Ok(Risk::High),
        other => Err(format!("teto de risco desconhecido `{other}`")),
    }
}

fn require_nonempty(value: &str, what: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{what} vazio"))
    } else {
        Ok(())
    }
}

fn require_positive(value: usize, what: &str) -> Result<(), String> {
    if value == 0 {
        Err(format!("{what} deve ser > 0"))
    } else {
        Ok(())
    }
}

/// Métricas do tier T0 (único tier com número nesta suite).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct T0Metrics {
    /// Casos.
    pub n: usize,
    /// Ação correta / n.
    pub action_accuracy: f64,
    /// Sucesso verificado / n.
    pub success_rate: f64,
    /// `expect` cite/abstain virou act / n.
    pub false_act_rate: f64,
    /// `expect` act virou abstain / n.
    pub false_abstain_rate: f64,
    /// Contagem absoluta de false-act.
    pub false_act: usize,
}

impl T0Metrics {
    fn to_json(&self) -> Json {
        Json::obj([
            ("n", Json::num(self.n as f64)),
            ("action_accuracy", Json::num(self.action_accuracy)),
            ("success_rate", Json::num(self.success_rate)),
            ("false_act_rate", Json::num(self.false_act_rate)),
            ("false_abstain_rate", Json::num(self.false_abstain_rate)),
            ("false_act", Json::num(self.false_act as f64)),
        ])
    }
}

/// Uma linha por tier no placar (nunca média entre tiers).
#[derive(Clone, Debug, PartialEq)]
pub struct TierLine {
    /// Rótulo do tier.
    pub tier: &'static str,
    /// Medido nesta execução?
    pub ready: bool,
    /// Motivo/N-A.
    pub note: String,
    /// Métricas — só o T0 carrega.
    pub metrics: Option<T0Metrics>,
}

/// Uma linha por caso (auditoria do placar).
#[derive(Clone, Debug, PartialEq)]
pub struct CaseRow {
    /// Id do caso.
    pub id: String,
    /// Ação esperada.
    pub expect: String,
    /// Ação observada.
    pub observed: String,
    /// Caso passou?
    pub ok: bool,
    /// Divergências legíveis.
    pub note: String,
}

/// Placar reproduzível de uma suite.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentScorecard {
    /// Schema.
    pub schema: &'static str,
    /// `sha256(dataset|n|seed|version|profile)[..16]`.
    pub run_id: String,
    /// Dataset (path relativo ou absoluto).
    pub dataset: String,
    /// SHA-256 do arquivo da suite.
    pub dataset_sha256: String,
    /// Casos.
    pub cases: usize,
    /// Seed.
    pub seed: u64,
    /// Versão do crate.
    pub version: String,
    /// Perfil (debug/release).
    pub profile: String,
    /// Host.
    pub host: String,
    /// Epoch UTC (s) — não entra no `run_id`.
    pub generated_at_unix: u64,
    /// Métricas T0.
    pub t0: T0Metrics,
    /// Tiers.
    pub tiers: Vec<TierLine>,
    /// Linhas por caso.
    pub rows: Vec<CaseRow>,
    /// Divergências.
    pub mismatches: Vec<String>,
    /// Invariante: nenhuma média cruza tiers.
    pub mixed_tiers: bool,
    /// Gate verde? (`mismatches` vazio e `false_act == 0`)
    pub ok: bool,
}

/// Path padrão do placar A2.
pub fn default_scorecard_path() -> PathBuf {
    PathBuf::from("target/aether_agent_scorecard.json")
}

/// Roda a suite com o agente real. Duas execuções do mesmo dataset são iguais.
pub fn run_suite(suite_path: &Path, cfg: &AgentConfig) -> Result<AgentScorecard, String> {
    let bytes =
        fs::read(suite_path).map_err(|e| format!("leitura {}: {e}", suite_path.display()))?;
    let dataset_sha256 = sha256_hex(&bytes);
    let text = String::from_utf8(bytes).map_err(|e| format!("suite não é UTF-8: {e}"))?;
    let cases = load_suite(&text)?;

    let mut agent = ToolAgent::new(cfg);
    let mut rows = Vec::with_capacity(cases.len());
    let mut mismatches = Vec::new();
    let mut wrong_action = 0usize;
    let mut wrong_success = 0usize;
    let mut false_act = 0usize;
    let mut false_abstain = 0usize;

    for case in &cases {
        let goal = parse_goal_spec(&case.goal)
            .map_err(|e| format!("caso `{}`: goal inválido `{}`: {e}", case.id, case.goal))?;
        let outcome = agent.solve(&goal);
        let observed = outcome.action;
        let mut notes: Vec<String> = Vec::new();
        if observed != case.action {
            wrong_action += 1;
            notes.push(format!(
                "ação {} != {}",
                observed.label(),
                case.action.label()
            ));
            match (case.action, observed) {
                (Action::Abstain | Action::Cite, Action::Act) => false_act += 1,
                (Action::Act, Action::Abstain) => false_abstain += 1,
                _ => {}
            }
        }
        if outcome.success != case.success {
            wrong_success += 1;
            notes.push(format!("success {} != {}", outcome.success, case.success));
        }
        if let Some(want) = &case.answer_contains {
            let answer = outcome.trace.answer.clone().unwrap_or_default();
            if !answer.to_lowercase().contains(&want.to_lowercase()) {
                notes.push(format!("answer {answer:?} não contém {want:?}"));
            }
        }
        let ok = notes.is_empty();
        if !ok {
            mismatches.push(format!("{}: {}", case.id, notes.join("; ")));
        }
        rows.push(CaseRow {
            id: case.id.clone(),
            expect: format!("{}/{}", case.action.label(), case.success),
            observed: format!("{}/{}", observed.label(), outcome.success),
            ok,
            note: notes.join("; "),
        });
    }

    let n = cases.len();
    let t0 = T0Metrics {
        n,
        action_accuracy: (n - wrong_action) as f64 / n as f64,
        success_rate: (n - wrong_success) as f64 / n as f64,
        false_act_rate: false_act as f64 / n as f64,
        false_abstain_rate: false_abstain as f64 / n as f64,
        false_act,
    };
    let version = env!("CARGO_PKG_VERSION").to_string();
    let profile = if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    };
    let seed = cfg.seed;
    let run_id =
        sha256_hex(format!("{}|{}|{}|{}|{}", dataset_sha256, n, seed, version, profile).as_bytes());
    let run_id = run_id[..16].to_string();

    let t3_note = match super::coder_t3_generator_declared() {
        Some(g) => format!(
            "gerador coder declarado={g}; T3 externo (HumanEval/Arena) continua N/A sem escopo externo"
        ),
        None => "sem gerador de linguagem; MMLU/GSM8K/HumanEval = N/A".to_string(),
    };
    let tiers = vec![
        TierLine {
            tier: "T0_self_suite",
            ready: true,
            note: "suite propria medida nesta execucao".into(),
            metrics: Some(t0.clone()),
        },
        TierLine {
            tier: "T1_coprocessor",
            ready: false,
            note: "latencia/recall/abstencao medidos pelos gates transplant/e3/agent, fora desta suite"
                .into(),
            metrics: None,
        },
        TierLine {
            tier: "T2_hybrid",
            ready: false,
            note: "exige LanguagePort ponta a ponta; N/A nesta suite".into(),
            metrics: None,
        },
        TierLine {
            tier: "T3_external",
            ready: false,
            note: t3_note,
            metrics: None,
        },
    ];
    let ok = mismatches.is_empty() && false_act == 0;
    Ok(AgentScorecard {
        schema: SCORECARD_SCHEMA,
        run_id,
        dataset: suite_path.display().to_string(),
        dataset_sha256,
        cases: n,
        seed,
        version,
        profile,
        host: super::eval::hostname(),
        generated_at_unix: super::eval::now_unix(),
        t0,
        tiers,
        rows,
        mismatches,
        mixed_tiers: false,
        ok,
    })
}

impl AgentScorecard {
    /// Serializa o placar (com linhagem completa).
    pub fn to_json(&self) -> Json {
        let tiers: Vec<Json> = self
            .tiers
            .iter()
            .map(|t| {
                Json::obj([
                    ("tier", Json::str(t.tier)),
                    ("ready", Json::Bool(t.ready)),
                    ("note", Json::str(t.note.clone())),
                    (
                        "metrics",
                        match &t.metrics {
                            Some(m) => m.to_json(),
                            None => Json::Null,
                        },
                    ),
                ])
            })
            .collect();
        let rows: Vec<Json> = self
            .rows
            .iter()
            .map(|r| {
                Json::obj([
                    ("id", Json::str(r.id.clone())),
                    ("expect", Json::str(r.expect.clone())),
                    ("observed", Json::str(r.observed.clone())),
                    ("ok", Json::Bool(r.ok)),
                    ("note", Json::str(r.note.clone())),
                ])
            })
            .collect();
        Json::obj([
            ("schema", Json::str(self.schema)),
            ("gate", Json::str("A2_T_EVAL")),
            ("ok", Json::Bool(self.ok)),
            ("run_id", Json::str(self.run_id.clone())),
            ("dataset", Json::str(self.dataset.clone())),
            ("dataset_sha256", Json::str(self.dataset_sha256.clone())),
            ("cases", Json::num(self.cases as f64)),
            ("seed", Json::num(self.seed as f64)),
            ("version", Json::str(self.version.clone())),
            ("profile", Json::str(self.profile.clone())),
            ("host", Json::str(self.host.clone())),
            (
                "generated_at_unix",
                Json::num(self.generated_at_unix as f64),
            ),
            (
                "rule",
                Json::str("T0_self_suite != T3_external; metrics so no T0; nao misturar tiers"),
            ),
            ("mixed_tiers", Json::Bool(self.mixed_tiers)),
            ("tiers", Json::Arr(tiers)),
            ("rows", Json::Arr(rows)),
            (
                "mismatches",
                Json::Arr(
                    self.mismatches
                        .iter()
                        .map(|m| Json::str(m.clone()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Grava o placar (cria diretórios).
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_json().to_string())?;
        Ok(())
    }
}

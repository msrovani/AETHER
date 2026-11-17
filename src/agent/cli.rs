//! CLI `aether-agent` — a face executável do organismo agêntico.
//!
//! ```text
//! aether-agent ask --subject Brasil --relation capital
//! aether-agent sat examples/sat.dimacs
//! aether-agent solve target/qubo.json
//! aether-agent teach --subject Itália --relation capital --value Roma
//! aether-agent demo
//! ```
//!
//! Tudo aceita `--json`. A CLI só orquestra: a decisão é do
//! [`super::AetherAgent`], com certificado e abstenção.

use std::io::Write;
use std::path::PathBuf;

use crate::core::logic_pipeline::Clause3;
use crate::json::Json;

use super::{
    Action, AetherAgent, AgentConfig, Claim, ExpertId, Feedback, Goal, Outcome, Task, ToolAgent,
};

/// Códigos de saída.
pub const EXIT_OK: i32 = 0;
/// Falha de execução.
pub const EXIT_FAIL: i32 = 1;
/// Uso incorreto.
pub const EXIT_USAGE: i32 = 2;

/// Ajuda.
pub const HELP: &str = "\
aether-agent — organismo agêntico do AETHER (inferência, MoE, coerência, aprendizado)

USO
  aether-agent <comando> [opções]

COMANDOS
  ask                     consulta um fato relacional
  sat <arquivo.dimacs>    resolve 3-SAT e devolve certificado
  solve <arquivo.json>    resolve QUBO {n, j, h}
  memory <consulta>       consulta a memória durável (SGDB)
  tools                   lista as ferramentas e custo/risco
  goal                    executa um Goal com planner/verificador
  teach                   ensina um fato novo com feedback verificado
  demo                    roda o conjunto de demonstração/gates
  stats                   mostra pesos do MoE e métricas
  help                    esta ajuda

OPÇÕES
  --subject <s>           sujeito da consulta (ask/teach)
  --relation <r>          relação (ask/teach)
  --value <v>             valor esperado (teach)
  --facts <arquivo>       JSONL do transplante (padrão data/facts.jsonl)
  --dim <n>               dimensão do FHRR (padrão 1024)
  --traces <n>            traços FHRR (padrão 8)
  --top-k <n>             especialistas por passo (padrão 2)
  --steps <n>             passos máximos do laço (padrão 2)
  --policy <arquivo>      política EXP3 persistida (JSON)
  --sgdb <spec>           memória durável: mock | http://…/mcp | stdio:cmd
  --seed <u64>            semente do roteador
  --json                  saída JSON
  --quiet                 só o essencial
  --help                  esta ajuda

EXEMPLOS
  aether-agent ask --subject Brasil --relation capital
  aether-agent ask --subject Lua --relation orbita --json
  aether-agent tools
  aether-agent goal --facts data/agent_facts.jsonl --subject AETHER --relation proibe
  aether-agent teach --subject Itália --relation capital --value Roma
  aether-agent sat target/sat.dimacs
  aether-agent demo --json
";

#[derive(Clone, Debug, Default)]
struct Args {
    command: Option<String>,
    positional: Vec<String>,
    subject: Option<String>,
    relation: Option<String>,
    value: Option<String>,
    facts: Option<PathBuf>,
    policy: Option<PathBuf>,
    sgdb: Option<String>,
    dim: Option<usize>,
    traces: Option<usize>,
    top_k: Option<usize>,
    steps: Option<usize>,
    seed: Option<u64>,
    json: bool,
    quiet: bool,
    help: bool,
}

impl Args {
    fn config(&self) -> AgentConfig {
        let mut cfg = AgentConfig::default();
        if let Some(path) = &self.facts {
            cfg.facts_path = path.clone();
        }
        if let Some(path) = &self.policy {
            cfg.policy_path = Some(path.clone());
        }
        if let Some(spec) = &self.sgdb {
            cfg.sgdb = Some(spec.clone());
        }
        if let Some(dim) = self.dim {
            cfg.dim = dim;
        }
        if let Some(traces) = self.traces {
            cfg.traces = traces;
        }
        if let Some(k) = self.top_k {
            cfg.top_k = k;
        }
        if let Some(steps) = self.steps {
            cfg.max_steps = steps;
        }
        if let Some(seed) = self.seed {
            cfg.seed = seed;
        }
        cfg
    }

    fn fact(&self) -> Option<Task> {
        let subject = self.subject.clone().or_else(|| {
            self.positional
                .first()
                .and_then(|v| v.split_once('|').map(|(s, _)| s.to_string()))
        })?;
        let relation = self.relation.clone().or_else(|| {
            self.positional
                .first()
                .and_then(|v| v.split_once('|').map(|(_, r)| r.to_string()))
        })?;
        Some(Task::Fact { subject, relation })
    }
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0usize;
    while i < argv.len() {
        let arg = &argv[i];
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        let value = |i: &mut usize| -> Result<String, String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| format!("{flag} exige um valor"))
        };
        match flag.as_str() {
            "-h" | "--help" => args.help = true,
            "--json" => args.json = true,
            "--quiet" => args.quiet = true,
            "--subject" => args.subject = Some(value(&mut i)?),
            "--relation" => args.relation = Some(value(&mut i)?),
            "--value" => args.value = Some(value(&mut i)?),
            "--facts" => args.facts = Some(PathBuf::from(value(&mut i)?)),
            "--policy" => args.policy = Some(PathBuf::from(value(&mut i)?)),
            "--sgdb" => args.sgdb = Some(value(&mut i)?),
            "--dim" => args.dim = Some(parse_num(&value(&mut i)?, "--dim")?),
            "--traces" => args.traces = Some(parse_num(&value(&mut i)?, "--traces")?),
            "--top-k" => args.top_k = Some(parse_num(&value(&mut i)?, "--top-k")?),
            "--steps" => args.steps = Some(parse_num(&value(&mut i)?, "--steps")?),
            "--seed" => args.seed = Some(parse_num(&value(&mut i)?, "--seed")?),
            other if other.starts_with("--") => {
                return Err(format!("flag desconhecida: {other} (use --help)"))
            }
            _ => {
                if args.command.is_none() {
                    args.command = Some(arg.clone());
                } else {
                    args.positional.push(arg.clone());
                }
            }
        }
        i += 1;
    }
    Ok(args)
}

fn parse_num<T: std::str::FromStr>(raw: &str, flag: &str) -> Result<T, String> {
    raw.parse()
        .map_err(|_| format!("{flag} espera um inteiro (veio {raw})"))
}

/// Executa a CLI; `argv` sem o nome do programa.
pub fn run(argv: &[String], out: &mut dyn Write) -> i32 {
    let args = match parse(argv) {
        Ok(args) => args,
        Err(e) => {
            let _ = writeln!(out, "erro de uso: {e}");
            let _ = writeln!(out, "dica: aether-agent --help");
            return EXIT_USAGE;
        }
    };
    if args.help || args.command.is_none() {
        let _ = write!(out, "{HELP}");
        return if args.help { EXIT_OK } else { EXIT_USAGE };
    }
    match args.command.as_deref().unwrap_or_default() {
        "help" => {
            let _ = write!(out, "{HELP}");
            EXIT_OK
        }
        "ask" | "query" => ask(&args, out),
        "sat" => sat(&args, out),
        "solve" | "qubo" => solve(&args, out),
        "memory" | "recall" => memory(&args, out),
        "tools" => tools(&args, out),
        "goal" => goal(&args, out),
        "teach" | "learn" => teach(&args, out),
        "stats" => stats(&args, out),
        "demo" => demo(&args, out),
        other => {
            let _ = writeln!(out, "comando desconhecido: {other}");
            let _ = writeln!(out, "dica: aether-agent --help");
            EXIT_USAGE
        }
    }
}

fn ask(args: &Args, out: &mut dyn Write) -> i32 {
    let Some(task) = args.fact() else {
        let _ = writeln!(
            out,
            "erro de uso: ask exige --subject e --relation (ou \"sujeito|relação\")"
        );
        return EXIT_USAGE;
    };
    let mut agent = AetherAgent::new(args.config());
    let outcome = agent.run(&task);
    print_outcome(out, &outcome, args.json);
    if outcome.action == Action::Abstain {
        EXIT_FAIL
    } else {
        EXIT_OK
    }
}

fn sat(args: &Args, out: &mut dyn Write) -> i32 {
    let Some(path) = args.positional.first() else {
        let _ = writeln!(out, "erro de uso: sat <arquivo.dimacs>");
        return EXIT_USAGE;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        let _ = writeln!(out, "erro: não li {path}");
        return EXIT_FAIL;
    };
    match parse_dimacs(&text) {
        Ok((n, clauses)) => {
            let mut agent = AetherAgent::new(args.config());
            let outcome = agent.satisfy(n, clauses);
            print_outcome(out, &outcome, args.json);
            if outcome.action == Action::Abstain {
                EXIT_FAIL
            } else {
                EXIT_OK
            }
        }
        Err(e) => {
            let _ = writeln!(out, "erro: {e}");
            EXIT_FAIL
        }
    }
}

fn solve(args: &Args, out: &mut dyn Write) -> i32 {
    let Some(path) = args.positional.first() else {
        let _ = writeln!(out, "erro de uso: solve <arquivo.json>");
        return EXIT_USAGE;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        let _ = writeln!(out, "erro: não li {path}");
        return EXIT_FAIL;
    };
    let Ok(json) = Json::parse(&text) else {
        let _ = writeln!(out, "erro: {path} não é JSON");
        return EXIT_FAIL;
    };
    let Some(n) = json.get("n").and_then(|v| v.as_u64()).map(|v| v as usize) else {
        let _ = writeln!(out, "erro: campo n ausente");
        return EXIT_FAIL;
    };
    let Some(j) = json
        .get("j")
        .and_then(|v| v.as_arr())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
    else {
        let _ = writeln!(out, "erro: campo j ausente");
        return EXIT_FAIL;
    };
    let h = json
        .get("h")
        .and_then(|v| v.as_arr())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![0.0; n]);
    let mut agent = AetherAgent::new(args.config());
    let outcome = agent.optimize(n, j, h);
    print_outcome(out, &outcome, args.json);
    if outcome.action == Action::Abstain {
        EXIT_FAIL
    } else {
        EXIT_OK
    }
}

fn memory(args: &Args, out: &mut dyn Write) -> i32 {
    let query = args.positional.join(" ");
    if query.trim().is_empty() {
        let _ = writeln!(out, "erro de uso: memory <consulta> --sgdb <spec>");
        return EXIT_USAGE;
    }
    let Some(spec) = &args.sgdb else {
        let _ = writeln!(
            out,
            "erro: memory exige --sgdb (mock, http://…/mcp ou stdio:cmd) \
             ou AETHER_SGDB_URL"
        );
        return EXIT_USAGE;
    };
    let mut cfg = args.config();
    cfg.sgdb = Some(spec.clone());
    let mut agent = AetherAgent::new(cfg);
    let outcome = agent.run(&Task::Memory { query });
    print_outcome(out, &outcome, args.json);
    EXIT_OK
}

fn tools(args: &Args, out: &mut dyn Write) -> i32 {
    let mut cfg = args.config();
    if let Some(path) = &args.policy {
        cfg.tool_policy_path = Some(path.clone());
    }
    let agent = ToolAgent::new(&cfg);
    if args.json {
        let list: Vec<Json> = agent
            .tools()
            .into_iter()
            .map(|schema| {
                let (cost, risk) = agent
                    .tool_stats(schema.name)
                    .unwrap_or((super::Cost::local(0.0), super::Risk::Low));
                Json::obj([
                    ("name", Json::str(schema.name)),
                    ("description", Json::str(schema.description)),
                    (
                        "args",
                        Json::Arr(schema.args.iter().map(|a| Json::str(*a)).collect()),
                    ),
                    ("cost_ms", Json::num(f64::from(cost.ms))),
                    ("tokens", Json::num(f64::from(cost.tokens))),
                    ("risk", Json::str(risk.label())),
                ])
            })
            .collect();
        let _ = writeln!(out, "{}", Json::Arr(list));
    } else {
        for schema in agent.tools() {
            let (cost, risk) = agent
                .tool_stats(schema.name)
                .unwrap_or((super::Cost::local(0.0), super::Risk::Low));
            let _ = writeln!(
                out,
                "{:<10} {:>6.1}ms {:>5} tok risk={:<6} [{}] {}",
                schema.name,
                cost.ms,
                cost.tokens,
                risk.label(),
                schema.args.join(","),
                schema.description
            );
        }
    }
    EXIT_OK
}

fn goal(args: &Args, out: &mut dyn Write) -> i32 {
    let mut cfg = args.config();
    if let Some(path) = &args.policy {
        cfg.tool_policy_path = Some(path.clone());
    }
    let goal = if let (Some(subject), Some(relation)) = (&args.subject, &args.relation) {
        Goal::fact(subject.clone(), relation.clone())
    } else if let Some(first) = args.positional.first() {
        if first.ends_with(".dimacs") {
            match std::fs::read_to_string(first) {
                Ok(text) => match parse_dimacs(&text) {
                    Ok((n, clauses)) => Goal::satisfy(n, clauses),
                    Err(e) => {
                        let _ = writeln!(out, "erro: {e}");
                        return EXIT_FAIL;
                    }
                },
                Err(e) => {
                    let _ = writeln!(out, "erro: não li {first}: {e}");
                    return EXIT_FAIL;
                }
            }
        } else if let Some((subject, relation)) = first.split_once('|') {
            Goal::fact(subject.to_string(), relation.to_string())
        } else {
            let _ = writeln!(out, "erro de uso: goal exige --subject/--relation ou \"sujeito|relação\"");
            return EXIT_USAGE;
        }
    } else {
        let _ = writeln!(out, "erro de uso: goal exige --subject e --relation");
        return EXIT_USAGE;
    };
    let mut agent = ToolAgent::new(&cfg);
    let outcome = agent.solve(&goal);
    if args.json {
        let _ = writeln!(out, "{}", outcome.trace.to_json());
    } else {
        let _ = writeln!(
            out,
            "sucesso={} ação={} | {}",
            outcome.success,
            outcome.action.label(),
            outcome.rationale
        );
        for (i, step) in outcome.steps.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {}. {:<10} score={:.3} met={} {}us — {}",
                i + 1,
                step.tool,
                step.score,
                step.met,
                step.elapsed_us,
                step.detail
            );
        }
        if let Some(claim) = outcome.outcome.as_ref().and_then(|o| o.first_claim()) {
            let _ = writeln!(out, "resposta: {} (fonte {})", claim.value, claim.source.label());
        }
    }
    if outcome.success {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn teach(args: &Args, out: &mut dyn Write) -> i32 {    let Some(task) = args.fact() else {
        let _ = writeln!(out, "erro de uso: teach exige --subject e --relation");
        return EXIT_USAGE;
    };
    let Some(value) = &args.value else {
        let _ = writeln!(out, "erro de uso: teach exige --value");
        return EXIT_USAGE;
    };
    let mut agent = AetherAgent::new(args.config());
    let before = agent.run(&task);
    let Task::Fact { subject, relation } = &task else {
        return EXIT_USAGE;
    };
    let feedback = Feedback {
        correct: Some(true),
        expected: Some(Claim::new(
            subject.clone(),
            relation.clone(),
            value.clone(),
            1.0,
            255,
            ExpertId::Transplant,
            crate::core::certificate::Provenance::Human,
        )),
    };
    agent.learn(&before, &feedback);
    let after = agent.run(&task);
    if args.json {
        let _ = writeln!(
            out,
            "{}",
            Json::obj([
                ("before", Json::str(before.action.label())),
                ("after", Json::str(after.action.label())),
                (
                    "answer",
                    Json::str(
                        after
                            .answer
                            .as_ref()
                            .map(|c| c.value.clone())
                            .unwrap_or_default()
                    )
                ),
                (
                    "weights",
                    Json::Arr(
                        agent
                            .router_weights()
                            .into_iter()
                            .map(|w| Json::num(f64::from(w)))
                            .collect()
                    )
                ),
            ])
        );
    } else {
        let _ = writeln!(
            out,
            "antes: {} | depois: {} | resposta: {}",
            before.action.label(),
            after.action.label(),
            after
                .answer
                .as_ref()
                .map(|c| c.value.as_str())
                .unwrap_or("-")
        );
    }
    EXIT_OK
}

fn stats(args: &Args, out: &mut dyn Write) -> i32 {
    let agent = AetherAgent::new(args.config());
    if args.json {
        let _ = writeln!(out, "{}", agent.status_json());
    } else {
        let metrics = agent.metrics();
        let _ = writeln!(
            out,
            "runs={} steps={} acts={} abst={} correct={} wrong={}",
            metrics.runs,
            metrics.steps,
            metrics.acts,
            metrics.abstentions,
            metrics.correct,
            metrics.wrong
        );
        for (id, weight) in agent.experts() {
            let _ = writeln!(out, "  {:<10} {:.4}", id.label(), weight);
        }
    }
    EXIT_OK
}

fn demo(args: &Args, out: &mut dyn Write) -> i32 {
    let cfg = args.config();
    let facts_path = cfg.facts_path.clone();
    let mut agent = AetherAgent::new(cfg);

    let mut known_ok = 0usize;
    let probes = [
        ("Brasil", "capital", "Brasília"),
        ("Lua", "orbita", "Terra"),
        ("Rust", "é_uma", "linguagem"),
        ("água", "é", "H2O"),
        ("sol", "é_uma", "estrela"),
    ];
    for (subject, relation, expected) in probes {
        let outcome = agent.ask_fact(subject, relation);
        let ok = outcome.action == Action::Act
            && outcome
                .answer
                .as_ref()
                .is_some_and(|c| c.value.eq_ignore_ascii_case(expected));
        if ok {
            known_ok += 1;
        }
    }

    let unknown = agent.ask_fact("Atlântida", "capital");
    let knew_abstain = unknown.action != Action::Act;

    let sat = agent.satisfy(
        3,
        vec![Clause3::new2(0, 1, 1, 1), Clause3::new2(0, -1, 2, 1)],
    );
    let n = 4;
    let qubo = agent.optimize(n, vec![0.0; n * n], vec![1.0; n]);

    // Autoaprendizado: 5 fatos novos, com feedback verificado.
    let new_facts = [
        ("Itália", "capital", "Roma"),
        ("Japão", "capital", "Tóquio"),
        ("Espanha", "capital", "Madri"),
        ("Egito", "capital", "Cairo"),
        ("Canadá", "capital", "Ottawa"),
    ];
    let mut learned_ok = 0usize;
    for (subject, relation, value) in new_facts {
        let task = Task::Fact {
            subject: subject.to_string(),
            relation: relation.to_string(),
        };
        let before = agent.run(&task);
        agent.learn(
            &before,
            &Feedback {
                correct: Some(true),
                expected: Some(Claim::new(
                    subject,
                    relation,
                    value,
                    1.0,
                    255,
                    ExpertId::Transplant,
                    crate::core::certificate::Provenance::Human,
                )),
            },
        );
        let after = agent.run(&task);
        if after.action == Action::Act && after.answer.as_ref().is_some_and(|c| c.value == value) {
            learned_ok += 1;
        }
    }

    let metrics = agent.metrics();
    if args.json {
        let _ = writeln!(
            out,
            "{}",
            Json::obj([
                ("facts", Json::str(facts_path.display().to_string())),
                ("known_ok", Json::num(known_ok as f64)),
                ("known_total", Json::num(5.0)),
                ("abstained_unknown", Json::Bool(knew_abstain)),
                ("sat_action", Json::str(sat.action.label())),
                ("qubo_action", Json::str(qubo.action.label())),
                ("learned_ok", Json::num(learned_ok as f64)),
                ("learned_total", Json::num(5.0)),
                ("runs", Json::num(metrics.runs as f64)),
                ("correct", Json::num(metrics.correct as f64)),
                (
                    "weights",
                    Json::Arr(
                        agent
                            .router_weights()
                            .into_iter()
                            .map(|w| Json::num(f64::from(w)))
                            .collect()
                    )
                ),
            ])
        );
    } else {
        let _ = writeln!(out, "AETHER Agent — demonstração");
        let _ = writeln!(out, "  fatos transplantados : {}", facts_path.display());
        let _ = writeln!(out, "  recall conhecido     : {known_ok}/5");
        let _ = writeln!(
            out,
            "  absteve no desconhecido: {}",
            if knew_abstain { "sim" } else { "NÃO" }
        );
        let _ = writeln!(out, "  SAT                  : {}", sat.action.label());
        let _ = writeln!(out, "  QUBO                 : {}", qubo.action.label());
        let _ = writeln!(out, "  fatos aprendidos     : {learned_ok}/5");
        let _ = writeln!(
            out,
            "  métricas             : runs={} correct={} wrong={}",
            metrics.runs, metrics.correct, metrics.wrong
        );
        let _ = write!(out, "  pesos MoE            :");
        for (id, weight) in agent.experts() {
            let _ = write!(out, " {}={:.3}", id.label(), weight);
        }
        let _ = writeln!(out);
    }
    if known_ok >= 4 && knew_abstain && learned_ok >= 4 {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn print_outcome(out: &mut dyn Write, outcome: &Outcome, json: bool) {
    if json {
        let certificate = outcome
            .outputs
            .iter()
            .find_map(|o| o.certificate.as_ref())
            .map(|cert| {
                Json::obj([
                    ("energy", Json::num(cert.energy)),
                    ("gap", Json::num(cert.gap)),
                    ("violations", Json::num(f64::from(cert.violations))),
                    ("consensus", Json::num(cert.consensus)),
                ])
            });
        let mut object: Vec<(&str, Json)> = vec![
            ("action", Json::str(outcome.action.label())),
            (
                "answer",
                outcome
                    .answer
                    .as_ref()
                    .map(|c| Json::str(c.value.clone()))
                    .unwrap_or(Json::Null),
            ),
            (
                "confidence",
                Json::num(f64::from(
                    outcome.answer.as_ref().map_or(0.0, |c| c.confidence),
                )),
            ),
            ("support", Json::num(f64::from(outcome.coherence.support))),
            (
                "consensus",
                Json::num(f64::from(outcome.coherence.consensus)),
            ),
            (
                "authority",
                Json::num(f64::from(outcome.coherence.authority)),
            ),
            (
                "contradictions",
                Json::num(outcome.coherence.contradictions as f64),
            ),
            ("steps", Json::num(outcome.steps as f64)),
            (
                "experts",
                Json::Arr(
                    outcome
                        .experts_used
                        .iter()
                        .map(|e| Json::str(e.label()))
                        .collect(),
                ),
            ),
            ("rationale", Json::str(outcome.coherence.rationale.clone())),
        ];
        if let Some(cert) = certificate {
            object.push(("certificate", cert));
        }
        let pairs: Vec<(String, Json)> = object
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let _ = writeln!(out, "{}", Json::Obj(pairs));
    } else {
        let _ = writeln!(
            out,
            "ação: {} | suporte {:.3} | consenso {:.2} | autoridade {}",
            outcome.action.label(),
            outcome.coherence.support,
            outcome.coherence.consensus,
            outcome.coherence.authority
        );
        match &outcome.answer {
            Some(claim) => {
                let _ = writeln!(
                    out,
                    "resposta: {} (fonte {}, confiança {:.3})",
                    claim.value,
                    claim.source.label(),
                    claim.confidence
                );
            }
            None => {
                let _ = writeln!(out, "resposta: — (abstenção)");
            }
        }
        let _ = writeln!(
            out,
            "motivo: {} | passos {} | especialistas {}",
            outcome.coherence.rationale,
            outcome.steps,
            outcome
                .experts_used
                .iter()
                .map(|e| e.label())
                .collect::<Vec<_>>()
                .join(",")
        );
        if let Some(details) = outcome.coherence.contradiction_details.first() {
            let _ = writeln!(
                out,
                "contradição: {}|{} = {:?}",
                details.subject, details.relation, details.values
            );
        }
        if let Some(cert) = outcome.outputs.iter().find_map(|o| o.certificate.as_ref()) {
            let _ = writeln!(
                out,
                "certificado: E={:.4} ΔE={:.4} viol={} consenso={:.2}",
                cert.energy, cert.gap, cert.violations, cert.consensus
            );
        }
    }
}

/// Parser DIMACS mínimo (`p cnf n m` + cláusulas terminadas em `0`).
pub fn parse_dimacs(text: &str) -> Result<(usize, Vec<Clause3>), String> {
    let mut n = 0usize;
    let mut clauses = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('c') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("p ") {
            let mut parts = rest.split_whitespace();
            let kind = parts.next().unwrap_or("");
            if kind != "cnf" {
                return Err("formato DIMACS sem 'p cnf'".into());
            }
            n = parts
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or("DIMACS: n inválido")?;
            continue;
        }
        let mut lits: Vec<(usize, i8)> = Vec::new();
        for token in line.split_whitespace() {
            let value: i64 = token
                .parse()
                .map_err(|_| format!("literal inválido: {token}"))?;
            if value == 0 {
                break;
            }
            let var = value.unsigned_abs() as usize;
            let sign = if value > 0 { 1 } else { -1 };
            if var == 0 {
                return Err("DIMACS: variável 0".into());
            }
            lits.push((var - 1, sign));
        }
        if lits.is_empty() {
            continue;
        }
        let pad = *lits.last().unwrap();
        while lits.len() < 3 {
            lits.push(pad);
        }
        clauses.push(Clause3::new3(
            lits[0].0, lits[0].1, lits[1].0, lits[1].1, lits[2].0, lits[2].1,
        ));
    }
    if n == 0 || clauses.is_empty() {
        return Err("DIMACS sem variáveis ou cláusulas".into());
    }
    for clause in &clauses {
        for (idx, _) in [clause.lit(0), clause.lit(1), clause.lit(2)] {
            if idx >= n {
                return Err(format!("DIMACS: variável {} > n={n}", idx + 1));
            }
        }
    }
    Ok((n, clauses))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_capture(args: &[&str]) -> (i32, String) {
        let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let code = run(&argv, &mut out);
        (code, String::from_utf8_lossy(&out).to_string())
    }

    #[test]
    fn tools_e_goal_mostram_autoescolha() {
        let (code, text) = run_capture(&["tools", "--json"]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"name\":\"fact\""), "{text}");
        assert!(text.contains("\"risk\":\"low\""), "{text}");

        let (code, text) = run_capture(&[
            "goal",
            "--facts",
            "data/agent_facts.jsonl",
            "--subject",
            "AETHER",
            "--relation",
            "proibe",
            "--json",
        ]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"success\":true"), "{text}");
        assert!(text.contains("\"tool\":\"fact\""), "{text}");
        assert!(text.contains("backpropagation"), "{text}");
    }

    #[test]
    fn ask_conhecido_age_json() {
        let (code, text) = run_capture(&[
            "ask",
            "--subject",
            "Brasil",
            "--relation",
            "capital",
            "--json",
        ]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"action\":\"act\""));
        assert!(text.contains("Brasília"));
        assert!(text.contains("\"experts\""));
    }

    #[test]
    fn ask_desconhecido_abstem_e_sai_1() {
        let (code, text) = run_capture(&[
            "ask",
            "--subject",
            "Atlântida",
            "--relation",
            "capital",
            "--json",
        ]);
        assert_eq!(code, EXIT_FAIL);
        assert!(text.contains("\"action\":\"abstain\""));
    }

    #[test]
    fn dimacs_parseia() {
        let text = "c teste\np cnf 3 2\n1 2 0\n-1 3 0\n";
        let (n, clauses) = parse_dimacs(text).expect("dimacs");
        assert_eq!(n, 3);
        assert_eq!(clauses.len(), 2);
        assert!(clauses[0].satisfied(&[1, 1, 1]));
    }

    #[test]
    fn help_e_uso() {
        let (code, text) = run_capture(&["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(text.contains("COMANDOS"));
        let (code, _) = run_capture(&["nada"]);
        assert_eq!(code, EXIT_USAGE);
    }
}

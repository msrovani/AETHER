//! CLI `aether-sgdb` — a usabilidade do `neural-sgdb` em um comando.
//!
//! A lógica vive aqui (não no binário) para ser testável sem processo: o
//! `src/bin/aether_sgdb.rs` só chama [`run`] e propaga o código de saída.
//!
//! ```text
//! aether-sgdb doctor
//! aether-sgdb put "o gate F4 roda no CI" --layer L3
//! aether-sgdb recall "gate" --k 5
//! aether-sgdb sync --outbox target/aether_sgdb.jsonl
//! ```
//!
//! Todos os comandos aceitam `--json` (saída de máquina) e as mesmas variáveis
//! de ambiente do cliente (`AETHER_SGDB_*`). Nada é inventado: quando o banco
//! está fora, o comando diz o que fazer e preserva o outbox.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use crate::core::knowledge_bridge_f1::ScopeId;
use crate::json::Json;
use crate::knowledge::sgdb::client::{open_transport, SgdbConfig};
use crate::knowledge::sgdb::ledger::{encode_phases, phase_key, SgdbLedger};
use crate::knowledge::sgdb::wire::PhaseDoc;
use crate::knowledge::sgdb::SgdbError;

/// Código de saída: sucesso.
pub const EXIT_OK: i32 = 0;
/// Código de saída: falha de execução (banco fora, arquivo inválido).
pub const EXIT_FAIL: i32 = 1;
/// Código de saída: uso incorreto.
pub const EXIT_USAGE: i32 = 2;

/// Versão do crate (mostrada em `--version` e `doctor`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Ajuda completa.
pub const HELP: &str = "\
aether-sgdb — ponte de linha de comando para o neural-sgdb

USO
  aether-sgdb <comando> [opções]

COMANDOS
  doctor                 diagnóstico de conexão, escopo e outbox
  ping                   health do servidor (view=status)
  put <texto>            grava um fato textual (L3 por padrão, autoridade 255)
  recall <consulta>      busca fatos (fase e texto)
  stat                   estado local + health do servidor
  sync                   reenvia o outbox e fecha o run
  export <arquivo>       exporta documentos de fase do escopo (JSONL)
  import <arquivo>       importa documentos de fase (JSONL)
  forget <chave>         aposenta uma memória (--reason)
  ttl <chave>            aplica TTL (--ms N ou --ttl-ms N)
  explain <chave>        proveniência completa (curate op=explain)
  demo                   round-trip completo com o mock (sem servidor)
  help                   esta ajuda

TRANSPORTE (um destes; senão usa AETHER_SGDB_URL/AETHER_SGDB_STDIO)
  --mock                 banco em memória (testes/demonstração)
  --url <URL>            servidor MCP HTTP: http://host:porta/mcp
  --stdio <cmd args>     servidor MCP local lançado por comando

ESCOPO E ESCRITA
  --organism N           organismo do escopo (padrão 1)
  --run N                run/linhagem (padrão 0)
  --app N                tag de aplicação (padrão 1)
  --scope-app NOME       scope_app do banco (padrão AETHER)
  --scope-agent NOME     scope_agent (padrão aether-engine)
  --layer L3|L4|...      camada do fato textual (padrão L3)
  --authority N          autoridade 0..255 (padrão 255 = HITL)
  --entity NOME          entidade extra (repetível)
  --ttl-ms N             TTL em ms
  --reason TEXTO         motivo do forget (padrão: operador)

OUTROS
  --outbox <arquivo>     diário local de replay (recomendado)
  --token T              Bearer token do MCP HTTP
  --timeout-ms N         timeout HTTP (padrão 2000)
  --retries N            tentativas extras em erro transitório (padrão 2)
  --k N                  hits por consulta (1..20; padrão 8)
  --json                 saída JSON (máquina)
  --quiet                só o essencial
  --version              versão
  --help                 esta ajuda

AMBIENTE
  AETHER_SGDB_URL, AETHER_SGDB_STDIO, AETHER_SGDB_TOKEN,
  AETHER_SGDB_OUTBOX, AETHER_SGDB_SCOPE_APP, AETHER_SGDB_SCOPE_AGENT,
  AETHER_SGDB_TTL_MS, AETHER_SGDB_RECALL_K, AETHER_SGDB_RETRIES,
  AETHER_SGDB_TIMEOUT_MS, AETHER_SGDB_AUDIT

EXEMPLOS
  aether-sgdb doctor --url http://127.0.0.1:8090/mcp
  aether-sgdb put \"gate F4 verde\" --entity aether/f4
  aether-sgdb recall gate --k 5 --json
  aether-sgdb sync --outbox target/aether_sgdb.jsonl
  aether-sgdb export target/fase.jsonl --organism 2742943747 --run 0
";

#[derive(Clone, Debug)]
struct Invocation {
    command: Option<String>,
    positional: Vec<String>,
    url: Option<String>,
    stdio: Option<String>,
    mock: bool,
    outbox: Option<PathBuf>,
    scope_app: Option<String>,
    scope_agent: Option<String>,
    organism: u64,
    run: u64,
    app: u32,
    token: Option<String>,
    json: bool,
    quiet: bool,
    k: usize,
    layer: String,
    authority: u8,
    ttl_ms: Option<u64>,
    retries: Option<u32>,
    timeout_ms: Option<u64>,
    reason: Option<String>,
    entities: Vec<String>,
    view: String,
    help: bool,
    version: bool,
}

impl Default for Invocation {
    fn default() -> Self {
        Self {
            command: None,
            positional: Vec::new(),
            url: None,
            stdio: None,
            mock: false,
            outbox: None,
            scope_app: None,
            scope_agent: None,
            organism: env_u64("AETHER_SGDB_ORGANISM").unwrap_or(1),
            run: env_u64("AETHER_SGDB_RUN").unwrap_or(0),
            app: env_u64("AETHER_SGDB_APP").unwrap_or(1) as u32,
            token: None,
            json: false,
            quiet: false,
            k: 8,
            layer: "L3".to_string(),
            authority: 255,
            ttl_ms: None,
            retries: None,
            timeout_ms: None,
            reason: None,
            entities: Vec::new(),
            view: "status".to_string(),
            help: false,
            version: false,
        }
    }
}

impl Invocation {
    fn scope(&self) -> ScopeId {
        ScopeId::new(self.organism, self.run, self.app)
    }

    fn text(&self) -> String {
        self.positional.join(" ")
    }

    fn config(&self) -> SgdbConfig {
        let mut cfg = SgdbConfig::from_env();
        if let Some(v) = &self.scope_app {
            cfg.scope_app = v.clone();
        }
        if let Some(v) = &self.scope_agent {
            cfg.scope_agent = v.clone();
        }
        if let Some(v) = self.ttl_ms {
            cfg.ttl_ms = Some(v);
        }
        if let Some(v) = self.retries {
            cfg.retries = v;
        }
        if let Some(v) = self.timeout_ms {
            cfg.timeout_ms = v;
        }
        if let Some(v) = &self.outbox {
            cfg.outbox = Some(v.clone());
        }
        if let Some(v) = &self.token {
            cfg.token = Some(v.clone());
        }
        cfg.recall_k = self.k.clamp(1, 20);
        cfg
    }

    fn spec(&self, _cfg: &SgdbConfig) -> Result<String, SgdbError> {
        if self.mock {
            return Ok("mock".to_string());
        }
        if let Some(command) = &self.stdio {
            return Ok(format!("stdio:{command}"));
        }
        if let Some(url) = &self.url {
            return Ok(url.clone());
        }
        if let Ok(url) = std::env::var("AETHER_SGDB_URL") {
            if !url.trim().is_empty() {
                return Ok(url);
            }
        }
        if let Ok(command) = std::env::var("AETHER_SGDB_STDIO") {
            if !command.trim().is_empty() {
                return Ok(format!("stdio:{command}"));
            }
        }
        Err(SgdbError::Config(
            "informe --mock, --url ou --stdio (ou AETHER_SGDB_URL)".to_string(),
        ))
    }
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

fn parse(argv: &[String]) -> Result<Invocation, String> {
    let mut inv = Invocation::default();
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
            "-h" | "--help" => inv.help = true,
            "-V" | "--version" => inv.version = true,
            "--mock" => inv.mock = true,
            "--json" => inv.json = true,
            "--quiet" => inv.quiet = true,
            "--url" => inv.url = Some(value(&mut i)?),
            "--stdio" => inv.stdio = Some(value(&mut i)?),
            "--outbox" => inv.outbox = Some(PathBuf::from(value(&mut i)?)),
            "--scope-app" => inv.scope_app = Some(value(&mut i)?),
            "--scope-agent" => inv.scope_agent = Some(value(&mut i)?),
            "--organism" => inv.organism = parse_num::<u64>(&value(&mut i)?, "--organism")?,
            "--run" | "--scope-run" => inv.run = parse_num::<u64>(&value(&mut i)?, "--run")?,
            "--app" => inv.app = parse_num::<u64>(&value(&mut i)?, "--app")? as u32,
            "--token" => inv.token = Some(value(&mut i)?),
            "--k" => inv.k = parse_num::<usize>(&value(&mut i)?, "--k")?,
            "--layer" => inv.layer = value(&mut i)?.to_uppercase(),
            "--authority" => {
                inv.authority = parse_num::<u64>(&value(&mut i)?, "--authority")?.min(255) as u8
            }
            "--ttl-ms" | "--ms" => {
                inv.ttl_ms = Some(parse_num::<u64>(&value(&mut i)?, "--ttl-ms")?)
            }
            "--retries" => {
                inv.retries = Some(parse_num::<u64>(&value(&mut i)?, "--retries")? as u32)
            }
            "--timeout-ms" => {
                inv.timeout_ms = Some(parse_num::<u64>(&value(&mut i)?, "--timeout-ms")?)
            }
            "--reason" => inv.reason = Some(value(&mut i)?),
            "--entity" => inv.entities.push(value(&mut i)?),
            "--text" | "--query" => inv.positional.push(value(&mut i)?),
            "--view" => inv.view = value(&mut i)?,
            other if other.starts_with("--") => {
                return Err(format!("flag desconhecida: {other} (use --help)"))
            }
            _ => {
                if inv.command.is_none() {
                    inv.command = Some(arg.clone());
                } else {
                    inv.positional.push(arg.clone());
                }
            }
        }
        i += 1;
    }
    Ok(inv)
}

fn parse_num<T: std::str::FromStr>(raw: &str, flag: &str) -> Result<T, String> {
    raw.parse()
        .map_err(|_| format!("{flag} espera um número inteiro (veio {raw})"))
}

fn record_failure(out: &mut dyn Write, error: &SgdbError) {
    let _ = writeln!(out, "erro: {error}");
    let _ = writeln!(out, "dica: {}", error.hint());
}

/// Executa a CLI. `argv` são os argumentos **sem** o nome do programa.
pub fn run(argv: &[String], out: &mut dyn Write) -> i32 {
    let inv = match parse(argv) {
        Ok(inv) => inv,
        Err(e) => {
            let _ = writeln!(out, "erro de uso: {e}");
            let _ = writeln!(out, "dica: aether-sgdb --help");
            return EXIT_USAGE;
        }
    };
    if inv.help || (inv.command.is_none() && !inv.version) {
        let _ = write!(out, "{HELP}");
        return if inv.help { EXIT_OK } else { EXIT_USAGE };
    }
    if inv.version {
        let _ = writeln!(out, "aether-sgdb {VERSION}");
        return EXIT_OK;
    }
    let command = match &inv.command {
        Some(c) => c.as_str(),
        None => {
            let _ = write!(out, "{HELP}");
            return EXIT_USAGE;
        }
    };
    match command {
        "help" => {
            let _ = write!(out, "{HELP}");
            EXIT_OK
        }
        "version" => {
            let _ = writeln!(out, "aether-sgdb {VERSION}");
            EXIT_OK
        }
        "demo" => demo(&inv, out),
        "doctor" => doctor(&inv, out),
        "ping" => ping(&inv, out),
        "put" | "remember" | "add" => put(&inv, out),
        "recall" | "search" => recall(&inv, out),
        "stat" | "status" => stat(&inv, out),
        "sync" | "flush" => sync(&inv, out),
        "export" => export(&inv, out),
        "import" => import(&inv, out),
        "forget" => forget(&inv, out),
        "ttl" => ttl(&inv, out),
        "explain" => explain(&inv, out),
        other => {
            let _ = writeln!(out, "comando desconhecido: {other}");
            let _ = writeln!(out, "dica: aether-sgdb --help");
            EXIT_USAGE
        }
    }
}

fn doctor(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let scope = inv.scope();
    let mut client = match super::client::open(&spec, cfg.clone()) {
        Ok(client) => client,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let t0 = Instant::now();
    let ping = client.health();
    let ping_ms = t0.elapsed().as_secs_f64() * 1_000.0;
    // Sem ping, não insiste no recall: o diagnóstico tem que ser rápido.
    let (recall, recall_ms) = if ping.is_ok() {
        let t1 = Instant::now();
        let recall = client.recall_scope(scope, 1);
        (Some(recall), t1.elapsed().as_secs_f64() * 1_000.0)
    } else {
        (None, 0.0)
    };
    let outbox_pending = cfg
        .outbox
        .as_ref()
        .map(|path| super::transport::Outbox::new(path).len())
        .unwrap_or(0);
    let ok = ping.is_ok();
    if inv.json {
        let mut object: Vec<(&str, Json)> = vec![
            ("ok", Json::Bool(ok)),
            ("version", Json::str(VERSION)),
            ("transport", Json::Str(client.describe())),
            ("scope_app", Json::str(cfg.scope_app.clone())),
            (
                "scope",
                Json::obj([
                    ("organism", Json::num(scope.organism_id as f64)),
                    ("run", Json::num(scope.run_id as f64)),
                    ("app", Json::num(f64::from(scope.app_tag))),
                ]),
            ),
            ("ping_ms", Json::num(ping_ms)),
            ("recall_ms", Json::num(recall_ms)),
            (
                "recall_hits",
                Json::num(
                    recall
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .map(|h| h.len())
                        .unwrap_or(0) as f64,
                ),
            ),
            ("retries", Json::num(f64::from(cfg.retries))),
            ("timeout_ms", Json::num(cfg.timeout_ms as f64)),
            (
                "outbox",
                Json::obj([
                    (
                        "path",
                        Json::str(
                            cfg.outbox
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default(),
                        ),
                    ),
                    ("pending", Json::num(outbox_pending as f64)),
                ]),
            ),
        ];
        if let Ok(health) = &ping {
            object.push(("health", health.clone()));
        }
        if let Err(e) = &ping {
            object.push((
                "error",
                Json::obj([
                    ("message", Json::str(e.to_string())),
                    ("hint", Json::str(e.hint())),
                ]),
            ));
        }
        let pairs: Vec<(String, Json)> = object
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let _ = writeln!(out, "{}", Json::Obj(pairs));
    } else {
        let _ = writeln!(out, "aether-sgdb {VERSION} — diagnóstico");
        let _ = writeln!(out, "  transporte : {}", client.describe());
        let _ = writeln!(
            out,
            "  escopo     : org={} run={} app={} (scope_app={}, agent={})",
            scope.organism_id, scope.run_id, scope.app_tag, cfg.scope_app, cfg.scope_agent
        );
        let _ = writeln!(
            out,
            "  outbox     : {} ({outbox_pending} pendente(s))",
            cfg.outbox
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "desligado".to_string())
        );
        match &ping {
            Ok(_) => {
                let _ = writeln!(out, "  ping       : ok ({ping_ms:.1} ms)");
            }
            Err(e) => {
                let _ = writeln!(out, "  ping       : FALHOU — {e}");
                let _ = writeln!(out, "  dica       : {}", e.hint());
            }
        }
        match &recall {
            Some(Ok(hits)) => {
                let _ = writeln!(
                    out,
                    "  recall     : ok ({recall_ms:.1} ms, {} hit(s))",
                    hits.len()
                );
            }
            Some(Err(e)) => {
                let _ = writeln!(out, "  recall     : FALHOU — {e}");
            }
            None => {
                let _ = writeln!(out, "  recall     : não testado (ping falhou)");
            }
        }
        let _ = writeln!(
            out,
            "  política   : retries={} backoff={} ms timeout={} ms",
            cfg.retries, cfg.retry_backoff_ms, cfg.timeout_ms
        );
        let _ = writeln!(
            out,
            "  status     : {}",
            if ok { "OK" } else { "SEM CONEXÃO" }
        );
        if inv.outbox.is_none() && !inv.mock {
            let _ = writeln!(
                out,
                "  aviso      : sem --outbox, uma queda perde o lote em memória"
            );
        }
    }
    if ok {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn ping(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match super::client::open(&spec, cfg) {
        Ok(mut client) => match client.health_view(&inv.view) {
            Ok(result) => {
                let _ = writeln!(out, "{}", result.to_text());
                EXIT_OK
            }
            Err(e) => {
                record_failure(out, &e);
                EXIT_FAIL
            }
        },
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn put(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let text = inv.text();
    if text.trim().is_empty() {
        let _ = writeln!(
            out,
            "erro de uso: put exige o texto como argumento ou --text"
        );
        return EXIT_USAGE;
    }
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let mut client = match super::client::open(&spec, cfg.clone()) {
        Ok(client) => client,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match client.remember_text(
        inv.scope(),
        &text,
        &inv.entities,
        &inv.layer,
        inv.authority,
        inv.ttl_ms,
    ) {
        Ok(key) => {
            if inv.json {
                let _ = writeln!(
                    out,
                    "{}",
                    Json::obj([
                        ("ok", Json::Bool(true)),
                        ("key", Json::Str(key.clone().unwrap_or_default())),
                        ("layer", Json::str(inv.layer.clone())),
                        ("authority", Json::num(f64::from(inv.authority))),
                    ])
                );
            } else {
                let _ = writeln!(
                    out,
                    "gravado: {} ({}, autoridade {}{})",
                    key.as_deref().unwrap_or("(chave não informada)"),
                    inv.layer,
                    inv.authority,
                    inv.ttl_ms
                        .map(|ttl| format!(", ttl {ttl} ms"))
                        .unwrap_or_default()
                );
            }
            EXIT_OK
        }
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn recall(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let query = inv.text();
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let mut client = match super::client::open(&spec, cfg) {
        Ok(client) => client,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let scope = inv.scope();
    let text_hits = client.recall_text(scope, &query, inv.k);
    let phase_hits = client.recall(scope, &query, inv.k);
    if text_hits.is_err() && phase_hits.is_err() {
        let error = text_hits.err().or_else(|| phase_hits.err());
        if let Some(error) = error {
            record_failure(out, &error);
        }
        return EXIT_FAIL;
    }
    let mut rows: Vec<(f32, String, String, f32, u8, bool)> = Vec::new();
    if let Ok(hits) = text_hits {
        for hit in hits {
            rows.push((
                hit.score,
                hit.text,
                hit.storage_key.unwrap_or_default(),
                hit.score,
                hit.authority,
                hit.is_phase,
            ));
        }
    }
    if let Ok(hits) = phase_hits {
        for hit in hits {
            rows.push((
                hit.score,
                format!(
                    "fase L{} E={:.3} ΔE={:.3}",
                    hit.doc.layer.code(),
                    hit.doc.certificate.energy,
                    hit.doc.certificate.gap
                ),
                hit.storage_key.unwrap_or_default(),
                hit.score,
                hit.authority,
                true,
            ));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    if inv.json {
        let array = rows
            .iter()
            .map(|(_, text, key, score, authority, is_phase)| {
                Json::obj([
                    ("text", Json::str(text.clone())),
                    ("key", Json::str(key.clone())),
                    ("score", Json::num(f64::from(*score))),
                    ("authority", Json::num(f64::from(*authority))),
                    ("kind", Json::str(if *is_phase { "phase" } else { "text" })),
                ])
            })
            .collect();
        let _ = writeln!(out, "{}", Json::Arr(array));
    } else if rows.is_empty() {
        let _ = writeln!(out, "nenhum hit para {query:?}");
    } else {
        for (i, (_, text, key, score, authority, is_phase)) in rows.iter().enumerate() {
            let body = if *is_phase {
                text.clone()
            } else {
                summarize(text, 160)
            };
            let _ = writeln!(
                out,
                "{:>2}. {score:.2} auth={authority:<3} {}  {}",
                i + 1,
                if key.is_empty() { "-" } else { key.as_str() },
                body
            );
        }
    }
    EXIT_OK
}

fn stat(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let transport = match open_transport(&spec, &cfg) {
        Ok(t) => t,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let mut ledger = SgdbLedger::new(transport, cfg.clone(), inv.scope());
    let status = ledger.status();
    let health = ledger.client_mut().health_view(&inv.view);
    if inv.json {
        let mut object: Vec<(&str, Json)> = vec![
            ("online", Json::Bool(status.online)),
            ("transport", Json::str(status.transport.clone())),
            ("pending", Json::num(status.pending as f64)),
            ("failures", Json::num(f64::from(status.failures))),
            (
                "metrics",
                Json::obj([
                    ("remembered", Json::num(status.metrics.remembered as f64)),
                    ("spooled", Json::num(status.metrics.spooled as f64)),
                    ("replayed", Json::num(status.metrics.replayed as f64)),
                    ("errors", Json::num(status.metrics.errors as f64)),
                    ("commits", Json::num(status.metrics.commits as f64)),
                ]),
            ),
        ];
        if let Some(error) = &status.last_error {
            object.push(("last_error", Json::str(error.clone())));
        }
        if let Ok(health) = health {
            object.push(("health", health));
        }
        let pairs: Vec<(String, Json)> = object
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let _ = writeln!(out, "{}", Json::Obj(pairs));
    } else {
        let _ = writeln!(out, "transporte : {}", status.transport);
        let _ = writeln!(
            out,
            "online     : {}",
            if status.online { "sim" } else { "não" }
        );
        let _ = writeln!(out, "pendentes  : {}", status.pending);
        let _ = writeln!(out, "falhas     : {}", status.failures);
        let _ = writeln!(
            out,
            "métricas   : gravados={} spooled={} replay={} erros={} commits={}",
            status.metrics.remembered,
            status.metrics.spooled,
            status.metrics.replayed,
            status.metrics.errors,
            status.metrics.commits
        );
        if let Some(error) = &status.last_error {
            let _ = writeln!(out, "último erro: {error}");
        }
        match health {
            Ok(health) => {
                let _ = writeln!(out, "health     : {}", health.to_text());
            }
            Err(e) => {
                let _ = writeln!(out, "health     : FALHOU — {e}");
                let _ = writeln!(out, "dica       : {}", e.hint());
            }
        }
    }
    if status.online {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn sync(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let transport = match open_transport(&spec, &cfg) {
        Ok(t) => t,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let scope = inv.scope();
    let mut ledger = SgdbLedger::new(transport, cfg.clone(), scope);
    let delivered = ledger.flush_now();
    let committed = ledger.client_mut().commit_run(scope).is_ok();
    let status = ledger.status();
    if inv.json {
        let _ = writeln!(
            out,
            "{}",
            Json::obj([
                ("ok", Json::Bool(status.online)),
                ("delivered", Json::num(delivered as f64)),
                ("pending", Json::num(status.pending as f64)),
                ("commit_run", Json::Bool(committed)),
                ("errors", Json::num(status.metrics.errors as f64)),
            ])
        );
    } else {
        let _ = writeln!(out, "entregues : {delivered}");
        let _ = writeln!(out, "pendentes : {}", status.pending);
        let _ = writeln!(
            out,
            "commit_run: {}",
            if committed { "ok" } else { "falhou" }
        );
        if let Some(error) = &status.last_error {
            let _ = writeln!(out, "erro      : {error}");
        }
    }
    if status.online {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn export(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let Some(path) = inv.positional.first() else {
        let _ = writeln!(out, "erro de uso: export <arquivo>");
        return EXIT_USAGE;
    };
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let mut client = match super::client::open(&spec, cfg) {
        Ok(client) => client,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match client.recall_all(inv.scope(), "", 20_000) {
        Ok(hits) => {
            let mut text = String::new();
            for hit in &hits {
                text.push_str(&hit.doc.to_text());
                text.push('\n');
            }
            if let Err(e) = std::fs::write(path, text) {
                let _ = writeln!(out, "erro: não escrevi {path}: {e}");
                return EXIT_FAIL;
            }
            if inv.json {
                let _ = writeln!(
                    out,
                    "{}",
                    Json::obj([
                        ("ok", Json::Bool(true)),
                        ("file", Json::str(path.clone())),
                        ("documents", Json::num(hits.len() as f64)),
                    ])
                );
            } else {
                let _ = writeln!(out, "exportados {} documento(s) para {path}", hits.len());
            }
            EXIT_OK
        }
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn import(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let Some(path) = inv.positional.first() else {
        let _ = writeln!(out, "erro de uso: import <arquivo.jsonl>");
        return EXIT_USAGE;
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            let _ = writeln!(out, "erro: não li {path}: {e}");
            return EXIT_FAIL;
        }
    };
    let mut docs: Vec<PhaseDoc> = text.lines().filter_map(PhaseDoc::parse).collect();
    let invalid = text.lines().filter(|l| !l.trim().is_empty()).count() - docs.len();
    if docs.is_empty() {
        let _ = writeln!(out, "erro: nenhum documento aether-phase/v1 em {path}");
        return EXIT_FAIL;
    }
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let mut client = match super::client::open(&spec, cfg) {
        Ok(client) => client,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match client.remember_docs(&mut docs) {
        Ok(written) => {
            if inv.json {
                let _ = writeln!(
                    out,
                    "{}",
                    Json::obj([
                        ("ok", Json::Bool(true)),
                        ("written", Json::num(written as f64)),
                        ("invalid", Json::num(invalid as f64)),
                    ])
                );
            } else {
                let _ = writeln!(
                    out,
                    "importados {written} documento(s) de {path} (inválidos: {invalid})"
                );
            }
            EXIT_OK
        }
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn forget(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let Some(key) = inv.positional.first() else {
        let _ = writeln!(out, "erro de uso: forget <chave> [--reason TEXTO]");
        return EXIT_USAGE;
    };
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    let reason = inv.reason.as_deref().unwrap_or("operador");
    match super::client::open(&spec, cfg) {
        Ok(mut client) => match client.forget(key, reason) {
            Ok(result) => {
                let _ = writeln!(out, "{}", result.to_text());
                EXIT_OK
            }
            Err(e) => {
                record_failure(out, &e);
                EXIT_FAIL
            }
        },
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn ttl(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let Some(key) = inv.positional.first() else {
        let _ = writeln!(out, "erro de uso: ttl <chave> --ttl-ms N");
        return EXIT_USAGE;
    };
    let Some(ttl_ms) = inv.ttl_ms else {
        let _ = writeln!(out, "erro de uso: ttl exige --ttl-ms N");
        return EXIT_USAGE;
    };
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match super::client::open(&spec, cfg) {
        Ok(mut client) => match client.set_ttl(key, ttl_ms) {
            Ok(result) => {
                let _ = writeln!(out, "{}", result.to_text());
                EXIT_OK
            }
            Err(e) => {
                record_failure(out, &e);
                EXIT_FAIL
            }
        },
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn explain(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let Some(key) = inv.positional.first() else {
        let _ = writeln!(out, "erro de uso: explain <chave>");
        return EXIT_USAGE;
    };
    let cfg = inv.config();
    let spec = match inv.spec(&cfg) {
        Ok(spec) => spec,
        Err(e) => {
            record_failure(out, &e);
            return EXIT_FAIL;
        }
    };
    match super::client::open(&spec, cfg) {
        Ok(mut client) => match client.explain(key) {
            Ok(result) => {
                let _ = writeln!(out, "{}", result.to_text());
                EXIT_OK
            }
            Err(e) => {
                record_failure(out, &e);
                EXIT_FAIL
            }
        },
        Err(e) => {
            record_failure(out, &e);
            EXIT_FAIL
        }
    }
}

fn demo(inv: &Invocation, out: &mut dyn Write) -> i32 {
    let cfg = SgdbConfig::default();
    let scope = inv.scope();
    let mut ledger = SgdbLedger::new(
        crate::knowledge::sgdb::transport::MockTransport::new(),
        cfg,
        scope,
    );
    let dims = 16;
    let patterns: Vec<Vec<f32>> = (0..3)
        .map(|p| {
            (0..dims)
                .map(|i| {
                    let v = (p * dims + i) as f32 * 0.37;
                    v.rem_euclid(std::f32::consts::TAU)
                })
                .collect()
        })
        .collect();
    for (idx, phases) in patterns.iter().enumerate() {
        let mut payload = Vec::new();
        encode_phases(phases, &mut payload);
        ledger.stage(crate::core::knowledge_bridge_f1::DraftRecord {
            key: phase_key(&payload),
            layer: crate::core::knowledge_bridge_f1::Layer::L4Semantic,
            certificate: crate::core::knowledge_bridge_f1::Certificate {
                energy: -1.0,
                gap: 0.05,
                violations: 0,
            },
            authority: 200,
            scope,
            tick: idx as u64,
            wall_ms: 1_700_000_000_000,
            payload,
        });
    }
    let committed = ledger.commit_run();
    let hits = ledger.recall(scope, "", 20);
    let mut query = Vec::with_capacity(dims * 2);
    for phase in &patterns[0] {
        query.push(phase.cos());
        query.push(phase.sin());
    }
    let mut seed = vec![0.0f32; dims];
    let cue = ledger.cue_into_scoped(scope, &query, &mut seed);
    let _ = writeln!(
        out,
        "demo: {committed} padrões gravados, {} hits, cue={:?}",
        hits.len(),
        cue.as_ref().map(|hit| hit.hits)
    );
    if inv.json {
        let _ = writeln!(
            out,
            "{}",
            Json::obj([
                ("ok", Json::Bool(cue.is_ok())),
                ("committed", Json::num(committed as f64)),
                ("hits", Json::num(hits.len() as f64)),
                (
                    "seed_error",
                    Json::num(
                        seed.iter()
                            .zip(patterns[0].iter())
                            .map(|(a, b)| {
                                let delta = (a - b).rem_euclid(std::f32::consts::TAU);
                                delta.min(std::f32::consts::TAU - delta)
                            })
                            .fold(0.0f32, f32::max) as f64,
                    ),
                ),
            ])
        );
    }
    if cue.is_ok() {
        EXIT_OK
    } else {
        EXIT_FAIL
    }
}

fn summarize(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push('…');
    }
    out.replace('\n', " ")
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

    fn temp_path(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "aether_sgdb_cli_{tag}_{}_{unique}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn ajuda_e_erro_de_uso_tem_codigos_distintos() {
        let (code, text) = run_capture(&["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(text.contains("COMANDOS"));
        let (code, text) = run_capture(&["--nope"]);
        assert_eq!(code, EXIT_USAGE);
        assert!(text.contains("flag desconhecida"));
        let (code, _) = run_capture(&["comando-que-nao-existe"]);
        assert_eq!(code, EXIT_USAGE);
    }

    #[test]
    fn doctor_mock_e_json() {
        let (code, text) = run_capture(&["doctor", "--mock", "--json"]);
        assert_eq!(code, EXIT_OK);
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains("\"transport\":\"mock"));
        assert!(text.contains("\"ping_ms\""));
    }

    #[test]
    fn put_grava_e_devolve_chave() {
        let (code, text) = run_capture(&["put", "gate F4 verde", "--mock", "--json"]);
        assert_eq!(code, EXIT_OK);
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains("\"key\":\"md/L3/aether/"));
    }

    #[test]
    fn import_e_export_de_fase() {
        let mut payload = Vec::new();
        encode_phases(&[0.0, 1.0, 2.0], &mut payload);
        let doc = PhaseDoc {
            key: phase_key(&payload),
            layer: crate::core::knowledge_bridge_f1::Layer::L4Semantic,
            certificate: crate::core::knowledge_bridge_f1::Certificate::default(),
            authority: 200,
            scope: ScopeId::new(1, 0, 1),
            tick: 1,
            wall_ms: 1,
            payload,
            storage_key: None,
        };
        let path = temp_path("import");
        std::fs::write(&path, format!("{}\n", doc.to_text())).unwrap();
        let (code, text) = run_capture(&["import", path.to_str().unwrap(), "--mock", "--json"]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"written\":1"));
        let exported = temp_path("export");
        let (code, text) = run_capture(&["export", exported.to_str().unwrap(), "--mock", "--json"]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"documents\":0"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exported);
    }

    #[test]
    fn sync_entrega_outbox() {
        let mut payload = Vec::new();
        encode_phases(&[0.5, 1.5], &mut payload);
        let doc = PhaseDoc {
            key: phase_key(&payload),
            layer: crate::core::knowledge_bridge_f1::Layer::L4Semantic,
            certificate: crate::core::knowledge_bridge_f1::Certificate::default(),
            authority: 200,
            scope: ScopeId::new(1, 0, 1),
            tick: 1,
            wall_ms: 1,
            payload,
            storage_key: None,
        };
        let path = temp_path("sync");
        std::fs::write(&path, format!("{}\n", doc.to_text())).unwrap();
        let (code, text) = run_capture(&[
            "sync",
            "--mock",
            "--outbox",
            path.to_str().unwrap(),
            "--json",
        ]);
        assert_eq!(code, EXIT_OK, "{text}");
        assert!(text.contains("\"delivered\":1"));
        assert!(!path.exists(), "outbox não foi drenado");
    }
}

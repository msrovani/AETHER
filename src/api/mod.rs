//! Borda externa **HTTP/JSON** (T2): como um app de IA (llama.cpp, Ollama,
//! SDK da OpenAI) consome o AETHER.
//!
//! # O que este módulo é — e o que não é
//!
//! O motor não gera texto: ele **recupera fase**, **resolve lógica** e devolve
//! **certificado de factibilidade**. Esta borda expõe exatamente isso em JSON,
//! compatível com o dialeto da OpenAI (`/v1/chat/completions`, inclusive SSE) e
//! com o do llama.cpp (`/completion`), para que um app externo possa apontar
//! para cá.
//!
//! A conversão texto → fase é um **hash determinístico e lossy** (documentado
//! no cabeçalho `X-Aether-Edge`): não é embedding semântico. Por isso a
//! resposta traz `verdict` e pode ser **abstenção** — que é uma resposta
//! válida, não um erro HTTP.
//!
//! # Invariantes preservados
//!
//! * Nenhuma dependência nova: HTTP/1.1 mínimo sobre `std::net`.
//! * O núcleo não conhece este módulo; `String` e I/O vivem só aqui (T2).
//! * Zero chamada ao ledger no caminho quente: `record` é do F1, e aqui só se
//!   **lê** o que já foi drenado.
//!
//! ```text
//! app (llama/agent) ──HTTP──► api ──► ScopeLedger (F1, T1/T2)
//!                              └───► roof_dual + solver + Certificate
//!                              └───► ledger.jsonl (persistência, T2)
//! ```

// O JSON mínimo vive no crate (sem features) porque o `sgdb` também precisa
// dele; aqui ele continua exposto no mesmo caminho público `api::json`.
pub use crate::json;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ndarray::{Array2, Array3};

use crate::agent::{
    AetherAgent, AgentConfig, Budget, Goal, Risk, SuccessCriteria, Task, ToolAgent,
};
use crate::core::logic_pipeline::Clause3;
use crate::core::certificate::{self, Decision, Provenance};
use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate as BridgeCertificate, DraftRecord, Layer, ScopeId, ScopeLedger,
};
use crate::core::presolve::{roof_dual_qumo, ROOF_MAX_N};
use crate::core::thermodynamic_solver::{IsingLandscape, ThermodynamicIsingSolver};
use crate::error::AetherError;
use crate::knowledge::sgdb::{open, SgdbClient, SgdbConfig, Transport};

use json::Json;

/// Dimensão de fase exposta pela borda (**fasores**).
pub const PHASE_DIMS: usize = 160;

/// Floats no payload: cada fasor é `(re, im)` ⇒ `2 × PHASE_DIMS = 320`
/// (1280 bytes, exatamente `MAX_PHASE_BYTES` do F1).
pub const PHASE_FLOATS: usize = 2 * PHASE_DIMS;

/// Corpo máximo aceito (1 MiB) — acima disso a conexão é fechada com 413.
pub const MAX_BODY: usize = 1 << 20;

/// Score mínimo de correlação para responder em vez de abster.
///
/// Duas fases aleatórias dão correlação ~0; o mesmo vetor dá 1. Abaixo de 0,5
/// o fato não é "parecido" — é ruído, e responder seria inventar.
pub const MIN_SCORE: f32 = 0.5;

/// Modelo sintético anunciado em `/v1/models`.
pub const MODEL_ID: &str = "aether-phase-160";

/// Versão do crate (anunciada em `/health` e `/props`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Configuração da borda.
#[derive(Clone, Debug)]
pub struct ApiConfig {
    /// Endereço de escuta.
    pub addr: SocketAddr,
    /// Arquivo JSONL do ledger (append-only). `None` = só memória.
    pub ledger_path: Option<PathBuf>,
    /// Token exigido (`Authorization: Bearer …` ou `X-Aether-Token`). `None` = aberto.
    pub token: Option<String>,
    /// Organismo permitido para o token. Requisições com outro escopo recebem 403.
    pub organism: Option<u64>,
    /// Teto do arquivo JSONL em bytes (`0` = sem compactação).
    pub max_ledger_bytes: u64,
    /// Workers de conexão (mínimo 1).
    pub threads: usize,
    /// Especificação do `neural-sgdb` (`mock`, `http://…/mcp`, `stdio:…`).
    ///
    /// `None` = a borda usa só o ledger local. Com SGDB, cada `remember` é
    /// replicado (best-effort) e o `cue` recupera de lá quando o cache local
    /// não tem o padrão.
    pub sgdb: Option<String>,
    /// Diário local de replay do SGDB.
    pub sgdb_outbox: Option<PathBuf>,
    /// Token `Bearer` do SGDB (ou `AETHER_SGDB_TOKEN`).
    pub sgdb_token: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8787".parse().expect("addr padrão"),
            ledger_path: None,
            token: None,
            organism: None,
            max_ledger_bytes: 0,
            threads: 4,
            sgdb: None,
            sgdb_outbox: None,
            sgdb_token: None,
        }
    }
}

/// Política de token + escopo.
#[derive(Clone, Debug)]
struct AuthPolicy {
    token: String,
    organism: Option<u64>,
}

/// Ledger em disco: uma linha JSON por operação, append-only.
///
/// `op=remember` grava o fato; `op=forget` grava o **túmulo**. Ao carregar, os
/// túmulos são aplicados em ordem, então um fato esquecido não ressuscita com
/// o reinício. Com `max_bytes > 0`, o arquivo é **compactado** (só vivos)
/// quando passa do teto.
#[derive(Clone, Debug)]
pub struct LedgerStore {
    path: PathBuf,
    max_bytes: u64,
    size: u64,
}

impl LedgerStore {
    /// Abre (ou cria) o arquivo do ledger. `max_bytes = 0` desliga a compactação.
    pub fn new(path: PathBuf, max_bytes: u64) -> std::io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            max_bytes,
            size,
        })
    }

    /// Caminho do arquivo.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Tamanho corrente (bytes).
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Teto configurado (`0` = sem teto).
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// `true` se o arquivo passou do teto e precisa de compactação.
    pub fn should_compact(&self) -> bool {
        self.max_bytes > 0 && self.size >= self.max_bytes
    }

    /// Grava um fato (`op=remember`).
    pub fn append(&mut self, record: &DraftRecord) -> std::io::Result<()> {
        self.append_line(&record_to_json(record))
    }

    /// Grava o túmulo de `key` no escopo (o fato deixa de ser citável no boot).
    pub fn tombstone(
        &mut self,
        scope: ScopeId,
        key: u128,
        reason: &str,
        wall_ms: u64,
    ) -> std::io::Result<()> {
        let line = Json::obj([
            ("op", Json::str("forget")),
            ("key", Json::str(key.to_string())),
            ("scope", scope_json(scope)),
            ("reason", Json::str(reason)),
            ("wall_ms", Json::num(wall_ms as f64)),
        ]);
        self.append_line(&line)
    }

    fn append_line(&mut self, line: &Json) -> std::io::Result<()> {
        let text = line.to_text();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(text.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()?;
        self.size += text.len() as u64 + 1;
        Ok(())
    }

    /// Lê o arquivo inteiro, aplicando túmulos em ordem.
    pub fn load(&self) -> std::io::Result<(Vec<DraftRecord>, usize)> {
        let f = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e),
        };
        let mut alive: Vec<DraftRecord> = Vec::new();
        let mut bad = 0usize;
        for line in BufReader::new(f).lines() {
            let line = line?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let Some(doc) = Json::parse(t).ok() else {
                bad += 1;
                continue;
            };
            match doc.get("op").and_then(Json::as_str) {
                Some("forget") => {
                    let scope = scope_from(doc.get("scope"));
                    let key = key_from(doc.get("key"));
                    match (scope, key) {
                        (Some(s), Some(k)) => alive.retain(|r| !(r.scope == s && r.key == k)),
                        _ => bad += 1,
                    }
                }
                Some("remember") | None => match record_from_json(&doc) {
                    Some(r) => alive.push(r),
                    None => bad += 1,
                },
                Some(_) => bad += 1,
            }
        }
        Ok((alive, bad))
    }

    /// Reescreve o arquivo só com os vivos (atômico: tmp + rename).
    pub fn compact(&mut self, records: &[DraftRecord]) -> std::io::Result<()> {
        let tmp = self.path.with_extension("jsonl.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            for r in records {
                f.write_all(record_to_json(r).to_text().as_bytes())?;
                f.write_all(b"\n")?;
            }
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        self.size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        Ok(())
    }
}

/// Estado da borda: ledger, política de auth, store e contadores.
#[derive(Debug, Default)]
pub struct ApiState {
    ledger: ScopeLedger,
    store: Option<LedgerStore>,
    auth: Option<AuthPolicy>,
    sgdb: Option<SgdbClient<Box<dyn Transport>>>,
    sgdb_error: Option<String>,
    agent: Option<AetherAgent>,
    requests: u64,
    bad_lines: usize,
}

impl ApiState {
    /// Estado vazio, sem auth nem persistência.
    pub fn new() -> Self {
        Self::default()
    }

    /// Estado a partir da configuração (carrega o ledger, se houver).
    pub fn load(cfg: &ApiConfig) -> std::io::Result<Self> {
        let mut state = Self::default();
        if let Some(path) = &cfg.ledger_path {
            let mut store = LedgerStore::new(path.clone(), cfg.max_ledger_bytes)?;
            let (records, bad) = store.load()?;
            for r in records {
                state.ledger.append(r);
            }
            state.bad_lines = bad;
            if store.should_compact() {
                let all: Vec<DraftRecord> = state.ledger.records().cloned().collect();
                store.compact(&all)?;
            }
            state.store = Some(store);
        }
        state.auth = cfg.token.clone().map(|token| AuthPolicy {
            token,
            organism: cfg.organism,
        });
        if let Some(spec) = &cfg.sgdb {
            let mut sgdb_cfg = SgdbConfig::from_env();
            if let Some(path) = &cfg.sgdb_outbox {
                sgdb_cfg.outbox = Some(path.clone());
            }
            if let Some(token) = &cfg.sgdb_token {
                sgdb_cfg.token = Some(token.clone());
            }
            sgdb_cfg.recall_k = sgdb_cfg.recall_k.max(32);
            match open(spec, sgdb_cfg) {
                Ok(client) => state.sgdb = Some(client),
                Err(e) => {
                    eprintln!(
                        "aviso: neural-sgdb não abriu ({e}) — a borda segue só com o ledger local"
                    );
                    state.sgdb_error = Some(e.to_string());
                }
            }
        }
        // Agente: especialistas locais (transplante/SAT/QUBO). Não usa o SGDB
        // diretamente para não abrir um segundo cliente; a rota `agent` é
        // inferência + coerência, e a memória durável tem rota própria.
        state.agent = Some(AetherAgent::new(AgentConfig::default()));
        Ok(state)
    }

    /// Estado do SGDB para `/health` (`off`/`on`/`error`).
    pub fn sgdb_status(&self) -> &'static str {
        if self.sgdb_error.is_some() {
            return "error";
        }
        match &self.sgdb {
            Some(_) => "on",
            None => "off",
        }
    }

    /// Registros no ledger (todos os escopos).
    pub fn ledger_len(&self) -> usize {
        self.ledger.len()
    }

    /// Requisições atendidas.
    pub fn requests(&self) -> u64 {
        self.requests
    }

    /// Linhas inválidas encontradas ao carregar o arquivo.
    pub fn bad_lines(&self) -> usize {
        self.bad_lines
    }
}

/// Liga o servidor em `addr` (porta `0` = efêmera).
pub fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr)
}

/// Atende conexões em série (compatibilidade e testes simples).
pub fn serve(listener: TcpListener) -> std::io::Result<()> {
    serve_listener(listener, ApiConfig::default())
}

/// Atende conexões com `cfg.threads` workers e estado compartilhado.
pub fn serve_config(cfg: ApiConfig) -> std::io::Result<()> {
    let listener = bind(cfg.addr)?;
    serve_listener(listener, cfg)
}

/// Atende um `TcpListener` já ligado.
pub fn serve_listener(listener: TcpListener, cfg: ApiConfig) -> std::io::Result<()> {
    let state = Arc::new(Mutex::new(ApiState::load(&cfg)?));
    let (tx, rx) = mpsc::channel::<TcpStream>();
    let rx = Arc::new(Mutex::new(rx));
    let workers = cfg.threads.max(1);

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let rx = Arc::clone(&rx);
        let state = Arc::clone(&state);
        handles.push(std::thread::spawn(move || loop {
            let next = {
                let guard = match rx.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                guard.recv()
            };
            match next {
                Ok(mut stream) => {
                    if let Ok(mut st) = state.lock() {
                        let _ = handle(&mut st, &mut stream);
                    }
                }
                Err(_) => break,
            }
        }));
    }

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if tx.send(s).is_err() {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    drop(tx);
    for h in handles {
        let _ = h.join();
    }
    Ok(())
}

/// Atende **uma** conexão (útil em teste e em servidor embutido).
pub fn handle(state: &mut ApiState, stream: &mut TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(stream)? else {
        return Ok(());
    };
    state.requests += 1;
    let reply = route(state, &req);
    write_reply(stream, reply)
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    token: Option<String>,
    too_large: bool,
}

enum Reply {
    Json(u16, Json),
    Sse(Vec<String>),
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let mut token: Option<String> = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("authorization") {
                let low = v.to_ascii_lowercase();
                token = Some(if let Some(rest) = low.strip_prefix("bearer ") {
                    v[v.len() - rest.len()..].to_string()
                } else {
                    v.to_string()
                });
            } else if k.eq_ignore_ascii_case("x-aether-token") {
                token = Some(v.to_string());
            }
        }
    }
    let too_large = content_length > MAX_BODY;
    let mut body = Vec::new();
    if too_large {
        // Drena até 4× o limite para o cliente conseguir ler o 413 em vez de
        // receber reset no meio do `write_all`. Acima disso, fecha.
        let mut scratch = [0u8; 8192];
        let mut left = content_length.min(MAX_BODY * 4);
        while left > 0 {
            let want = left.min(scratch.len());
            match reader.read(&mut scratch[..want]) {
                Ok(0) => break,
                Ok(n) => left -= n,
                Err(_) => break,
            }
        }
    } else if content_length > 0 {
        body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request {
        method,
        path,
        body,
        token,
        too_large,
    }))
}

fn write_reply(stream: &mut TcpStream, reply: Reply) -> std::io::Result<()> {
    match reply {
        Reply::Json(status, body) => write_response(stream, status, &body),
        Reply::Sse(chunks) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\n\
                 content-type: text/event-stream; charset=utf-8\r\n\
                 cache-control: no-cache\r\n\
                 connection: close\r\n\
                 access-control-allow-origin: *\r\n\
                 x-aether-version: {VERSION}\r\n\
                 x-aether-edge: text-phase-hash-lossy\r\n\
                 \r\n"
            )?;
            for c in chunks {
                stream.write_all(c.as_bytes())?;
                stream.flush()?;
            }
            Ok(())
        }
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &Json) -> std::io::Result<()> {
    let text = body.to_text();
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         content-type: application/json; charset=utf-8\r\n\
         content-length: {}\r\n\
         connection: close\r\n\
         access-control-allow-origin: *\r\n\
         x-aether-version: {VERSION}\r\n\
         x-aether-edge: text-phase-hash-lossy\r\n\
         \r\n",
        text.len()
    )?;
    stream.write_all(text.as_bytes())?;
    stream.flush()
}

fn route(state: &mut ApiState, req: &Request) -> Reply {
    if req.too_large {
        return Reply::Json(413, error_json("payload_too_large", "corpo acima de 1 MiB"));
    }

    // Saúde e catálogo são públicos (probe); o resto exige token quando há auth.
    let needs_auth = matches!(
        (req.method.as_str(), req.path.as_str()),
        (
            "POST",
            "/v1/aether/remember"
                | "/v1/aether/cue"
                | "/v1/aether/solve"
                | "/v1/aether/forget"
                | "/v1/chat/completions"
                | "/completion"
        )
    );
    let bound = if needs_auth {
        match check_auth(state, req) {
            Ok(b) => b,
            Err(r) => return r,
        }
    } else {
        None
    };

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/health") => Reply::Json(200, health(state)),
        ("GET", "/props") => Reply::Json(200, props(state)),
        ("GET", "/v1/models") => Reply::Json(200, models()),
        ("POST", "/v1/aether/remember") => remember(state, &req.body, bound),
        ("POST", "/v1/aether/cue") => cue(state, &req.body, bound),
        ("POST", "/v1/aether/solve") => solve(&req.body),
        ("POST", "/v1/aether/agent") => agent_route(state, &req.body),
        ("POST", "/v1/aether/forget") => forget(state, &req.body, bound),
        ("POST", "/v1/chat/completions") => chat(state, &req.body, bound),
        ("POST", "/completion") => completion(state, &req.body, bound),
        ("OPTIONS", _) => Reply::Json(204, Json::Null),
        _ => Reply::Json(
            404,
            error_json("not_found", "rota inexistente (ver docs/EXTERNAL_API.md)"),
        ),
    }
}

fn check_auth(state: &ApiState, req: &Request) -> Result<Option<u64>, Reply> {
    let Some(policy) = &state.auth else {
        return Ok(None);
    };
    match &req.token {
        Some(t) if token_eq(t, &policy.token) => Ok(policy.organism),
        _ => Err(Reply::Json(
            401,
            error_json("unauthorized", "token ausente ou inválido"),
        )),
    }
}

fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn scope_or_bound(value: Option<&Json>, bound: Option<u64>) -> Option<ScopeId> {
    match value {
        Some(v) => scope_from(Some(v)),
        None => bound.map(|organism| ScopeId::new(organism, 0, 0)),
    }
}

fn scope_allowed(scope: ScopeId, bound: Option<u64>) -> bool {
    match bound {
        Some(org) => scope.organism_id == org,
        None => true,
    }
}

fn health(state: &ApiState) -> Json {
    Json::obj([
        ("status", Json::str("ok")),
        ("engine", Json::str("aether")),
        ("version", Json::str(VERSION)),
        (
            "invariants",
            Json::obj([
                ("tokens", Json::num(0.0)),
                ("backprop", Json::num(0.0)),
                ("t0_ledger_calls", Json::num(0.0)),
            ]),
        ),
        ("requests", Json::num(state.requests as f64)),
        ("ledger_entries", Json::num(state.ledger_len() as f64)),
        ("persisted", Json::Bool(state.store.is_some())),
        (
            "ledger_bytes",
            Json::num(state.store.as_ref().map_or(0, LedgerStore::size) as f64),
        ),
        (
            "ledger_max_bytes",
            Json::num(state.store.as_ref().map_or(0, LedgerStore::max_bytes) as f64),
        ),
        ("auth", Json::Bool(state.auth.is_some())),
        ("bad_lines", Json::num(state.bad_lines as f64)),
        ("sgdb", Json::str(state.sgdb_status())),
        ("agent", Json::Bool(state.agent.is_some())),
    ])
}

fn props(state: &ApiState) -> Json {
    Json::obj([
        ("model_path", Json::str(MODEL_ID)),
        ("n_ctx", Json::num(PHASE_DIMS as f64)),
        ("total_slots", Json::num(1.0)),
        (
            "aether",
            Json::obj([
                ("version", Json::str(VERSION)),
                ("phase_dims", Json::num(PHASE_DIMS as f64)),
                ("ledger_entries", Json::num(state.ledger_len() as f64)),
                ("persisted", Json::Bool(state.store.is_some())),
                ("sgdb", Json::str(state.sgdb_status())),
                ("edge", Json::str("text-phase-hash-lossy")),
            ]),
        ),
    ])
}

fn models() -> Json {
    Json::obj([
        ("object", Json::str("list")),
        (
            "data",
            Json::Arr(vec![Json::obj([
                ("id", Json::str(MODEL_ID)),
                ("object", Json::str("model")),
                ("owned_by", Json::str("aether")),
                ("created", Json::num(0.0)),
            ])]),
        ),
    ])
}

fn remember(state: &mut ApiState, body: &[u8], bound: Option<u64>) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let Some(scope) = scope_or_bound(doc.get("scope"), bound) else {
        return bad("scope ausente: use {organism,run,app}");
    };
    if !scope_allowed(scope, bound) {
        return forbidden(scope, bound);
    }
    let Some(phases) = phases_from(doc.get("phases"), PHASE_FLOATS) else {
        return bad("phases ausente, ímpar ou com mais de 320 floats");
    };
    let key = match key_from(doc.get("key")) {
        Some(k) => k,
        None => return bad("key ausente (número ou string decimal)"),
    };
    let certificate = cert_from(doc.get("certificate"));
    let authority = doc
        .get("authority")
        .and_then(Json::as_u64)
        .unwrap_or(0)
        .min(255) as u8;
    let layer = match doc.get("layer").and_then(Json::as_u64).unwrap_or(4) {
        2 => Layer::L2Episodic,
        6 => Layer::L6Graph,
        _ => Layer::L4Semantic,
    };
    let tick = doc.get("tick").and_then(Json::as_u64).unwrap_or(0);
    let wall_ms = doc
        .get("wall_ms")
        .and_then(Json::as_u64)
        .unwrap_or_else(now_ms);

    let record = DraftRecord {
        key,
        layer,
        certificate,
        authority,
        scope,
        tick,
        wall_ms,
        payload: phases_to_payload(&phases),
    };
    let scope_key = record.scope_key();
    let mut persisted = false;
    if let Some(store) = state.store.as_mut() {
        match store.append(&record) {
            Ok(()) => persisted = true,
            Err(e) => {
                return Reply::Json(
                    500,
                    error_json("ledger_io", &format!("falha ao gravar: {e}")),
                )
            }
        }
    }
    let result = state.sgdb.as_mut().map(|client| {
        // O contrato do SGDB é fase complexa (`re, im`); a borda HTTP fala
        // fases puras. Converte na fronteira para um único formato durável.
        let durable = DraftRecord {
            payload: phases_to_complex(&record.payload),
            ..record.clone()
        };
        client.remember_records(std::slice::from_ref(&durable))
    });
    let mut sgdb_state = "off";
    match result {
        Some(Ok(1)) => {
            sgdb_state = "stored";
            state.sgdb_error = None;
        }
        Some(Ok(_)) => {
            sgdb_state = "empty";
            state.sgdb_error = None;
        }
        Some(Err(e)) => {
            sgdb_state = "offline";
            state.sgdb_error = Some(e.to_string());
        }
        None => {}
    }
    state.ledger.append(record);

    // Compactação: reescreve só os vivos quando o arquivo passa do teto.
    let mut compacted = false;
    let mut compact_error: Option<String> = None;
    if state.store.as_ref().map(LedgerStore::should_compact) == Some(true) {
        let all: Vec<DraftRecord> = state.ledger.records().cloned().collect();
        if let Some(store) = state.store.as_mut() {
            match store.compact(&all) {
                Ok(()) => compacted = true,
                Err(e) => compact_error = Some(e.to_string()),
            }
        }
    }

    let mut fields: Vec<(String, Json)> = vec![
        ("stored".into(), Json::Bool(true)),
        ("persisted".into(), Json::Bool(persisted)),
        ("compacted".into(), Json::Bool(compacted)),
        ("scope".into(), scope_json(scope)),
        ("scope_key".into(), Json::str(scope_key)),
        ("total".into(), Json::num(state.ledger.len() as f64)),
        ("sgdb".into(), Json::str(sgdb_state)),
    ];
    if let Some(e) = compact_error {
        fields.push(("compact_error".into(), Json::str(e)));
    }
    Reply::Json(200, Json::Obj(fields))
}

/// Remove um fato do escopo e grava o túmulo (para não ressuscitar no boot).
///
/// Exige `authority >= 128` (HITL): apagar memória é ato com mandato, não
/// efeito colateral de uma requisição qualquer.
fn forget(state: &mut ApiState, body: &[u8], bound: Option<u64>) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let Some(scope) = scope_or_bound(doc.get("scope"), bound) else {
        return bad("scope ausente: use {organism,run,app}");
    };
    if !scope_allowed(scope, bound) {
        return forbidden(scope, bound);
    }
    let Some(key) = key_from(doc.get("key")) else {
        return bad("key ausente (número ou string decimal)");
    };
    let authority = doc
        .get("authority")
        .and_then(Json::as_u64)
        .unwrap_or(0)
        .min(255) as u8;
    if authority < 128 {
        return Reply::Json(
            403,
            error_json(
                "authority_required",
                "forget exige authority >= 128 (mandato explícito)",
            ),
        );
    }
    let reason = doc.get("reason").and_then(Json::as_str).unwrap_or("manual");

    let removed = state.ledger.forget(scope, key);
    let mut tombstoned = false;
    if removed {
        if let Some(store) = state.store.as_mut() {
            if let Err(e) = store.tombstone(scope, key, reason, now_ms()) {
                return Reply::Json(
                    500,
                    error_json("ledger_io", &format!("falha ao gravar túmulo: {e}")),
                );
            }
            tombstoned = true;
        }
    }
    Reply::Json(
        200,
        Json::obj([
            ("forgot", Json::Bool(removed)),
            ("tombstoned", Json::Bool(tombstoned)),
            ("key", Json::str(key.to_string())),
            ("scope", scope_json(scope)),
            ("remaining", Json::num(state.ledger.len() as f64)),
        ]),
    )
}

fn cue(state: &mut ApiState, body: &[u8], bound: Option<u64>) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let Some(scope) = scope_or_bound(doc.get("scope"), bound) else {
        return bad("scope ausente: use {organism,run,app}");
    };
    if !scope_allowed(scope, bound) {
        return forbidden(scope, bound);
    }
    let Some(query) = phases_from(doc.get("query"), PHASE_FLOATS) else {
        return bad("query ausente, ímpar ou com mais de 320 floats");
    };
    let mut out = vec![0.0f32; query.len()];
    let mut result = state.ledger.cue_scored(scope, &query, &mut out);
    let mut source = "local";
    let needs_remote = !matches!(&result, Ok((_, score)) if *score >= MIN_SCORE);
    if needs_remote {
        // Sem hit local (ex.: processo reiniciou): reacende do SGDB e tenta
        // de novo. O ledger textual não substitui a fase — ele a reacende.
        let hydrated = hydrate_from_sgdb(state, scope);
        if hydrated > 0 {
            result = state.ledger.cue_scored(scope, &query, &mut out);
            source = "sgdb";
        }
    }
    match result {
        Ok((hit, score)) if score >= MIN_SCORE => Reply::Json(
            200,
            Json::obj([
                ("abstain", Json::Bool(false)),
                ("hits", Json::num(hit.hits as f64)),
                ("key", Json::str(hit.key.to_string())),
                ("authority", Json::num(f64::from(hit.authority))),
                ("score", Json::num(f64::from(score))),
                ("source", Json::str(source)),
                (
                    "phases",
                    Json::Arr(out.iter().map(|p| Json::num(f64::from(*p))).collect()),
                ),
                (
                    "isolation",
                    Json::obj([
                        (
                            "cross_scope_hits",
                            Json::num(state.ledger.cross_scope_hits() as f64),
                        ),
                        (
                            "cross_scope_skipped",
                            Json::num(state.ledger.cross_scope_skipped() as f64),
                        ),
                    ]),
                ),
            ]),
        ),
        Ok((_, score)) => Reply::Json(200, abstain_scored("low_similarity", score)),
        Err(BridgeError::EmptyScope) => Reply::Json(200, abstain("empty_scope")),
        Err(e) => Reply::Json(400, error_json("cue_failed", &e.to_string())),
    }
}

/// Reacende o cache local com os documentos de fase do escopo no SGDB.
///
/// Devolve quantos registros entraram. Falha de rede não derruba o `cue`:
/// apenas registra o erro no estado (`/health` mostra `sgdb: error`).
fn hydrate_from_sgdb(state: &mut ApiState, scope: ScopeId) -> usize {
    let Some(client) = state.sgdb.as_mut() else {
        return 0;
    };
    match client.recall_scope(scope, 32) {
        Ok(hits) => {
            let count = hits.len();
            for hit in hits {
                state.ledger.append(DraftRecord {
                    key: hit.doc.key,
                    layer: hit.doc.layer,
                    certificate: hit.doc.certificate,
                    authority: hit.authority,
                    scope: hit.doc.scope,
                    tick: hit.doc.tick,
                    wall_ms: hit.doc.wall_ms,
                    payload: complex_to_phases(&hit.doc.payload),
                });
            }
            count
        }
        Err(e) => {
            state.sgdb_error = Some(e.to_string());
            0
        }
    }
}

/// Rota do agente: fato, SAT, QUBO, memória **ou** pipeline `steps[]`.
fn agent_route(state: &mut ApiState, body: &[u8]) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    // Workflow multi-passo (MSG 0287/0319): não toca `/v1/chat/completions`.
    if doc.get("pipeline").is_some() || doc.get("steps").and_then(|v| v.as_arr()).is_some() {
        return agent_pipeline_route(&doc);
    }
    let task = if let (Some(subject), Some(relation)) = (
        doc.get("subject").and_then(|v| v.as_str()),
        doc.get("relation").and_then(|v| v.as_str()),
    ) {
        Task::Fact {
            subject: subject.to_string(),
            relation: relation.to_string(),
        }
    } else if let Some(clauses) = clauses_from_json(doc.get("clauses")) {
        let n = doc
            .get("n")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or_else(|| {
                clauses
                    .iter()
                    .flat_map(|c| [c.lit(0).0, c.lit(1).0, c.lit(2).0])
                    .max()
                    .unwrap_or(0)
                    + 1
            });
        Task::Satisfy { n, clauses }
    } else if let Some(n) = doc.get("n").and_then(|v| v.as_u64()).map(|v| v as usize) {
        let Some(j) = flat_matrix(doc.get("j"), n, n) else {
            return bad("j deve ser n×n");
        };
        let h = flat_matrix(doc.get("h"), n, 1).unwrap_or_else(|| vec![0.0; n]);
        Task::Optimize { n, j, h }
    } else if let Some(query) = doc.get("query").and_then(|v| v.as_str()) {
        Task::Memory {
            query: query.to_string(),
        }
    } else {
        return bad(
            "use {subject,relation}, {clauses,n}, {n,j,h}, {query} ou {steps|pipeline}",
        );
    };
    let Some(agent) = state.agent.as_mut() else {
        return Reply::Json(503, error_json("agent_off", "agente indisponível"));
    };
    let outcome = agent.run(&task);
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
    let mut fields: Vec<(String, Json)> = vec![
        ("action".into(), Json::str(outcome.action.label())),
        (
            "answer".into(),
            outcome
                .answer
                .as_ref()
                .map(|claim| Json::str(claim.value.clone()))
                .unwrap_or(Json::Null),
        ),
        (
            "confidence".into(),
            Json::num(f64::from(
                outcome.answer.as_ref().map_or(0.0, |c| c.confidence),
            )),
        ),
        (
            "support".into(),
            Json::num(f64::from(outcome.coherence.support)),
        ),
        (
            "consensus".into(),
            Json::num(f64::from(outcome.coherence.consensus)),
        ),
        (
            "authority".into(),
            Json::num(f64::from(outcome.coherence.authority)),
        ),
        (
            "contradictions".into(),
            Json::num(outcome.coherence.contradictions as f64),
        ),
        ("steps".into(), Json::num(outcome.steps as f64)),
        (
            "experts".into(),
            Json::Arr(
                outcome
                    .experts_used
                    .iter()
                    .map(|e| Json::str(e.label()))
                    .collect(),
            ),
        ),
        (
            "rationale".into(),
            Json::str(outcome.coherence.rationale.clone()),
        ),
    ];
    if let Some(cert) = certificate {
        fields.push(("certificate".into(), cert));
    }
    Reply::Json(200, Json::Obj(fields))
}

/// Pipeline HTTP: espelha o schema da CLI (`steps`/`pipeline` + budget/risk).
fn agent_pipeline_route(doc: &Json) -> Reply {
    let workflow = match pipeline_goal_from_json(doc) {
        Ok(goal) => goal,
        Err(e) => return bad(&e),
    };
    let mut agent = ToolAgent::new(&AgentConfig::default());
    let outcome = agent.solve(&workflow);
    let answer = outcome
        .outcome
        .as_ref()
        .and_then(|o| o.first_claim())
        .map(|c| Json::str(c.value.clone()))
        .unwrap_or(Json::Null);
    Reply::Json(
        200,
        Json::obj([
            ("pipeline", Json::Bool(true)),
            ("success", Json::Bool(outcome.success)),
            ("action", Json::str(outcome.action.label())),
            ("rationale", Json::str(outcome.rationale.clone())),
            ("answer", answer),
            ("step_count", Json::num(outcome.steps.len() as f64)),
            ("trace", outcome.trace.to_json()),
        ]),
    )
}

fn pipeline_goal_from_json(doc: &Json) -> Result<Goal, String> {
    let steps = doc
        .get("pipeline")
        .and_then(|v| v.as_arr())
        .or_else(|| doc.get("steps").and_then(|v| v.as_arr()))
        .ok_or_else(|| "pipeline exige `steps` ou `pipeline` array".to_string())?;
    let mut goals = Vec::new();
    for step in steps {
        goals.push(pipeline_step_from_json(step)?);
    }
    if goals.is_empty() {
        return Err("pipeline sem steps".into());
    }
    let mut workflow = Goal::pipeline(goals);
    if let Some(budget) = doc.get("budget") {
        workflow.budget = pipeline_parse_budget(budget)?;
    }
    if let Some(risk) = doc.get("risk_ceiling").and_then(|v| v.as_str()) {
        workflow.risk_ceiling = pipeline_parse_risk(risk)?;
    }
    Ok(workflow)
}

fn pipeline_step_from_json(json: &Json) -> Result<Goal, String> {
    let kind = json
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or("step sem kind")?;
    let mut goal = match kind {
        "fact" | "answer" => {
            let subject = json
                .get("subject")
                .and_then(|v| v.as_str())
                .ok_or("fact sem subject")?;
            let relation = json
                .get("relation")
                .and_then(|v| v.as_str())
                .ok_or("fact sem relation")?;
            let mut goal = Goal::fact(subject, relation);
            if let Some(value) = json.get("value").and_then(|v| v.as_str()) {
                goal.success = SuccessCriteria::Claim {
                    subject: subject.to_string(),
                    relation: relation.to_string(),
                    value: Some(value.to_string()),
                };
            }
            goal
        }
        "sat" => {
            let n = json.get("n").and_then(|v| v.as_u64()).ok_or("sat sem n")? as usize;
            let clauses = json
                .get("clauses")
                .and_then(pipeline_clauses)
                .ok_or("sat sem clauses válidas")?;
            Goal::satisfy(n, clauses)
        }
        "qubo" | "optimize" => {
            let n = json.get("n").and_then(|v| v.as_u64()).ok_or("qubo sem n")? as usize;
            let j = json
                .get("j")
                .and_then(|v| v.as_arr())
                .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
                .ok_or("qubo sem j")?;
            let h = json
                .get("h")
                .and_then(|v| v.as_arr())
                .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
                .unwrap_or_else(|| vec![0.0; n]);
            Goal::optimize(n, j, h)
        }
        "memory" | "retrieve" => {
            let query = json
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("memory sem query")?;
            Goal::memory(query)
        }
        other => return Err(format!("kind desconhecido: {other}")),
    };
    if let Some(budget) = json.get("budget") {
        goal.budget = pipeline_parse_budget(budget)?;
    }
    if let Some(risk) = json.get("risk_ceiling").and_then(|v| v.as_str()) {
        goal.risk_ceiling = pipeline_parse_risk(risk)?;
    }
    Ok(goal)
}

fn pipeline_parse_budget(json: &Json) -> Result<Budget, String> {
    Ok(Budget {
        max_steps: json.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(4) as usize,
        max_ms: json.get("max_ms").and_then(|v| v.as_u64()).unwrap_or(2_000),
        max_tokens: json.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    })
}

fn pipeline_parse_risk(raw: &str) -> Result<Risk, String> {
    match raw {
        "low" => Ok(Risk::Low),
        "medium" => Ok(Risk::Medium),
        "high" => Ok(Risk::High),
        other => Err(format!("risco inválido: {other}")),
    }
}

fn pipeline_clauses(value: &Json) -> Option<Vec<Clause3>> {
    let arr = value.as_arr()?;
    let mut out = Vec::with_capacity(arr.len());
    for clause in arr {
        let ints: Vec<i64> = clause
            .as_arr()?
            .iter()
            .filter_map(|v| v.as_i64())
            .filter(|v| *v != 0)
            .collect();
        if ints.is_empty() || ints.len() > 3 {
            return None;
        }
        let mut lits: Vec<(usize, i8)> = ints
            .iter()
            .map(|v| {
                (
                    (v.unsigned_abs() as usize) - 1,
                    if *v > 0 { 1 } else { -1 },
                )
            })
            .collect();
        let pad = *lits.last()?;
        while lits.len() < 3 {
            lits.push(pad);
        }
        out.push(Clause3::new3(
            lits[0].0, lits[0].1, lits[1].0, lits[1].1, lits[2].0, lits[2].1,
        ));
    }
    Some(out)
}

fn clauses_from_json(value: Option<&Json>) -> Option<Vec<crate::core::logic_pipeline::Clause3>> {
    use crate::core::logic_pipeline::Clause3;
    let arr = value?.as_arr()?;
    let mut out = Vec::with_capacity(arr.len());
    for clause in arr {
        let ints: Vec<i64> = clause
            .as_arr()?
            .iter()
            .filter_map(|v| v.as_i64())
            .filter(|v| *v != 0)
            .collect();
        if ints.is_empty() || ints.len() > 3 {
            return None;
        }
        let mut lits: Vec<(usize, i8)> = ints
            .iter()
            .map(|v| ((v.unsigned_abs() as usize) - 1, if *v > 0 { 1 } else { -1 }))
            .collect();
        let pad = *lits.last()?;
        while lits.len() < 3 {
            lits.push(pad);
        }
        out.push(Clause3::new3(
            lits[0].0, lits[0].1, lits[1].0, lits[1].1, lits[2].0, lits[2].1,
        ));
    }
    Some(out)
}

fn solve(body: &[u8]) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let Some(n) = doc.get("n").and_then(Json::as_u64).map(|v| v as usize) else {
        return bad("n ausente");
    };
    if n == 0 || n > 256 {
        return bad("n fora de 1..=256");
    }
    let Some(j) = flat_matrix(doc.get("j"), n, n) else {
        return bad("j deve ser n×n (array de arrays ou plano)");
    };
    let Some(h) = flat_matrix(doc.get("h"), 1, n) else {
        return bad("h deve ter n elementos");
    };
    let steps = doc
        .get("steps")
        .and_then(Json::as_u64)
        .unwrap_or(80)
        .clamp(1, 4096) as usize;
    let restarts = doc
        .get("restarts")
        .and_then(Json::as_u64)
        .unwrap_or(4)
        .clamp(1, 32) as usize;
    let seed = doc.get("seed").and_then(Json::as_u64).unwrap_or(0xAE71);
    let authority = doc
        .get("authority")
        .and_then(Json::as_u64)
        .unwrap_or(0)
        .min(255) as u8;

    let pre = roof_dual_qumo(n, &j, &h);

    let j3 = match Array3::from_shape_vec((1, n, n), j.clone()) {
        Ok(a) => a,
        Err(e) => return Reply::Json(400, error_json("bad_shape", &e.to_string())),
    };
    let h2 = match Array2::from_shape_vec((1, n), h.clone()) {
        Ok(a) => a,
        Err(e) => return Reply::Json(400, error_json("bad_shape", &e.to_string())),
    };

    let mut solver = ThermodynamicIsingSolver::new(1, n, steps, 0.05);
    let mut best = f64::INFINITY;
    let mut best_s: Vec<i8> = vec![-1; n];
    let mut same = 0usize;
    for r in 0..restarts {
        solver.reset(seed.wrapping_add(r as u64));
        let out = match solver.relax(IsingLandscape::new(j3.view(), h2.view())) {
            Ok(o) => o,
            Err(e) => return Reply::Json(500, error_json("solve_failed", &e.to_string())),
        };
        let e = out.best_energy[0];
        if e < best - 1e-9 {
            best = e;
            best_s = out.states.row(0).to_vec();
            same = 1;
        } else if (e - best).abs() <= 1e-9 {
            same += 1;
        }
    }
    let energy = certificate::energy(&j, &h, n, &best_s);
    let gap = certificate::local_gap(&j, &h, n, &best_s);
    let consensus = same as f64 / restarts as f64;
    let cert = certificate::Certificate::new(energy, gap, 0, consensus, Provenance::Inferred);
    let decision = cert.decide(authority, 0.05);

    Reply::Json(
        200,
        Json::obj([
            ("energy", Json::num(energy)),
            ("gap", Json::num(gap)),
            ("violations", Json::num(0.0)),
            ("consensus", Json::num(consensus)),
            ("coherent", Json::Bool(cert.is_coherent())),
            ("decision", Json::str(decision_name(decision))),
            (
                "states",
                Json::Arr(best_s.iter().map(|s| Json::num(f64::from(*s))).collect()),
            ),
            ("provenance", Json::str("inferred")),
            (
                "persistency",
                Json::obj([
                    // O roof dual tem buffers fixos (ROOF_MAX_N): acima disso ele
                    // NÃO roda, e `fixed` reflete só a dominância. Sem este campo
                    // o cliente leria "0 fixas" como informação, quando é ausência.
                    ("applicable", Json::Bool(n <= ROOF_MAX_N)),
                    ("cap", Json::num(ROOF_MAX_N as f64)),
                    ("fixed", Json::num(pre.fixed_count() as f64)),
                    ("residual", Json::num(pre.residual.len() as f64)),
                    ("rounds", Json::num(pre.rounds as f64)),
                ]),
            ),
        ]),
    )
}

fn chat(state: &mut ApiState, body: &[u8], bound: Option<u64>) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let Some(text) = last_user_text(&doc) else {
        return bad("messages ausente ou sem mensagem de usuário");
    };
    let scope = match doc.get("aether").and_then(|a| a.get("scope")) {
        Some(s) => match scope_from(Some(s)) {
            Some(v) => v,
            None => return bad("aether.scope inválido"),
        },
        None => bound.map(|o| ScopeId::new(o, 0, 0)).unwrap_or_default(),
    };
    if !scope_allowed(scope, bound) {
        return forbidden(scope, bound);
    }
    let evidence = evidence_for(state, scope, &text);
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let stream = doc.get("stream").and_then(Json::as_bool).unwrap_or(false);
    let id = format!("chatcmpl-aether-{}", state.requests);
    let content = evidence.to_text();

    if stream {
        return Reply::Sse(chat_chunks(&id, &content, abstained));
    }

    Reply::Json(
        200,
        Json::obj([
            ("id", Json::str(id)),
            ("object", Json::str("chat.completion")),
            ("created", Json::num((now_ms() / 1000) as f64)),
            ("model", Json::str(MODEL_ID)),
            (
                "choices",
                Json::Arr(vec![Json::obj([
                    ("index", Json::num(0.0)),
                    (
                        "message",
                        Json::obj([
                            ("role", Json::str("assistant")),
                            ("content", Json::str(content)),
                        ]),
                    ),
                    (
                        "finish_reason",
                        Json::str(if abstained { "content_filter" } else { "stop" }),
                    ),
                ])]),
            ),
            (
                "usage",
                Json::obj([
                    ("prompt_tokens", Json::num(0.0)),
                    ("completion_tokens", Json::num(0.0)),
                    ("total_tokens", Json::num(0.0)),
                ]),
            ),
            ("x_aether", evidence),
        ]),
    )
}

fn chat_chunks(id: &str, content: &str, abstained: bool) -> Vec<String> {
    let base = |delta: Json, finish: Json| {
        format!(
            "data: {}\n\n",
            Json::obj([
                ("id", Json::str(id)),
                ("object", Json::str("chat.completion.chunk")),
                ("model", Json::str(MODEL_ID)),
                (
                    "choices",
                    Json::Arr(vec![Json::obj([
                        ("index", Json::num(0.0)),
                        ("delta", delta),
                        ("finish_reason", finish),
                    ])]),
                ),
            ])
            .to_text()
        )
    };
    let finish = if abstained { "content_filter" } else { "stop" };
    vec![
        base(Json::obj([("role", Json::str("assistant"))]), Json::Null),
        base(Json::obj([("content", Json::str(content))]), Json::Null),
        base(Json::Obj(Vec::new()), Json::str(finish)),
        "data: [DONE]\n\n".to_string(),
    ]
}

fn completion(state: &mut ApiState, body: &[u8], bound: Option<u64>) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    let text = doc
        .get("prompt")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let scope = match doc.get("aether").and_then(|a| a.get("scope")) {
        Some(s) => match scope_from(Some(s)) {
            Some(v) => v,
            None => return bad("aether.scope inválido"),
        },
        None => bound.map(|o| ScopeId::new(o, 0, 0)).unwrap_or_default(),
    };
    if !scope_allowed(scope, bound) {
        return forbidden(scope, bound);
    }
    let evidence = evidence_for(state, scope, &text);
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    Reply::Json(
        200,
        Json::obj([
            ("content", Json::str(evidence.to_text())),
            ("stop", Json::Bool(true)),
            ("tokens_predicted", Json::num(0.0)),
            (
                "stopping_word",
                Json::str(if abstained { "<abstain>" } else { "" }),
            ),
            ("x_aether", evidence),
        ]),
    )
}

/// Evidência certificada para um texto, no escopo dado (ou abstenção).
fn evidence_for(state: &mut ApiState, scope: ScopeId, text: &str) -> Json {
    let query = text_to_phases(text);
    let mut out = vec![0.0f32; query.len()];
    match state.ledger.cue_scored(scope, &query, &mut out) {
        Ok((hit, score)) if score >= MIN_SCORE => {
            let (energy, gap, violations) =
                record_cert(&state.ledger, scope, hit.key).unwrap_or((0.0, 0.0, 0));
            let cert =
                certificate::Certificate::new(energy, gap, violations, 1.0, Provenance::Inferred);
            let decision = cert.decide(hit.authority, 0.05);
            Json::obj([
                ("abstain", Json::Bool(false)),
                ("verdict", Json::str(decision_name(decision))),
                ("key", Json::str(hit.key.to_string())),
                ("authority", Json::num(f64::from(hit.authority))),
                ("scope", scope_json(scope)),
                (
                    "certificate",
                    Json::obj([
                        ("energy", Json::num(energy)),
                        ("gap", Json::num(gap)),
                        ("violations", Json::num(f64::from(violations))),
                    ]),
                ),
                (
                    "phases",
                    Json::Arr(out.iter().map(|p| Json::num(f64::from(*p))).collect()),
                ),
                ("hits", Json::num(hit.hits as f64)),
                ("score", Json::num(f64::from(score))),
                ("edge", Json::str("text-phase-hash-lossy")),
            ])
        }
        Ok((_, score)) => abstain_scored("low_similarity", score),
        Err(BridgeError::EmptyScope) => abstain("empty_scope"),
        Err(e) => abstain(&format!("cue_failed:{e}")),
    }
}

fn record_cert(ledger: &ScopeLedger, scope: ScopeId, key: u128) -> Option<(f64, f64, u32)> {
    ledger.scoped(scope).find(|r| r.key == key).map(|r| {
        (
            f64::from(r.certificate.energy),
            f64::from(r.certificate.gap),
            r.certificate.violations,
        )
    })
}

fn abstain(reason: &str) -> Json {
    Json::obj([
        ("abstain", Json::Bool(true)),
        ("reason", Json::str(reason)),
        ("edge", Json::str("text-phase-hash-lossy")),
    ])
}

fn abstain_scored(reason: &str, score: f32) -> Json {
    Json::obj([
        ("abstain", Json::Bool(true)),
        ("reason", Json::str(reason)),
        ("score", Json::num(f64::from(score))),
        ("min_score", Json::num(f64::from(MIN_SCORE))),
        ("edge", Json::str("text-phase-hash-lossy")),
    ])
}

fn decision_name(d: Decision) -> &'static str {
    match d {
        Decision::Act => "act",
        Decision::Cite => "cite",
        Decision::Propose => "propose",
        Decision::Abstain => "abstain",
    }
}

/// Texto → fasores: hash FNV-1a + xorshift **determinístico**.
///
/// Devolve `2 × PHASE_DIMS` floats — pares `(re, im)` de **fasores unitários**
/// (`cos φ, sin φ`), que é o que o F1 espera. A correlação complexa entre dois
/// conjuntos aleatórios é ~0; entre o mesmo texto, é 1.
///
/// Não é embedding: é a borda lossy documentada. Dois textos diferentes geram
/// fases diferentes; o mesmo texto gera sempre as mesmas fases.
pub fn text_to_phases(text: &str) -> Vec<f32> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut st = h | 1;
    let mut out = Vec::with_capacity(PHASE_FLOATS);
    for _ in 0..PHASE_DIMS {
        st ^= st << 13;
        st ^= st >> 7;
        st ^= st << 17;
        let phi = (st as f64 / u64::MAX as f64) as f32 * std::f32::consts::TAU;
        out.push(phi.cos());
        out.push(phi.sin());
    }
    out
}

fn phases_to_payload(phases: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(phases.len() * 4);
    for p in phases {
        out.extend_from_slice(&p.to_le_bytes());
    }
    out
}

/// Fases puras (`f32`) → fase complexa (`re, im`) para o SGDB.
fn phases_to_complex(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() * 2);
    for chunk in payload.chunks_exact(4) {
        let phase = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        out.extend_from_slice(&phase.cos().to_le_bytes());
        out.extend_from_slice(&phase.sin().to_le_bytes());
    }
    out
}

/// Fase complexa (`re, im`) → fases puras (`atan2`), inverso do anterior.
fn complex_to_phases(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() / 2);
    for chunk in payload.chunks_exact(8) {
        let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        out.extend_from_slice(&im.atan2(re).to_le_bytes());
    }
    out
}

fn payload_to_phases(payload: &[u8]) -> Vec<f32> {
    payload
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn record_to_json(r: &DraftRecord) -> Json {
    Json::obj([
        ("op", Json::str("remember")),
        ("key", Json::str(r.key.to_string())),
        ("layer", Json::num(f64::from(r.layer.code()))),
        (
            "certificate",
            Json::obj([
                ("energy", Json::num(f64::from(r.certificate.energy))),
                ("gap", Json::num(f64::from(r.certificate.gap))),
                ("violations", Json::num(f64::from(r.certificate.violations))),
            ]),
        ),
        ("authority", Json::num(f64::from(r.authority))),
        ("scope", scope_json(r.scope)),
        ("tick", Json::num(r.tick as f64)),
        ("wall_ms", Json::num(r.wall_ms as f64)),
        (
            "phases",
            Json::Arr(
                payload_to_phases(&r.payload)
                    .iter()
                    .map(|p| Json::num(f64::from(*p)))
                    .collect(),
            ),
        ),
    ])
}

fn record_from_json(v: &Json) -> Option<DraftRecord> {
    let key = key_from(v.get("key"))?;
    let scope = scope_from(v.get("scope"))?;
    let phases = phases_from(v.get("phases"), PHASE_FLOATS)?;
    let layer = match v.get("layer").and_then(Json::as_u64).unwrap_or(4) {
        2 => Layer::L2Episodic,
        6 => Layer::L6Graph,
        _ => Layer::L4Semantic,
    };
    Some(DraftRecord {
        key,
        layer,
        certificate: cert_from(v.get("certificate")),
        authority: v
            .get("authority")
            .and_then(Json::as_u64)
            .unwrap_or(0)
            .min(255) as u8,
        scope,
        tick: v.get("tick").and_then(Json::as_u64).unwrap_or(0),
        wall_ms: v.get("wall_ms").and_then(Json::as_u64).unwrap_or(0),
        payload: phases_to_payload(&phases),
    })
}

fn phases_from(value: Option<&Json>, max: usize) -> Option<Vec<f32>> {
    let arr = value?.as_arr()?;
    // Fasores vêm em pares (re, im): comprimento ímpar é erro de borda.
    if arr.is_empty() || arr.len() > max || arr.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let x = v.as_f64()?;
        if !x.is_finite() {
            return None;
        }
        out.push(x as f32);
    }
    Some(out)
}

fn flat_matrix(value: Option<&Json>, rows: usize, cols: usize) -> Option<Vec<f64>> {
    let v = value?;
    let check = |x: f64| -> Option<f64> {
        if x.is_finite() {
            Some(x)
        } else {
            None
        }
    };
    if let Some(outer) = v.as_arr() {
        if outer.len() == rows && outer.iter().all(|r| r.as_arr().is_some()) {
            let mut out = Vec::with_capacity(rows * cols);
            for row in outer {
                let r = row.as_arr()?;
                if r.len() != cols {
                    return None;
                }
                for x in r {
                    out.push(check(x.as_f64()?)?);
                }
            }
            return Some(out);
        }
        let mut out = Vec::with_capacity(rows * cols);
        for x in outer {
            out.push(check(x.as_f64()?)?);
        }
        if out.len() == rows * cols {
            return Some(out);
        }
    }
    None
}

fn scope_from(value: Option<&Json>) -> Option<ScopeId> {
    let v = value?;
    let organism = v.get("organism").and_then(Json::as_u64).unwrap_or(0);
    let run = v.get("run").and_then(Json::as_u64).unwrap_or(0);
    let app = v.get("app").and_then(Json::as_u64).unwrap_or(0) as u32;
    Some(ScopeId::new(organism, run, app))
}

fn scope_json(scope: ScopeId) -> Json {
    Json::obj([
        ("organism", Json::num(scope.organism_id as f64)),
        ("run", Json::num(scope.run_id as f64)),
        ("app", Json::num(f64::from(scope.app_tag))),
    ])
}

fn key_from(value: Option<&Json>) -> Option<u128> {
    let v = value?;
    match v {
        Json::Str(s) => s.parse().ok(),
        _ => v.as_f64().map(|n| n as u128),
    }
}

fn cert_from(value: Option<&Json>) -> BridgeCertificate {
    let Some(v) = value else {
        return BridgeCertificate::default();
    };
    let finite = |x: f64| if x.is_finite() { x } else { 0.0 };
    BridgeCertificate {
        energy: finite(v.get("energy").and_then(Json::as_f64).unwrap_or(0.0)) as f32,
        gap: finite(v.get("gap").and_then(Json::as_f64).unwrap_or(0.0)) as f32,
        violations: v.get("violations").and_then(Json::as_u64).unwrap_or(0) as u32,
    }
}

fn last_user_text(doc: &Json) -> Option<String> {
    let msgs = doc.get("messages")?.as_arr()?;
    msgs.iter()
        .rev()
        .find(|m| m.get("role").and_then(Json::as_str) == Some("user"))
        .and_then(|m| m.get("content"))
        .and_then(|c| match c {
            Json::Str(s) => Some(s.clone()),
            Json::Arr(parts) => {
                let mut s = String::new();
                for p in parts {
                    if let Some(t) = p.get("text").and_then(Json::as_str) {
                        s.push_str(t);
                    }
                }
                Some(s)
            }
            _ => None,
        })
}

fn parse_json(body: &[u8]) -> std::result::Result<Json, (u16, Json)> {
    let text = match std::str::from_utf8(body) {
        Ok(t) => t,
        Err(_) => return Err(bad_pair("corpo não é UTF-8")),
    };
    // Clientes Windows (PowerShell `Set-Content`) mandam BOM: aceitar e remover.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Ok(Json::Obj(Vec::new()));
    }
    Json::parse(text).map_err(|e| (400, error_json("bad_json", &e.to_string())))
}

fn bad(msg: &str) -> Reply {
    Reply::Json(400, error_json("bad_request", msg))
}

fn bad_pair(msg: &str) -> (u16, Json) {
    (400, error_json("bad_request", msg))
}

fn forbidden(scope: ScopeId, bound: Option<u64>) -> Reply {
    Reply::Json(
        403,
        Json::obj([(
            "error",
            Json::obj([
                ("type", Json::str("forbidden_scope")),
                (
                    "message",
                    Json::str(format!(
                        "escopo {:?} não permitido para este token (organismo {:?})",
                        scope.organism_id, bound
                    )),
                ),
            ]),
        )]),
    )
}

fn error_json(kind: &str, msg: &str) -> Json {
    Json::obj([(
        "error",
        Json::obj([("type", Json::str(kind)), ("message", Json::str(msg))]),
    )])
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Erro de borda (não vaza para o núcleo).
pub type ApiResult<T> = std::result::Result<T, AetherError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fases_sao_deterministicas_e_distintas() {
        let a = text_to_phases("gato");
        let b = text_to_phases("gato");
        let c = text_to_phases("gata");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), PHASE_FLOATS);
        // fasores unitários: |re|,|im| <= 1 e o par tem norma ~1
        assert!(a.iter().all(|p| p.is_finite() && p.abs() <= 1.0));
        for pair in a.chunks_exact(2) {
            let n = (pair[0] * pair[0] + pair[1] * pair[1]).sqrt();
            assert!((n - 1.0).abs() < 1e-5, "fasor fora do círculo: {n}");
        }
    }

    #[test]
    fn scope_e_chave() {
        let s = Json::parse(r#"{"organism":1,"run":2,"app":3}"#).unwrap();
        assert_eq!(scope_from(Some(&s)), Some(ScopeId::new(1, 2, 3)));
        assert_eq!(
            key_from(Some(&Json::str("340282366920938463463374607431768211455"))),
            Some(u128::MAX)
        );
        assert_eq!(key_from(Some(&Json::num(7.0))), Some(7));
        assert_eq!(
            scope_json(ScopeId::new(1, 2, 3))
                .get("app")
                .and_then(Json::as_i64),
            Some(3)
        );
    }

    #[test]
    fn matriz_plana_e_aninhada() {
        let flat = Json::parse("[1,2,3,4]").unwrap();
        let nested = Json::parse("[[1,2],[3,4]]").unwrap();
        assert_eq!(
            flat_matrix(Some(&flat), 2, 2),
            Some(vec![1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(
            flat_matrix(Some(&nested), 2, 2),
            Some(vec![1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(flat_matrix(Some(&flat), 3, 2), None);
    }

    #[test]
    fn registro_roundtrip_json() {
        let r = DraftRecord {
            key: 42,
            layer: Layer::L6Graph,
            certificate: BridgeCertificate {
                energy: -1.5,
                gap: 0.25,
                violations: 3,
            },
            authority: 200,
            scope: ScopeId::new(1, 2, 3),
            tick: 10,
            wall_ms: 20,
            payload: phases_to_payload(&[0.5, 1.0, 1.5, 2.0]),
        };
        let text = record_to_json(&r).to_text();
        let back = Json::parse(&text).unwrap();
        let r2 = record_from_json(&back).unwrap();
        assert_eq!(r2.key, 42);
        assert_eq!(r2.layer, Layer::L6Graph);
        assert_eq!(r2.scope, ScopeId::new(1, 2, 3));
        assert_eq!(r2.authority, 200);
        assert_eq!(r2.payload, r.payload);
    }

    #[test]
    fn token_comparison() {
        assert!(token_eq("abc", "abc"));
        assert!(!token_eq("abc", "abd"));
        assert!(!token_eq("abc", "ab"));
    }
}

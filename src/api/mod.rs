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

pub mod json;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ndarray::{Array2, Array3};

use crate::core::certificate::{self, Decision, Provenance};
use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate as BridgeCertificate, DraftRecord, Layer, ScopeId, ScopeLedger,
};
use crate::core::presolve::{roof_dual_qumo, ROOF_MAX_N};
use crate::core::thermodynamic_solver::{IsingLandscape, ThermodynamicIsingSolver};
use crate::error::AetherError;

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
    /// Workers de conexão (mínimo 1).
    pub threads: usize,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8787".parse().expect("addr padrão"),
            ledger_path: None,
            token: None,
            organism: None,
            threads: 4,
        }
    }
}

/// Política de token + escopo.
#[derive(Clone, Debug)]
struct AuthPolicy {
    token: String,
    organism: Option<u64>,
}

/// Ledger em disco: uma linha JSON por fato, append-only.
#[derive(Clone, Debug)]
pub struct LedgerStore {
    path: PathBuf,
}

impl LedgerStore {
    /// Abre (ou cria) o arquivo do ledger.
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self { path })
    }

    /// Caminho do arquivo.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Grava um fato (uma linha). Erro de I/O é do chamador decidir.
    pub fn append(&self, record: &DraftRecord) -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(record_to_json(record).to_text().as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()
    }

    /// Lê o arquivo inteiro (linhas inválidas são ignoradas e contadas).
    pub fn load(&self) -> std::io::Result<(Vec<DraftRecord>, usize)> {
        let f = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        let mut bad = 0usize;
        for line in BufReader::new(f).lines() {
            let line = line?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            match Json::parse(t).ok().as_ref().and_then(record_from_json) {
                Some(r) => out.push(r),
                None => bad += 1,
            }
        }
        Ok((out, bad))
    }
}

/// Estado da borda: ledger, política de auth, store e contadores.
#[derive(Debug, Default)]
pub struct ApiState {
    ledger: ScopeLedger,
    store: Option<LedgerStore>,
    auth: Option<AuthPolicy>,
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
            let store = LedgerStore::new(path.clone())?;
            let (records, bad) = store.load()?;
            for r in records {
                state.ledger.append(r);
            }
            state.bad_lines = bad;
            state.store = Some(store);
        }
        state.auth = cfg.token.clone().map(|token| AuthPolicy {
            token,
            organism: cfg.organism,
        });
        Ok(state)
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
            "/v1/aether/remember" | "/v1/aether/cue" | "/v1/aether/solve" | "/v1/chat/completions"
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
        ("auth", Json::Bool(state.auth.is_some())),
        ("bad_lines", Json::num(state.bad_lines as f64)),
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
    if let Some(store) = &state.store {
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
    state.ledger.append(record);
    Reply::Json(
        200,
        Json::obj([
            ("stored", Json::Bool(true)),
            ("persisted", Json::Bool(persisted)),
            ("scope", scope_json(scope)),
            ("scope_key", Json::str(scope_key)),
            ("total", Json::num(state.ledger.len() as f64)),
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
    match state.ledger.cue_scored(scope, &query, &mut out) {
        Ok((hit, score)) if score >= MIN_SCORE => Reply::Json(
            200,
            Json::obj([
                ("abstain", Json::Bool(false)),
                ("hits", Json::num(hit.hits as f64)),
                ("key", Json::str(hit.key.to_string())),
                ("authority", Json::num(f64::from(hit.authority))),
                ("score", Json::num(f64::from(score))),
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
    let finish = if abstained {
        "content_filter"
    } else {
        "stop"
    };
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

fn payload_to_phases(payload: &[u8]) -> Vec<f32> {
    payload
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn record_to_json(r: &DraftRecord) -> Json {
    Json::obj([
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
        Json::obj([
            (
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
            ),
        ]),
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
            key_from(Some(&Json::str(
                "340282366920938463463374607431768211455"
            ))),
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



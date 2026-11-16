//! Borda externa **HTTP/JSON** (T2): como um app de IA (llama.cpp, Ollama,
//! SDK da OpenAI) consome o AETHER.
//!
//! # O que este módulo é — e o que não é
//!
//! O motor não gera texto: ele **recupera fase**, **resolve lógica** e devolve
//! **certificado de factibilidade**. Esta borda expõe exatamente isso em JSON,
//! compatível com o dialeto da OpenAI (`/v1/chat/completions`) e com o do
//! llama.cpp (`/completion`), para que um app externo possa apontar para cá.
//!
//! A conversão texto → fase é um **hash determinístico e lossy** (documentado
//! no cabeçalho `X-Aether-Edge`): não é embedding semântico. Por isso a
//! resposta traz `verdict` e pode ser **abstenção** — que é uma resposta
//! válida, não um erro HTTP.
//!
//! # Invariantes preservados
//!
//! * Nenhuma dependência nova: HTTP/1.1 mínimo sobre `std::net`.
//! * O núcleo não conhece este módulo; `String` vive só aqui (T2).
//! * Zero chamada ao ledger no caminho quente: `record` é do F1, e aqui só se
//!   **lê** o que já foi drenado.
//!
//! ```text
//! app (llama/agent) ──HTTP──► api ──► ScopeLedger (F1, T1/T2)
//!                              └───► roof_dual + solver + Certificate
//! ```

pub mod json;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use ndarray::{Array2, Array3};

use crate::core::certificate::{self, Decision, Provenance};
use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate as BridgeCertificate, DraftRecord, KnowledgeSource, Layer, ScopeId,
    ScopeLedger,
};
use crate::core::presolve::roof_dual_qumo;
use crate::core::thermodynamic_solver::{IsingLandscape, ThermodynamicIsingSolver};
use crate::error::AetherError;

use json::Json;

/// Dimensão de fase exposta pela borda.
pub const PHASE_DIMS: usize = 160;

/// Corpo máximo aceito (1 MiB) — acima disso a conexão é fechada com 413.
pub const MAX_BODY: usize = 1 << 20;

/// Modelo sintético anunciado em `/v1/models`.
pub const MODEL_ID: &str = "aether-phase-160";

/// Versão do crate (anunciada em `/health` e `/props`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Estado da borda: um ledger de escopo (F1) e um contador de requisições.
#[derive(Debug, Default)]
pub struct ApiState {
    ledger: ScopeLedger,
    requests: u64,
}

impl ApiState {
    /// Estado vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registros no ledger (todos os escopos).
    pub fn ledger_len(&self) -> usize {
        self.ledger.len()
    }

    /// Requisições atendidas.
    pub fn requests(&self) -> u64 {
        self.requests
    }
}

/// Liga o servidor em `addr` (porta `0` = efêmera).
pub fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr)
}

/// Atende conexões para sempre (uma por vez, `Connection: close`).
pub fn serve(listener: TcpListener) -> std::io::Result<()> {
    let mut state = ApiState::new();
    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let _ = handle(&mut state, &mut s);
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

/// Atende **uma** conexão (útil em teste e em servidor embutido).
pub fn handle(state: &mut ApiState, stream: &mut TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(stream)? else {
        return Ok(());
    };
    state.requests += 1;
    let (status, body) = route(state, &req);
    write_response(stream, status, &body)
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
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
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_BODY {
        return Ok(Some(Request {
            method,
            path,
            body: Vec::new(),
        }));
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request { method, path, body }))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &Json) -> std::io::Result<()> {
    let text = body.to_text();
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
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

fn route(state: &mut ApiState, req: &Request) -> (u16, Json) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/health") => (200, health(state)),
        ("GET", "/props") => (200, props(state)),
        ("GET", "/v1/models") => (200, models()),
        ("POST", "/v1/aether/remember") => remember(state, &req.body),
        ("POST", "/v1/aether/cue") => cue(state, &req.body),
        ("POST", "/v1/aether/solve") => solve(&req.body),
        ("POST", "/v1/chat/completions") => chat(state, &req.body),
        ("POST", "/completion") => completion(state, &req.body),
        ("OPTIONS", _) => (204, Json::Null),
        _ => (
            404,
            error_json("not_found", "rota inexistente (ver docs/EXTERNAL_API.md)"),
        ),
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

fn remember(state: &mut ApiState, body: &[u8]) -> (u16, Json) {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(scope) = scope_from(doc.get("scope")) else {
        return bad("scope ausente: use {organism,run,app}");
    };
    let Some(phases) = phases_from(doc.get("phases"), PHASE_DIMS) else {
        return bad("phases ausente ou com mais de 160 elementos");
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
    state.ledger.append(record);
    (
        200,
        Json::obj([
            ("stored", Json::Bool(true)),
            ("scope", scope_json(scope)),
            ("scope_key", Json::str(scope_key)),
            ("total", Json::num(state.ledger.len() as f64)),
        ]),
    )
}

fn cue(state: &mut ApiState, body: &[u8]) -> (u16, Json) {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(scope) = scope_from(doc.get("scope")) else {
        return bad("scope ausente: use {organism,run,app}");
    };
    let Some(query) = phases_from(doc.get("query"), PHASE_DIMS) else {
        return bad("query ausente (fases [f32])");
    };
    let mut out = vec![0.0f32; query.len()];
    match state.ledger.cue_into(scope, &query, &mut out) {
        Ok(hit) => (
            200,
            Json::obj([
                ("abstain", Json::Bool(false)),
                ("hits", Json::num(hit.hits as f64)),
                ("key", Json::str(hit.key.to_string())),
                ("authority", Json::num(f64::from(hit.authority))),
                ("phases", Json::Arr(out.iter().map(|p| Json::num(f64::from(*p))).collect())),
                (
                    "isolation",
                    Json::obj([
                        ("cross_scope_hits", Json::num(state.ledger.cross_scope_hits() as f64)),
                        (
                            "cross_scope_skipped",
                            Json::num(state.ledger.cross_scope_skipped() as f64),
                        ),
                    ]),
                ),
            ]),
        ),
        Err(BridgeError::EmptyScope) => (200, abstain("empty_scope")),
        Err(e) => (400, error_json("cue_failed", &e.to_string())),
    }
}

fn solve(body: &[u8]) -> (u16, Json) {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return e,
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
    let steps = doc.get("steps").and_then(Json::as_u64).unwrap_or(80).clamp(1, 4096) as usize;
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
        Err(e) => return (400, error_json("bad_shape", &e.to_string())),
    };
    let h2 = match Array2::from_shape_vec((1, n), h.clone()) {
        Ok(a) => a,
        Err(e) => return (400, error_json("bad_shape", &e.to_string())),
    };

    let mut solver = ThermodynamicIsingSolver::new(1, n, steps, 0.05);
    let mut best = f64::INFINITY;
    let mut best_s: Vec<i8> = vec![-1; n];
    let mut same = 0usize;
    for r in 0..restarts {
        solver.reset(seed.wrapping_add(r as u64));
        let out = match solver.relax(IsingLandscape::new(j3.view(), h2.view())) {
            Ok(o) => o,
            Err(e) => return (500, error_json("solve_failed", &e.to_string())),
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

    (
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
                    ("fixed", Json::num(pre.fixed_count() as f64)),
                    ("residual", Json::num(pre.residual.len() as f64)),
                    ("rounds", Json::num(pre.rounds as f64)),
                ]),
            ),
        ]),
    )
}

fn chat(state: &mut ApiState, body: &[u8]) -> (u16, Json) {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(text) = last_user_text(&doc) else {
        return bad("messages ausente ou sem mensagem de usuário");
    };
    let scope = scope_from(doc.get("aether").and_then(|a| a.get("scope")))
        .unwrap_or_default();
    let evidence = evidence_for(state, scope, &text);
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let content = evidence.to_text();
    let id = format!("chatcmpl-aether-{}", state.requests);
    (
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

fn completion(state: &mut ApiState, body: &[u8]) -> (u16, Json) {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let text = doc
        .get("prompt")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let scope = scope_from(doc.get("aether").and_then(|a| a.get("scope")))
        .unwrap_or_default();
    let evidence = evidence_for(state, scope, &text);
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    (
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
    match state.ledger.cue_into(scope, &query, &mut out) {
        Ok(hit) => {
            let (energy, gap, violations) = record_cert(&state.ledger, scope, hit.key)
                .unwrap_or((-0.0, 0.0, 0));
            let cert = certificate::Certificate::new(
                energy,
                gap,
                violations,
                1.0,
                Provenance::Inferred,
            );
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
                    Json::Arr(
                        out.iter()
                            .map(|p| Json::num(f64::from(*p)))
                            .collect(),
                    ),
                ),
                ("hits", Json::num(hit.hits as f64)),
                ("edge", Json::str("text-phase-hash-lossy")),
            ])
        }
        Err(BridgeError::EmptyScope) => abstain("empty_scope"),
        Err(e) => abstain(&format!("cue_failed:{e}")),
    }
}

fn record_cert(ledger: &ScopeLedger, scope: ScopeId, key: u128) -> Option<(f64, f64, u32)> {
    ledger
        .scoped(scope)
        .find(|r| r.key == key)
        .map(|r| {
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

fn decision_name(d: Decision) -> &'static str {
    match d {
        Decision::Act => "act",
        Decision::Cite => "cite",
        Decision::Propose => "propose",
        Decision::Abstain => "abstain",
    }
}

/// Texto → fases: hash FNV-1a + xorshift **determinístico**.
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
    let mut out = Vec::with_capacity(PHASE_DIMS);
    for _ in 0..PHASE_DIMS {
        st ^= st << 13;
        st ^= st >> 7;
        st ^= st << 17;
        out.push((st as f64 / u64::MAX as f64) as f32 * std::f32::consts::TAU);
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

fn phases_from(value: Option<&Json>, max: usize) -> Option<Vec<f32>> {
    let arr = value?.as_arr()?;
    if arr.is_empty() || arr.len() > max {
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        out.push(v.as_f64()? as f32);
    }
    Some(out)
}

fn flat_matrix(value: Option<&Json>, rows: usize, cols: usize) -> Option<Vec<f64>> {
    let v = value?;
    if let Some(outer) = v.as_arr() {
        if outer.len() == rows && outer.iter().all(|r| r.as_arr().is_some()) {
            let mut out = Vec::with_capacity(rows * cols);
            for row in outer {
                let r = row.as_arr()?;
                if r.len() != cols {
                    return None;
                }
                for x in r {
                    out.push(x.as_f64()?);
                }
            }
            return Some(out);
        }
        let mut out = Vec::with_capacity(rows * cols);
        for x in outer {
            out.push(x.as_f64()?);
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
    BridgeCertificate {
        energy: v.get("energy").and_then(Json::as_f64).unwrap_or(0.0) as f32,
        gap: v.get("gap").and_then(Json::as_f64).unwrap_or(0.0) as f32,
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
    let text = std::str::from_utf8(body).map_err(|_| bad("corpo não é UTF-8"))?;
    if text.trim().is_empty() {
        return Ok(Json::Obj(Vec::new()));
    }
    Json::parse(text).map_err(|e| (400, error_json("bad_json", &e.to_string())))
}

fn bad(msg: &str) -> (u16, Json) {
    (400, error_json("bad_request", msg))
}

fn error_json(kind: &str, msg: &str) -> Json {
    Json::obj([
        (
            "error",
            Json::obj([("type", Json::str(kind)), ("message", Json::str(msg))]),
        ),
    ])
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
        assert_eq!(a.len(), PHASE_DIMS);
        assert!(a.iter().all(|p| p.is_finite() && *p >= 0.0 && *p < TAU_TEST));
    }

    const TAU_TEST: f32 = std::f32::consts::TAU;

    #[test]
    fn scope_e_chave() {
        let s = Json::parse(r#"{"organism":1,"run":2,"app":3}"#).unwrap();
        assert_eq!(scope_from(Some(&s)), Some(ScopeId::new(1, 2, 3)));
        assert_eq!(key_from(Some(&Json::str("340282366920938463463374607431768211455"))), Some(u128::MAX));
        assert_eq!(key_from(Some(&Json::num(7.0))), Some(7));
        assert_eq!(scope_json(ScopeId::new(1, 2, 3)).get("app").and_then(Json::as_i64), Some(3));
    }

    #[test]
    fn matriz_plana_e_aninhada() {
        let flat = Json::parse("[1,2,3,4]").unwrap();
        let nested = Json::parse("[[1,2],[3,4]]").unwrap();
        assert_eq!(flat_matrix(Some(&flat), 2, 2), Some(vec![1.0, 2.0, 3.0, 4.0]));
        assert_eq!(flat_matrix(Some(&nested), 2, 2), Some(vec![1.0, 2.0, 3.0, 4.0]));
        assert_eq!(flat_matrix(Some(&flat), 3, 2), None);
    }
}


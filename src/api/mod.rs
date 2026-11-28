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
use std::time::{Duration, Instant};

use aether_decision::{
    Answer, DecisionError, DecisionMeta, DecisionPort, DecisionRequest, DecisionResponse, StubPort,
};
use ndarray::{Array2, Array3};

use crate::agent::{
    AetherAgent, AgentConfig, Budget, Goal, GoalKind, Risk, SuccessCriteria, Task, ToolAgent,
    AUTHORITY_QUARANTINE,
};
use crate::core::certificate::{self, Decision, Provenance};
use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate as BridgeCertificate, DraftRecord, Layer, ScopeId, ScopeLedger,
};
use crate::core::logic_pipeline::Clause3;
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

/// Corpo máximo aceito (1 MiB) - acima disso a conexão é fechada com 413.
pub const MAX_BODY: usize = 1 << 20;

/// Deadline **total** de um request (todas as leituras do cabeçalho + corpo).
/// Acima disso a conexão é fechada sem resposta (R2 — anti slow-loris).
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(30);

/// Tamanho máximo de uma única linha de cabeçalho (R2).
pub const MAX_HEADER_LINE: usize = 8 * 1024;

/// Número máximo de cabeçalhos por request (R2).
pub const MAX_HEADERS: usize = 64;

/// Score mínimo de correlação para responder em vez de abster.
///
/// Duas fases aleatórias dão correlação ~0; o mesmo vetor dá 1. Abaixo de 0,5
/// o fato não é "parecido" — é ruído, e responder seria inventar.
pub const MIN_SCORE: f32 = 0.5;

/// Autoridade a partir da qual `forget` só grava túmulo e escala para HITL
/// (ADR-014/MSG 0306).
pub const AUTHORITY_HITL: u8 = 200;

/// Autoridade aprovada por humano: único valor que purga de fato, e só com
/// confirmação explícita (`confirm: true`).
pub const AUTHORITY_HUMAN: u8 = 255;

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
    /// Segredo **servidor-side** que autoriza escrita/purga de fatos em
    /// autoridade `>= 200` (HITL). Populado de `AETHER_HITL_TOKEN`.
    ///
    /// Vale para as rotas de memória expostas ao cliente
    /// (`/v1/aether/memory/remember`, `/v1/aether/memory/forget`): nelas o
    /// corpo nunca concede poder. Nas rotas de ledger local
    /// (`/v1/aether/remember`, `/v1/aether/forget`) a `authority` do host é o
    /// mandato e este segredo não é exigido.
    pub hitl_token: Option<String>,
    /// Tamanho da fila de conexões entre o accept-loop e os workers.
    ///
    /// Fila cheia → o accept responde `503` e fecha (backpressure real; antes
    /// era `mpsc::channel` ilimitado — memória crescia sem teto).
    pub queue_limit: usize,
    /// Teto de conexões simultâneas aceitas (defesa de exaustão de fds).
    pub max_conns: usize,
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
            hitl_token: std::env::var("AETHER_HITL_TOKEN")
                .ok()
                .filter(|token| !token.trim().is_empty()),
            queue_limit: 128,
            max_conns: 256,
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
///
/// R9 (sharding): o cliente SGDB **não** mora mais aqui — vive em
/// [`SgdbEdge`] sob o próprio `Mutex`, e o I/O de rede acontece **fora** do
/// lock global (que cobre ledger/agent/contadores, todos compute-local).
#[derive(Debug, Default)]
pub struct ApiState {
    ledger: ScopeLedger,
    store: Option<LedgerStore>,
    auth: Option<AuthPolicy>,
    /// Segredo servidor-side para escrever/purgar em autoridade `>= 200`.
    hitl: Option<String>,
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
        state.hitl = cfg
            .hitl_token
            .clone()
            .filter(|token| !token.trim().is_empty());
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

/// Constrói o agente da borda a partir da config (T5: shard próprio).
fn agent_from_cfg(cfg: &ApiConfig) -> AetherAgent {
    let mut agent_cfg = AgentConfig::default();
    if let Some(spec) = &cfg.sgdb {
        // Especialista de MEMÓRIA quando `--sgdb` está presente (JARBAS I3).
        // O MemoryExpert abre o próprio cliente em modo leitura; o transporte
        // é o único escritor do arquivo.
        agent_cfg.sgdb = Some(spec.clone());
    }
    AetherAgent::new(agent_cfg)
}

/// Shard do SGDB (R9): cliente + último erro, sob o **próprio** Mutex — o I/O
/// de rede nunca é feito segurando o lock do [`ApiState`] (head-of-line
/// blocking de 1500 ms congelava a borda inteira).
///
/// O **status** é lido sem lock (`AtomicBool`): `/health`/`/props` não podem
/// esperar o mutex do edge, senão o I/O do SGDB volta a bloqueá-los.
#[derive(Debug, Default)]
pub struct SgdbEdge {
    client: Option<SgdbClient<Box<dyn Transport>>>,
    had_error: std::sync::atomic::AtomicBool,
}

impl SgdbEdge {
    /// Abre o cliente a partir da config (best-effort; offline → erro honesto).
    fn load(cfg: &ApiConfig) -> Self {
        let Some(spec) = &cfg.sgdb else {
            return Self::default();
        };
        let mut sgdb_cfg = SgdbConfig::from_env();
        if let Some(path) = &cfg.sgdb_outbox {
            sgdb_cfg.outbox = Some(path.clone());
        }
        if let Some(token) = &cfg.sgdb_token {
            sgdb_cfg.token = Some(token.clone());
        }
        sgdb_cfg.recall_k = sgdb_cfg.recall_k.max(32);
        match open(spec, sgdb_cfg) {
            Ok(client) => Self {
                client: Some(client),
                had_error: std::sync::atomic::AtomicBool::new(false),
            },
            Err(e) => {
                eprintln!(
                    "aviso: neural-sgdb não abriu ({e}) — a borda segue só com o ledger local"
                );
                Self {
                    client: None,
                    had_error: std::sync::atomic::AtomicBool::new(true),
                }
            }
        }
    }

    /// Status do SGDB (`off`/`on`/`error`) — **sem lock** (atomics).
    pub fn status(&self) -> &'static str {
        if self.had_error.load(std::sync::atomic::Ordering::Relaxed) {
            return "error";
        }
        match &self.client {
            Some(_) => "on",
            None => "off",
        }
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

/// Drena o que o cliente enviou e fecha (usado nas respostas de overload).
///
/// No Windows, fechar um socket com dados recebidos não lidos envia RST e o
/// cliente perde a resposta escrita (10054). Drenar antes do drop garante que
/// o `503` chegue. Cap de `4 * MAX_BODY` para não virar um amplificador.
fn drain_and_close(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    let mut scratch = [0u8; 8192];
    let mut total = 0usize;
    loop {
        match s.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total += n;
                if total > 4 * MAX_BODY {
                    break;
                }
            }
        }
    }
    drop(s);
}

/// Atende um `TcpListener` já ligado.
pub fn serve_listener(listener: TcpListener, cfg: ApiConfig) -> std::io::Result<()> {
    let state = Arc::new(Mutex::new(ApiState::load(&cfg)?));
    // R9 (sharding): o SGDB tem o próprio lock — rede nunca sob o lock global.
    let sgdb = Arc::new(Mutex::new(SgdbEdge::load(&cfg)));
    // T5 (sharding): o agente tem o próprio lock — `agent.run` (que pode
    // falar rede via MemoryExpert) nunca segura o lock do estado.
    let agent: Arc<Mutex<Option<AetherAgent>>> = Arc::new(Mutex::new(Some(agent_from_cfg(&cfg))));
    let (tx, rx) = mpsc::sync_channel::<TcpStream>(cfg.queue_limit.max(1));
    let rx = Arc::new(Mutex::new(rx));
    let workers = cfg.threads.max(1);
    // Backpressure (R2): contador de conexões vivas; acima de `max_conns` o
    // accept responde 503 e fecha, em vez de acumular streams na fila.
    let conns = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let max_conns = cfg.max_conns.max(workers);

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let rx = Arc::clone(&rx);
        let state = Arc::clone(&state);
        let sgdb = Arc::clone(&sgdb);
        let agent = Arc::clone(&agent);
        let conns = Arc::clone(&conns);
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
                    // Um panic dentro de `handle` (payload hostil que estoura
                    // índice no núcleo) não pode derrubar o worker nem envenenar
                    // o estado: o guard é recuperado com `into_inner` e a
                    // conexão recebe 500 — a borda continua atendendo.
                    //
                    // CONTRATO DE PERFIL: `catch_unwind` só captura panics que
                    // fazem *unwind*. Este guard é REAL quando a borda é
                    // construída com `--profile release-unwind`
                    // ([profile.release-unwind] no Cargo.toml, `panic =
                    // "unwind"`). Com `panic = "abort"` ([profile.release]) o
                    // runtime aborta o processo e este ramo é dead-code —
                    // política de abort vale para os demais binários. Tests
                    // rodam com unwind (Cargo ignora `panic` em perfis de
                    // test), então este ramo é exercitado pela suíte.
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle(&state, &sgdb, &agent, &mut stream)
                    }));
                    if outcome.is_err() {
                        // Best-effort: se o cabeçalho da resposta ainda não
                        // saiu, o cliente lê um 500 em vez de reset.
                        let _ = write_response(
                            &mut stream,
                            500,
                            &error_json("internal", "panic ao atender a requisição"),
                        );
                    }
                    drop(stream);
                    // A conexão deixou de estar "em serviço": libera o teto.
                    conns.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
                Err(_) => break,
            }
        }));
    }

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let cur = conns.load(std::sync::atomic::Ordering::Relaxed);
                if cur >= max_conns {
                    // Sobrecarga explícita: 503 e fecha (o cliente pode tentar
                    // de novo) em vez de crescer a fila sem teto.
                    let _ = write_response(&mut s, 503, &error_json("overload", "fila cheia"));
                    drain_and_close(s);
                    continue;
                }
                conns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match tx.try_send(s) {
                    Ok(()) => {}
                    Err(std::sync::mpsc::TrySendError::Full(mut s)) => {
                        conns.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        let _ = write_response(&mut s, 503, &error_json("overload", "fila cheia"));
                        drain_and_close(s);
                    }
                    Err(std::sync::mpsc::TrySendError::Disconnected(s)) => {
                        conns.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        drop(s);
                        break;
                    }
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
///
/// R9: recebe os dois shards por `Mutex` — o lock global do estado é tomado
/// **somente** nas seções que tocam ledger/agent/contadores; o I/O de rede do
/// SGDB acontece fora dele (ver `remember`/`cue`/`evidence_for`).
pub fn handle(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    agent: &Mutex<Option<AetherAgent>>,
    stream: &mut TcpStream,
) -> std::io::Result<()> {
    let Some(req) = read_request(stream)? else {
        return Ok(());
    };
    let started = std::time::Instant::now();
    // Contador de requests é atomicamente consistente com a fila de conexões.
    {
        let mut st = lock_state(state);
        st.requests += 1;
    }
    let reply = route(state, sgdb, agent, &req);
    // T14 (mitigado): telemetria por request — off por default (AETHER_LOG).
    crate::log::event(
        "api",
        &format!("{} {} {:?}", req.method, req.path, started.elapsed()),
    );
    write_reply(stream, reply)
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    token: Option<String>,
    /// Cabeçalho `x-aether-hitl` (segredo HITL; nunca vem do corpo).
    hitl: Option<String>,
    too_large: bool,
}

enum Reply {
    Json(u16, Json),
    Sse(Vec<String>),
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    // Deadline **total** do request (R2): o read-timeout de 10 s é por leitura;
    // um cliente lento que goteja 1 byte a cada 9 s antes segurava um worker
    // indefinidamente. Agora o request inteiro tem `REQUEST_DEADLINE`.
    let deadline = Instant::now() + REQUEST_DEADLINE;
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
    let mut hitl: Option<String> = None;
    let mut header_count = 0usize;
    loop {
        line.clear();
        if Instant::now() > deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "deadline do request estourado",
            ));
        }
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        // Caps de header (R2): linha de 8 KB e 64 headers — um cliente hostil
        // antes podia crescer a `line` sem teto (memória) e empilhar headers.
        if line.len() > MAX_HEADER_LINE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "linha de header acima do limite",
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        header_count += 1;
        if header_count > MAX_HEADERS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "número de headers acima do limite",
            ));
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
            } else if k.eq_ignore_ascii_case("x-aether-hitl") {
                hitl = Some(v.to_string());
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
        hitl,
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
        409 => "Conflict",
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
         x-aether-version: {VERSION}\r\n\
         x-aether-edge: text-phase-hash-lossy\r\n\
         \r\n",
        text.len()
    )?;
    stream.write_all(text.as_bytes())?;
    stream.flush()
}

/// Lock com recuperação de poison (o guard nunca vira pânico no worker).
fn lock_state(state: &Mutex<ApiState>) -> std::sync::MutexGuard<'_, ApiState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

/// Lock do shard do agente (T5) com recuperação de poison.
fn lock_state_agent(
    agent: &Mutex<Option<AetherAgent>>,
) -> std::sync::MutexGuard<'_, Option<AetherAgent>> {
    match agent.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

/// Lock do shard SGDB com recuperação de poison.
fn lock_edge(sgdb: &Mutex<SgdbEdge>) -> std::sync::MutexGuard<'_, SgdbEdge> {
    match sgdb.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

fn route(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    agent: &Mutex<Option<AetherAgent>>,
    req: &Request,
) -> Reply {
    if req.too_large {
        return Reply::Json(413, error_json("payload_too_large", "corpo acima de 1 MiB"));
    }

    // Saúde e catálogo são públicos (probe); o resto exige token quando há auth.
    // Com `memory.write` no ToolAgent, o pipeline HTTP também exige token
    // quando a borda está configurada com auth (sem token, check_auth libera).
    let needs_auth = matches!(
        (req.method.as_str(), req.path.as_str()),
        (
            "POST",
            "/v1/aether/remember"
                | "/v1/aether/memory/remember"
                | "/v1/aether/cue"
                | "/v1/aether/solve"
                | "/v1/aether/agent"
                | "/v1/aether/decision"
                | "/v1/aether/forget"
                | "/v1/aether/memory/forget"
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
        ("GET", "/health") => Reply::Json(200, health(state, sgdb, agent)),
        ("GET", "/props") => Reply::Json(200, props(state, sgdb, agent)),
        ("GET", "/v1/models") => Reply::Json(200, models()),
        ("POST", "/v1/aether/remember") => remember(state, sgdb, req, bound, false),
        ("POST", "/v1/aether/memory/remember") => remember(state, sgdb, req, bound, true),
        ("POST", "/v1/aether/cue") => cue(state, sgdb, &req.body, bound),
        ("POST", "/v1/aether/solve") => solve(&req.body),
        ("POST", "/v1/aether/agent") => agent_route(state, agent, &req.body, bound),
        ("POST", "/v1/aether/decision") => decision_route(&req.body),
        ("POST", "/v1/aether/forget") => forget(state, req, bound, false),
        ("POST", "/v1/aether/memory/forget") => forget(state, req, bound, true),
        ("POST", "/v1/chat/completions") => chat(state, sgdb, &req.body, bound),
        ("POST", "/completion") => completion(state, sgdb, &req.body, bound),
        ("OPTIONS", _) => Reply::Json(204, Json::Null),
        _ => Reply::Json(
            404,
            error_json("not_found", "rota inexistente (ver docs/EXTERNAL_API.md)"),
        ),
    }
}

fn check_auth(state: &Mutex<ApiState>, req: &Request) -> Result<Option<u64>, Reply> {
    let st = lock_state(state);
    let Some(policy) = &st.auth else {
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
    // T7: sem leak de comprimento — compara sobre `max(len)` com padding 0;
    // o XOR de comprimentos entra no acumulador. Sem dep (`subtle`).
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        let x = a.as_bytes().get(i).copied().unwrap_or(0);
        let y = b.as_bytes().get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

/// Autoriza operação em autoridade `>= 200` (HITL).
///
/// O segredo é **do servidor** (`ApiConfig.hitl_token` / `AETHER_HITL_TOKEN`) e
/// chega em `x-aether-hitl` (ou em `Authorization`/`X-Aether-Token`, quando é o
/// mesmo segredo). O corpo (`authority`, `confirm`) nunca conta. Sem segredo
/// configurado não existe canal humano: 200/255 ficam indisponíveis.
fn hitl_authorized(state: &ApiState, req: &Request) -> bool {
    let Some(expected) = state.hitl.as_deref() else {
        return false;
    };
    if expected.trim().is_empty() {
        return false;
    }
    [req.hitl.as_deref(), req.token.as_deref()]
        .into_iter()
        .flatten()
        .any(|presented| token_eq(presented.trim(), expected))
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

fn health(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    agent: &Mutex<Option<AetherAgent>>,
) -> Json {
    let st = lock_state(state);
    // R9: /health NÃO espera o mutex do edge (I/O do SGDB em curso) — try_lock:
    // se ocupado, há cliente configurado e ativo → "on".
    let sgdb_status = match sgdb.try_lock() {
        Ok(edge) => edge.status(),
        Err(_) => "on",
    };
    // T5: idem para o agente — try_lock; ocupado = agente em solve → "on".
    let agent_on = match agent.try_lock() {
        Ok(a) => a.is_some(),
        Err(_) => true,
    };
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
        ("requests", Json::num(st.requests as f64)),
        ("ledger_entries", Json::num(st.ledger_len() as f64)),
        ("persisted", Json::Bool(st.store.is_some())),
        (
            "ledger_bytes",
            Json::num(st.store.as_ref().map_or(0, LedgerStore::size) as f64),
        ),
        (
            "ledger_max_bytes",
            Json::num(st.store.as_ref().map_or(0, LedgerStore::max_bytes) as f64),
        ),
        ("auth", Json::Bool(st.auth.is_some())),
        ("bad_lines", Json::num(st.bad_lines as f64)),
        ("sgdb", Json::str(sgdb_status)),
        ("agent", Json::Bool(agent_on)),
    ])
}

fn props(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    agent: &Mutex<Option<AetherAgent>>,
) -> Json {
    let st = lock_state(state);
    // R9: idem /health — o edge nunca bloqueia os probes.
    let sgdb_status = match sgdb.try_lock() {
        Ok(edge) => edge.status(),
        Err(_) => "on",
    };
    Json::obj([
        ("model_path", Json::str(MODEL_ID)),
        ("n_ctx", Json::num(PHASE_DIMS as f64)),
        ("total_slots", Json::num(1.0)),
        (
            "aether",
            Json::obj([
                ("version", Json::str(VERSION)),
                ("phase_dims", Json::num(PHASE_DIMS as f64)),
                ("ledger_entries", Json::num(st.ledger_len() as f64)),
                ("persisted", Json::Bool(st.store.is_some())),
                ("sgdb", Json::str(sgdb_status)),
                ("edge", Json::str("text-phase-hash-lossy")),
                // Freeze v0.1 — docs/V01_DELIVERY_PLAN.md
                (
                    "frontier",
                    Json::str("LLM gera; AETHER cita, certifica ou abstem"),
                ),
                ("actions", Json::str("act|cite|propose|abstain")),
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

/// Grava um fato no ledger (F1).
///
/// Dois contratos explícitos, por rota:
///
/// * `/v1/aether/remember` — **ledger local confiável** (organismo/chat e
///   avaliação): a `authority` do corpo é o mandato e é preservada (0..=255).
///   É a rota que a documentação usa para semear fatos com 255.
/// * `/v1/aether/memory/remember` — **memória exposta ao cliente**: ignora a
///   autoridade do corpo. Fato novo entra em [`AUTHORITY_QUARANTINE`] (96), e
///   200/255 só com o segredo HITL do servidor no cabeçalho (promoção
///   server-side). A resposta informa `authority` efetiva e, quando o pedido
///   foi rebaixado, `authority_requested`.
fn remember(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    req: &Request,
    bound: Option<u64>,
    memory: bool,
) -> Reply {
    let doc = match parse_json(&req.body) {
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
    let requested = doc
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

    // --- Fase A (lock curto do estado): HITL + append + compactação ---------
    // R9: nada de rede aqui — a chamada SGDB de 1500 ms antes congelava a
    // borda inteira (head-of-line blocking sob o lock global).
    let authority;
    let demoted;
    let persisted;
    let mut compacted = false;
    let mut compact_error: Option<String> = None;
    let scope_key;
    let total;
    let record;
    {
        let mut st = lock_state(state);
        // Rota de memória: o cliente não promove. Rota de ledger: autoridade é
        // do host local (ver doc acima) e é preservada como sempre foi.
        authority = if !memory || (requested >= AUTHORITY_HITL && hitl_authorized(&st, req)) {
            requested
        } else {
            AUTHORITY_QUARANTINE
        };
        demoted = requested > authority;
        record = DraftRecord {
            key,
            layer,
            certificate,
            authority,
            scope,
            tick,
            wall_ms,
            payload: phases_to_payload(&phases),
        };
        scope_key = record.scope_key();
        let mut p = false;
        if let Some(store) = st.store.as_mut() {
            match store.append(&record) {
                Ok(()) => p = true,
                Err(e) => {
                    return Reply::Json(
                        500,
                        error_json("ledger_io", &format!("falha ao gravar: {e}")),
                    )
                }
            }
        }
        persisted = p;
        st.ledger.append(record.clone());

        // Compactação: reescreve só os vivos quando o arquivo passa do teto.
        if st.store.as_ref().map(LedgerStore::should_compact) == Some(true) {
            let all: Vec<DraftRecord> = st.ledger.records().cloned().collect();
            if let Some(store) = st.store.as_mut() {
                match store.compact(&all) {
                    Ok(()) => compacted = true,
                    Err(e) => compact_error = Some(e.to_string()),
                }
            }
        }
        total = st.ledger.len();
    }

    // --- Fase B (SEM lock): rede SGDB ---------------------------------------
    // O contrato do SGDB é fase complexa (`re, im`); a borda HTTP fala fases
    // puras. Converte na fronteira para um único formato durável.
    let durable = DraftRecord {
        payload: phases_to_complex(&record.payload),
        ..record.clone()
    };
    let sgdb_state;
    {
        let mut edge = lock_edge(sgdb);
        let result = edge
            .client
            .as_mut()
            .map(|client| client.remember_records(std::slice::from_ref(&durable)));
        match result {
            Some(Ok(1)) => {
                sgdb_state = "stored";
                edge.had_error
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            Some(Ok(_)) => {
                sgdb_state = "empty";
                edge.had_error
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            Some(Err(e)) => {
                sgdb_state = "offline";
                edge.had_error
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = e;
            }
            None => sgdb_state = "off",
        }
    }

    let mut fields: Vec<(String, Json)> = vec![
        ("stored".into(), Json::Bool(true)),
        ("persisted".into(), Json::Bool(persisted)),
        ("compacted".into(), Json::Bool(compacted)),
        // Autoridade **efetiva** (servidor): o corpo não promove.
        ("authority".into(), Json::num(f64::from(authority))),
        ("scope".into(), scope_json(scope)),
        ("scope_key".into(), Json::str(scope_key)),
        ("total".into(), Json::num(total as f64)),
        ("sgdb".into(), Json::str(sgdb_state)),
    ];
    if demoted {
        fields.push((
            "authority_requested".into(),
            Json::num(f64::from(requested)),
        ));
    }
    if let Some(e) = compact_error {
        fields.push(("compact_error".into(), Json::str(e)));
    }
    Reply::Json(200, Json::Obj(fields))
}

/// Remove um fato do escopo e grava o túmulo (para não ressuscitar no boot).
///
/// Dois contratos explícitos, por rota:
///
/// * `/v1/aether/forget` — **ledger local confiável**: a `authority` do corpo é
///   o mandato (como em `remember`). `< 200` → `403 authority_required`;
///   `200..=254` grava túmulo e **não** purga (`forgot:false`, HITL pendente);
///   `255` purga **somente** com store (túmulo antes do purge); sem store →
///   `500 ledger_io` (MSG 0413/0415 — sem purge silencioso).
/// * `/v1/aether/memory/forget` — memória exposta ao cliente: o corpo não
///   concede poder. `200/255` exigem o **segredo HITL do servidor** no
///   cabeçalho `x-aether-hitl` (ou `Authorization`/`X-Aether-Token`, quando for
///   o mesmo segredo); sem ele, `403 hitl_required`. Para 255 exige ledger
///   persistente (túmulo durável antes do purge), senão `500 ledger_io`.
///
/// Em ambas, o túmulo é gravado **antes** da remoção: se o disco falhar (ou não
/// houver store), o fato continua citável e o cliente vê `500`, em vez de
/// perder o registro sem rastro.
fn forget(state: &Mutex<ApiState>, req: &Request, bound: Option<u64>, memory: bool) -> Reply {
    let doc = match parse_json(&req.body) {
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
    if authority < AUTHORITY_HITL {
        return Reply::Json(
            403,
            error_json(
                "authority_required",
                "forget exige authority >= 200 (túmulo + HITL)",
            ),
        );
    }
    // Só a rota de memória depende do segredo do servidor: na rota de ledger a
    // `authority` do host local é o mandato (mesma distinção de `remember`).
    if memory && !hitl_authorized(&lock_state(state), req) {
        return Reply::Json(
            403,
            error_json(
                "hitl_required",
                "forget 200/255 exige o segredo HITL do servidor (x-aether-hitl)",
            ),
        );
    }
    let reason = doc.get("reason").and_then(Json::as_str).unwrap_or("manual");

    // Tudo abaixo é compute-local (ledger/store): lock único do estado.
    let mut st = lock_state(state);
    // Só grava túmulo de chave existente: um `forget` de chave ausente não pode
    // matar uma escrita futura com a mesma chave no próximo boot.
    let exists = st.ledger.scoped(scope).any(|r| r.key == key);
    // Túmulo ANTES da remoção (ADR-014: intenção antes do efeito).
    let mut tombstoned = false;
    if exists {
        if let Some(store) = st.store.as_mut() {
            if let Err(e) = store.tombstone(scope, key, reason, now_ms()) {
                return Reply::Json(
                    500,
                    error_json("ledger_io", &format!("falha ao gravar túmulo: {e}")),
                );
            }
            tombstoned = true;
        } else if authority == AUTHORITY_HUMAN {
            // Sem store não há túmulo durável: purgar apagaria o fato e ele
            // ressuscitaria no próximo boot (MSG 0413/0415 — ledger e memory).
            return Reply::Json(
                500,
                error_json("ledger_io", "sem store para gravar o túmulo antes do purge"),
            );
        }
    }
    // 200..=254 é HITL: marca, mas não purga (o purge é o 255 autorizado).
    let hitl = authority < AUTHORITY_HUMAN;
    let removed = if hitl {
        false
    } else {
        st.ledger.forget(scope, key)
    };
    let mut fields: Vec<(String, Json)> = vec![
        ("forgot".into(), Json::Bool(removed)),
        ("tombstoned".into(), Json::Bool(tombstoned)),
        ("key".into(), Json::str(key.to_string())),
        ("scope".into(), scope_json(scope)),
        ("remaining".into(), Json::num(st.ledger.len() as f64)),
    ];
    // Campo só no ramo novo: a resposta do purge 255 autorizado mantém o shape
    // antigo (`forgot`/`tombstoned`/`key`/`scope`/`remaining`).
    if hitl {
        fields.insert(2, ("hitl".into(), Json::Bool(true)));
    }
    Reply::Json(200, Json::Obj(fields))
}

fn cue(state: &Mutex<ApiState>, sgdb: &Mutex<SgdbEdge>, body: &[u8], bound: Option<u64>) -> Reply {
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
    // Fase A (lock curto): score local.
    let mut out = vec![0.0f32; query.len()];
    let mut result;
    let mut source = "local";
    {
        let mut st = lock_state(state);
        result = st.ledger.cue_scored(scope, &query, &mut out);
        let needs_remote = !matches!(&result, Ok((_, score)) if *score >= MIN_SCORE);
        if needs_remote {
            // Fase B (sem lock do estado): rede SGDB. Sem hit local (ex.:
            // processo reiniciou): reacende do SGDB e tenta de novo. O ledger
            // textual não substitui a fase — ele a reacende.
            let hydrated = hydrate_from_sgdb(state, sgdb, scope);
            if hydrated > 0 {
                result = st.ledger.cue_scored(scope, &query, &mut out);
                source = "sgdb";
            }
        }
    }
    let cross = {
        let st = lock_state(state);
        (
            st.ledger.cross_scope_hits(),
            st.ledger.cross_scope_skipped(),
        )
    };
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
                        ("cross_scope_hits", Json::num(cross.0 as f64)),
                        ("cross_scope_skipped", Json::num(cross.1 as f64)),
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
/// apenas registra o erro no shard SGDB (`/health` mostra `sgdb: error`).
/// R9: o I/O acontece fora do lock do estado; os hits são re-anexados sob
/// lock curto no fim.
fn hydrate_from_sgdb(state: &Mutex<ApiState>, sgdb: &Mutex<SgdbEdge>, scope: ScopeId) -> usize {
    let recall = {
        let mut edge = lock_edge(sgdb);
        let Some(client) = edge.client.as_mut() else {
            return 0;
        };
        match client.recall_scope(scope, 32) {
            Ok(hits) => Some(hits),
            Err(_) => {
                edge.had_error
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    };
    let Some(hits) = recall else {
        return 0;
    };
    let count = hits.len();
    let mut st = lock_state(state);
    for hit in hits {
        st.ledger.append(DraftRecord {
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

/// Rota do agente: fato, SAT, QUBO, memória **ou** pipeline `steps[]`.
fn agent_route(
    state: &Mutex<ApiState>,
    agent_mux: &Mutex<Option<AetherAgent>>,
    body: &[u8],
    bound: Option<u64>,
) -> Reply {
    let doc = match parse_json(body) {
        Ok(v) => v,
        Err(e) => return Reply::Json(e.0, e.1),
    };
    // Workflow multi-passo (MSG 0287/0319): não toca `/v1/chat/completions`.
    // `pipeline`/`steps` só desviam a rota quando são **array** (`null` num
    // corpo de tarefa única continua sendo tarefa única).
    match pipeline_requested(&doc) {
        Ok(true) => return agent_pipeline_route(&doc, bound),
        Ok(false) => {}
        Err(e) => return bad(&e),
    }
    let task = if let (Some(subject), Some(relation)) = (
        doc.get("subject").and_then(|v| v.as_str()),
        doc.get("relation").and_then(|v| v.as_str()),
    ) {
        Task::Fact {
            subject: subject.to_string(),
            relation: relation.to_string(),
        }
    } else if let Some(clauses_json) = doc.get("clauses").filter(|v| **v != Json::Null) {
        let raw = match raw_clauses(Some(clauses_json)) {
            Ok(v) => v,
            Err(e) => return bad(&e),
        };
        let n = match doc.get("n").and_then(|v| v.as_u64()).map(|v| v as usize) {
            Some(n) => n,
            // Sem `n`: a maior variável citada manda (1-based).
            None => raw
                .iter()
                .flatten()
                .map(|v| v.unsigned_abs() as usize)
                .max()
                .unwrap_or(0),
        };
        if n == 0 || n > AGENT_MAX_N {
            return bad(&format!("sat n fora de 1..={AGENT_MAX_N}"));
        }
        let clauses = match clauses_from_raw(&raw, n) {
            Ok(v) => v,
            Err(e) => return bad(&e),
        };
        Task::Satisfy { n, clauses }
    } else if let Some(n) = doc.get("n").and_then(|v| v.as_u64()).map(|v| v as usize) {
        // Teto **antes** de qualquer alocação: `flat_matrix` reserva n² floats.
        if n == 0 || n > AGENT_MAX_N {
            return bad(&format!("qubo n fora de 1..={AGENT_MAX_N}"));
        }
        let Some(j) = flat_matrix(doc.get("j"), n, n) else {
            return bad(&format!("j deve ter {} elementos (n×n)", n * n));
        };
        let h = match doc.get("h") {
            None | Some(Json::Null) => vec![0.0; n],
            Some(_) => match flat_matrix(doc.get("h"), n, 1) {
                Some(v) => v,
                None => return bad(&format!("h deve ter {n} elementos")),
            },
        };
        Task::Optimize { n, j, h }
    } else if let Some(query) = doc.get("query").and_then(|v| v.as_str()) {
        Task::Memory {
            query: query.to_string(),
        }
    } else {
        return bad("use {subject,relation}, {clauses,n}, {n,j,h}, {query} ou {steps|pipeline}");
    };
    // T5: o agente tem o PRÓPRIO lock — `agent.run` (que pode falar rede via
    // MemoryExpert) nunca segura o lock do estado; solve do agente serializa
    // apenas consigo mesmo.
    let mut agent_shard = lock_state_agent(agent_mux);
    let Some(agent) = agent_shard.as_mut() else {
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
///
/// O agente recebe o journal de undo durável ([`undo_journal_path`]) para que
/// `memory.write` funcione pela borda e o rollback do workflow seja real:
/// `SgdbUndoBackend::new` nunca panica em erro de I/O, então a rota não tem
/// caminho de falha por causa do arquivo.
fn agent_pipeline_route(doc: &Json, bound: Option<u64>) -> Reply {
    let scope = if doc.get("scope").is_some() {
        let Some(scope) = scope_or_bound(doc.get("scope"), bound) else {
            return bad("scope ausente: use {organism,run,app}");
        };
        if !scope_allowed(scope, bound) {
            return forbidden(scope, bound);
        }
        Some(scope)
    } else {
        None
    };
    let workflow = match pipeline_goal_from_json(doc) {
        Ok(goal) => goal,
        Err(e) => return bad(&e),
    };
    let mut agent =
        ToolAgent::new(&agent_pipeline_config(bound, scope)).with_undo_journal(undo_journal_path());
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

/// Teto de passos no workflow HTTP (MSG 0340).
const PIPELINE_MAX_STEPS: usize = 8;

/// Config do `ToolAgent` usado pela borda.
///
/// Sem `scope` no corpo, o token liga o pipeline a `(organism, 0, 0)`, o mesmo
/// mapeamento de `remember`/`cue`. Com `scope` explícito, `run` e `app` entram
/// — desde que o organismo seja o do token.
fn agent_pipeline_config(bound: Option<u64>, scope: Option<ScopeId>) -> AgentConfig {
    let mut cfg = AgentConfig::default();
    if let Some(scope) = scope {
        cfg.scope = scope;
    } else if let Some(organism) = bound {
        cfg.scope = ScopeId::new(organism, 0, 0);
    }
    cfg
}

/// Journal de undo da borda HTTP.
///
/// `AETHER_UNDO_JOURNAL` tem precedência (é o mesmo nome usado pela CLI);
/// sem ele, cai num arquivo **por processo** no diretório temporário para que
/// dois servidores não compartilhem *stage*/expectativa de undo.
fn undo_journal_path() -> PathBuf {
    match std::env::var("AETHER_UNDO_JOURNAL") {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path.trim()),
        _ => std::env::temp_dir().join(format!("aether_undo_http_{}.jsonl", std::process::id())),
    }
}

/// Teto de `n` em sat/qubo na rota do agente — pipeline **e** tarefa única.
///
/// Herdado do `PIPELINE_MAX_N` (MSG 0340) e mantido em 64: o expert SAT de
/// default aceita até 96, mas a borda HTTP resolve de forma **síncrona** com o
/// mutex de estado na mão; 64 é o maior `n` cujo custo cabe no orçamento da
/// rota e mantém `n×n` (QUBO) em 4096 floats — nada de alocação abusiva vinda
/// de `{"n":1000000}`.
const AGENT_MAX_N: usize = 64;

/// `true` se o corpo pede workflow multi-passo.
///
/// `pipeline`/`steps` só contam quando são array: exigir o tipo aqui evita que
/// `"pipeline": null` (ou lixo) num corpo de tarefa única caia na rota errada.
fn pipeline_requested(doc: &Json) -> Result<bool, String> {
    let mut found = false;
    for key in ["pipeline", "steps"] {
        match doc.get(key) {
            None | Some(Json::Null) => {}
            Some(v) if v.as_arr().is_some() => found = true,
            Some(_) => return Err(format!("`{key}` deve ser array de steps")),
        }
    }
    Ok(found)
}

fn pipeline_goal_from_json(doc: &Json) -> Result<Goal, String> {
    let steps = doc
        .get("pipeline")
        .and_then(|v| v.as_arr())
        .or_else(|| doc.get("steps").and_then(|v| v.as_arr()))
        .ok_or_else(|| "pipeline exige `steps` ou `pipeline` array".to_string())?;
    if steps.len() > PIPELINE_MAX_STEPS {
        return Err(format!(
            "pipeline acima de {PIPELINE_MAX_STEPS} steps (recebeu {})",
            steps.len()
        ));
    }
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
        // `act` grava na memória com before-image (`memory.write`): é o passo
        // que exercita o undo real do pipeline HTTP.
        "act" | "write" => {
            let subject = json
                .get("subject")
                .and_then(|v| v.as_str())
                .ok_or("act sem subject")?;
            let relation = json
                .get("relation")
                .and_then(|v| v.as_str())
                .ok_or("act sem relation")?;
            let value = json
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("act sem query")?;
            let mut goal = Goal::fact(subject, relation);
            goal.kind = GoalKind::Act;
            goal.query = Some(value.to_string());
            goal.success = SuccessCriteria::Claim {
                subject: subject.to_string(),
                relation: relation.to_string(),
                value: Some(value.to_string()),
            };
            // `memory.write` é risco Medium: sem teto Medium a pré-condição
            // nunca passa e o passo não teria efeito para desfazer.
            goal.risk_ceiling = Risk::Medium;
            goal
        }
        "sat" => {
            let n = json.get("n").and_then(|v| v.as_u64()).ok_or("sat sem n")? as usize;
            if n == 0 || n > AGENT_MAX_N {
                return Err(format!("sat n fora de 1..={AGENT_MAX_N}"));
            }
            let raw = raw_clauses(json.get("clauses"))?;
            let clauses = clauses_from_raw(&raw, n)?;
            Goal::satisfy(n, clauses)
        }
        "qubo" | "optimize" => {
            let n = json.get("n").and_then(|v| v.as_u64()).ok_or("qubo sem n")? as usize;
            if n == 0 || n > AGENT_MAX_N {
                return Err(format!("qubo n fora de 1..={AGENT_MAX_N}"));
            }
            let j = json
                .get("j")
                .and_then(|v| v.as_arr())
                .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
                .ok_or("qubo sem j")?;
            if j.len() != n * n {
                return Err(format!(
                    "qubo j deve ter {} elementos (n×n, recebeu {})",
                    n * n,
                    j.len()
                ));
            }
            let h = match json.get("h") {
                None | Some(Json::Null) => vec![0.0; n],
                Some(v) => {
                    let arr = v.as_arr().ok_or("qubo h deve ser array")?;
                    let h: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
                    if h.len() != n {
                        return Err(format!("qubo h deve ter {n} elementos"));
                    }
                    h
                }
            };
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
    let max_steps = json.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    Ok(Budget {
        max_steps: max_steps.clamp(1, PIPELINE_MAX_STEPS),
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

/// Literais crus de `clauses` (`[[1,2],[-3]]`), ainda sem padding.
///
/// Valida o shape **antes** de qualquer alocação derivada de `n`: um corpo
/// hostil não deve conseguir reservar memória só por declarar um `n`.
fn raw_clauses(value: Option<&Json>) -> Result<Vec<Vec<i64>>, String> {
    let arr = value
        .and_then(|v| v.as_arr())
        .ok_or("clauses deve ser array de arrays de literais")?;
    if arr.is_empty() {
        return Err("clauses vazio (nenhuma cláusula para resolver)".into());
    }
    let mut out = Vec::with_capacity(arr.len());
    for clause in arr {
        let items = clause
            .as_arr()
            .ok_or("cada clause deve ser array de literais")?;
        let ints: Vec<i64> = items
            .iter()
            .filter_map(|v| v.as_i64())
            .filter(|v| *v != 0)
            .collect();
        if ints.is_empty() || ints.len() > 3 {
            return Err("cada clause deve ter 1..=3 literais não nulos".into());
        }
        out.push(ints);
    }
    Ok(out)
}

/// Converte literais 1-based em [`Clause3`] exigindo `1..=n`.
///
/// O núcleo indexa `s[lit - 1]` em `Clause3::satisfied`; literal fora do
/// intervalo panicava (e envenenava o mutex do servidor), então a validação
/// acontece aqui, na borda, e vira 400.
fn clause_from_literals(ints: &[i64], n: usize) -> Result<Clause3, String> {
    let mut lits: Vec<(usize, i8)> = Vec::with_capacity(3);
    for v in ints {
        let idx = v.unsigned_abs() as usize;
        if idx == 0 || idx > n {
            return Err(format!("literal {v} fora de 1..={n} (n={n})"));
        }
        lits.push((idx - 1, if *v > 0 { 1 } else { -1 }));
    }
    let pad = *lits.last().ok_or("clause sem literais")?;
    while lits.len() < 3 {
        lits.push(pad);
    }
    Ok(Clause3::new3(
        lits[0].0, lits[0].1, lits[1].0, lits[1].1, lits[2].0, lits[2].1,
    ))
}

fn clauses_from_raw(raw: &[Vec<i64>], n: usize) -> Result<Vec<Clause3>, String> {
    raw.iter()
        .map(|ints| clause_from_literals(ints, n))
        .collect()
}

/// F5 T4 — rota de decisão do híbrido.
///
/// O port é evidência e degradação honesta, NUNCA um segundo decisor: com o
/// default `StubPort` a resposta abste com `engine_unavailable` e o app externo
/// recebe o schema tipado do crate. Nenhuma chamada externa por default
/// (Eixo C) e nenhum atalho para o Python — o engine real entra pelo mesmo
/// `DecisionPort` quando o daemon opta por ele.
fn decision_route(body: &[u8]) -> Reply {
    let req: DecisionRequest = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return bad(&format!("decision request inválido: {e}")),
    };
    // Eixo B: o estado é digest de certificado (≤320 tokens ≈ 1280 bytes),
    // nunca payload bruto. A borda impõe o teto antes de passar ao port.
    if req.state.len() > 320 * 4 {
        return bad("state acima do orçamento de 320 tokens");
    }
    if !req.questions.is_object() || req.questions.as_object().is_some_and(|o| o.is_empty()) {
        return bad("questions deve ser um objeto não vazio");
    }

    let port = StubPort;
    let resp = match port.decide(&req) {
        Ok(r) => r,
        Err(err) => {
            let answers: Vec<Answer> = req
                .questions
                .as_object()
                .map(|o| {
                    o.keys()
                        .map(|k| err.clone().into_abstain(k))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            DecisionResponse {
                answers,
                conformal_set: None,
                meta: DecisionMeta {
                    latency_ms: 0.0,
                    engine: "stub".into(),
                    degraded: true,
                    vram_used_mb: None,
                },
            }
        }
    };

    let bytes = match serde_json::to_vec(&resp) {
        Ok(v) => v,
        Err(e) => return bad(&format!("serialização da resposta: {e}")),
    };
    match parse_json(&bytes) {
        Ok(v) => Reply::Json(200, v),
        Err(e) => Reply::Json(e.0, e.1),
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

fn chat(state: &Mutex<ApiState>, sgdb: &Mutex<SgdbEdge>, body: &[u8], bound: Option<u64>) -> Reply {
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
    // R9: o trio (T-NL-IN/T-HITL/T-NL-OUT) abre o próprio cliente SGDB a
    // partir do env — o I/O dele NÃO pode acontecer sob o lock do estado.
    let id = {
        let mut st = lock_state(state);
        let id = format!("chatcmpl-aether-{}", st.requests);
        st.requests += 1;
        id
    };
    let mut evidence = evidence_for(state, sgdb, scope, &text);
    // T1: OOV do trio não pode esconder o ledger local — recall por fases do
    // texto: fases idênticas (texto exato) → cita; variação → abstém (a borda
    // é hash, não embedding). O corpo do cliente nunca promove autoridade.
    if evidence.get("abstain").and_then(Json::as_bool) == Some(true) {
        if let Some(cite) = ledger_cite(state, scope, &text) {
            evidence = cite;
        }
    }
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    let stream = doc.get("stream").and_then(Json::as_bool).unwrap_or(false);
    let content = evidence_content(&evidence);

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

fn completion(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    body: &[u8],
    bound: Option<u64>,
) -> Reply {
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
    let mut evidence = evidence_for(state, sgdb, scope, &text);
    // T1: OOV do trio não pode esconder o ledger local — recall por fases do
    // texto: fases idênticas (texto exato) → cita; variação → abstém (a borda
    // é hash, não embedding). O corpo do cliente nunca promove autoridade.
    if evidence.get("abstain").and_then(Json::as_bool) == Some(true) {
        if let Some(cite) = ledger_cite(state, scope, &text) {
            evidence = cite;
        }
    }
    let abstained = evidence
        .get("abstain")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    Reply::Json(
        200,
        Json::obj([
            ("content", Json::str(evidence_content(&evidence))),
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

/// `AETHER_SGDB_STDIO` no `.env` é comando solto; o cliente só aceita
/// `mock` | `http://…` | `stdio:cmd` (mesma regra do agente / MSG 1056).
fn normalize_sgdb_spec(spec: &str) -> String {
    let s = spec.trim();
    if s.is_empty() {
        return s.to_string();
    }
    if s == "mock"
        || s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("stdio:")
    {
        return s.to_string();
    }
    format!("stdio:{s}")
}

/// Evidência no fio comum T-NL-IN → MoE/coh → T-NL-OUT (P2) + learn/HITL.
///
/// Recicla `run_trio_turn` (parser + CertifiedUtterance + n-sgdb). O ledger de
/// fase continua disponível em rotas `/v1/aether/*`; o chat usa o barramento.
fn evidence_for(
    state: &Mutex<ApiState>,
    sgdb: &Mutex<SgdbEdge>,
    scope: ScopeId,
    text: &str,
) -> Json {
    use crate::agent::{run_trio_turn, TrioOpts};

    // R9/T5: o trio roda SEM o lock do estado (ele pode abrir transporte SGDB
    // e falar rede). Só consultamos se há SGDB configurado, via try_lock do
    // shard (ocupado = em uso = configurado).
    let has_sgdb = sgdb
        .try_lock()
        .map(|edge| edge.status() != "off")
        .unwrap_or(true);
    let raw = if has_sgdb {
        Some(
            std::env::var("AETHER_SGDB_STDIO")
                .or_else(|_| std::env::var("AETHER_SGDB_URL"))
                .unwrap_or_else(|_| "mock".into()),
        )
    } else {
        std::env::var("AETHER_SGDB_STDIO")
            .ok()
            .or_else(|| std::env::var("AETHER_SGDB_URL").ok())
    };
    let sgdb = raw
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_sgdb_spec(&s));

    let opts = TrioOpts {
        sgdb,
        agent_cfg: None,
        learn: true,
    };
    let turn = run_trio_turn(text, &opts);
    // Anexa escopo + content espelhado para a bolha / Console.
    match turn.evidence {
        Json::Obj(mut pairs) => {
            pairs.push(("scope".into(), scope_json(scope)));
            pairs.push(("content".into(), Json::str(&turn.content)));
            pairs.push(("edge".into(), Json::str("trio-bus")));
            // Compat: content da mensagem = prosa certificada ou “não sei”.
            // O chat usa evidence.to_text() — sobrescrevemos com content.
            let _ = state;
            Json::Obj(pairs)
        }
        other => other,
    }
}

/// Evidência do **ledger local** para OOV do trio (T1): converte o texto em
/// fases e faz `cue_scored` no escopo. Fases idênticas ⇒ score alto ⇒ cita a
/// `key`; variação de texto gera outras fases ⇒ abaixo de `MIN_SCORE` ⇒ `None`
/// (o trio abstém como antes). Autoridade da citação caps em quarentena — o
/// corpo do cliente nunca promove.
fn ledger_cite(state: &Mutex<ApiState>, scope: ScopeId, text: &str) -> Option<Json> {
    let query = text_to_phases(text);
    let mut out = vec![0.0f32; query.len()];
    let mut st = lock_state(state);
    let (hit, score) = st.ledger.cue_scored(scope, &query, &mut out).ok()?;
    if score < MIN_SCORE as f32 {
        return None;
    }
    let auth = hit.authority.min(AUTHORITY_QUARANTINE);
    Some(Json::obj([
        ("abstain", Json::Bool(false)),
        ("verdict", Json::str("cite")),
        ("reason", Json::str("ledger_exact_phases")),
        (
            "memory_citations",
            Json::Arr(vec![Json::obj([
                ("key", Json::str(hit.key.to_string())),
                ("score", Json::num(f64::from(score))),
                ("authority", Json::num(f64::from(auth))),
            ])]),
        ),
        ("certified", Json::Null),
        ("parse_oov", Json::Bool(true)),
        ("scope", scope_json(scope)),
        (
            "content",
            Json::str(format!(
                "citado do ledger local: key={} (score {:.2}, auth {auth})",
                hit.key.to_string(),
                score
            )),
        ),
        ("edge", Json::str("ledger-cite")),
    ]))
}

/// Conteúdo legível da evidência trio (prosa) — prefer `content` se presente.
fn evidence_content(evidence: &Json) -> String {
    if let Some(c) = evidence.get("content").and_then(Json::as_str) {
        return c.to_string();
    }
    evidence.to_text()
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
        // Forma plana: conferir o tamanho **antes** de reservar `rows × cols`
        // (um `n` enorme não deve virar alocação só porque o campo é array).
        if outer.len() != rows * cols {
            return None;
        }
        let mut out = Vec::with_capacity(rows * cols);
        for x in outer {
            out.push(check(x.as_f64()?)?);
        }
        return Some(out);
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

    #[test]
    fn config_do_pipeline_usa_o_organismo_do_token() {
        // O `bound` vem do token: o pipeline tem de rodar no organismo dele,
        // como `remember`/`cue` já fazem via `scope_or_bound`.
        assert_eq!(
            agent_pipeline_config(Some(7), None).scope,
            ScopeId::new(7, 0, 0)
        );
        let explicit = ScopeId::new(7, 2, 3);
        assert_eq!(
            agent_pipeline_config(Some(7), Some(explicit)).scope,
            explicit
        );
        // Sem auth (`bound = None`) o escopo default da config é preservado.
        assert_eq!(
            agent_pipeline_config(None, None).scope,
            AgentConfig::default().scope
        );
    }

    #[test]
    fn journal_de_undo_prefere_o_env() {
        let previous = std::env::var("AETHER_UNDO_JOURNAL").ok();
        std::env::set_var("AETHER_UNDO_JOURNAL", "target/test_undo_journal.jsonl");
        let path = undo_journal_path();
        match previous {
            Some(value) => std::env::set_var("AETHER_UNDO_JOURNAL", value),
            None => std::env::remove_var("AETHER_UNDO_JOURNAL"),
        }
        assert_eq!(path, PathBuf::from("target/test_undo_journal.jsonl"));
    }
}

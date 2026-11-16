//! Transportes do cliente `neural-sgdb`.
//!
//! O cliente fala **JSON-RPC 2.0** (`initialize` + `tools/call`) e não sabe qual
//! é o transporte. Três implementações cobrem o uso real e o teste:
//!
//! | Transporte | Quando | Rede |
//! |---|---|---|
//! | [`MockTransport`] | testes, demonstração, modo offline | nenhuma |
//! | [`StdioTransport`] | servidor MCP local lançado por comando | stdio |
//! | [`HttpTransport`] | servidor MCP já rodando | HTTP/1.1 |
//!
//! Nenhum deles entra no caminho quente: o `T2` roda em thread própria
//! (ver [`crate::knowledge::sgdb::worker`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use crate::json::Json;
use crate::knowledge::sgdb::wire::{parse_hit, MemoryHit, PhaseDoc};
use crate::knowledge::sgdb::SgdbError;

/// Limite de leitura de uma resposta HTTP (1 MiB).
pub const MAX_RESPONSE: usize = 1 << 20;

/// Protocolo MCP anunciado no `initialize`.
pub const MCP_PROTOCOL: &str = "2025-06-18";

/// Chamada JSON-RPC crua. O transporte só troca bytes; a semântica é do cliente.
pub trait Transport: std::fmt::Debug + Send {
    /// Envia uma requisição e devolve o `result` (erro JSON-RPC vira [`SgdbError`]).
    fn call(&mut self, request: &Json) -> Result<Json, SgdbError>;

    /// Descrição curta do destino (usada por `doctor`/CLI).
    fn describe(&self) -> String {
        "transporte".to_string()
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn call(&mut self, request: &Json) -> Result<Json, SgdbError> {
        (**self).call(request)
    }

    fn describe(&self) -> String {
        (**self).describe()
    }
}

/// Envelope de requisição (`id` já resolvido pelo cliente).
pub fn request(id: u64, method: &str, params: Json) -> Json {
    Json::obj([
        ("jsonrpc", Json::str("2.0")),
        ("id", Json::num(id as f64)),
        ("method", Json::str(method)),
        ("params", params),
    ])
}

/// Requisição `tools/call`.
pub fn tool_request(id: u64, name: &str, arguments: Json) -> Json {
    request(
        id,
        "tools/call",
        Json::obj([("name", Json::str(name)), ("arguments", arguments)]),
    )
}

/// Desembrulha `result`/`error` de uma resposta JSON-RPC.
pub fn unwrap_response(response: &Json) -> Result<Json, SgdbError> {
    if let Some(err) = response.get("error") {
        let message = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("erro JSON-RPC sem mensagem");
        let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        return Err(SgdbError::Rpc(format!("{message} (code {code})")));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| SgdbError::Protocol("resposta sem result/error".into()))
}

// ---------------------------------------------------------------------------
// Mock
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MockEntry {
    doc: PhaseDoc,
    storage_key: String,
    scope_app: String,
    scope_run: String,
    entities: Vec<String>,
    created_ms: u64,
    expires_at: Option<u64>,
    reinforced: u64,
}

/// Fato textual (não-fase) guardado pelo mock.
#[derive(Clone, Debug)]
struct MockText {
    text: String,
    storage_key: String,
    scope_app: String,
    scope_run: String,
    entities: Vec<String>,
    authority: u8,
    created_ms: u64,
    expires_at: Option<u64>,
    reinforced: u64,
}

/// Servidor `neural-sgdb` em memória, com a semântica que importa ao motor.
///
/// Implementa o subconjunto do contrato que o `T2` usa: ADD-only, escopo
/// (null-scoping), autoridade, TTL, `supersede` e auditoria. Serve de oráculo
/// para os gates e de fallback quando não há servidor.
#[derive(Debug)]
pub struct MockTransport {
    entries: Vec<MockEntry>,
    texts: Vec<MockText>,
    /// Clock do banco (ms) — controlável nos testes.
    now_ms: u64,
    /// `true` simula queda de rede (toda chamada falha).
    offline: bool,
    next_key: u64,
    calls: u64,
    recalls: u64,
    commits: u64,
    checkpoints: u64,
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTransport {
    /// Banco vazio com o relógio do sistema.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            texts: Vec::new(),
            now_ms: now_ms(),
            offline: false,
            next_key: 1,
            calls: 0,
            recalls: 0,
            commits: 0,
            checkpoints: 0,
        }
    }

    /// Congela o relógio do banco (TTL determinístico).
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Liga/desliga a rede simulada.
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    /// `true` se a última escrita simulada falhou.
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// Chamadas totais (auditoria de "quantas vezes o T2 falou com o banco").
    pub fn calls(&self) -> u64 {
        self.calls
    }

    /// Memórias vivas (TTL respeitado).
    pub fn live_len(&mut self) -> usize {
        self.expire();
        self.entries.len()
    }

    /// Fatos textuais vivos (não-fase).
    pub fn text_len(&mut self) -> usize {
        self.expire();
        self.texts.len()
    }

    /// Memórias de um escopo vivo (sem TTL aplicado ao resto).
    pub fn scope_len(&mut self, scope: crate::core::knowledge_bridge_f1::ScopeId) -> usize {
        self.expire();
        self.entries.iter().filter(|e| e.doc.scope == scope).count()
    }

    /// Documento armazenado para uma chave de conteúdo (auditoria de payload).
    pub fn stored(&mut self, key: u128) -> Option<&PhaseDoc> {
        self.expire();
        self.entries
            .iter()
            .find(|e| e.doc.key == key)
            .map(|e| &e.doc)
    }

    fn expire(&mut self) {
        let now = self.now_ms;
        self.entries
            .retain(|e| e.expires_at.map_or(true, |t| t > now));
        self.texts
            .retain(|t| t.expires_at.map_or(true, |expires| expires > now));
    }

    fn insert_text(
        &mut self,
        text: &str,
        scope_app: String,
        scope_run: String,
        entities: Vec<String>,
    ) -> String {
        let storage_key = format!("md/L3/aether/{:06x}", self.next_key);
        self.next_key += 1;
        self.texts.push(MockText {
            text: text.to_string(),
            storage_key: storage_key.clone(),
            scope_app,
            scope_run,
            entities,
            authority: 0,
            created_ms: self.now_ms,
            expires_at: None,
            reinforced: 0,
        });
        storage_key
    }

    fn remember(&mut self, args: &Json) -> Result<Json, SgdbError> {
        let mut items: Vec<Json> = Vec::new();
        if let Some(memories) = args.get("memories").and_then(|m| m.as_arr()) {
            items.extend_from_slice(memories);
        } else if args.get("text").is_some() {
            items.push(args.clone());
        }
        if items.len() > 64 {
            return Err(SgdbError::Protocol("remember acima de 64 memórias".into()));
        }
        let mut keys = Vec::with_capacity(items.len());
        for item in items {
            let text = item
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SgdbError::Protocol("remember sem text".into()))?;
            let scope_app = item
                .get("scope_app")
                .and_then(|v| v.as_str())
                .unwrap_or("AETHER")
                .to_string();
            let scope_run = item
                .get("scope_run")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let entities: Vec<String> = item
                .get("entities")
                .and_then(|v| v.as_arr())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let if_exists = item
                .get("if_exists")
                .and_then(|v| v.as_str())
                .unwrap_or("add");
            let Some(doc) = PhaseDoc::parse(text) else {
                // Fato textual genérico (L3/L4 por tipo): também faz parte do
                // contrato; o motor não o consome, mas o operador sim.
                let existing = self.texts.iter().position(|t| {
                    t.scope_app == scope_app && t.scope_run == scope_run && t.text == text
                });
                let storage_key = match (existing, if_exists) {
                    (Some(pos), "reinforce") => {
                        let key = self.texts[pos].storage_key.clone();
                        self.texts[pos].reinforced += 1;
                        self.texts[pos].created_ms = self.now_ms;
                        key
                    }
                    (Some(pos), "reject") => self.texts[pos].storage_key.clone(),
                    (Some(pos), _) => {
                        self.texts.remove(pos);
                        self.insert_text(text, scope_app, scope_run, entities)
                    }
                    (None, _) => self.insert_text(text, scope_app, scope_run, entities),
                };
                keys.push(storage_key);
                continue;
            };
            let existing = self
                .entries
                .iter()
                .position(|e| e.doc.scope == doc.scope && e.doc.key == doc.key);
            let storage_key = match (existing, if_exists) {
                (Some(pos), "reinforce") => {
                    let key = self.entries[pos].storage_key.clone();
                    self.entries[pos].reinforced += 1;
                    self.entries[pos].created_ms = self.now_ms;
                    key
                }
                (Some(pos), "reject") => self.entries[pos].storage_key.clone(),
                (Some(pos), "supersede") => {
                    // O antigo sai (no banco real, o histórico fica no audit).
                    self.entries.remove(pos);
                    self.insert(doc, scope_app, scope_run, entities)
                }
                // `add`: doutrina ADD-only — o novo entra ao lado do antigo.
                (Some(_), _) => self.insert(doc, scope_app, scope_run, entities),
                (None, _) => self.insert(doc, scope_app, scope_run, entities),
            };
            keys.push(storage_key);
        }
        Ok(Json::obj([
            ("status", Json::str("ok")),
            ("remembered", Json::num(keys.len() as f64)),
            ("keys", Json::Arr(keys.into_iter().map(Json::Str).collect())),
        ]))
    }

    fn insert(
        &mut self,
        doc: PhaseDoc,
        scope_app: String,
        scope_run: String,
        entities: Vec<String>,
    ) -> String {
        let storage_key = format!("md/L4/aether/{:06x}", self.next_key);
        self.next_key += 1;
        self.entries.push(MockEntry {
            doc,
            storage_key: storage_key.clone(),
            scope_app,
            scope_run,
            entities,
            created_ms: self.now_ms,
            expires_at: None,
            reinforced: 0,
        });
        storage_key
    }

    fn recall(&mut self, args: &Json) -> Result<Json, SgdbError> {
        self.expire();
        self.recalls += 1;
        let scope_run = args.get("scope_run").and_then(|v| v.as_str()).unwrap_or("");
        let scope_app = args.get("scope_app").and_then(|v| v.as_str());
        let entities: Vec<&str> = args
            .get("entities")
            .and_then(|v| v.as_arr())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, 20) as usize;
        let mut hits: Vec<(f32, Json)> = Vec::new();
        for entry in self.entries.iter() {
            // Null-scoping: sem `scope_app` só memórias globais; com ele, só
            // as do mesmo app e do mesmo run.
            match scope_app {
                Some(app) if entry.scope_app == app => {}
                _ => continue,
            }
            if !scope_run.is_empty() && entry.scope_run != scope_run {
                continue;
            }
            if !entities.is_empty()
                && !entities
                    .iter()
                    .any(|e| entry.entities.iter().any(|x| x == e))
            {
                continue;
            }
            let age = self.now_ms.saturating_sub(entry.created_ms) as f32;
            let score =
                (score_text(&entry.doc.to_text(), &query) + 1.0 / (1.0 + age / 60_000.0) * 1e-3)
                    .min(1.0);
            hits.push((
                score,
                Json::obj([
                    (
                        "key",
                        Json::str(entry.storage_key.clone()),
                    ),
                    ("text", Json::str(entry.doc.to_text())),
                    ("score", Json::num(f64::from(score))),
                    ("authority", Json::num(f64::from(entry.doc.authority))),
                    ("created_tick", Json::num(entry.doc.wall_ms as f64)),
                ]),
            ));
        }
        for text in self.texts.iter() {
            match scope_app {
                Some(app) if text.scope_app == app => {}
                _ => continue,
            }
            if !scope_run.is_empty() && text.scope_run != scope_run {
                continue;
            }
            if !entities.is_empty()
                && !entities
                    .iter()
                    .any(|e| text.entities.iter().any(|x| x == e))
            {
                continue;
            }
            let age = self.now_ms.saturating_sub(text.created_ms) as f32;
            let score =
                (score_text(&text.text, &query) + 1.0 / (1.0 + age / 60_000.0) * 1e-3).min(1.0);
            hits.push((
                score,
                Json::obj([
                    ("key", Json::str(text.storage_key.clone())),
                    ("text", Json::str(text.text.clone())),
                    ("score", Json::num(f64::from(score))),
                    ("authority", Json::num(f64::from(text.authority))),
                    ("created_tick", Json::num(text.created_ms as f64)),
                ]),
            ));
        }
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        let total = hits.len();
        let start = args
            .get("cursor")
            .and_then(|v| v.as_str())
            .and_then(|c| c.parse::<usize>().ok())
            .unwrap_or(0);
        let page: Vec<&(f32, Json)> = hits.iter().skip(start).take(k).collect();
        let next = (start + page.len() < total).then(|| (start + page.len()).to_string());
        let json_hits: Vec<Json> = page.iter().map(|(_, json)| (*json).clone()).collect();
        let mut result = vec![
            ("status".to_string(), Json::str("ok")),
            ("hit_count".to_string(), Json::num(total as f64)),
            ("hits".to_string(), Json::Arr(json_hits)),
        ];
        if let Some(next) = next {
            result.push(("cursor".to_string(), Json::Str(next)));
        }
        Ok(Json::Obj(result))
    }

    fn curate(&mut self, args: &Json) -> Result<Json, SgdbError> {
        let op = args
            .get("op")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SgdbError::Protocol("curate sem op".into()))?;
        match op {
            "commit_run" => {
                self.commits += 1;
                self.expire();
                Ok(Json::obj([
                    ("status", Json::str("ok")),
                    ("op", Json::str("commit_run")),
                    ("committed", Json::num((self.entries.len() + self.texts.len()) as f64)),
                ]))
            }
            "audit_checkpoint" => {
                self.checkpoints += 1;
                let mut h: u128 = 0xcbf2_9ce4_8422_2325;
                for entry in self.entries.iter() {
                    for b in entry.doc.payload.iter() {
                        h ^= u128::from(*b);
                        h = h.wrapping_mul(0x100_0000_01b3);
                    }
                }
                for text in self.texts.iter() {
                    for b in text.text.as_bytes() {
                        h ^= u128::from(*b);
                        h = h.wrapping_mul(0x100_0000_01b3);
                    }
                }
                Ok(Json::obj([
                    ("status", Json::str("ok")),
                    ("op", Json::str("audit_checkpoint")),
                    ("checkpoint", Json::str(format!("{h:032x}"))),
                ]))
            }
            "set_ttl" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SgdbError::Protocol("set_ttl sem key".into()))?;
                let ttl_ms = args
                    .get("ttl_episodic_ms")
                    .or_else(|| args.get("ttl_ms"))
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| SgdbError::Protocol("set_ttl sem ttl".into()))?;
                let expires = self.now_ms.saturating_add(ttl_ms);
                let mut found = false;
                for entry in self.entries.iter_mut() {
                    if entry.storage_key == key {
                        entry.expires_at = Some(expires);
                        found = true;
                        break;
                    }
                }
                if !found {
                    for text in self.texts.iter_mut() {
                        if text.storage_key == key {
                            text.expires_at = Some(expires);
                            found = true;
                            break;
                        }
                    }
                }
                Ok(Json::obj([
                    ("status", Json::str(if found { "ok" } else { "missing" })),
                    ("expires_at", Json::num(expires as f64)),
                ]))
            }
            "set_authority" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SgdbError::Protocol("set_authority sem key".into()))?;
                let authority = args
                    .get("authority")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    .min(255) as u8;
                for entry in self.entries.iter_mut() {
                    if entry.storage_key == key {
                        entry.doc.authority = authority;
                    }
                }
                for text in self.texts.iter_mut() {
                    if text.storage_key == key {
                        text.authority = authority;
                    }
                }
                Ok(Json::obj([
                    ("status", Json::str("ok")),
                    ("authority", Json::num(f64::from(authority))),
                ]))
            }
            "forget_purge" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SgdbError::Protocol("forget_purge sem key".into()))?;
                let before = self.entries.len() + self.texts.len();
                self.entries.retain(|e| e.storage_key != key);
                self.texts.retain(|t| t.storage_key != key);
                Ok(Json::obj([
                    ("status", Json::str("ok")),
                    (
                        "purged",
                        Json::num((before - self.entries.len() - self.texts.len()) as f64),
                    ),
                ]))
            }
            "supersede" => {
                let old = args
                    .get("old")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SgdbError::Protocol("supersede sem old".into()))?;
                self.entries.retain(|e| e.storage_key != old);
                self.texts.retain(|t| t.storage_key != old);
                Ok(Json::obj([("status", Json::str("ok"))]))
            }
            other => Err(SgdbError::Tool(format!("curate op desconhecida: {other}"))),
        }
    }
}

impl Transport for MockTransport {
    fn call(&mut self, request: &Json) -> Result<Json, SgdbError> {
        self.calls += 1;
        if self.offline {
            return Err(SgdbError::Offline);
        }
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SgdbError::Protocol("requisição sem method".into()))?;
        match method {
            "initialize" => Ok(Json::obj([
                ("protocolVersion", Json::str(MCP_PROTOCOL)),
                ("serverInfo", Json::str("mock-neural-sgdb")),
            ])),
            "tools/call" => {
                let params = request
                    .get("params")
                    .ok_or_else(|| SgdbError::Protocol("tools/call sem params".into()))?;
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SgdbError::Protocol("tools/call sem name".into()))?;
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(Json::Obj(Vec::new()));
                match name {
                    "remember" => self.remember(&args),
                    "recall" => self.recall(&args),
                    "curate" => self.curate(&args),
                    "health" => Ok(Json::obj([
                        ("status", Json::str("ok")),
                        ("backend", Json::str("mock")),
                    ])),
                    other => Err(SgdbError::Tool(format!("ferramenta desconhecida: {other}"))),
                }
            }
            other => Err(SgdbError::Protocol(format!("método sem resposta: {other}"))),
        }
    }

    fn describe(&self) -> String {
        "mock (memória; não fala rede)".to_string()
    }
}

impl MockTransport {
    /// Converte hits crus em [`MemoryHit`] (atalho para testes/gates).
    pub fn decode_hits(result: &Json) -> Vec<MemoryHit> {
        crate::knowledge::sgdb::wire::hits_json(result)
            .iter()
            .filter_map(parse_hit)
            .collect()
    }

    /// Interpreta o `scope` textual de um documento (para asserções).
    pub fn doc_scope(text: &str) -> Option<crate::core::knowledge_bridge_f1::ScopeId> {
        PhaseDoc::parse(text).map(|doc| doc.scope)
    }
}

// ---------------------------------------------------------------------------
// HTTP (MCP Streamable HTTP, sem dependência)
// ---------------------------------------------------------------------------

/// Endereço HTTP já validado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl HttpEndpoint {
    /// Parse de `http://host[:porta][/caminho]`.
    pub fn parse(url: &str) -> Result<Self, SgdbError> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| SgdbError::NotSupported(format!("apenas http:// (veio {url})")))?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse::<u16>()
                    .map_err(|_| SgdbError::Config(format!("porta inválida: {p}")))?,
            ),
            None => (authority.to_string(), 80),
        };
        if host.is_empty() {
            return Err(SgdbError::Config(format!("host vazio em {url}")));
        }
        Ok(Self {
            host,
            port,
            path: path.to_string(),
        })
    }
}

/// Cliente MCP sobre HTTP/1.1 (`POST` JSON, resposta JSON ou SSE).
#[derive(Debug)]
pub struct HttpTransport {
    endpoint: HttpEndpoint,
    token: Option<String>,
    session: Option<String>,
    timeout: Duration,
    next_id: u64,
    calls: u64,
}

impl HttpTransport {
    /// Cria o transporte para o endpoint informado.
    pub fn new(url: &str) -> Result<Self, SgdbError> {
        Ok(Self {
            endpoint: HttpEndpoint::parse(url)?,
            token: None,
            session: None,
            timeout: Duration::from_millis(1500),
            next_id: 1,
            calls: 0,
        })
    }

    /// Chamadas HTTP feitas (diagnóstico).
    pub fn calls(&self) -> u64 {
        self.calls
    }

    /// Token `Authorization: Bearer` opcional.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Timeout de conexão/leitura.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn post(&mut self, body: &str) -> Result<(String, String), SgdbError> {
        let addr = format!("{}:{}", self.endpoint.host, self.endpoint.port);
        let socket = addr
            .to_socket_addrs()
            .map_err(|_| SgdbError::Offline)?
            .next()
            .ok_or(SgdbError::Offline)?;
        let mut stream = TcpStream::connect_timeout(&socket, self.timeout)
            .map_err(|_| SgdbError::Offline)?;
        let _ = stream.set_read_timeout(Some(self.timeout));
        let _ = stream.set_write_timeout(Some(self.timeout));
        let mut headers = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\
             Connection: close\r\n",
            self.endpoint.path,
            self.endpoint.host,
            body.len()
        );
        if let Some(token) = &self.token {
            headers.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if let Some(session) = &self.session {
            headers.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
        }
        headers.push_str("\r\n");
        stream
            .write_all(headers.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .map_err(|_| SgdbError::Offline)?;
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    head.push_str(&line);
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }
                Err(_) => return Err(SgdbError::Timeout),
            }
        }
        if head.is_empty() {
            return Err(SgdbError::Offline);
        }
        if !head.starts_with("HTTP/1.1 2") && !head.starts_with("HTTP/1.0 2") {
            let status = head.lines().next().unwrap_or("HTTP ?");
            return Err(SgdbError::Protocol(format!("HTTP {status}")));
        }
        if let Some(value) = header_value(&head, "mcp-session-id") {
            self.session = Some(value);
        }
        let lower = head.to_ascii_lowercase();
        let length = header_value(&head, "content-length").and_then(|v| v.parse::<usize>().ok());
        let mut bytes = Vec::new();
        if lower.contains("transfer-encoding: chunked") {
            loop {
                let mut size_line = String::new();
                if reader
                    .read_line(&mut size_line)
                    .map_err(|_| SgdbError::Timeout)?
                    == 0
                {
                    break;
                }
                let size_text = size_line.split(';').next().unwrap_or("").trim();
                let size = usize::from_str_radix(size_text, 16).unwrap_or(0);
                if size == 0 {
                    break;
                }
                let mut chunk = vec![0u8; size];
                reader
                    .read_exact(&mut chunk)
                    .map_err(|_| SgdbError::Timeout)?;
                bytes.extend_from_slice(&chunk);
                let mut crlf = [0u8; 2];
                let _ = reader.read_exact(&mut crlf);
                if bytes.len() > MAX_RESPONSE {
                    return Err(SgdbError::Protocol("resposta acima de MAX_RESPONSE".into()));
                }
            }
        } else if let Some(length) = length {
            if length > MAX_RESPONSE {
                return Err(SgdbError::Protocol("resposta acima de MAX_RESPONSE".into()));
            }
            bytes.resize(length, 0);
            reader
                .read_exact(&mut bytes)
                .map_err(|_| SgdbError::Timeout)?;
        } else {
            // SSE/streaming sem `Content-Length`: aceita o que chegar até o
            // timeout (o MCP manda o evento e mantém a conexão aberta).
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes.extend_from_slice(&buf[..n]);
                        if bytes.len() >= MAX_RESPONSE {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        let content = String::from_utf8_lossy(&bytes).to_string();
        // SSE: pega a última linha `data:` com JSON.
        let content = if content.trim_start().starts_with("data:") {
            content
                .lines()
                .rev()
                .find_map(|line| {
                    let payload = line.strip_prefix("data:")?.trim();
                    payload.starts_with('{').then_some(payload)
                })
                .unwrap_or("")
                .to_string()
        } else {
            content
        };
        Ok((content, head))
    }
}

/// Valor de um cabeçalho HTTP (case-insensitive).
fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

/// Score lexical do mock: fração dos tokens da consulta presentes no texto.
fn score_text(text: &str, query: &str) -> f32 {
    if query.is_empty() {
        return 1.0;
    }
    let text = text.to_lowercase();
    let tokens: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .collect();
    if tokens.is_empty() {
        return 1.0;
    }
    let matched = tokens.iter().filter(|t| text.contains(**t)).count();
    matched as f32 / tokens.len() as f32
}

impl Transport for HttpTransport {
    fn call(&mut self, request: &Json) -> Result<Json, SgdbError> {
        self.calls += 1;
        let id = self.next_id;
        let body = request.to_text();
        let (content, _) = self.post(&body)?;
        let response = Json::parse(&content)
            .map_err(|e| SgdbError::Protocol(format!("JSON inválido: {e}")))?;
        // Alguns servidores só abrem sessão depois do initialize; o handshake
        // explícito é feito pelo cliente antes da primeira ferramenta.
        let _ = id;
        unwrap_response(&response)
    }

    fn describe(&self) -> String {
        format!(
            "http://{}:{}{}",
            self.endpoint.host, self.endpoint.port, self.endpoint.path
        )
    }
}

// ---------------------------------------------------------------------------
// Stdio (MCP local, JSON por linha)
// ---------------------------------------------------------------------------

/// Cliente MCP que lança o servidor como processo filho.
#[derive(Debug)]
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    calls: u64,
}

impl StdioTransport {
    /// Lança `program` com `args` e conecta stdin/stdout.
    pub fn spawn(program: &str, args: &[String]) -> Result<Self, SgdbError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| SgdbError::Config(format!("não lançou {program}: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SgdbError::Protocol("stdin indisponível".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SgdbError::Protocol("stdout indisponível".into()))?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            calls: 0,
        })
    }

    /// PID do servidor (diagnóstico).
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Chamadas feitas (diagnóstico).
    pub fn calls(&self) -> u64 {
        self.calls
    }
}

impl Transport for StdioTransport {
    fn call(&mut self, request: &Json) -> Result<Json, SgdbError> {
        self.calls += 1;
        let mut line = request.to_text();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|_| SgdbError::Offline)?;
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = self
                .stdout
                .read_line(&mut buf)
                .map_err(|_| SgdbError::Timeout)?;
            if n == 0 {
                return Err(SgdbError::Offline);
            }
            let trimmed = buf.trim();
            if trimmed.is_empty() || !trimmed.starts_with('{') {
                continue;
            }
            let response = Json::parse(trimmed)
                .map_err(|e| SgdbError::Protocol(format!("JSON inválido: {e}")))?;
            // Respostas de notificação não têm `id`; ignora.
            if response.get("id").is_none() {
                continue;
            }
            return unwrap_response(&response);
        }
    }

    fn describe(&self) -> String {
        format!("stdio (pid {})", self.child.id())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Relógio de parede em ms (base do `created_tick`/TTL).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Outbox (spool local)
// ---------------------------------------------------------------------------

/// Diário append-only de documentos pendentes (offline → replay).
///
/// O arquivo fica **fora** do caminho quente: só o `T2` escreve. Se o SGDB
/// cair, o fato não se perde; ele espera em disco com o payload íntegro.
#[derive(Clone, Debug)]
pub struct Outbox {
    path: std::path::PathBuf,
}

impl Outbox {
    /// Abre (ou cria) o diário no caminho.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Caminho do diário.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Anexa documentos (uma linha JSON por documento).
    pub fn append(&self, docs: &[PhaseDoc]) -> Result<(), SgdbError> {
        if docs.is_empty() {
            return Ok(());
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| SgdbError::Io(e.to_string()))?;
        for doc in docs {
            writeln!(file, "{}", doc.to_text()).map_err(|e| SgdbError::Io(e.to_string()))?;
        }
        file.flush().map_err(|e| SgdbError::Io(e.to_string()))
    }

    /// Quantas linhas ainda estão pendentes.
    pub fn len(&self) -> usize {
        std::fs::read_to_string(&self.path)
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    /// `true` se não há pendências.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lê os documentos pendentes (linhas inválidas são descartadas).
    pub fn read(&self) -> Vec<PhaseDoc> {
        std::fs::read_to_string(&self.path)
            .map(|text| text.lines().filter_map(PhaseDoc::parse).collect())
            .unwrap_or_default()
    }

    /// Reescreve o diário só com o que ainda não foi entregue.
    pub fn rewrite(&self, pending: &[PhaseDoc]) -> Result<(), SgdbError> {
        if pending.is_empty() {
            let _ = std::fs::remove_file(&self.path);
            return Ok(());
        }
        let mut out = String::with_capacity(pending.len() * 512);
        for doc in pending {
            out.push_str(&doc.to_text());
            out.push('\n');
        }
        std::fs::write(&self.path, out).map_err(|e| SgdbError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_http_valido_e_invalido() {
        let e = HttpEndpoint::parse("http://127.0.0.1:8090/mcp").unwrap();
        assert_eq!(e.host, "127.0.0.1");
        assert_eq!(e.port, 8090);
        assert_eq!(e.path, "/mcp");
        assert!(HttpEndpoint::parse("https://x/y").is_err());
        assert!(HttpEndpoint::parse("http:///x").is_err());
    }

    #[test]
    fn http_transport_le_chunked() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let addr = listener.local_addr().expect("addr");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).expect("body");
            let payload = br#"{"jsonrpc":"2.0","id":1,"result":{"status":"chunked"}}"#;
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
                payload.len()
            )
            .expect("head");
            stream.write_all(payload).expect("chunk");
            stream.write_all(b"\r\n0\r\n\r\n").expect("end");
        });
        let mut transport = HttpTransport::new(&format!("http://{addr}/mcp")).expect("transporte");
        let result = transport
            .call(&tool_request(1, "health", Json::obj([])))
            .expect("result");
        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("chunked")
        );
        server.join().expect("join");
    }

    #[test]
    fn mock_offline_falha_e_volta() {
        let mut mock = MockTransport::new();
        mock.set_offline(true);
        let req = tool_request(1, "health", Json::Obj(Vec::new()));
        assert!(matches!(mock.call(&req), Err(SgdbError::Offline)));
        mock.set_offline(false);
        assert!(mock.call(&req).is_ok());
        assert_eq!(mock.calls(), 2);
    }

    #[test]
    fn http_transport_le_resultado_e_sessao() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let addr = listener.local_addr().expect("addr");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line == "\r\n" || line == "\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                request.push_str(&line);
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).expect("body");
            request.push_str(&String::from_utf8_lossy(&body));
            assert!(request.contains("\"jsonrpc\":\"2.0\""));
            assert!(request.contains("\"method\":\"tools/call\""));
            let response_body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"ok"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                 Mcp-Session-Id: sess-1\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let mut stream = stream;
            stream.write_all(response.as_bytes()).expect("write");
        });
        let mut transport = HttpTransport::new(&format!("http://{addr}/mcp")).expect("transporte");
        let result = transport
            .call(&tool_request(1, "health", Json::obj([])))
            .expect("result");
        assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("ok"));
        assert_eq!(transport.session.as_deref(), Some("sess-1"));
        server.join().expect("join");
    }
}

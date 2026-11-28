//! Cliente `neural-sgdb`: MCP + mapeamento do fato de fase.
//!
//! O cliente é **síncrono e de borda** (`T2`). Ele não decide o que entra no
//! prompt: devolve hits tipados (`MemoryHit`) e evidência (`Evidence`) para que
//! o host aplique a matriz 2D do Vol. III §6.
//!
//! # Escopo (null-scoping)
//!
//! Toda escrita e toda leitura levam `scope_app` **e** `scope_run`
//! (`org:<organism>/run:<run>`). Sem `scope_app`, o SGDB só devolve memórias
//! globais — a leitura nunca "vê" outro organismo por acidente. O `app_tag`
//! viaja na entidade de escopo (`aether/scope/<org>/<run>/<app>`).

use std::path::PathBuf;
use std::time::Duration;

use crate::core::certificate::{Evidence, EvidenceSource};
use crate::core::knowledge_bridge_f1::{DraftRecord, ScopeId};
use crate::json::Json;
use crate::knowledge::sgdb::transport::{
    self, tool_request, HttpTransport, MockTransport, StdioTransport, Transport,
};
use crate::knowledge::sgdb::wire::{
    self, hits_json, parse_hit, MemoryHit, PhaseDoc, RememberConfig, ENTITY_PHASE, MAX_BATCH,
};
use crate::knowledge::sgdb::SgdbError;

/// Configuração do cliente.
#[derive(Clone, Debug)]
pub struct SgdbConfig {
    /// `scope_app` do banco (default `AETHER`).
    pub scope_app: String,
    /// `scope_agent` do banco (default `aether-engine`).
    pub scope_agent: String,
    /// Política de escrita: `add` (ADD-only puro) ou `reinforce` (default).
    pub if_exists: String,
    /// Prefixo de chave crua. `None` = o banco gera `md/L3/<key>`.
    pub key_prefix: Option<String>,
    /// TTL aplicado após cada escrita (`None` = sem TTL).
    pub ttl_ms: Option<u64>,
    /// Teto de memórias por `remember` (≤ 64).
    pub max_batch: usize,
    /// Emite `audit_checkpoint` no fim de cada commit.
    pub audit: bool,
    /// Diário local para replay quando o banco está fora.
    pub outbox: Option<PathBuf>,
    /// Hits padrão de leitura por consulta.
    pub recall_k: usize,
    /// Tentativas extras em erro transitório (default 2).
    pub retries: u32,
    /// Base do backoff exponencial em ms (default 100; teto de 2 s).
    pub retry_backoff_ms: u64,
    /// Timeout de conexão/leitura do transporte HTTP em ms (default 2000).
    pub timeout_ms: u64,
    /// Token `Authorization: Bearer` (também lido de `AETHER_SGDB_TOKEN`).
    pub token: Option<String>,
}

impl Default for SgdbConfig {
    fn default() -> Self {
        Self {
            scope_app: "AETHER".to_string(),
            scope_agent: "aether-engine".to_string(),
            if_exists: "reinforce".to_string(),
            key_prefix: None,
            ttl_ms: None,
            max_batch: MAX_BATCH,
            audit: false,
            outbox: None,
            recall_k: 8,
            retries: 2,
            retry_backoff_ms: 100,
            timeout_ms: 2_000,
            token: None,
        }
    }
}

impl SgdbConfig {
    /// Configuração com diário local (o modo durável recomendado).
    pub fn durable(outbox: impl Into<PathBuf>) -> Self {
        Self {
            outbox: Some(outbox.into()),
            ..Self::default()
        }
    }

    /// Lê os ajustes de ambiente (`AETHER_SGDB_*`), ignorando valores inválidos.
    ///
    /// | Variável | Campo |
    /// |---|---|
    /// | `AETHER_SGDB_SCOPE_APP` | [`Self::scope_app`] |
    /// | `AETHER_SGDB_SCOPE_AGENT` | [`Self::scope_agent`] |
    /// | `AETHER_SGDB_IF_EXISTS` | [`Self::if_exists`] |
    /// | `AETHER_SGDB_OUTBOX` | [`Self::outbox`] |
    /// | `AETHER_SGDB_TTL_MS` | [`Self::ttl_ms`] |
    /// | `AETHER_SGDB_RECALL_K` | [`Self::recall_k`] |
    /// | `AETHER_SGDB_RETRIES` | [`Self::retries`] |
    /// | `AETHER_SGDB_TIMEOUT_MS` | [`Self::timeout_ms`] |
    /// | `AETHER_SGDB_TOKEN` | [`Self::token`] |
    /// | `AETHER_SGDB_AUDIT` (`1`/`true`) | [`Self::audit`] |
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Some(v) = env_nonempty("AETHER_SGDB_SCOPE_APP") {
            cfg.scope_app = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_SCOPE_AGENT") {
            cfg.scope_agent = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_IF_EXISTS") {
            if matches!(v.as_str(), "add" | "reinforce" | "supersede" | "reject") {
                cfg.if_exists = v;
            }
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_OUTBOX") {
            cfg.outbox = Some(PathBuf::from(v));
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_TTL_MS").and_then(|v| v.parse().ok()) {
            cfg.ttl_ms = Some(v);
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_RECALL_K").and_then(|v| v.parse().ok()) {
            cfg.recall_k = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_RETRIES").and_then(|v| v.parse().ok()) {
            cfg.retries = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_TIMEOUT_MS").and_then(|v| v.parse().ok()) {
            cfg.timeout_ms = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_RETRY_BACKOFF_MS").and_then(|v| v.parse().ok()) {
            cfg.retry_backoff_ms = v;
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_TOKEN") {
            cfg.token = Some(v);
        }
        if let Some(v) = env_nonempty("AETHER_SGDB_AUDIT") {
            cfg.audit = matches!(v.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        cfg
    }

    /// Timeout já como [`Duration`].
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.max(1))
    }

    fn remember<'a>(&'a self) -> RememberConfig<'a> {
        RememberConfig {
            scope_app: &self.scope_app,
            scope_agent: &self.scope_agent,
            if_exists: &self.if_exists,
            key_prefix: self.key_prefix.as_deref(),
        }
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Primeira chave de armazenamento devolvida por `remember`.
fn storage_key_of(result: &Json) -> Option<String> {
    let payload = wire::payload_json(result);
    let from = |json: &Json| {
        json.get("storage_key")
            .or_else(|| json.get("key"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                json.get("keys")
                    .and_then(|v| v.as_arr())
                    .and_then(|keys| keys.first())
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
    };
    from(&payload)
        .or_else(|| result.get("structuredContent").and_then(from))
        .or_else(|| from(result))
}

/// Abre o transporte a partir de uma especificação.
///
/// ```text
/// mock
/// http://host:porta/mcp
/// stdio:programa args...
/// ```
///
/// Aplica timeout e token da configuração ao HTTP.
pub fn open_transport(spec: &str, cfg: &SgdbConfig) -> Result<Box<dyn Transport>, SgdbError> {
    if spec == "mock" {
        return Ok(Box::new(MockTransport::new()));
    }
    if spec.starts_with("http://") {
        let mut transport = HttpTransport::new(spec)?.with_timeout(cfg.timeout());
        if let Some(token) = &cfg.token {
            transport = transport.with_token(token.clone());
        }
        return Ok(Box::new(transport));
    }
    if let Some(command) = spec.strip_prefix("stdio:") {
        let mut parts = command.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| SgdbError::Config("stdio: exige um programa".into()))?;
        let args: Vec<String> = parts.map(str::to_string).collect();
        guard_stdio_spawn(program, &args)?;
        let transport = StdioTransport::spawn(program, &args)?;
        return Ok(Box::new(transport));
    }
    Err(SgdbError::Config(format!(
        "especificação desconhecida: {spec} (use mock, http://…/mcp ou stdio:programa args)"
    )))
}

/// Guarda de spawn stdio (T2). O env é config confiável, mas a invariante
/// "sem shell" (W1/W8) vale aqui também:
/// * shells duros (`sh`/`bash`/`dash`/`zsh`/`ksh`/`pwsh`/`cmd`) **nunca**;
/// * `powershell` **só** no shape do launcher documentado:
///   `-NoProfile -File <caminho.ps1>` (sem `-Command`/`-EncodedCommand`/
///   `-ExecutionPolicy` — nada de string de comando).
fn guard_stdio_spawn(program: &str, args: &[String]) -> Result<(), SgdbError> {
    let lower = program.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    if matches!(
        stem,
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "pwsh" | "cmd"
    ) {
        return Err(SgdbError::Config(format!(
            "stdio: shell não permitido: {program} (use binário concreto)"
        )));
    }
    if stem == "powershell" {
        let shape_ok = args.len() == 3
            && args[0].eq_ignore_ascii_case("-noprofile")
            && args[1].eq_ignore_ascii_case("-file")
            && args[2].to_ascii_lowercase().ends_with(".ps1");
        if !shape_ok {
            return Err(SgdbError::Config(
                "stdio: powershell só aceita `-NoProfile -File <caminho.ps1>` (sem -Command/-ExecutionPolicy)"
                    .into(),
            ));
        }
    }
    Ok(())
}

/// Abre um cliente a partir de uma especificação de transporte.
pub fn open(spec: &str, cfg: SgdbConfig) -> Result<SgdbClient<Box<dyn Transport>>, SgdbError> {
    let transport = open_transport(spec, &cfg)?;
    Ok(SgdbClient::new(transport, cfg))
}

/// Página de `recall` (cursor do MCP).
#[derive(Clone, Debug, Default)]
pub struct RecallPage {
    /// Hits decodificados (fase de documentos `aether-phase/v1`).
    pub hits: Vec<MemoryHit>,
    /// Cursor da próxima página (`None` = fim).
    pub cursor: Option<String>,
    /// Total informado pelo banco, quando disponível.
    pub hit_count: usize,
}

/// Hit textual genérico (memórias que não são de fase).
#[derive(Clone, Debug, PartialEq)]
pub struct TextHit {
    /// Texto da memória.
    pub text: String,
    /// Chave de armazenamento (para `ttl`/`forget`).
    pub storage_key: Option<String>,
    /// Score de recuperação.
    pub score: f32,
    /// Autoridade efetiva.
    pub authority: u8,
    /// `true` se o texto é um documento de fase (namespace separado).
    pub is_phase: bool,
}

impl TextHit {
    /// Decodifica um hit cru.
    pub fn parse(json: &Json) -> Option<Self> {
        let text = json
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| json.as_str())?
            .to_string();
        Some(Self {
            is_phase: PhaseDoc::parse(&text).is_some(),
            text,
            storage_key: json
                .get("key")
                .and_then(|v| v.as_str())
                .or_else(|| json.get("storage_key").and_then(|v| v.as_str()))
                .map(str::to_string),
            score: json
                .get("score")
                .and_then(|v| v.as_f64())
                .or_else(|| json.get("relevance").and_then(|v| v.as_f64()))
                .unwrap_or(1.0) as f32,
            authority: json
                .get("authority")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(255) as u8)
                .unwrap_or(0),
        })
    }
}

/// Cliente MCP.
#[derive(Debug)]
pub struct SgdbClient<T: Transport> {
    transport: T,
    cfg: SgdbConfig,
    next_id: u64,
    initialized: bool,
    remembered: u64,
    recalled: u64,
    failures: u64,
}

impl<T: Transport> SgdbClient<T> {
    /// Envolve um transporte já construído.
    pub fn new(transport: T, cfg: SgdbConfig) -> Self {
        Self {
            transport,
            cfg,
            next_id: 1,
            initialized: false,
            remembered: 0,
            recalled: 0,
            failures: 0,
        }
    }

    /// Configuração em uso.
    pub fn config(&self) -> &SgdbConfig {
        &self.cfg
    }

    /// Configuração mutável (ajustes pontuais do host).
    pub fn config_mut(&mut self) -> &mut SgdbConfig {
        &mut self.cfg
    }

    /// Transporte (acesso ao `MockTransport` nos testes).
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Transporte mutável (ex.: `set_offline` no mock).
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Documentos gravados com sucesso.
    pub fn remembered(&self) -> u64 {
        self.remembered
    }

    /// Consultas bem-sucedidas.
    pub fn recalled(&self) -> u64 {
        self.recalled
    }

    /// Falhas de transporte/ferramenta.
    pub fn failures(&self) -> u64 {
        self.failures
    }

    fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    fn ensure_ready(&mut self) -> Result<(), SgdbError> {
        if self.initialized {
            return Ok(());
        }
        let id = self.id();
        let request = transport::request(
            id,
            "initialize",
            Json::obj([
                ("protocolVersion", Json::str(transport::MCP_PROTOCOL)),
                ("capabilities", Json::obj([("tools", Json::obj([]))])),
                (
                    "clientInfo",
                    Json::obj([
                        ("name", Json::str("aether-engine")),
                        ("version", Json::str(env!("CARGO_PKG_VERSION"))),
                    ]),
                ),
            ]),
        );
        self.transport.call(&request)?;
        // Notificação obrigatória do MCP; alguns servidores ignoram e o mock
        // responde erro — nenhum dos dois é fatal.
        let note = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("method", Json::str("notifications/initialized")),
        ]);
        let _ = self.transport.notify(&note);
        self.initialized = true;
        Ok(())
    }

    /// Chama uma ferramenta MCP e devolve o `result`.
    ///
    /// Erros transitórios ([`SgdbError::is_transient`]) são tentados de novo
    /// com backoff exponencial (`retries` + `retry_backoff_ms`). Escritas são
    /// seguras para repetir porque o default é `if_exists=reinforce`.
    pub fn call_tool(&mut self, name: &str, arguments: Json) -> Result<Json, SgdbError> {
        let attempts = self.cfg.retries.saturating_add(1);
        let mut last: Option<SgdbError> = None;
        for attempt in 0..attempts {
            if let Err(e) = self.ensure_ready() {
                if e.is_transient() && attempt + 1 < attempts {
                    self.backoff(attempt);
                    last = Some(e);
                    continue;
                }
                self.failures += 1;
                return Err(e);
            }
            let id = self.id();
            match self
                .transport
                .call(&tool_request(id, name, arguments.clone()))
            {
                Ok(result) => return Ok(result),
                Err(e) if e.is_transient() && attempt + 1 < attempts => {
                    self.backoff(attempt);
                    last = Some(e);
                }
                Err(e) => {
                    self.failures += 1;
                    return Err(e);
                }
            }
        }
        self.failures += 1;
        Err(last.unwrap_or(SgdbError::Offline))
    }

    fn backoff(&self, attempt: u32) {
        let factor = 1u64 << attempt.min(5);
        let delay = self.cfg.retry_backoff_ms.saturating_mul(factor).min(2_000);
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
    }

    /// Grava documentos de fase (lotes de até [`MAX_BATCH`]).
    ///
    /// Devolve quantos foram aceitos. Em erro, **nenhum** documento é
    /// descartado: o chamador decide spoolar (o [`super::SgdbLedger`] faz isso).
    pub fn remember_docs(&mut self, docs: &mut [PhaseDoc]) -> Result<usize, SgdbError> {
        let batch = self.cfg.max_batch.clamp(1, MAX_BATCH);
        let mut written = 0usize;
        for chunk_start in (0..docs.len()).step_by(batch) {
            let chunk_end = (chunk_start + batch).min(docs.len());
            let chunk = &mut docs[chunk_start..chunk_end];
            let memories: Vec<Json> = chunk
                .iter()
                .map(|d| d.memory_args(&self.cfg.remember()))
                .collect();
            let args = Json::obj([
                ("author", Json::str("agent")),
                ("memories", Json::Arr(memories)),
            ]);
            let result = self.call_tool("remember", args)?;
            let payload = wire::payload_json(&result);
            if let Some(keys) = payload.get("keys").and_then(|v| v.as_arr()) {
                for (doc, key) in chunk.iter_mut().zip(keys.iter()) {
                    if let Some(key) = key.as_str() {
                        doc.storage_key = Some(key.to_string());
                    }
                }
            }
            if let Some(ttl) = self.cfg.ttl_ms {
                for doc in chunk.iter() {
                    if let Some(key) = &doc.storage_key {
                        let _ = self.call_tool(
                            "curate",
                            Json::obj([
                                ("op", Json::str("set_ttl")),
                                ("key", Json::str(key.clone())),
                                ("ttl_episodic_ms", Json::num(ttl as f64)),
                            ]),
                        );
                    }
                }
            }
            written += chunk.len();
        }
        self.remembered += written as u64;
        if self.cfg.audit {
            let _ = self.call_tool("curate", Json::obj([("op", Json::str("audit_checkpoint"))]));
        }
        Ok(written)
    }

    /// Grava registros drenados do `T1`.
    pub fn remember_records(&mut self, records: &[DraftRecord]) -> Result<usize, SgdbError> {
        let mut docs: Vec<PhaseDoc> = records.iter().map(PhaseDoc::from_record).collect();
        self.remember_docs(&mut docs)
    }

    /// Uma página de `recall` escopada e decodificada (cursor do MCP).
    ///
    /// `k` respeita o teto de 20 do banco; use [`Self::recall_all`] para
    /// atravessar páginas.
    pub fn recall_page(
        &mut self,
        scope: ScopeId,
        query: &str,
        k: usize,
        cursor: Option<&str>,
    ) -> Result<RecallPage, SgdbError> {
        let mut args = vec![
            (
                "entities".to_string(),
                Json::Arr(vec![
                    Json::str(wire::scope_entity(scope)),
                    Json::str(ENTITY_PHASE),
                ]),
            ),
            (
                "scope_app".to_string(),
                Json::str(self.cfg.scope_app.clone()),
            ),
            ("scope_run".to_string(), Json::str(wire::scope_run(scope))),
            ("k".to_string(), Json::num(k.clamp(1, 20) as f64)),
            ("format".to_string(), Json::str("json")),
        ];
        if !query.is_empty() {
            args.push(("query".to_string(), Json::str(query)));
        }
        if let Some(cursor) = cursor {
            args.push(("cursor".to_string(), Json::str(cursor)));
        }
        let result = self.call_tool("recall", Json::Obj(args))?;
        self.recalled += 1;
        let payload = wire::payload_json(&result);
        let hit_count = payload
            .get("hit_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let cursor = payload
            .get("cursor")
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("next_cursor").and_then(|v| v.as_str()))
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        let mut hits: Vec<MemoryHit> = hits_json(&result)
            .iter()
            .filter_map(parse_hit)
            .filter(|hit| hit.doc.scope == scope)
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(k.clamp(1, 20));
        Ok(RecallPage {
            hits,
            cursor,
            hit_count,
        })
    }

    /// `recall` escopada (uma página).
    pub fn recall(
        &mut self,
        scope: ScopeId,
        query: &str,
        k: usize,
    ) -> Result<Vec<MemoryHit>, SgdbError> {
        Ok(self.recall_page(scope, query, k, None)?.hits)
    }

    /// Atravessa páginas até `max` hits (ou até o cursor acabar).
    ///
    /// Corrige o teto de `k ≤ 20` sem mudar o contrato do banco.
    pub fn recall_all(
        &mut self,
        scope: ScopeId,
        query: &str,
        max: usize,
    ) -> Result<Vec<MemoryHit>, SgdbError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self.recall_page(scope, query, 20, cursor.as_deref())?;
            let before = out.len();
            out.extend(page.hits.iter().cloned());
            if out.len() >= max || page.hits.is_empty() || out.len() == before {
                break;
            }
            match page.cursor {
                Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
                _ => break,
            }
        }
        out.truncate(max);
        Ok(out)
    }

    /// Todas as memórias de fase do escopo (sem consulta textual).
    pub fn recall_scope(&mut self, scope: ScopeId, k: usize) -> Result<Vec<MemoryHit>, SgdbError> {
        self.recall(scope, "", k)
    }

    /// Grava um fato **textual** (fora do namespace de fase).
    ///
    /// Usa `author=system` + `layer` explícita (exigência do banco para
    /// escrita operacional) e marca as entidades `aether/scope/...` +
    /// `aether/fact`. Devolve a chave de armazenamento, se o servidor informar.
    pub fn remember_text(
        &mut self,
        scope: ScopeId,
        text: &str,
        entities: &[String],
        layer: &str,
        authority: u8,
        ttl_ms: Option<u64>,
    ) -> Result<Option<String>, SgdbError> {
        let mut all_entities = vec![wire::scope_entity(scope), "aether/fact".to_string()];
        all_entities.extend(entities.iter().cloned());
        let args = Json::obj([
            ("author", Json::str("system")),
            ("layer", Json::str(layer)),
            ("text", Json::str(text)),
            ("type", Json::str("text")),
            (
                "entities",
                Json::Arr(all_entities.iter().cloned().map(Json::Str).collect()),
            ),
            ("scope_app", Json::str(self.cfg.scope_app.clone())),
            ("scope_agent", Json::str(self.cfg.scope_agent.clone())),
            ("scope_run", Json::str(wire::scope_run(scope))),
            ("if_exists", Json::str(self.cfg.if_exists.clone())),
        ]);
        let result = self.call_tool("remember", args)?;
        let key = storage_key_of(&result);
        if let Some(key) = key.as_deref() {
            if authority > 0 {
                let _ = self.call_tool(
                    "curate",
                    Json::obj([
                        ("op", Json::str("set_authority")),
                        ("key", Json::str(key)),
                        ("authority", Json::num(f64::from(authority))),
                    ]),
                );
            }
            if let Some(ttl) = ttl_ms.or(self.cfg.ttl_ms) {
                let _ = self.set_ttl(key, ttl);
            }
        }
        self.remembered += 1;
        Ok(key)
    }

    /// Busca fatos textuais (inclui memórias de fase; veja [`TextHit::is_phase`]).
    pub fn recall_text(
        &mut self,
        scope: ScopeId,
        query: &str,
        k: usize,
    ) -> Result<Vec<TextHit>, SgdbError> {
        // SEM `entities`: no mcp_server >= 1.5.x, `entities` desvia o recall
        // para a rota de ativação 1-hop (mode=entities), que ignora a `query`
        // e devolve os documentos da entidade em ordem de inserção com
        // score 1.0 fixo — o ranking lexical (BM25) nunca roda. O filtro de
        // escopo (`scope_app` + `scope_run`) já limita a busca; a entidade
        // `aether/fact` fica na ESCRITA, não na leitura.
        let mut args = vec![
            (
                "scope_app".to_string(),
                Json::str(self.cfg.scope_app.clone()),
            ),
            ("scope_run".to_string(), Json::str(wire::scope_run(scope))),
            ("k".to_string(), Json::num(k.clamp(1, 20) as f64)),
            ("format".to_string(), Json::str("json")),
        ];
        if !query.is_empty() {
            args.push(("query".to_string(), Json::str(query)));
        }
        let result = self.call_tool("recall", Json::Obj(args))?;
        self.recalled += 1;
        Ok(wire::hits_json(&result)
            .iter()
            .filter_map(TextHit::parse)
            .collect())
    }

    /// Descrição do transporte (endpoint/stdio/mock) para `doctor`.
    pub fn describe(&self) -> String {
        self.transport.describe()
    }

    /// Fecha a execução no banco (`curate(op=commit_run)`).
    pub fn commit_run(&mut self, scope: ScopeId) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("commit_run")),
                ("scope_app", Json::str(self.cfg.scope_app.clone())),
                ("scope_run", Json::str(wire::scope_run(scope))),
            ]),
        )
    }

    /// Liga duas chaves com `contradicts` sem apagar nenhuma das duas.
    pub fn associate_contradicts(&mut self, a: &str, b: &str) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("associate")),
                ("a", Json::str(a)),
                ("b", Json::str(b)),
                ("kind", Json::str("contradicts")),
            ]),
        )
    }

    /// Lista as chaves que contradizem `key` (não apaga o fato).
    pub fn contradicts_of(&mut self, key: &str) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([("op", Json::str("contradicts")), ("key", Json::str(key))]),
        )
    }

    /// Grava um checkpoint de auditoria.
    pub fn audit_checkpoint(&mut self) -> Result<Json, SgdbError> {
        self.call_tool("curate", Json::obj([("op", Json::str("audit_checkpoint"))]))
    }

    /// Aposenta uma memória (`forget_purge` com motivo).
    pub fn forget(&mut self, storage_key: &str, reason: &str) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("forget_purge")),
                ("key", Json::str(storage_key)),
                ("reason", Json::str(reason)),
            ]),
        )
    }

    /// Aplica TTL a uma memória já gravada.
    pub fn set_ttl(&mut self, storage_key: &str, ttl_ms: u64) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("set_ttl")),
                ("key", Json::str(storage_key)),
                ("ttl_episodic_ms", Json::num(ttl_ms as f64)),
            ]),
        )
    }

    /// Estado do banco (`health`).
    pub fn health(&mut self) -> Result<Json, SgdbError> {
        self.health_view("status")
    }

    /// Estado do banco em uma visão específica (`status`, `era`, `tensions`, …).
    pub fn health_view(&mut self, view: &str) -> Result<Json, SgdbError> {
        self.call_tool("health", Json::obj([("view", Json::str(view))]))
    }

    /// Proveniência completa de uma memória (`curate(op=explain, full)`).
    pub fn explain(&mut self, storage_key: &str) -> Result<Json, SgdbError> {
        self.call_tool(
            "curate",
            Json::obj([
                ("op", Json::str("explain")),
                ("key", Json::str(storage_key)),
                ("full", Json::Bool(true)),
            ]),
        )
    }

    /// Evidência pronta para [`crate::core::certificate::Certificate`].
    pub fn evidence(&mut self, scope: ScopeId, query: &str, k: usize) -> Vec<Evidence> {
        match self.recall(scope, query, k) {
            Ok(hits) => hits
                .iter()
                .map(|hit| Evidence {
                    source: EvidenceSource::Ledger,
                    id: wire::evidence_id(hit.doc.key),
                    score: hit.score,
                    authority: hit.authority,
                })
                .collect(),
            Err(e) => {
                self.failures += 1;
                let _ = e;
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::sgdb::transport::MockTransport;
    use crate::knowledge::sgdb::wire::content_key;

    fn record(scope: ScopeId, key: u8, authority: u8) -> DraftRecord {
        let mut payload = Vec::new();
        for i in 0..8u8 {
            payload.extend_from_slice(&(f32::from(i) + f32::from(key)).to_le_bytes());
            payload.extend_from_slice(&(-(f32::from(i))).to_le_bytes());
        }
        DraftRecord {
            key: content_key(&payload),
            layer: crate::core::knowledge_bridge_f1::Layer::L4Semantic,
            certificate: crate::core::knowledge_bridge_f1::Certificate {
                energy: -1.0,
                gap: 0.1,
                violations: 0,
            },
            authority,
            scope,
            tick: 1,
            wall_ms: 1_700_000_000_000,
            payload,
        }
    }

    #[test]
    fn remember_e_recall_fazem_round_trip() {
        let scope = ScopeId::new(1, 2, 3);
        let mut client = SgdbClient::new(MockTransport::new(), SgdbConfig::default());
        let rec = record(scope, 1, 200);
        assert_eq!(
            client.remember_records(std::slice::from_ref(&rec)).unwrap(),
            1
        );
        let hits = client.recall_scope(scope, 4).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc.payload, rec.payload);
        assert_eq!(hits[0].doc.certificate, rec.certificate);
        assert_eq!(hits[0].authority, 200);
        assert!(hits[0].storage_key.is_some());
    }

    #[test]
    fn recall_nao_atravessa_escopo() {
        let a = ScopeId::new(1, 1, 1);
        let b = ScopeId::new(2, 1, 1);
        let mut client = SgdbClient::new(MockTransport::new(), SgdbConfig::default());
        client.remember_records(&[record(a, 1, 200)]).unwrap();
        assert_eq!(client.recall_scope(a, 4).unwrap().len(), 1);
        assert_eq!(client.recall_scope(b, 4).unwrap().len(), 0);
    }

    #[test]
    fn evidencia_ledger_tem_autoridade_do_hit() {
        let scope = ScopeId::new(4, 5, 6);
        let mut client = SgdbClient::new(MockTransport::new(), SgdbConfig::default());
        client.remember_records(&[record(scope, 2, 255)]).unwrap();
        let ev = client.evidence(scope, "", 3);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].source, EvidenceSource::Ledger);
        assert_eq!(ev[0].authority, 255);
        assert!(ev[0].score > 0.0);
    }

    #[test]
    fn ttl_expira_no_banco() {
        let scope = ScopeId::new(7, 8, 9);
        let mut mock = MockTransport::new();
        mock.set_now(1_000);
        let cfg = SgdbConfig {
            ttl_ms: Some(100),
            ..Default::default()
        };
        let mut client = SgdbClient::new(mock, cfg);
        client.remember_records(&[record(scope, 1, 100)]).unwrap();
        assert_eq!(client.recall_scope(scope, 4).unwrap().len(), 1);
        client.transport_mut().set_now(1_500);
        assert_eq!(client.recall_scope(scope, 4).unwrap().len(), 0);
    }

    #[test]
    fn recall_all_atravessa_o_teto_de_20() {
        let scope = ScopeId::new(5, 5, 5);
        let mut client = SgdbClient::new(MockTransport::new(), SgdbConfig::default());
        let records: Vec<DraftRecord> = (0..25u8).map(|k| record(scope, k, 200)).collect();
        client.remember_records(&records).unwrap();
        assert_eq!(client.recall_scope(scope, 20).unwrap().len(), 20);
        assert_eq!(client.recall_all(scope, "", 40).unwrap().len(), 25);
    }

    #[test]
    fn texto_faz_round_trip_e_nao_mistura_com_fase() {
        let scope = ScopeId::new(6, 6, 6);
        let mut client = SgdbClient::new(MockTransport::new(), SgdbConfig::default());
        let key = client
            .remember_text(
                scope,
                "o gate F4 roda no CI",
                &["aether/f4".to_string()],
                "L3",
                255,
                None,
            )
            .unwrap();
        assert!(key.is_some());
        client.remember_records(&[record(scope, 1, 200)]).unwrap();
        let hits = client.recall_text(scope, "gate", 8).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "o gate F4 roda no CI");
        assert_eq!(hits[0].authority, 255);
        assert!(!hits[0].is_phase);
    }
}

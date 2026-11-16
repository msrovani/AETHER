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

use crate::core::certificate::{Evidence, EvidenceSource};
use crate::core::knowledge_bridge_f1::{DraftRecord, ScopeId};
use crate::json::Json;
use crate::knowledge::sgdb::transport::{self, tool_request, Transport};
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

    fn remember<'a>(&'a self) -> RememberConfig<'a> {
        RememberConfig {
            scope_app: &self.scope_app,
            scope_agent: &self.scope_agent,
            if_exists: &self.if_exists,
            key_prefix: self.key_prefix.as_deref(),
        }
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
        let _ = self.transport.call(&note);
        self.initialized = true;
        Ok(())
    }

    /// Chama uma ferramenta MCP e devolve o `result`.
    pub fn call_tool(&mut self, name: &str, arguments: Json) -> Result<Json, SgdbError> {
        self.ensure_ready()?;
        let id = self.id();
        match self.transport.call(&tool_request(id, name, arguments)) {
            Ok(result) => Ok(result),
            Err(e) => {
                self.failures += 1;
                Err(e)
            }
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

    /// `recall` escopada e decodificada.
    pub fn recall(
        &mut self,
        scope: ScopeId,
        query: &str,
        k: usize,
    ) -> Result<Vec<MemoryHit>, SgdbError> {
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
        let result = self.call_tool("recall", Json::Obj(args))?;
        self.recalled += 1;
        let mut hits: Vec<MemoryHit> = hits_json(&result)
            .iter()
            .filter_map(parse_hit)
            .filter(|hit| hit.doc.scope == scope)
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(k.max(1));
        Ok(hits)
    }

    /// Todas as memórias de fase do escopo (sem consulta textual).
    pub fn recall_scope(&mut self, scope: ScopeId, k: usize) -> Result<Vec<MemoryHit>, SgdbError> {
        self.recall(scope, "", k)
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
        self.call_tool("health", Json::obj([("view", Json::str("status"))]))
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
        assert_eq!(client.remember_records(&[rec.clone()]).unwrap(), 1);
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
        let mut cfg = SgdbConfig::default();
        cfg.ttl_ms = Some(100);
        let mut client = SgdbClient::new(mock, cfg);
        client.remember_records(&[record(scope, 1, 100)]).unwrap();
        assert_eq!(client.recall_scope(scope, 4).unwrap().len(), 1);
        client.transport_mut().set_now(1_500);
        assert_eq!(client.recall_scope(scope, 4).unwrap().len(), 0);
    }
}

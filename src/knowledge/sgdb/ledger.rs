//! `SgdbLedger` — o `T2` durável: escrita *write-through*, leitura *read-through*.
//!
//! O [`FusionLedger`] existente continua sendo a **semântica** local (matriz 2D,
//! *gist*, escopo). Este tipo o envolve e acrescenta o que faltava:
//!
//! ```text
//! T0 ──(ring SPSC)──► T1 drain ──► SgdbLedger::commit_run ──► SGDB (MCP)
//!                                        │                        │
//!                                        ├─ falhou? outbox ◄──────┘
//!                                        └─ FusionLedger (cache + matriz 2D)
//!
//! consulta ──► SGDB recall ──► payload de fase ──► cue_into (semente)
//!                   └─ offline? cache local ──► cue_into
//! ```
//!
//! Regra de ouro (Vol. III §13): **o motor nunca espera o ledger.** O
//! `SgdbLedger` só é chamado no `T1`/`T2`; a thread quente usa a ponte SPSC.
//!
//! # Nada se perde
//!
//! Se o banco estiver fora, o fato vai para o *outbox* (arquivo append-only) ou
//! para a fila em memória; o próximo commit tenta de novo **antes** do lote
//! novo. O replay é idempotente por `(scope, key)` com `if_exists=reinforce`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::core::certificate::Evidence;
use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate, CueHit, DraftRecord, Layer, ScopeId,
};
use crate::core::knowledge_fusion::{Decision, FusionLedger};
use crate::knowledge::sgdb::client::{SgdbClient, SgdbConfig};
use crate::knowledge::sgdb::transport::{Outbox, Transport};
use crate::knowledge::sgdb::wire::{self, MemoryHit, PhaseDoc};
use crate::knowledge::sgdb::SgdbError;

/// Telemetria acumulada do `T2` (inteiros; não ordena decisão).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SgdbMetrics {
    /// Documentos aceitos pelo banco.
    pub remembered: u64,
    /// Documentos enviados ao outbox/fila por falha de rede.
    pub spooled: u64,
    /// Documentos recuperados do outbox e entregues.
    pub replayed: u64,
    /// Consultas com ao menos um hit.
    pub recall_hits: u64,
    /// Consultas sem hit (ou offline).
    pub recall_misses: u64,
    /// Ciclos fechados (`commit_run`).
    pub commits: u64,
    /// Erros de transporte/ferramenta.
    pub errors: u64,
}

/// Ledger de conhecimento com o SGDB como fonte durável.
#[derive(Debug)]
pub struct SgdbLedger<T: Transport> {
    client: SgdbClient<T>,
    local: FusionLedger,
    scope: ScopeId,
    staged: Vec<DraftRecord>,
    pending: Vec<PhaseDoc>,
    outbox: Option<Outbox>,
    metrics: SgdbMetrics,
    last_error: Option<String>,
}

impl<T: Transport> SgdbLedger<T> {
    /// Cria o ledger para um escopo com o transporte informado.
    pub fn new(transport: T, cfg: SgdbConfig, scope: ScopeId) -> Self {
        let outbox = cfg.outbox.clone().map(Outbox::new);
        Self {
            client: SgdbClient::new(transport, cfg),
            local: FusionLedger::new(),
            scope,
            staged: Vec::new(),
            pending: Vec::new(),
            outbox,
            metrics: SgdbMetrics::default(),
            last_error: None,
        }
    }

    /// Cria com um cliente já configurado (ex.: transporte compartilhado).
    pub fn with_client(client: SgdbClient<T>, scope: ScopeId) -> Self {
        let outbox = client.config().outbox.clone().map(Outbox::new);
        Self {
            client,
            local: FusionLedger::new(),
            scope,
            staged: Vec::new(),
            pending: Vec::new(),
            outbox,
            metrics: SgdbMetrics::default(),
            last_error: None,
        }
    }

    /// Cliente MCP.
    pub fn client(&self) -> &SgdbClient<T> {
        &self.client
    }

    /// Cliente MCP mutável.
    pub fn client_mut(&mut self) -> &mut SgdbClient<T> {
        &mut self.client
    }

    /// Cache local (matriz 2D, *gists*, escopo).
    pub fn local(&self) -> &FusionLedger {
        &self.local
    }

    /// Cache local mutável.
    pub fn local_mut(&mut self) -> &mut FusionLedger {
        &mut self.local
    }

    /// Escopo padrão de escrita.
    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Telemetria do `T2`.
    pub fn metrics(&self) -> SgdbMetrics {
        self.metrics
    }

    /// Último erro de transporte (diagnóstico; `None` = online).
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// `true` se há documentos esperando entrega.
    pub fn is_offline(&self) -> bool {
        !self.pending.is_empty()
            || self
                .outbox
                .as_ref()
                .is_some_and(|outbox| !outbox.is_empty())
    }

    /// Fila do `T1`: recebe um registro drenado.
    pub fn stage(&mut self, record: DraftRecord) {
        self.staged.push(record);
    }

    /// Estagia um lote drenado.
    pub fn stage_batch(&mut self, records: impl IntoIterator<Item = DraftRecord>) {
        self.staged.extend(records);
    }

    /// Fecha o ciclo: entrega o lote ao SGDB e consolida no cache local.
    ///
    /// Nunca perde um fato: em falha, o lote vai para o outbox/fila. Devolve
    /// quantos registros entraram no estado autoritativo local.
    pub fn commit_run(&mut self) -> usize {
        let batch = std::mem::take(&mut self.staged);
        if !batch.is_empty() {
            let replayed = self.flush();
            if replayed > 0 {
                self.metrics.replayed += replayed as u64;
            }
            let mut docs: Vec<PhaseDoc> = batch.iter().map(PhaseDoc::from_record).collect();
            let count = docs.len();
            match self.client.remember_docs(&mut docs) {
                Ok(written) => {
                    self.metrics.remembered += written as u64;
                    self.last_error = None;
                }
                Err(e) => {
                    self.metrics.errors += 1;
                    self.last_error = Some(e.to_string());
                    self.spool(docs);
                    self.metrics.spooled += count as u64;
                }
            }
            self.local.stage_batch(batch);
        }
        let committed = self.local.commit_run();
        self.metrics.commits += 1;
        committed
    }

    /// Reenvia o que ficou pendente (outbox ou fila em memória).
    ///
    /// Devolve quantos documentos foram entregues agora. Não lança: o pior caso
    /// é continuar offline (e o chamador segue operando).
    pub fn flush(&mut self) -> usize {
        let mut docs = self.drain_pending();
        if docs.is_empty() {
            return 0;
        }
        match self.client.remember_docs(&mut docs) {
            Ok(written) => {
                self.last_error = None;
                let remaining = docs.split_off(written.min(docs.len()));
                self.store_pending(remaining);
                written
            }
            Err(e) => {
                self.metrics.errors += 1;
                self.last_error = Some(e.to_string());
                self.store_pending(docs);
                0
            }
        }
    }

    /// Consulta o SGDB (leitura tipada, escopada).
    pub fn recall(&mut self, scope: ScopeId, query: &str, k: usize) -> Vec<MemoryHit> {
        match self.client.recall(scope, query, k) {
            Ok(hits) => {
                if hits.is_empty() {
                    self.metrics.recall_misses += 1;
                } else {
                    self.metrics.recall_hits += 1;
                }
                hits
            }
            Err(e) => {
                self.metrics.recall_misses += 1;
                self.metrics.errors += 1;
                self.last_error = Some(e.to_string());
                Vec::new()
            }
        }
    }

    /// Evidência `Ledger` para o certificado (matriz 2D).
    pub fn evidence(&mut self, scope: ScopeId, query: &str, k: usize) -> Vec<Evidence> {
        self.client.evidence(scope, query, k)
    }

    /// Reacende o cache local com o que está no banco.
    ///
    /// É o *warm start* de um organismo que reiniciou: os padrões de fase
    /// voltam do SGDB e ficam disponíveis para `cue_into` mesmo offline. É o
    /// "o ledger não substitui a fase — ele a reacende" do Vol. III §7.
    pub fn hydrate(&mut self, scope: ScopeId, k: usize) -> usize {
        let hits = self.recall(scope, "", k);
        if hits.is_empty() {
            return 0;
        }
        let records: Vec<DraftRecord> = hits
            .iter()
            .map(|hit| DraftRecord {
                key: hit.doc.key,
                layer: hit.doc.layer,
                certificate: hit.doc.certificate,
                authority: hit.authority,
                scope: hit.doc.scope,
                tick: hit.doc.tick,
                wall_ms: hit.doc.wall_ms,
                payload: hit.doc.payload.clone(),
            })
            .collect();
        self.local.stage_batch(records);
        self.local.commit_run()
    }

    /// Consulta **filtrada por escopo** com *read-through*: banco primeiro,
    /// cache local como fallback.
    ///
    /// Escreve o padrão mais próximo em `out` como fases (`atan2(im, re)`) e
    /// devolve a chave. Não aloca no caminho feliz além do `recall` do `T2`.
    pub fn cue_into_scoped(
        &mut self,
        scope: ScopeId,
        query: &[f32],
        out: &mut [f32],
    ) -> Result<CueHit, BridgeError> {
        let dims = query.len() / 2;
        if dims == 0 || out.len() < dims {
            return Err(BridgeError::BufTooSmall);
        }
        let k = self.client.config().recall_k.max(1);
        if let Ok(hits) = self.client.recall_scope(scope, k) {
            let mut best: Option<(f32, usize)> = None;
            for (i, hit) in hits.iter().enumerate() {
                if hit.doc.dims() != dims {
                    continue;
                }
                let score = score_complex(&hit.doc.payload, query, dims);
                if best.map_or(true, |(s, _)| score > s) {
                    best = Some((score, i));
                }
            }
            if let Some((_, i)) = best {
                write_seed(&hits[i].doc.payload, out, dims);
                self.metrics.recall_hits += 1;
                return Ok(CueHit {
                    key: hits[i].doc.key,
                    scope,
                    authority: hits[i].authority,
                    hits: hits.len(),
                });
            }
            self.metrics.recall_misses += 1;
        } else {
            self.metrics.recall_misses += 1;
        }
        self.local.cue_into_scoped(scope, query, out)
    }

    /// Oráculo da matriz 2D (`autoridade × factibilidade`) do cache local.
    pub fn decide(&mut self, authority: u8, certificate: Certificate) -> Decision {
        self.local.decide(authority, certificate)
    }

    fn spool(&mut self, docs: Vec<PhaseDoc>) {
        self.store_pending(docs);
    }

    fn store_pending(&mut self, docs: Vec<PhaseDoc>) {
        if docs.is_empty() {
            return;
        }
        if let Some(outbox) = &self.outbox {
            if let Err(e) = outbox.append(&docs) {
                self.last_error = Some(e.to_string());
                self.pending.extend(docs);
            }
        } else {
            self.pending.extend(docs);
        }
    }

    fn drain_pending(&mut self) -> Vec<PhaseDoc> {
        if let Some(outbox) = &self.outbox {
            let mut docs = outbox.read();
            // Nada em memória quando há outbox; mas garante o que caiu lá.
            docs.extend(std::mem::take(&mut self.pending));
            docs
        } else {
            std::mem::take(&mut self.pending)
        }
    }
}

/// Similaridade média `cos(Δφ)` entre payload gravado e a consulta complexa.
fn score_complex(payload: &[u8], query: &[f32], dims: usize) -> f32 {
    let mut acc = 0.0f64;
    for i in 0..dims {
        let off = i * 8;
        if off + 8 > payload.len() {
            break;
        }
        let ar = f32::from_le_bytes([
            payload[off],
            payload[off + 1],
            payload[off + 2],
            payload[off + 3],
        ]);
        let ai = f32::from_le_bytes([
            payload[off + 4],
            payload[off + 5],
            payload[off + 6],
            payload[off + 7],
        ]);
        let br = query[2 * i];
        let bi = query[2 * i + 1];
        acc += f64::from(ar * br + ai * bi);
    }
    (acc / dims as f64) as f32
}

/// Materializa o payload como fases para a memória de atratores.
fn write_seed(payload: &[u8], out: &mut [f32], dims: usize) {
    for (i, item) in out.iter_mut().enumerate().take(dims) {
        let off = i * 8;
        if off + 8 > payload.len() {
            break;
        }
        let re = f32::from_le_bytes([
            payload[off],
            payload[off + 1],
            payload[off + 2],
            payload[off + 3],
        ]);
        let im = f32::from_le_bytes([
            payload[off + 4],
            payload[off + 5],
            payload[off + 6],
            payload[off + 7],
        ]);
        *item = im.atan2(re);
    }
}

/// Codifica fases em payload complexo intercalado (`re, im` em `f32` LE).
///
/// É a operação inversa de [`write_seed`]; o `T0` reusa o buffer.
pub fn encode_phases(phases: &[f32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(phases.len() * 8);
    for phase in phases {
        out.extend_from_slice(&phase.cos().to_le_bytes());
        out.extend_from_slice(&phase.sin().to_le_bytes());
    }
}

/// Chave de conteúdo de um payload (reexport para o chamador do `T0`).
pub fn phase_key(payload: &[u8]) -> u128 {
    wire::content_key(payload)
}

/// Camada usada para padrões estáveis do organismo.
pub const ORGANISM_LAYER: Layer = Layer::L4Semantic;

/// Autoridade de um padrão medido pelo próprio organismo (sem HITL).
pub const ORGANISM_AUTHORITY: u8 = 200;

/// Tempo máximo de espera de um `commit_run` no encerramento (diagnóstico).
pub const CLOSE_GRACE: Duration = Duration::from_millis(750);

/// Janela de agregação do `T1` (não é o período do ciclo).
pub const COMMIT_WINDOW: Duration = Duration::from_millis(250);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::knowledge_bridge_f1::Certificate;
    use crate::knowledge::sgdb::transport::MockTransport;

    fn record(scope: ScopeId, key: u8) -> DraftRecord {
        let mut phases = Vec::new();
        for i in 0..8u8 {
            phases.push(f32::from(i) + f32::from(key));
        }
        let mut payload = Vec::new();
        encode_phases(&phases, &mut payload);
        DraftRecord {
            key: phase_key(&payload),
            layer: Layer::L4Semantic,
            certificate: Certificate {
                energy: -1.0,
                gap: 0.2,
                violations: 0,
            },
            authority: ORGANISM_AUTHORITY,
            scope,
            tick: 10,
            wall_ms: 1_700_000_000_000,
            payload,
        }
    }

    #[test]
    fn commit_grava_no_banco_e_no_cache() {
        let scope = ScopeId::new(1, 2, 3);
        let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
        ledger.stage_batch(vec![record(scope, 1), record(scope, 2)]);
        assert_eq!(ledger.commit_run(), 2);
        assert_eq!(ledger.metrics().remembered, 2);
        assert_eq!(ledger.local().len(), 2);
        assert!(!ledger.is_offline());
    }

    #[test]
    fn falha_de_rede_nao_perde_e_replay_entrega() {
        let scope = ScopeId::new(4, 5, 6);
        let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
        ledger.client_mut().transport_mut().set_offline(true);
        ledger.stage(record(scope, 7));
        ledger.commit_run();
        assert_eq!(ledger.metrics().spooled, 1);
        assert!(ledger.is_offline());
        // Volta a rede: o próximo commit reenvia o pendente.
        ledger.client_mut().transport_mut().set_offline(false);
        ledger.stage(record(scope, 8));
        ledger.commit_run();
        assert_eq!(ledger.metrics().replayed, 1);
        assert_eq!(ledger.metrics().remembered, 1);
        assert!(!ledger.is_offline());
    }

    #[test]
    fn cue_usa_o_payload_do_banco() {
        let scope = ScopeId::new(9, 9, 9);
        let rec = record(scope, 3);
        let phases: Vec<f32> = rec
            .payload
            .chunks_exact(8)
            .map(|c| {
                let re = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                let im = f32::from_le_bytes([c[4], c[5], c[6], c[7]]);
                im.atan2(re)
            })
            .collect();
        let mut query = Vec::new();
        for p in &phases {
            query.push(p.cos());
            query.push(p.sin());
        }
        let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
        ledger.stage(rec.clone());
        ledger.commit_run();
        let mut out = vec![0.0f32; phases.len()];
        let hit = ledger
            .cue_into_scoped(scope, &query, &mut out)
            .expect("cue");
        assert_eq!(hit.key, rec.key);
        for (a, b) in out.iter().zip(phases.iter()) {
            assert!((a - b).abs() < 1e-5, "semente divergiu: {a} vs {b}");
        }
    }

    #[test]
    fn hydrate_reaquece_cache_de_um_reinicio() {
        let scope = ScopeId::new(11, 12, 13);
        let mut first = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
        first.stage(record(scope, 4));
        first.commit_run();
        let transport = first.client_mut().transport_mut();
        let transport = std::mem::replace(transport, MockTransport::new());
        let mut restarted = SgdbLedger::new(transport, SgdbConfig::default(), scope);
        assert_eq!(restarted.hydrate(scope, 16), 1);
        assert_eq!(restarted.local().len(), 1);
    }

    #[test]
    fn falha_de_transporte_nao_derruba_o_commit() {
        let scope = ScopeId::new(1, 1, 1);
        let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
        ledger.client_mut().transport_mut().set_offline(true);
        ledger.stage(record(scope, 1));
        assert_eq!(ledger.commit_run(), 1);
        assert!(ledger.last_error().is_some());
        assert_eq!(ledger.metrics().errors, 1);
    }
}

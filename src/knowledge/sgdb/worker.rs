//! Worker `T2`: thread própria que drena o anel e fala com o SGDB.
//!
//! O ciclo do organismo (`T0`) nunca bloqueia: ele só empurra um evento POD de
//! 32 bytes + payload para o anel SPSC. Esta thread é a única que conhece
//! `String`, JSON, socket e latência.
//!
//! ```text
//! organismo ──record (0 alloc, 0 syscall)──► ring SPSC ──drain──► SgdbLedger
//!                                                               ├─ commit_run
//!                                                               ├─ outbox (offline)
//!                                                               └─ audit_checkpoint
//! ```
//!
//! O worker agrega por janela ([`SUPER::COMMIT_WINDOW`]) **e** por tamanho
//! (64 eventos), o que mantém o número de round trips baixo sem atrasar a
//! durabilidade além de um quarto de segundo.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::core::knowledge_bridge_f1::{
    BridgeError, BridgeMetrics, Certificate, KnowledgeEvent, KnowledgeSink, Layer, ScopeId,
    SpscKnowledgePhaseBridge,
};
use crate::knowledge::sgdb::client::SgdbConfig;
use crate::knowledge::sgdb::ledger::{SgdbLedger, COMMIT_WINDOW};
use crate::knowledge::sgdb::transport::Transport;

/// Capacidade do anel do `T2` (potência de dois; 256 slots).
pub const T2_RING: usize = 256;

/// Máximo drenado por passagem (o resto fica para a próxima).
const DRAIN_MAX: usize = 64;

/// Contadores do worker, legíveis de fora (sem `Mutex`).
#[derive(Debug, Default)]
pub struct T2Runtime {
    /// Eventos drenados do anel.
    pub drained: AtomicU64,
    /// Documentos aceitos pelo banco.
    pub remembered: AtomicU64,
    /// Documentos em outbox/fila.
    pub spooled: AtomicU64,
    /// Documentos reenviados do outbox.
    pub replayed: AtomicU64,
    /// Ciclos fechados.
    pub commits: AtomicU64,
    /// Erros de transporte.
    pub errors: AtomicU64,
}

impl T2Runtime {
    fn snapshot(&self, ledger: &SgdbLedger<impl Transport>) {
        let m = ledger.metrics();
        self.remembered.store(m.remembered, Ordering::Relaxed);
        self.spooled.store(m.spooled, Ordering::Relaxed);
        self.replayed.store(m.replayed, Ordering::Relaxed);
        self.commits.store(m.commits, Ordering::Relaxed);
        self.errors.store(m.errors, Ordering::Relaxed);
    }
}

/// Worker `T2` com produtor no lado do organismo.
pub struct T2Worker {
    tx: SpscKnowledgePhaseBridge<T2_RING>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    runtime: Arc<T2Runtime>,
}

impl std::fmt::Debug for T2Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("T2Worker")
            .field("capacity", &T2_RING)
            .field("running", &self.handle.is_some())
            .finish()
    }
}

impl T2Worker {
    /// Sobe a thread e devolve o produtor ligado ao organismo.
    pub fn start<T>(transport: T, cfg: SgdbConfig, scope: ScopeId) -> Result<Self, BridgeError>
    where
        T: Transport + 'static,
    {
        let (tx, mut rx) = SpscKnowledgePhaseBridge::<T2_RING>::new();
        let stop = Arc::new(AtomicBool::new(false));
        let runtime = Arc::new(T2Runtime::default());
        let flag = Arc::clone(&stop);
        let shared = Arc::clone(&runtime);
        let handle = thread::Builder::new()
            .name("aether-t2-sgdb".to_string())
            .spawn(move || {
                let mut ledger = SgdbLedger::new(transport, cfg, scope);
                let mut last = Instant::now();
                let mut batch: Vec<crate::core::knowledge_bridge_f1::DraftRecord> =
                    Vec::with_capacity(DRAIN_MAX);
                loop {
                    if batch.len() < DRAIN_MAX {
                        let room = DRAIN_MAX - batch.len();
                        let drained = rx.drain_batch(&mut batch, room);
                        shared
                            .drained
                            .fetch_add(drained as u64, Ordering::Relaxed);
                    }
                    let due = last.elapsed() >= COMMIT_WINDOW;
                    if !batch.is_empty() && (batch.len() >= DRAIN_MAX || due) {
                        ledger.stage_batch(std::mem::take(&mut batch));
                        ledger.commit_run();
                        shared.snapshot(&ledger);
                        last = Instant::now();
                    }
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                // Descarga final: o anel pode ter eventos que chegaram durante
                // o último sleep; nada fica para trás no shutdown normal.
                if !batch.is_empty() {
                    ledger.stage_batch(std::mem::take(&mut batch));
                }
                loop {
                    let mut tail = Vec::with_capacity(DRAIN_MAX);
                    let drained = rx.drain_batch(&mut tail, DRAIN_MAX);
                    shared
                        .drained
                        .fetch_add(drained as u64, Ordering::Relaxed);
                    if tail.is_empty() {
                        break;
                    }
                    ledger.stage_batch(tail);
                }
                ledger.commit_run();
                ledger.flush();
                let _ = ledger.client_mut().commit_run(scope);
                shared.snapshot(&ledger);
            })
            .map_err(|_| BridgeError::RingFull)?;
        Ok(Self {
            tx,
            stop,
            handle: Some(handle),
            runtime,
        })
    }

    /// Enfileira um padrão de fase (0 alloc, 0 syscall na thread chamadora).
    ///
    /// `payload` são bytes `f32` complexos intercalados; `wall_ms` é o relógio
    /// externo (base do TTL) e `tick` o relógio interno (nunca promovido).
    #[allow(clippy::too_many_arguments)]
    fn record_event(
        &mut self,
        key: u128,
        layer: Layer,
        certificate: Certificate,
        authority: u8,
        scope: ScopeId,
        tick: u64,
        wall_ms: u64,
        payload: &[u8],
    ) -> bool {
        let event = KnowledgeEvent::new(key, layer, scope, tick, wall_ms)
            .with_certificate(certificate)
            .with_authority(authority)
            .with_payload(payload);
        self.tx.record(&event).is_ok()
    }

    /// Versão com escopo explícito (o organismo real tem organização/run).
    #[allow(clippy::too_many_arguments)]
    pub fn record_scoped(
        &mut self,
        key: u128,
        layer: Layer,
        certificate: Certificate,
        authority: u8,
        scope: ScopeId,
        tick: u64,
        wall_ms: u64,
        payload: &[u8],
    ) -> bool {
        self.record_event(key, layer, certificate, authority, scope, tick, wall_ms, payload)
    }

    /// Contadores do lado do produtor (anel).
    pub fn bridge_metrics(&self) -> &BridgeMetrics {
        self.tx.metrics()
    }

    /// Contadores do lado do consumidor (banco).
    pub fn runtime(&self) -> &T2Runtime {
        &self.runtime
    }

    /// Para a thread (drena o resto e faz `commit_run`).
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for T2Worker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::knowledge_bridge_f1::Certificate;
    use crate::knowledge::sgdb::ledger::encode_phases;
    use crate::knowledge::sgdb::transport::MockTransport;

    #[test]
    fn worker_drena_e_grava_tudo_no_shutdown() {
        let scope = ScopeId::new(3, 4, 5);
        let mut worker =
            T2Worker::start(MockTransport::new(), SgdbConfig::default(), scope).unwrap();
        let mut payload = Vec::new();
        encode_phases(&[0.0, 0.5, 1.0, 1.5], &mut payload);
        let key = crate::knowledge::sgdb::wire::content_key(&payload);
        for _ in 0..10 {
            assert!(worker.record_scoped(
                key,
                Layer::L4Semantic,
                Certificate {
                    energy: -1.0,
                    gap: 0.2,
                    violations: 0,
                },
                crate::knowledge::sgdb::ledger::ORGANISM_AUTHORITY,
                scope,
                1,
                1_700_000_000_000,
                &payload,
            ));
        }
        worker.shutdown();
        assert_eq!(worker.runtime().drained.load(Ordering::Relaxed), 10);
        assert_eq!(worker.bridge_metrics().t0_ledger_calls(), 0);
        assert!(worker.runtime().remembered.load(Ordering::Relaxed) >= 1);
    }
}

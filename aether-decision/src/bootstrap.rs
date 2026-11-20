//! Máquina de estados do warmup (load + fit) — F1: a subida do daemon não
//! bloqueia o chat; enquanto o engine não está `Ready`, o port degrada para
//! abstain `engine_unavailable` (honesto, nunca finge decisão).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Fases do bootstrap. Ordem: Queued → Loading → Calibrating → Ready.
/// Qualquer falha → Failed (com detalhe legível para o fórum/chat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Queued = 0,
    Loading = 1,
    Calibrating = 2,
    Ready = 3,
    Failed = 4,
}

impl Phase {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Queued),
            1 => Some(Self::Loading),
            2 => Some(Self::Calibrating),
            3 => Some(Self::Ready),
            4 => Some(Self::Failed),
            _ => None,
        }
    }

    /// O port pode decidir? Só Ready.
    pub fn can_decide(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Estado compartilhado do bootstrap. Thread-safe por construção.
#[derive(Debug, Default)]
pub struct Bootstrap {
    phase: AtomicU8,
    detail: Mutex<Option<String>>,
}

impl Bootstrap {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            phase: AtomicU8::new(Phase::Queued as u8),
            detail: Mutex::new(None),
        })
    }

    pub fn phase(&self) -> Phase {
        Phase::from_u8(self.phase.load(Ordering::Relaxed)).unwrap_or(Phase::Failed)
    }

    /// Detalhe da fase atual (ex.: mensagem de erro do Failed).
    pub fn detail(&self) -> Option<String> {
        self.detail.lock().expect("bootstrap detail").clone()
    }

    pub fn set_loading(&self) {
        self.phase.store(Phase::Loading as u8, Ordering::Relaxed);
    }

    pub fn set_calibrating(&self) {
        self.phase
            .store(Phase::Calibrating as u8, Ordering::Relaxed);
    }

    pub fn set_ready(&self) {
        self.phase.store(Phase::Ready as u8, Ordering::Relaxed);
    }

    pub fn set_failed(&self, detail: impl Into<String>) {
        self.phase.store(Phase::Failed as u8, Ordering::Relaxed);
        *self.detail.lock().expect("bootstrap detail") = Some(detail.into());
    }

    /// Roda o `work` num thread próprio (warmup na subida do daemon).
    /// Convenção para `work`: marcar `set_calibrating` antes do fit e
    /// `set_ready` no fim; `Err` vira `Failed` automaticamente.
    pub fn spawn_work<F>(self: &Arc<Self>, work: F) -> JoinHandle<()>
    where
        F: FnOnce(&Arc<Self>) -> Result<(), String> + Send + 'static,
    {
        let state = Arc::clone(self);
        std::thread::Builder::new()
            .name("aether-engine-bootstrap".into())
            .spawn(move || {
                state.set_loading();
                if let Err(e) = work(&state) {
                    if state.phase() != Phase::Failed {
                        state.set_failed(e);
                    }
                }
            })
            .expect("spawn bootstrap thread")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_queued_and_cannot_decide() {
        let b = Bootstrap::new();
        assert_eq!(b.phase(), Phase::Queued);
        assert!(!b.phase().can_decide());
    }

    #[test]
    fn happy_path_transitions_to_ready() {
        let b = Bootstrap::new();
        let h = b.spawn_work(|s| {
            s.set_calibrating();
            assert!(!s.phase().can_decide());
            s.set_ready();
            Ok(())
        });
        h.join().unwrap();
        assert_eq!(b.phase(), Phase::Ready);
        assert!(b.phase().can_decide());
        assert!(b.detail().is_none());
    }

    #[test]
    fn error_becomes_failed_with_detail_never_ready() {
        let b = Bootstrap::new();
        let h = b.spawn_work(|_| Err("vegaml não encontrado no interpreter".to_string()));
        h.join().unwrap();
        assert_eq!(b.phase(), Phase::Failed);
        assert!(!b.phase().can_decide());
        assert_eq!(
            b.detail().as_deref(),
            Some("vegaml não encontrado no interpreter")
        );
    }
}

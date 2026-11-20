//! Watchdog de VRAM — o freio do engine (Eixo E do plano).
//!
//! O plano fixa: *"watchdog VRAM: >5GB → `Abstain{reason:"engine_unavailable"}`
//! — nunca trava"*. O watchdog **não mata ninguém**: ele mede antes da
//! decisão e, se o orçamento estourou, o port devolve abstain tipado.
//!
//! A medição usa `nvidia-smi` (o motor da GPU do AETHER é NVIDIA). Sem
//! `nvidia-smi` no PATH a medição devolve `None` = **desconhecido**, e
//! desconhecido **não** é estouro: o gate segue. Falhar fechado aqui
//! derrubaria o chat inteiro por falta de ferramenta de leitura.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Medidor de VRAM: devolve os bytes usados, ou `None` se não souber.
pub type Probe = Arc<dyn Fn() -> Option<u64> + Send + Sync>;

/// Freio de VRAM com orçamento fixo.
#[derive(Clone)]
pub struct VramWatchdog {
    budget_bytes: u64,
    probe: Probe,
    ttl: Duration,
    cache: Arc<Mutex<Option<(Instant, Option<u64>)>>>,
}

impl std::fmt::Debug for VramWatchdog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VramWatchdog")
            .field("budget_bytes", &self.budget_bytes)
            .field("ttl", &self.ttl)
            .finish()
    }
}

impl VramWatchdog {
    /// Watchdog com o orçamento em MiB e a sonda padrão (`nvidia-smi`).
    pub fn new(budget_mb: u64) -> Self {
        Self::with_probe(budget_mb.saturating_mul(1024 * 1024), Arc::new(probe_nvidia_smi))
    }

    /// Watchdog com sonda injetada (teste determinístico, sem GPU).
    pub fn with_probe(budget_bytes: u64, probe: Probe) -> Self {
        Self {
            budget_bytes,
            probe,
            ttl: Duration::from_millis(500),
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Orçamento em bytes.
    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Bytes em uso (com cache curto — a medição custa um processo).
    pub fn used_bytes(&self) -> Option<u64> {
        let mut guard = self.cache.lock().expect("vram cache poisoned");
        if let Some((at, val)) = guard.as_ref() {
            if at.elapsed() < self.ttl {
                return *val;
            }
        }
        let val = (self.probe)();
        *guard = Some((Instant::now(), val));
        val
    }

    /// O orçamento estourou? `None` = desconhecido ⇒ **não** estourou.
    pub fn over_budget(&self) -> Option<bool> {
        self.used_bytes().map(|used| used > self.budget_bytes)
    }

    /// Usado, em MiB, para o `meta` auditável.
    pub fn used_mb(&self) -> Option<f64> {
        self.used_bytes().map(|b| b as f64 / (1024.0 * 1024.0))
    }
}

/// Sonda padrão: `nvidia-smi --query-gpu=memory.used`.
///
/// O motor do AETHER roda num único dispositivo; por isso o **máximo** entre
/// as GPUs (e não a soma) é o número que compara com o orçamento.
pub fn probe_nvidia_smi() -> Option<u64> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut max: Option<u64> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(mib) = t.parse::<u64>() {
            let bytes = mib.saturating_mul(1024 * 1024);
            max = Some(max.map_or(bytes, |m: u64| m.max(bytes)));
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn unknown_is_not_over_budget() {
        let w = VramWatchdog::with_probe(1024, Arc::new(|| None));
        assert_eq!(w.over_budget(), None);
        assert_eq!(w.used_mb(), None);
    }

    #[test]
    fn over_budget_is_detected() {
        let w = VramWatchdog::with_probe(1024 * 1024, Arc::new(|| Some(2 * 1024 * 1024)));
        assert_eq!(w.over_budget(), Some(true));
    }

    #[test]
    fn under_budget_is_fine() {
        let w = VramWatchdog::with_probe(5 * 1024 * 1024, Arc::new(|| Some(3 * 1024 * 1024)));
        assert_eq!(w.over_budget(), Some(false));
        assert_eq!(w.used_mb(), Some(3.0));
    }

    #[test]
    fn probe_is_cached_for_the_ttl() {
        let calls = Arc::new(AtomicU64::new(0));
        let c = calls.clone();
        let w = VramWatchdog::with_probe(
            1024,
            Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
                Some(0)
            }),
        );
        let _ = w.used_bytes();
        let _ = w.used_bytes();
        let _ = w.used_bytes();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a sonda não pode correr a cada decisão");
    }

    #[test]
    fn budget_from_megabytes() {
        let w = VramWatchdog::new(5120);
        assert_eq!(w.budget_bytes(), 5120 * 1024 * 1024);
    }
}

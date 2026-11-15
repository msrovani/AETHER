//! Afinidade de núcleo e orçamento de residência L1d (opt-in, sem `unsafe`).
//!
//! A pinagem usa o crate `core_affinity` atrás da feature `affinity`. Sem a
//! feature, as funções viram no-op documentado (retornam `false`) — o gate do
//! CI não depende de pinning.
//!
//! Orçamento reflexivo: [`L1D_REFLEX_BUDGET_BYTES`] = 32 KiB. O estado quente
//! da thread reflexiva deve caber nesse envelope para maximizar hit-rate L1d.

/// Orçamento de estado reflexivo rápido (Cache L1d típico de 32–48 KiB).
pub const L1D_REFLEX_BUDGET_BYTES: usize = 32 * 1024;

/// Resultado de uma tentativa de pinagem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinResult {
    /// Thread fixada no núcleo pedido.
    Pinned { core_index: usize },
    /// Feature `affinity` desligada ou núcleo indisponível.
    Unavailable,
    /// Índice fora do intervalo.
    OutOfRange { requested: usize, available: usize },
}

/// Quantos núcleos lógicos o OS reporta (1 se a feature estiver off).
pub fn logical_core_count() -> usize {
    #[cfg(feature = "affinity")]
    {
        num_cpus::get().max(1)
    }
    #[cfg(not(feature = "affinity"))]
    {
        1
    }
}

/// Verifica se `bytes` cabe no orçamento L1d reflexivo.
#[inline]
pub fn fits_l1d_budget(bytes: usize) -> bool {
    bytes <= L1D_REFLEX_BUDGET_BYTES
}

/// Fixa a thread corrente no núcleo lógico `core_index` (0 = primeiro).
///
/// Em Windows isso acaba em `SetThreadAffinityMask` **dentro** do crate
/// `core_affinity` — nosso código permanece sem blocos `unsafe`.
pub fn pin_current_thread(core_index: usize) -> PinResult {
    #[cfg(feature = "affinity")]
    {
        let cores = core_affinity::get_core_ids().unwrap_or_default();
        if cores.is_empty() {
            return PinResult::Unavailable;
        }
        if core_index >= cores.len() {
            return PinResult::OutOfRange {
                requested: core_index,
                available: cores.len(),
            };
        }
        if core_affinity::set_for_current(cores[core_index]) {
            PinResult::Pinned { core_index }
        } else {
            PinResult::Unavailable
        }
    }
    #[cfg(not(feature = "affinity"))]
    {
        let _ = core_index;
        PinResult::Unavailable
    }
}

/// Percentil linear em fatia já ordenada (ou ordena cópia em stack pequena).
///
/// Para amostras grandes o caller deve passar slice ordenado; para ≤ 4096
/// ordenamos in-place no buffer mutável fornecido.
pub fn percentile_us(sorted_asc: &[f64], p: f64) -> f64 {
    if sorted_asc.is_empty() {
        return f64::NAN;
    }
    let p = p.clamp(0.0, 100.0);
    if sorted_asc.len() == 1 {
        return sorted_asc[0];
    }
    let rank = (p / 100.0) * (sorted_asc.len() as f64 - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted_asc[lo]
    } else {
        let w = rank - lo as f64;
        sorted_asc[lo] * (1.0 - w) + sorted_asc[hi] * w
    }
}

/// Ordena `samples` in-place e devolve (p50, p99, p99.9) em µs.
pub fn latency_tail_us(samples: &mut [f64]) -> (f64, f64, f64) {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (
        percentile_us(samples, 50.0),
        percentile_us(samples, 99.0),
        percentile_us(samples, 99.9),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_32kib() {
        assert!(fits_l1d_budget(32 * 1024));
        assert!(!fits_l1d_budget(32 * 1024 + 1));
    }

    #[test]
    fn percentile_extremes() {
        let s = [1.0, 2.0, 3.0, 4.0];
        assert!((percentile_us(&s, 0.0) - 1.0).abs() < 1e-9);
        assert!((percentile_us(&s, 100.0) - 4.0).abs() < 1e-9);
    }
}

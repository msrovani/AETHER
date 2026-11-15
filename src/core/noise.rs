//! Fontes de ruído zero-alloc para Langevin / sonhador / sondas.
//!
//! Default: [`DeterministicNoise`] (`ChaCha8` semeado) — reproduzível e
//! compatível com o gate. Entropia de hardware fica atrás da feature
//! `hw-entropy` (opt-in; nunca default).
//!
//! Nenhum `unsafe`. Nenhuma alocação nos métodos `next_*` / `fill_*`.

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Gerador de ruído com API de pilha (sem `Vec` no hot-path).
pub trait NoiseSource {
    /// Próximo `u64` uniforme.
    fn next_u64(&mut self) -> u64;

    /// Preenche `out` com `u64` uniformes (sem alocar).
    fn fill_u64(&mut self, out: &mut [u64]) {
        for x in out.iter_mut() {
            *x = self.next_u64();
        }
    }

    /// Uniforme em `[0, 1)`.
    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / ((1u64 << 24) as f32))
    }

    /// Gaussiana N(0,1) via Box–Muller (dois uniforms; guarda o 2º em cache).
    fn next_gaussian(&mut self) -> f32;
}

/// Cache do 2º desvio Box–Muller (evita `sin_cos` duplicado).
#[derive(Clone, Debug, Default)]
struct GaussSpare {
    has: bool,
    spare: f32,
}

impl GaussSpare {
    #[inline]
    fn next<R: NoiseSource + ?Sized>(slot: &mut Self, rng: &mut R) -> f32 {
        if slot.has {
            slot.has = false;
            return slot.spare;
        }
        // Rejeição na coroa unitária (Marsaglia) — sem trig.
        loop {
            let u = rng.next_f32() * 2.0 - 1.0;
            let v = rng.next_f32() * 2.0 - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                slot.has = true;
                slot.spare = v * m;
                return u * m;
            }
        }
    }
}

/// Ruído determinístico (default do crate).
#[derive(Clone, Debug)]
pub struct DeterministicNoise {
    rng: ChaCha8Rng,
    gauss: GaussSpare,
}

impl DeterministicNoise {
    /// Semente explícita.
    pub fn from_seed(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            gauss: GaussSpare::default(),
        }
    }

    /// Alias estável para sondas.
    pub fn seeded(seed: u64) -> Self {
        Self::from_seed(seed)
    }
}

impl Default for DeterministicNoise {
    fn default() -> Self {
        Self::from_seed(0xA37E_5015)
    }
}

impl NoiseSource for DeterministicNoise {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    fn next_gaussian(&mut self) -> f32 {
        GaussSpare::next(&mut self.gauss, self)
    }
}

/// Entropia de SO (`OsRng`) — apenas com `--features hw-entropy`.
#[cfg(feature = "hw-entropy")]
#[derive(Clone, Debug, Default)]
pub struct HwEntropyNoise {
    gauss: GaussSpare,
}

#[cfg(feature = "hw-entropy")]
impl HwEntropyNoise {
    /// Construtor explícito (sem estado global).
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(feature = "hw-entropy")]
impl NoiseSource for HwEntropyNoise {
    #[inline]
    fn next_u64(&mut self) -> u64 {
        use rand::rngs::OsRng;
        OsRng.next_u64()
    }

    fn next_gaussian(&mut self) -> f32 {
        GaussSpare::next(&mut self.gauss, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_reproduz() {
        let mut a = DeterministicNoise::seeded(42);
        let mut b = DeterministicNoise::seeded(42);
        for _ in 0..32 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn gaussian_finita() {
        let mut n = DeterministicNoise::seeded(7);
        for _ in 0..256 {
            let g = n.next_gaussian();
            assert!(g.is_finite());
        }
    }
}

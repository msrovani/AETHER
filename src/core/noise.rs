//! Fontes de ruído plugáveis para a dinâmica de Langevin.
//!
//! O default é **determinístico** (ChaCha8 semeado), para que testes e
//! auditorias sejam reprodutíveis. A fonte de entropia de hardware fica atrás
//! da feature `hw-entropy` e é opt-in: ela usa o `SiliconEntropy` do módulo
//! `silicon_entropy` (jitter de TSC/Instant), mas o caminho padrão continua
//! bit-a-bit reprodutível.
//!
//! `fill_into` e `fill_gaussian_into` não alocam: escrevem no buffer do
//! chamador. A implementação de hardware apenas delega ao `SiliconEntropy`.

#![allow(clippy::needless_range_loop)]

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Fonte de ruído escalar, object-safe e sem alocação no preenchimento.
pub trait NoiseSource: std::fmt::Debug {
    /// Próxima amostra uniforme em `[0, 1)`.
    fn next_f32(&mut self) -> f32;

    /// Nome curto para telemetria.
    fn name(&self) -> &'static str;

    /// Preenche `out` com amostras uniformes (zero alocação).
    fn fill_into(&mut self, out: &mut [f32]) {
        for v in out.iter_mut() {
            *v = self.next_f32();
        }
    }

    /// Preenche `out` com gaussianas padrão (Box-Muller, zero alocação).
    ///
    /// O par de Box-Muller gera duas gaussianas por duas uniformes; a segunda
    /// é guardada no buffer interno de cada implementação.
    fn fill_gaussian_into(&mut self, out: &mut [f32]);
}

/// Fonte determinística (ChaCha8). Mesma semente ⇒ mesma sequência.
#[derive(Clone, Debug)]
pub struct ChaChaNoise {
    rng: ChaCha8Rng,
    spare: Option<f32>,
}

impl ChaChaNoise {
    /// Cria a fonte com a semente dada.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            spare: None,
        }
    }

    /// Semente padrão da auditoria (determinística).
    pub fn default_seeded() -> Self {
        Self::new(0xA37E_0001)
    }

    fn gaussian(&mut self) -> f32 {
        if let Some(s) = self.spare.take() {
            return s;
        }
        let u1 = self.rng.gen::<f32>().max(f32::MIN_POSITIVE);
        let u2 = self.rng.gen::<f32>();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = std::f32::consts::TAU * u2;
        self.spare = Some(r * theta.sin());
        r * theta.cos()
    }
}

impl NoiseSource for ChaChaNoise {
    fn next_f32(&mut self) -> f32 {
        self.rng.gen::<f32>()
    }

    fn name(&self) -> &'static str {
        "chacha8-deterministic"
    }

    fn fill_gaussian_into(&mut self, out: &mut [f32]) {
        for v in out.iter_mut() {
            *v = self.gaussian();
        }
    }
}

/// Fonte de entropia de hardware (opt-in via feature `hw-entropy`).
///
/// Usa o `SiliconEntropy` (jitter de contador de ciclos) como semente/ruído.
/// Não é o default: testes devem permanecer determinísticos.
#[cfg(feature = "hw-entropy")]
#[derive(Debug)]
pub struct HardwareNoise {
    entropy: crate::core::silicon_entropy::SiliconEntropy,
}

#[cfg(feature = "hw-entropy")]
impl HardwareNoise {
    /// Cria a fonte alimentada pelo jitter do silício.
    pub fn new() -> Self {
        Self {
            entropy: crate::core::silicon_entropy::SiliconEntropy::new(),
        }
    }

    /// `true` se o contador de ciclos é o TSC real (x86_64).
    pub fn uses_tsc(&self) -> bool {
        crate::core::silicon_entropy::has_hardware_tsc()
    }
}

#[cfg(feature = "hw-entropy")]
impl Default for HardwareNoise {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "hw-entropy")]
impl NoiseSource for HardwareNoise {
    fn next_f32(&mut self) -> f32 {
        self.entropy.next_f32()
    }

    fn name(&self) -> &'static str {
        "silicon-tsc-jitter"
    }

    fn fill_gaussian_into(&mut self, out: &mut [f32]) {
        self.entropy.fill_gaussian(out);
    }
}

/// Fonte default da auditoria: determinística.
pub fn default_source(seed: u64) -> ChaChaNoise {
    ChaChaNoise::new(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinismo_e_gaussianas_finitas() {
        let mut a = ChaChaNoise::new(7);
        let mut b = ChaChaNoise::new(7);
        let mut ga = [0.0f32; 32];
        let mut gb = [0.0f32; 32];
        a.fill_gaussian_into(&mut ga);
        b.fill_gaussian_into(&mut gb);
        assert_eq!(ga, gb, "mesma semente deve dar mesma sequencia");
        assert!(ga.iter().all(|v| v.is_finite()));
        let mean = ga.iter().sum::<f32>() / ga.len() as f32;
        assert!(mean.abs() < 1.0, "media gaussiana fora de faixa: {mean}");
        let mut c = ChaChaNoise::new(8);
        let mut gc = [0.0f32; 32];
        c.fill_gaussian_into(&mut gc);
        assert_ne!(ga, gc, "sementes diferentes devem divergir");
    }

    #[test]
    fn fill_uniforme_nao_aloca_e_respeita_faixa() {
        let mut n = ChaChaNoise::default_seeded();
        let mut buf = [0.0f32; 128];
        n.fill_into(&mut buf);
        assert!(buf.iter().all(|v| (0.0..1.0).contains(v)));
    }
}

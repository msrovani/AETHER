//! Banho térmico de silício — entropia por *jitter* do contador de ciclos.
//!
//! Extrai bits do ruído temporal real da máquina (jitter de `RDTSC` causado
//! por cache, predição de desvio, DVFS, SMT e interrupções), branqueia com
//! **Murmur3 `fmix64`** + **SplitMix64** e entrega:
//!
//! * `u64` / `f32 ∈ (0,1]` uniformes ([`SiliconEntropy::next_u64`],
//!   [`SiliconEntropy::next_f32_open`]);
//! * gaussianas `N(0,1)` por **Box-Muller** rápido (ângulo por LUT BAM,
//!   [`SiliconEntropy::next_gaussian`], [`SiliconEntropy::fill_gaussian`]);
//! * passos de **Langevin** (Euler–Maruyama) sem alocação
//!   ([`LangevinBath`]).
//!
//! ## Garantias e limites (leia antes de usar)
//!
//! * **Sem estado global**: cada [`SiliconEntropy`] é dono do seu estado;
//!   nenhuma `static`, nenhum `Mutex`, nenhum atômico compartilhado. Uma
//!   instância por thread ⇒ *lock-free* por construção.
//! * **Zero alocação** em todos os métodos de geração.
//! * `x86_64`: `_rdtsc()`. Outras arquiteturas (e `aarch64`/`wasm`): *fallback*
//!   por [`std::time::Instant`] (resolução de ns).
//! * O jitter de baixa ordem do TSC é uma fonte **fraca e não certificada**:
//!   o branqueamento remove viés, mas **não** cria entropia que a fonte não
//!   tenha. Serve para ruído térmico de simulação (Langevin, exploração),
//!   **não** para criptografia.
//! * [`SiliconEntropy::from_seed`] desliga a leitura do TSC: determinístico e
//!   reprodutível (testes/CI).

#![allow(unsafe_code)] // única unsafe: `_rdtsc` em `read_cycle_counter`.

use std::time::Instant;

use crate::core::bam_phase::{BamLut, BamPhase};

/// Constante de Weyl do SplitMix64 (`2^64 / φ`).
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// `2^-24`: converte 24 bits em `f32 ∈ [0,1)`.
const INV_2_24: f32 = 1.0 / 16_777_216.0;

/// Finalizador do Murmur3 (64 bits): avalanche completo.
#[inline(always)]
pub fn murmur3_fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    k ^= k >> 33;
    k = k.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    k ^ (k >> 33)
}

/// Um passo do SplitMix64 (Steele, Lea & Flood, 2014).
#[inline(always)]
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(GOLDEN_GAMMA);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Lê o contador de ciclos (`RDTSC` em x86_64; `Instant` nos demais).
#[inline(always)]
pub fn read_cycle_counter(epoch: &Instant) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        let _ = epoch;
        // Em toolchains novos `_rdtsc` é segura; em antigos exige `unsafe`.
        #[allow(unused_unsafe)]
        unsafe {
            std::arch::x86_64::_rdtsc()
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        epoch.elapsed().as_nanos() as u64
    }
}

/// `true` se o contador de ciclos é o `RDTSC` real (x86_64).
pub const fn has_hardware_tsc() -> bool {
    cfg!(target_arch = "x86_64")
}

/// Extrator de entropia de silício (uma instância por thread).
#[derive(Debug)]
pub struct SiliconEntropy {
    /// Estado do SplitMix64.
    state: u64,
    /// *Pool* branqueado de jitter (Murmur3).
    pool: u64,
    /// Última leitura do contador (delta = jitter).
    last_tsc: u64,
    /// Referência temporal do *fallback* por `Instant`.
    epoch: Instant,
    /// Segunda gaussiana do par de Box-Muller.
    spare: f32,
    has_spare: bool,
    /// `false` ⇒ modo determinístico (não lê o TSC).
    jitter: bool,
    /// Leituras de TSC já colhidas (diagnóstico).
    harvested: u64,
}

impl SiliconEntropy {
    /// Instância alimentada pelo jitter real do silício.
    pub fn new() -> Self {
        let epoch = Instant::now();
        let mut s = Self {
            state: GOLDEN_GAMMA,
            pool: 0,
            last_tsc: read_cycle_counter(&epoch),
            epoch,
            spare: 0.0,
            has_spare: false,
            jitter: true,
            harvested: 0,
        };
        // Semeadura: 64 rodadas de coleta sobre carga de memória/ALU.
        let seed = s.harvest(64);
        s.state ^= murmur3_fmix64(seed ^ (&s as *const Self as usize as u64));
        s
    }

    /// Instância **determinística** (sem leitura do TSC): reprodutível.
    pub fn from_seed(seed: u64) -> Self {
        let epoch = Instant::now();
        Self {
            state: murmur3_fmix64(seed ^ GOLDEN_GAMMA),
            pool: murmur3_fmix64(seed),
            last_tsc: 0,
            epoch,
            spare: 0.0,
            has_spare: false,
            jitter: false,
            harvested: 0,
        }
    }

    /// `true` se esta instância lê o jitter do TSC a cada saída.
    pub fn is_hardware_seeded(&self) -> bool {
        self.jitter
    }

    /// Leituras de contador colhidas até agora.
    pub fn harvested(&self) -> u64 {
        self.harvested
    }

    /// Coleta `rounds` amostras de jitter (delta do TSC em torno de uma
    /// pequena carga de dependência de dados) e devolve o *pool* branqueado.
    pub fn harvest(&mut self, rounds: u32) -> u64 {
        let mut x = self.pool | 1;
        for _ in 0..rounds {
            let a = read_cycle_counter(&self.epoch);
            // Carga curta e dependente: o tempo observado varia com o estado
            // térmico/cache/pipeline (o "banho").
            for _ in 0..4 {
                x = x.wrapping_mul(0x2545_F491_4F6C_DD1D).rotate_left(23) ^ a;
            }
            std::hint::black_box(x);
            let b = read_cycle_counter(&self.epoch);
            let delta = b.wrapping_sub(a);
            self.pool = murmur3_fmix64(self.pool.rotate_left(13) ^ delta ^ b.rotate_left(32));
            self.harvested += 1;
        }
        self.last_tsc = read_cycle_counter(&self.epoch);
        self.pool
    }

    /// Próximo `u64` uniforme (1 leitura de TSC por chamada em modo hardware).
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        if self.jitter {
            let t = read_cycle_counter(&self.epoch);
            let d = t.wrapping_sub(self.last_tsc);
            self.last_tsc = t;
            self.pool = murmur3_fmix64(self.pool ^ d.rotate_left(17) ^ t);
            self.state ^= self.pool;
            self.harvested += 1;
        }
        splitmix64(&mut self.state)
    }

    /// Próximo `u32` uniforme.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// `f32` uniforme em `[0, 1)` (24 bits).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * INV_2_24
    }

    /// `f32` uniforme em `(0, 1]` (seguro para `ln`).
    #[inline]
    pub fn next_f32_open(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32 + 1.0) * INV_2_24
    }

    /// Fase BAM uniforme.
    #[inline]
    pub fn next_phase(&mut self) -> BamPhase {
        BamPhase((self.next_u64() >> 48) as u16)
    }

    /// Preenche `out` com `u64` uniformes.
    pub fn fill_u64(&mut self, out: &mut [u64]) {
        for o in out.iter_mut() {
            *o = self.next_u64();
        }
    }

    /// Gaussiana padrão `N(0,1)` por Box-Muller (par em cache).
    ///
    /// Ângulo por LUT BAM interpolada; raio `√(−2 ln u)` com
    /// `u ∈ (0,1]` de 24 bits ⇒ cauda truncada em ≈ 5,77 σ.
    #[inline]
    pub fn next_gaussian(&mut self) -> f32 {
        if self.has_spare {
            self.has_spare = false;
            return self.spare;
        }
        let bits = self.next_u64();
        let u1 = (((bits >> 40) as f32) + 1.0) * INV_2_24;
        let theta = BamPhase((bits & 0xFFFF) as u16);
        let r = (-2.0 * u1.ln()).sqrt();
        let (s, c) = BamLut::global().sin_cos(theta);
        self.spare = r * s;
        self.has_spare = true;
        r * c
    }

    /// Preenche `out` com `N(0,1)` (sem alocação).
    pub fn fill_gaussian(&mut self, out: &mut [f32]) {
        for o in out.iter_mut() {
            *o = self.next_gaussian();
        }
    }
}

impl Default for SiliconEntropy {
    fn default() -> Self {
        Self::new()
    }
}

/// Banho de Langevin sobre-amortecido: `dx = f(x)·dt + √(2·T·dt)·ξ`.
///
/// Euler–Maruyama; `ξ ~ N(0,1)` do [`SiliconEntropy`] do próprio banho.
#[derive(Debug)]
pub struct LangevinBath {
    entropy: SiliconEntropy,
    temperature: f32,
}

impl LangevinBath {
    /// Banho à temperatura `T ≥ 0` com entropia de hardware.
    pub fn new(temperature: f32) -> Self {
        Self::with_entropy(temperature, SiliconEntropy::new())
    }

    /// Banho com extrator fornecido (ex.: determinístico).
    pub fn with_entropy(temperature: f32, entropy: SiliconEntropy) -> Self {
        let temperature = if temperature.is_finite() && temperature > 0.0 {
            temperature
        } else {
            0.0
        };
        Self {
            entropy,
            temperature,
        }
    }

    /// Temperatura atual.
    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    /// Ajusta a temperatura (valores inválidos viram 0).
    pub fn set_temperature(&mut self, temperature: f32) {
        self.temperature = if temperature.is_finite() && temperature > 0.0 {
            temperature
        } else {
            0.0
        };
    }

    /// Acesso ao extrator (ex.: para `u_rand` do planejador).
    pub fn entropy_mut(&mut self) -> &mut SiliconEntropy {
        &mut self.entropy
    }

    /// Um passo `x += drift·dt + √(2·T·dt)·ξ` (zero alocação).
    ///
    /// `drift.len()` deve ser `x.len()`; `dt ≤ 0` ou não finito é ignorado.
    pub fn step(&mut self, x: &mut [f32], drift: &[f32], dt: f32) {
        if x.len() != drift.len() || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let sigma = (2.0 * self.temperature * dt).sqrt();
        for (xi, fi) in x.iter_mut().zip(drift) {
            *xi += fi * dt + sigma * self.entropy.next_gaussian();
        }
    }

    /// Soma ruído térmico puro `√(2·T·dt)·ξ` em `x`.
    pub fn inject(&mut self, x: &mut [f32], dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let sigma = (2.0 * self.temperature * dt).sqrt();
        for xi in x.iter_mut() {
            *xi += sigma * self.entropy.next_gaussian();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinismo_com_semente() {
        let mut a = SiliconEntropy::from_seed(7);
        let mut b = SiliconEntropy::from_seed(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert!(!a.is_hardware_seeded());
    }

    #[test]
    fn gaussiana_tem_momentos_corretos() {
        let mut e = SiliconEntropy::from_seed(0xC0FFEE);
        let n = 200_000usize;
        let (mut m1, mut m2, mut m4) = (0.0f64, 0.0f64, 0.0f64);
        for _ in 0..n {
            let g = e.next_gaussian() as f64;
            m1 += g;
            m2 += g * g;
            m4 += g * g * g * g;
        }
        let nf = n as f64;
        assert!((m1 / nf).abs() < 0.01, "media {}", m1 / nf);
        assert!((m2 / nf - 1.0).abs() < 0.02, "var {}", m2 / nf);
        assert!((m4 / nf - 3.0).abs() < 0.15, "curtose {}", m4 / nf);
    }

    #[test]
    fn modo_hardware_gera_bits_balanceados() {
        let mut e = SiliconEntropy::new();
        assert!(e.is_hardware_seeded());
        let n = 20_000u32;
        let ones: u32 = (0..n).map(|_| e.next_u64().count_ones()).sum();
        let frac = ones as f64 / (n as f64 * 64.0);
        assert!((frac - 0.5).abs() < 0.01, "fracao de 1s = {frac}");
    }
}

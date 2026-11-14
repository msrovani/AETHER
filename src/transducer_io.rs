//! Módulo 8a — transdutor universal: a fronteira física do organismo.
//!
//! Todo produto AETHER (escudo de sistema, copiloto veicular, ambiente
//! doméstico) compartilha a mesma borda: uma fonte física qualquer — contadores
//! do kernel, barramento CAN/OBD-II, CSI de Wi-Fi, áudio ambiente — é convertida
//! em um campo contínuo `u(x, t) ∈ [-1, 1]^{FIELD_DIM}`.
//!
//! Regras desta borda:
//! * nenhuma `String`, token ou comando entra no caminho de sinal;
//! * `sample_into` escreve em buffer pré-alocado (zero alocação no hot path);
//! * a fonte concreta é um detalhe de backend: o motor só vê `f32`.
//!
//! Backends previstos:
//! * [`SyntheticTransducer`] — fonte determinística para testes e demonstração;
//! * telemetria do SO (daemon, feature `m8`);
//! * CAN/OBD-II (produto A);
//! * CSI Wi-Fi / acústica (produto B).

use std::f32::consts::TAU;

use crate::error::{AetherError, Result};

/// Dimensão canônica do campo físico (Módulo 8).
pub const FIELD_DIM: usize = 64;

/// Canais de fase que o campo alimenta no protocolo de 40 bytes (M3/M5).
pub const PHASE_CHANNELS: usize = 4;

/// Fronteira única entre o mundo físico e o motor contínuo.
///
/// Implementações devem ser capazes de amostrar em [`TransducerIo::nominal_hz`]
/// sem alocar. Qualquer erro de forma é [`AetherError::Shape`].
pub trait TransducerIo {
    /// Número de dimensões do campo produzido.
    fn dim(&self) -> usize;

    /// Taxa nominal de amostragem, em Hz.
    fn nominal_hz(&self) -> f32;

    /// Amostra um quadro em `out` (comprimento exato `dim()`), em `[-1, 1]`.
    fn sample_into(&mut self, out: &mut [f32]) -> Result<()>;
}

/// Gerador determinístico leve (SplitMix64): sem dependência e sem alocação.
#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f32(&mut self) -> f32 {
        // 24 bits de mantissa, em [0, 1).
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }
}

/// Rajada de anomalia agendada sobre a linha de base.
#[derive(Debug, Clone, Copy)]
struct Burst {
    start_tick: u64,
    end_tick: u64,
    amplitude: f32,
}

/// Fonte sintética: linha de base harmônica + ruído leve, com rajada agendável.
///
/// Serve para validar a cadeia inteira sem hardware: a "assinatura saudável" é
/// um conjunto de senoides estáveis; a anomalia é uma rajada de banda larga
/// (equivalente a escrita massiva em disco, ataque ou perda de aderência).
#[derive(Debug)]
pub struct SyntheticTransducer {
    dim: usize,
    hz: f32,
    rng: SplitMix64,
    tick: u64,
    burst: Option<Burst>,
}

impl SyntheticTransducer {
    /// Cria a fonte com `dim` canais a `hz` amostras por segundo.
    pub fn new(dim: usize, hz: f32, seed: u64) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("dim do transdutor nao pode ser 0".into()));
        }
        if !(hz.is_finite() && hz > 0.0) {
            return Err(AetherError::Config("hz do transdutor deve ser > 0".into()));
        }
        Ok(Self {
            dim,
            hz,
            rng: SplitMix64::new(seed),
            tick: 0,
            burst: None,
        })
    }

    /// Agenda uma anomalia de `duration_s` segundos a partir de `start_s`.
    pub fn schedule_burst(&mut self, start_s: f32, duration_s: f32, amplitude: f32) {
        let start = (start_s.max(0.0) * self.hz) as u64;
        let len = (duration_s.max(0.0) * self.hz) as u64;
        self.burst = Some(Burst {
            start_tick: start,
            end_tick: start + len,
            amplitude: amplitude.clamp(0.0, 1.0),
        });
    }

    /// Instante atual da fonte, em segundos.
    pub fn time_s(&self) -> f32 {
        self.tick as f32 / self.hz
    }
}

impl TransducerIo for SyntheticTransducer {
    fn dim(&self) -> usize {
        self.dim
    }

    fn nominal_hz(&self) -> f32 {
        self.hz
    }

    fn sample_into(&mut self, out: &mut [f32]) -> Result<()> {
        if out.len() != self.dim {
            return Err(AetherError::shape(vec![self.dim], vec![out.len()]));
        }
        let t = self.time_s();
        let active = match self.burst {
            Some(b) => self.tick >= b.start_tick && self.tick < b.end_tick,
            None => false,
        };
        let amp = self.burst.map(|b| b.amplitude).unwrap_or(0.0);

        for (i, slot) in out.iter_mut().enumerate() {
            let fi = i as f32;
            // Linha de base: duas senoides lentas por canal + ruído baixo.
            let base = 0.32 * (TAU * (0.45 + 0.031 * fi) * t).sin()
                + 0.14 * (TAU * (0.11 + 0.007 * fi) * t + fi).sin();
            let noise = (self.rng.next_f32() - 0.5) * 0.04;
            let mut value = base + noise;

            if active {
                // Rajada de banda larga: energia alta e descoordenada.
                let hi = (TAU * (2.7 + 0.19 * fi) * t).sin();
                value += amp * (0.55 * hi + 0.25) * if i % 2 == 0 { 1.0 } else { -0.6 };
            }

            *slot = value.clamp(-1.0, 1.0);
        }

        self.tick += 1;
        Ok(())
    }
}

/// Normalizador online por canal: z-score com média/variância exponenciais,
/// comprimido por `tanh` para `(-1, 1)`.
///
/// Adapta-se à assinatura de cada dispositivo sem alocar após a construção.
#[derive(Debug)]
pub struct FieldNormalizer {
    mean: Vec<f32>,
    var: Vec<f32>,
    alpha: f32,
    eps: f32,
    primed: bool,
}

impl FieldNormalizer {
    /// Cria o normalizador para um campo de `dim` canais.
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("dim do normalizador nao pode ser 0".into()));
        }
        Ok(Self {
            mean: vec![0.0; dim],
            var: vec![1.0; dim],
            alpha: 0.02,
            eps: 1e-4,
            primed: false,
        })
    }

    /// Atualiza as estatísticas com `x` e escreve o campo normalizado em `out`.
    pub fn update_into(&mut self, x: &[f32], out: &mut [f32]) -> Result<()> {
        let dim = self.mean.len();
        if x.len() != dim || out.len() != dim {
            return Err(AetherError::shape(vec![dim], vec![x.len(), out.len()]));
        }

        if !self.primed {
            self.mean.copy_from_slice(x);
            self.primed = true;
            out.iter_mut().for_each(|v| *v = 0.0);
            return Ok(());
        }

        for i in 0..dim {
            let dev = x[i] - self.mean[i];
            self.mean[i] += self.alpha * dev;
            self.var[i] += self.alpha * (dev * dev - self.var[i]);
            let z = dev / (self.var[i] + self.eps).sqrt();
            out[i] = (z * 0.5).tanh();
        }
        Ok(())
    }
}

/// Medidor de surpresa `F(t)`: erro de predição contra a linha de base
/// adaptativa, normalizado pela própria escala de erro.
///
/// É o análogo online da Energia Livre Variacional: a "assinatura saudável" é
/// o preditor; a anomalia é o erro que ele não consegue absorver.
#[derive(Debug)]
pub struct SurpriseTracker {
    pred: Vec<f32>,
    scale: Vec<f32>,
    alpha: f32,
    beta: f32,
    eps: f32,
}

impl SurpriseTracker {
    /// Cria o medidor para um campo de `dim` canais.
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("dim do medidor de surpresa nao pode ser 0".into()));
        }
        Ok(Self {
            pred: vec![0.0; dim],
            scale: vec![1e-3; dim],
            alpha: 0.03,
            beta: 0.02,
            eps: 1e-6,
        })
    }

    /// Observa um quadro e devolve `F ∈ [0, 1)`.
    pub fn observe(&mut self, x: &[f32]) -> Result<f32> {
        let dim = self.pred.len();
        if x.len() != dim {
            return Err(AetherError::shape(vec![dim], vec![x.len()]));
        }

        let mut acc = 0.0f32;
        for i in 0..dim {
            let err = x[i] - self.pred[i];
            let e2 = err * err;
            acc += e2 / (self.scale[i] + self.eps);
            self.scale[i] += self.beta * (e2 - self.scale[i]);
            self.pred[i] += self.alpha * err;
        }
        let mean = acc / dim as f32;
        Ok((mean * 0.25).tanh())
    }
}

/// Projeta o campo de 64 canais em 4 fases de oscilador (contrato do M3/M5).
#[derive(Debug, Clone, Copy)]
pub struct PhaseMapper {
    dim: usize,
}

impl PhaseMapper {
    /// Cria o projetor para um campo de `dim` canais.
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("dim do mapeador de fase nao pode ser 0".into()));
        }
        Ok(Self { dim })
    }

    /// Escreve 4 fases em `[0, 2π)` derivadas de projeções de Fourier do campo.
    pub fn map_into(&self, field: &[f32], out: &mut [f32; PHASE_CHANNELS]) -> Result<()> {
        if field.len() != self.dim {
            return Err(AetherError::shape(vec![self.dim], vec![field.len()]));
        }
        let n = self.dim as f32;
        for (k, slot) in out.iter_mut().enumerate() {
            let harmonic = (k + 1) as f32;
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (i, value) in field.iter().enumerate() {
                let angle = TAU * harmonic * i as f32 / n;
                re += value * angle.cos();
                im += value * angle.sin();
            }
            let phase = im.atan2(re);
            *slot = if phase < 0.0 { phase + TAU } else { phase };
        }
        Ok(())
    }
}

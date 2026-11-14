//! Módulo 2 — Operador Neural de Fourier (interferência espectral).
//!
//! O campo sensorial `u(x) ∈ R^{B×C_in×L}` propaga no domínio da frequência e
//! interfere consigo mesmo, sem nenhuma matriz de atenção espacial:
//!
//! ```text
//! v(x) = σ( W_local · u(x) + F⁻¹{ R_Φ(k) ⊙ F{u}(k) } )
//! ```
//!
//! * `torch.fft` ⇒ `rustfft`/`realfft` com planos pré-compilados;
//! * truncamento modal em `K_max` bins (custo O(L log L) com modulação
//!   independente de L);
//! * `R_Φ(k) ∈ C^{C_out×C_in×K_max}` modula **fase** (atraso de onda) e
//!   **amplitude** (ganho/atenuação), com envelope passa-baixa — é isso que
//!   faz o bloco agir como filtro físico por interferência destrutiva;
//! * pesos indexados **apenas por bin**: nenhum parâmetro depende de `L`, logo
//!   o mesmo bloco processa L=128, 512, 2048, 4096 sem tocar nos pesos.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use ndarray::{Array2, Array3, ArrayView2, ArrayView3};
use num_complex::Complex32;
use rand::distributions::{Distribution, Normal};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::error::{AetherError, Result};

/// Comprimento efetivo da FFT: próxima potência de dois ≥ `l` (sempre par,
/// exigência do `realfft`). Sinais de comprimento "ruim" recebem zero-padding —
/// documentado: `forward` devolve `L` amostras, o processamento interno usa
/// `pad_len(L)`.
#[inline]
pub fn pad_len(l: usize) -> usize {
    let mut p = 4usize;
    while p < l {
        p <<= 1;
    }
    p
}

/// Bloco de interferência espectral (invariante à resolução por construção).
pub struct WaveInterferenceBlock {
    c_in: usize,
    c_out: usize,
    modes: usize,
    max_len: usize,
    /// Projeção local 1×1 `[C_out, C_in]`.
    w_local: Array2<f32>,
    /// Matriz de transferência complexa `[C_out, C_in, K_max]`.
    r_phi: Array3<Complex32>,
    /// Planos FFT pré-compilados por comprimento efetivo.
    plans: HashMap<usize, (Arc<dyn RealToComplex<f32>>, Arc<dyn ComplexToReal<f32>>)>,
    /// Espectro de entrada `[C_in, bins]` (scratch).
    spec_in: Array2<Complex32>,
    /// Espectro transferido `[C_out, bins]`.
    spec_out: Array2<Complex32>,
    /// Buffer de tempo pré-alocado (scratch).
    fft_in: Vec<f32>,
    ifft_out: Vec<f32>,
}

impl fmt::Debug for WaveInterferenceBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaveInterferenceBlock")
            .field("c_in", &self.c_in)
            .field("c_out", &self.c_out)
            .field("modes", &self.modes)
            .field("max_len", &self.max_len)
            .finish()
    }
}

impl WaveInterferenceBlock {
    /// Cria o bloco com `max_len = 4096`.
    pub fn new(c_in: usize, c_out: usize, modes: usize, seed: u64) -> Self {
        Self::with_max_len(c_in, c_out, modes, 4096, seed)
    }

    /// Cria o bloco com limites explícitos (pré-planeja todas as potências de
    /// dois até `max_len`; depois disso o caminho quente não aloca nada).
    pub fn with_max_len(c_in: usize, c_out: usize, modes: usize, max_len: usize, seed: u64) -> Self {
        assert!(c_in > 0 && c_out > 0 && modes > 0 && max_len >= 4);

        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        // W_local: Xavier uniforme.
        let limit = (6.0f32 / (c_in + c_out) as f32).sqrt();
        let mut w_local = Array2::<f32>::zeros((c_out, c_in));
        for v in w_local.iter_mut() {
            *v = (rng.gen::<f32>() * 2.0 - 1.0) * limit;
        }

        // R_Φ: amplitude ~ N(0, 1/sqrt(C_in·K)) com envelope passa-baixa e
        // fase ~ U(0, 2π). O envelope concentra energia nos modos baixos — é
        // o que dá o caráter de filtro por interferência destrutiva.
        let sigma = 1.0 / (c_in * modes) as f32;
        let normal = Normal::new(0.0, sigma.sqrt()).unwrap_or_else(|_| Normal::new(0.0, 1.0).unwrap());
        let kc = (modes.max(1) as f32) / 4.0;
        let mut r_phi = Array3::<Complex32>::zeros((c_out, c_in, modes));
        for o in 0..c_out {
            for c in 0..c_in {
                for k in 0..modes {
                    let amp = normal.sample(&mut rng).abs();
                    let envelope = 1.0 / (1.0 + k as f32 / kc);
                    let phase = rng.gen::<f32>() * std::f32::consts::TAU;
                    r_phi[[o, c, k]] = Complex32::new(
                        amp * envelope * phase.cos(),
                        amp * envelope * phase.sin(),
                    );
                }
            }
        }

        // Planos FFT pré-compilados para todas as potências de dois até max_len.
        let mut planner = RealFftPlanner::<f32>::new();
        let mut plans = HashMap::with_capacity(16);
        let mut p = 4usize;
        while p <= max_len {
            let fwd = planner.plan_fft_forward(p);
            let inv = planner.plan_fft_inverse(p);
            plans.insert(p, (fwd, inv));
            p <<= 1;
        }

        let bins = max_len / 2 + 1;
        Self {
            c_in,
            c_out,
            modes,
            max_len,
            w_local,
            r_phi,
            plans,
            spec_in: Array2::<Complex32>::zeros((c_in, bins)),
            spec_out: Array2::<Complex32>::zeros((c_out, bins)),
            fft_in: vec![0.0f32; max_len],
            ifft_out: vec![0.0f32; max_len],
        }
    }

    /// Comprimento interno atual (último `L` processado).
    pub fn modes(&self) -> usize {
        self.modes
    }

    /// Canais de entrada.
    pub fn c_in(&self) -> usize {
        self.c_in
    }

    /// Canais de saída.
    pub fn c_out(&self) -> usize {
        self.c_out
    }

    /// Espectro transferido `[C_out, bins]` (caminho de onda, sem projeção
    /// local). Válido até a próxima chamada de `forward*` com outro `L`.
    pub fn wave_spectrum(&self) -> ArrayView2<'_, Complex32> {
        self.spec_out.view()
    }

    /// Pesos espectrais (prova de invariância de resolução).
    pub fn r_phi(&self) -> ArrayView3<'_, Complex32> {
        self.r_phi.view()
    }

    /// Pesos locais (prova de invariância de resolução).
    pub fn w_local(&self) -> ArrayView2<'_, f32> {
        self.w_local.view()
    }

    /// Propagação espectral completa: `u[C_in, L] -> v[C_out, L]`.
    ///
    /// Sem alocação (usa buffers internos pré-alocados). `L` pode variar
    /// entre chamadas — os pesos não mudam.
    pub fn forward_into(
        &mut self,
        u: ArrayView2<'_, f32>,
        out: ArrayViewMut2<'_, f32>,
    ) -> Result<()> {
        let (ci, l) = u.dim();
        if ci != self.c_in {
            return Err(AetherError::Shape {
                expected: vec![self.c_in, l],
                got: vec![ci, l],
            });
        }
        if l > self.max_len {
            return Err(AetherError::Config(format!(
                "L={l} excede max_len={}; recrie o bloco com with_max_len",
                self.max_len
            )));
        }
        if out.shape() != &[self.c_out, l] {
            return Err(AetherError::Shape {
                expected: vec![self.c_out, l],
                got: out.shape().to_vec(),
            });
        }
        let u_slice = u
            .as_slice()
            .ok_or_else(|| AetherError::Config("campo de entrada precisa ser contíguo".into()))?;

        let eff = pad_len(l);
        let bins = eff / 2 + 1;
        let keep = self.modes.min(bins);
        let (fwd, inv) = self
            .plans
            .get(&eff)
            .cloned()
            .ok_or_else(|| AetherError::Config(format!("sem plano FFT para {eff}")))?;

        let fft_in = &mut self.fft_in[..eff];
        let spec_in = self.spec_in.as_slice_mut().expect("spec_in contíguo");
        let spec_out = self.spec_out.as_slice_mut().expect("spec_out contíguo");
        let ifft_out = &mut self.ifft_out[..eff];
        let r_phi = self.r_phi.as_slice().expect("r_phi contíguo");
        let w_local = self.w_local.as_slice().expect("w_local contíguo");
        let out_slice = out
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("saída precisa ser contígua".into()))?;

        // 1) ida ao domínio da frequência (zero-padding até `eff`).
        for c in 0..ci {
            fft_in[..l].copy_from_slice(&u_slice[c * l..(c + 1) * l]);
            fft_in[l..eff].fill(0.0);
            fwd.process(fft_in, &mut spec_in[c * bins..(c + 1) * bins])
                .map_err(|e| AetherError::Degenerate(format!("fft: {e}")))?;
        }

        // 2) interferência espectral: contracão de R_Φ(k) ⊙ U(k) sobre C_in,
        //    truncada nos `keep` primeiros modos (ruído de alta frequência
        //    simplesmente não existe aqui).
        for o in 0..self.c_out {
            let obase = o * bins;
            spec_out[obase..obase + bins].fill(Complex32::new(0.0, 0.0));
            for c in 0..ci {
                let cbase = c * bins;
                let rbase = (o * ci + c) * self.modes;
                for k in 0..keep {
                    spec_out[obase + k] += r_phi[rbase + k] * spec_in[cbase + k];
                }
            }
        }

        // 3) volta ao domínio contínuo (a inversa não é normalizada: dividir
        //    por `eff`), soma da projeção local 1×1 e ativação σ = tanh
        //    (simetria de fase: queremos valores negativos também).
        let inv_scale = 1.0f32 / eff as f32;
        for o in 0..self.c_out {
            inv
                .process(
                    &mut spec_out[o * bins..o * bins + bins],
                    ifft_out,
                )
                .map_err(|e| AetherError::Degenerate(format!("ifft: {e}")))?;
            let wbase = o * ci;
            for k in 0..l {
                let mut local = 0.0f32;
                for c in 0..ci {
                    local += w_local[wbase + c] * u_slice[c * l + k];
                }
                out_slice[o * l + k] = (local + ifft_out[k] * inv_scale).tanh();
            }
        }

        Ok(())
    }

    /// Conveniência que aloca a saída.
    pub fn forward(&mut self, u: ArrayView2<'_, f32>) -> Result<Array2<f32>> {
        let (ci, l) = u.dim();
        let mut out = Array2::<f32>::zeros((self.c_out, l));
        if ci != self.c_in {
            return Err(AetherError::Shape {
                expected: vec![self.c_in, l],
                got: vec![ci, l],
            });
        }
        self.forward_into(u, out.view_mut())?;
        Ok(out)
    }

    /// Lote `[B, C_in, L] -> [B, C_out, L]`.
    pub fn forward_batched(&mut self, u: ArrayView3<'_, f32>) -> Result<Array3<f32>> {
        let (b, ci, l) = u.dim();
        if ci != self.c_in {
            return Err(AetherError::Shape {
                expected: vec![b, self.c_in, l],
                got: vec![b, ci, l],
            });
        }
        let mut out = Array3::<f32>::zeros((b, self.c_out, l));
        for k in 0..b {
            self.forward_into(
                u.index_axis(ndarray::Axis(0), k),
                out.index_axis_mut(ndarray::Axis(0), k),
            )?;
        }
        Ok(out)
    }

    /// Entropia espectral do campo **de entrada**, normalizada em [0,1]:
    ///
    /// `H = -Σ p_k ln p_k / ln K`, com `p_k = |U_k|² / Σ|U|²` sobre os `K`
    /// modos retidos, média sobre os canais. Campo coerente → H baixa;
    /// campo de ruído → H alta.
    pub fn compute_spectral_entropy(&mut self, u: ArrayView2<'_, f32>) -> Result<f64> {
        let (ci, l) = u.dim();
        if ci != self.c_in {
            return Err(AetherError::Shape {
                expected: vec![self.c_in, l],
                got: vec![ci, l],
            });
        }
        if l > self.max_len {
            return Err(AetherError::Config(format!(
                "L={l} excede max_len={}",
                self.max_len
            )));
        }
        let u_slice = u
            .as_slice()
            .ok_or_else(|| AetherError::Config("campo de entrada precisa ser contíguo".into()))?;
        let eff = pad_len(l);
        let bins = eff / 2 + 1;
        let keep = self.modes.min(bins).max(1);
        let (fwd, _) = self
            .plans
            .get(&eff)
            .cloned()
            .ok_or_else(|| AetherError::Config(format!("sem plano FFT para {eff}")))?;

        let fft_in = &mut self.fft_in[..eff];
        let spec_in = self.spec_in.as_slice_mut().expect("spec_in contíguo");
        for c in 0..ci {
            fft_in[..l].copy_from_slice(&u_slice[c * l..(c + 1) * l]);
            fft_in[l..eff].fill(0.0);
            fwd.process(fft_in, &mut spec_in[c * bins..(c + 1) * bins])
                .map_err(|e| AetherError::Degenerate(format!("fft: {e}")))?;
        }

        let ln_k = (keep as f64).ln();
        if ln_k <= 0.0 {
            return Ok(0.0);
        }
        let mut h_total = 0.0f64;
        for c in 0..ci {
            let base = c * bins;
            let mut total = 0.0f64;
            for k in 0..keep {
                total += spec_in[base + k].norm_sqr() as f64;
            }
            if total <= 0.0 {
                continue;
            }
            let mut h = 0.0f64;
            for k in 0..keep {
                let p = spec_in[base + k].norm_sqr() as f64 / total;
                if p > 0.0 {
                    h -= p * p.ln();
                }
            }
            h_total += h / ln_k;
        }
        Ok(h_total / ci as f64)
    }

    /// Entropia do espectro **transferido** (após a interferência), sobre os
    /// mesmos `K` modos. Com o envelope passa-baixa de `R_Φ`, o resultado é
    /// sistematicamente menor que a entropia do campo cru — é a medida do
    /// colapso de incerteza.
    pub fn compute_wave_entropy(&self) -> f64 {
        let bins = self.spec_out.ncols();
        let keep = self.modes.min(bins).max(1);
        let cells = self.c_out * keep;
        if cells <= 1 {
            return 0.0;
        }
        let ln_cells = (cells as f64).ln();
        let mut total = 0.0f64;
        for o in 0..self.c_out {
            for k in 0..keep {
                total += self.spec_out[[o, k]].norm_sqr() as f64;
            }
        }
        if total <= 0.0 {
            return 0.0;
        }
        let mut h = 0.0f64;
        for o in 0..self.c_out {
            for k in 0..keep {
                let p = self.spec_out[[o, k]].norm_sqr() as f64 / total;
                if p > 0.0 {
                    h -= p * p.ln();
                }
            }
        }
        h / ln_cells
    }
}

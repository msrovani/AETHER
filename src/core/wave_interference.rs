//! Módulo 2 — Operador Neural de Fourier (interferência espectral).
//!
//! Camada puramente contínua, sem atenção e sem tokens. Para cada canal de
//! entrada `u ∈ R^{C_in × L}` o bloco executa
//!
//! ```text
//! v(x) = tanh( W_local · u(x)  +  F^{-1}{ R_phi(k) ⊙ F{u} }(x) )
//! ```
//!
//! com `R_phi ∈ C^{C_out × C_in × modes}` modulando amplitude (ganho/atenuação)
//! e fase (atraso) dos primeiros `modes` bins espectrais. O resto do espectro é
//! descartado (truncamento modal): o custo da modulação independe de `L` e o
//! bloco funciona como filtro de banda.
//!
//! # Invariância de resolução (`INV-W1`, `INV-W2`)
//!
//! `R_phi` é indexado **apenas** por bin (`0..modes`); nenhum parâmetro depende
//! de `L`. A mesma instância processa `L = 128`, `512`, `2048`, `4096`, ... com
//! pesos bit-a-bit idênticos: o plano FFT de cada `L` novo entra no cache
//! (`HashMap<usize, Arc<dyn RealToComplex<f32>>>`) na primeira chamada, e é
//! isso — somente isso — que muda.
//!
//! # Comprimento ímpar / "ruim"
//!
//! `realfft` transforma comprimentos pares com melhor desempenho. Para `L`
//! ímpar o sinal é **zero-padded** até a próxima potência de dois
//! (`L = 255 -> 256`, `L = 999 -> 1024`). A FFT usa esse comprimento efetivo e
//! a saída continua sendo recortada de volta para `[C_out, L]`, de modo que
//! `forward` **sempre retorna `[C_out, L]`** com o `L` original do pedido.
//! Para `L` par o comprimento efetivo é o próprio `L`.
//!
//! # Caminho quente sem alocação
//!
//! `with_max_len(.., max_len, ..)` pré-aloca os buffers de FFT/scratch e os
//! planos para o comprimento efetivo de `max_len`. Enquanto `L <= max_len`,
//! `forward_into` não aloca nada (planos e scratch são reutilizados; o
//! `Arc::clone` do plano é uma contagem de referência, não uma alocação).
//! `forward`/`forward_batched` alocam apenas o tensor de saída.
//!
//! # Entropias
//!
//! * [`WaveInterferenceBlock::compute_spectral_entropy`] mede a entropia
//!   espectral normalizada do **campo cru**
//!   `H = -Σ p_k ln p_k / ln K`, `p_k = |U_k|² / Σ|U|²`, `K = L_eff/2 + 1`
//!   (todos os bins). Fica em `[0, 1]`; campo coerente (senoide) dá `H ≈ 0`.
//! * [`WaveInterferenceBlock::compute_wave_entropy`] mede a ocupação modal do
//!   **espectro já transferido** (`wave_spectrum`, `modes` bins) usando o
//!   **mesmo denominador** `ln K` (normalização comum). Como o caminho de onda
//!   é band-limited por construção (`modes` bins), a entropia resultante é
//!   limitada por `ln(modes)/ln(K) < 1` e, para ruído de banda larga, é
//!   estritamente menor que a entropia do campo cru — é o `INV-W5`.
//!
//! Nada aqui usa `unsafe`, `Vec<Vec<_>>`, `serde` ou alocação no laço quente.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use ndarray::{Array2, Array3, ArrayView2, ArrayView3, ArrayViewMut2, Axis};
use num_complex::Complex32;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::error::{AetherError, Result};

/// Fator `2π` em `f32`.
const TAU: f32 = std::f32::consts::TAU;

/// Comprimento efetivo da FFT para um pedido de `l` amostras.
///
/// Par → `l`; ímpar → próxima potência de dois (zero-padding documentado no
/// cabeçalho do módulo). `l == 0` nunca chega aqui (é rejeitado antes), mas o
/// caso degenerado devolve `2` para manter a função total.
#[inline]
fn fft_len_for(l: usize) -> usize {
    if l == 0 {
        2
    } else if l % 2 == 0 {
        l
    } else {
        l.next_power_of_two().max(2)
    }
}

/// Módulo ao quadrado de um bin, em `f64` (potência espectral).
#[inline]
fn bin_power(z: &Complex32) -> f64 {
    let (re, im) = (f64::from(z.re), f64::from(z.im));
    re * re + im * im
}

/// Erro de FFT mapeado para o domínio do crate.
#[inline]
fn fft_error(err: realfft::FftError) -> AetherError {
    AetherError::Degenerate(format!("fft: {err}"))
}

/// `tanh` vetorizável (Padé 7/6, sem `libm`).
///
/// A chamada a `f32::tanh` dominava o caminho quente (~30 ns por elemento, e a
/// projeção local tem `C_out * L` elementos). Esta forma é aritmética pura —
/// logo, vetorizável pelo LLVM — com erro absoluto ≤ 1e-4 em `|x| ≤ 5.5` e
/// saturação em `±1` fora da faixa, preservando simetria e limites.
///
/// A faixa de saturação é 5.5 (e não 4.5) porque `1 - tanh(T) = 2e^{-2T}`:
/// truncar em 4.5 deixaria 2.5e-4 de erro no degrau.
#[inline(always)]
fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-5.5, 5.5);
    let x2 = x * x;
    let num = x * (135_135.0 + x2 * (17_325.0 + x2 * (378.0 + x2)));
    let den = 135_135.0 + x2 * (62_370.0 + x2 * (3_150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}

/// Amostra de uma normal padrão via Box-Muller (sem dependência extra).
#[inline]
fn normal_unit(rng: &mut ChaCha8Rng) -> f32 {
    let u1 = rng.gen::<f32>().max(f32::MIN_POSITIVE);
    let u2 = rng.gen::<f32>();
    (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos()
}

/// Planos FFT cacheados por comprimento (frente e inverso).
///
/// Vive atrás de um `RefCell` porque a API canônica expõe
/// `compute_spectral_entropy(&self, ..)`: o cache de planos precisa poder
/// crescer mesmo sem `&mut self` (a entropia usa o mesmo transformador real).
struct FftPlans {
    planner: RealFftPlanner<f32>,
    kernels: HashMap<usize, Arc<dyn RealToComplex<f32>>>,
    inverses: HashMap<usize, Arc<dyn ComplexToReal<f32>>>,
}

impl FftPlans {
    fn new() -> Self {
        Self {
            planner: RealFftPlanner::new(),
            kernels: HashMap::new(),
            inverses: HashMap::new(),
        }
    }

    /// Garante os planos para `n` e devolve cópias `Arc` (contagem de
    /// referência, sem alocação de heap).
    fn ensure(&mut self, n: usize) -> (Arc<dyn RealToComplex<f32>>, Arc<dyn ComplexToReal<f32>>) {
        let fwd = match self.kernels.get(&n) {
            Some(k) => Arc::clone(k),
            None => {
                let k = self.planner.plan_fft_forward(n);
                self.kernels.insert(n, Arc::clone(&k));
                k
            }
        };
        let inv = match self.inverses.get(&n) {
            Some(k) => Arc::clone(k),
            None => {
                let k = self.planner.plan_fft_inverse(n);
                self.inverses.insert(n, Arc::clone(&k));
                k
            }
        };
        (fwd, inv)
    }
}

impl fmt::Debug for FftPlans {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FftPlans")
            .field("lengths", &self.kernels.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Buffers do caminho quente, dimensionados por `(C_in, C_out, L_fft)`.
struct Scratch {
    len: usize,
    bins: usize,
    time_in: Array2<f32>,
    spec_in: Array2<Complex32>,
    spec_out: Array2<Complex32>,
    time_out: Array2<f32>,
    fwd_scratch: Vec<Complex32>,
    inv_scratch: Vec<Complex32>,
}

impl Scratch {
    fn empty() -> Self {
        Self {
            len: 0,
            bins: 0,
            time_in: Array2::zeros((0, 0)),
            spec_in: Array2::zeros((0, 0)),
            spec_out: Array2::zeros((0, 0)),
            time_out: Array2::zeros((0, 0)),
            fwd_scratch: Vec::new(),
            inv_scratch: Vec::new(),
        }
    }

    /// Redimensiona somente quando o comprimento efetivo muda. No laço quente
    /// (`L` constante) esta chamada é um `if` e retorna.
    fn ensure(&mut self, c_in: usize, c_out: usize, n: usize, fwd_s: usize, inv_s: usize) {
        if self.len == n && self.time_in.nrows() == c_in && self.time_out.nrows() == c_out {
            return;
        }
        self.len = n;
        self.bins = n / 2 + 1;
        self.time_in = Array2::zeros((c_in, n));
        self.spec_in = Array2::zeros((c_in, self.bins));
        self.spec_out = Array2::zeros((c_out, self.bins));
        self.time_out = Array2::zeros((c_out, n));
        self.fwd_scratch = vec![Complex32::new(0.0, 0.0); fwd_s];
        self.inv_scratch = vec![Complex32::new(0.0, 0.0); inv_s];
    }
}

/// Scratch exclusivo da entropia do campo cru (usado por `&self`).
struct EntropyScratch {
    len: usize,
    time: Array2<f32>,
    spec: Array2<Complex32>,
    scratch: Vec<Complex32>,
}

impl EntropyScratch {
    fn empty() -> Self {
        Self {
            len: 0,
            time: Array2::zeros((0, 0)),
            spec: Array2::zeros((0, 0)),
            scratch: Vec::new(),
        }
    }

    fn ensure(&mut self, c_in: usize, n: usize, scratch_len: usize) {
        if self.len == n && self.time.nrows() == c_in {
            return;
        }
        self.len = n;
        self.time = Array2::zeros((c_in, n));
        self.spec = Array2::zeros((c_in, n / 2 + 1));
        self.scratch = vec![Complex32::new(0.0, 0.0); scratch_len];
    }
}

/// Operador neural de Fourier 1D.
///
/// Ver o cabeçalho do módulo para o contrato matemático e os invariantes.
pub struct WaveInterferenceBlock {
    c_in: usize,
    c_out: usize,
    modes: usize,
    /// `0` = sem limite (scratch sob demanda); `> 0` = caminho quente fixo.
    max_len: usize,
    /// `[C_out, C_in]` em ordem C.
    w_local: Array2<f32>,
    /// `[C_out, C_in, modes]` em ordem C (amplitude + fase por bin).
    r_phi: Array3<Complex32>,
    plans: RefCell<FftPlans>,
    scratch: Scratch,
    entropy: RefCell<EntropyScratch>,
    /// Espectro transferido do último `forward` — `[C_out, modes]`.
    wave_spectrum: Array2<Complex32>,
    /// `L_fft / 2 + 1` do último `forward` (denominador comum das entropias).
    last_bins: usize,
}

impl fmt::Debug for WaveInterferenceBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaveInterferenceBlock")
            .field("c_in", &self.c_in)
            .field("c_out", &self.c_out)
            .field("modes", &self.modes)
            .field("max_len", &self.max_len)
            .field("fft_len", &self.scratch.len)
            .field("wave_bins", &self.last_bins)
            .finish_non_exhaustive()
    }
}

impl WaveInterferenceBlock {
    /// Bloco sem comprimento máximo declarado: o scratch de cada `L` novo é
    /// alocado na primeira chamada (permitido fora do laço quente).
    pub fn new(c_in: usize, c_out: usize, modes: usize, seed: u64) -> Self {
        Self::with_max_len(c_in, c_out, modes, 0, seed)
    }

    /// Bloco com scratch pré-alocado para `L <= max_len`.
    ///
    /// Planos FFT, buffers e scratch são criados aqui — depois disso
    /// [`Self::forward_into`] com `L <= max_len` não aloca.
    pub fn with_max_len(
        c_in: usize,
        c_out: usize,
        modes: usize,
        max_len: usize,
        seed: u64,
    ) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        // R_phi: amplitude ~ N(0, 1/sqrt(C_in * modes)), fase ~ U(0, 2π).
        let sigma = if c_in > 0 && modes > 0 {
            1.0 / ((c_in * modes) as f32).sqrt()
        } else {
            0.0
        };
        let mut r_phi = Array3::<Complex32>::zeros((c_out, c_in, modes));
        for value in r_phi.iter_mut() {
            let amp = normal_unit(&mut rng) * sigma;
            let phase = rng.gen::<f32>() * TAU;
            *value = Complex32::new(amp * phase.cos(), amp * phase.sin());
        }

        // W_local: Xavier uniforme, limit = sqrt(6 / (fan_in + fan_out)).
        let limit = if c_in + c_out > 0 {
            (6.0 / (c_in + c_out) as f32).sqrt()
        } else {
            0.0
        };
        let mut w_local = Array2::<f32>::zeros((c_out, c_in));
        for value in w_local.iter_mut() {
            *value = if limit > 0.0 {
                rng.gen_range(-limit..=limit)
            } else {
                0.0
            };
        }

        let mut block = Self {
            c_in,
            c_out,
            modes,
            max_len,
            w_local,
            r_phi,
            plans: RefCell::new(FftPlans::new()),
            scratch: Scratch::empty(),
            entropy: RefCell::new(EntropyScratch::empty()),
            wave_spectrum: Array2::<Complex32>::zeros((c_out, modes)),
            last_bins: 0,
        };
        // Pré-aquecimento: planos + buffers + scratch ficam prontos antes do
        // primeiro quadro.
        block.reserve(max_len);
        block
    }

    /// Pré-aloca planos e buffers para o comprimento efetivo de `max_len`.
    fn reserve(&mut self, max_len: usize) {
        if max_len == 0 {
            return;
        }
        let n = fft_len_for(max_len);
        let (fwd, inv) = self.plans.borrow_mut().ensure(n);
        self.scratch.ensure(
            self.c_in,
            self.c_out,
            n,
            fwd.get_scratch_len(),
            inv.get_scratch_len(),
        );
    }

    /// Comprimento efetivo de FFT usado para um pedido de `l` amostras.
    #[inline]
    fn working_len(&self, l: usize) -> usize {
        if self.max_len == 0 {
            fft_len_for(l)
        } else {
            fft_len_for(self.max_len.max(l))
        }
    }

    /// `u: [C_in, L] -> v: [C_out, L]` (aloca a saída).
    ///
    /// Para `L` ímpar a FFT interna usa a próxima potência de dois (zero
    /// padding); a saída é recortada para o `L` pedido — ver o cabeçalho.
    pub fn forward(&mut self, u: ArrayView2<'_, f32>) -> Result<Array2<f32>> {
        let l = u.ncols();
        let mut out = Array2::<f32>::zeros((self.c_out, l));
        self.dispatch(u, out.view_mut())?;
        Ok(out)
    }

    /// Caminho quente `u: [C_in, L] -> out: [C_out, L]`, sem alocação quando o
    /// bloco foi criado com [`Self::with_max_len`] e `L <= max_len`.
    pub fn forward_into(&mut self, u: ArrayView2<'_, f32>, out: &mut Array2<f32>) -> Result<()> {
        self.dispatch(u, out.view_mut())
    }

    /// `u: [B, C_in, L] -> v: [B, C_out, L]`. Cada item usa exatamente o mesmo
    /// caminho escalar, logo o resultado é bit-a-bit igual ao de `forward`
    /// item a item.
    pub fn forward_batched(&mut self, u: ArrayView3<'_, f32>) -> Result<Array3<f32>> {
        let (batch, channels, l) = u.dim();
        if channels != self.c_in {
            return Err(AetherError::shape(
                [batch, self.c_in, l],
                [batch, channels, l],
            ));
        }
        let mut out = Array3::<f32>::zeros((batch, self.c_out, l));
        for b in 0..batch {
            self.dispatch(u.index_axis(Axis(0), b), out.index_axis_mut(Axis(0), b))?;
        }
        Ok(out)
    }

    /// Núcleo único de `forward`/`forward_into`/`forward_batched`.
    fn dispatch(&mut self, u: ArrayView2<'_, f32>, mut out: ArrayViewMut2<'_, f32>) -> Result<()> {
        let (c_in, c_out, modes) = (self.c_in, self.c_out, self.modes);
        if c_in == 0 || c_out == 0 || modes == 0 {
            return Err(AetherError::Config(
                "wave: c_in, c_out e modes devem ser > 0".into(),
            ));
        }
        if u.nrows() != c_in {
            return Err(AetherError::shape([c_in, u.ncols()], u.shape().to_vec()));
        }
        let l = u.ncols();
        if l == 0 {
            return Err(AetherError::Config("wave: L == 0".into()));
        }
        if out.dim() != (c_out, l) {
            return Err(AetherError::shape([c_out, l], [out.nrows(), out.ncols()]));
        }

        let n = self.working_len(l);
        let nb = n / 2 + 1;
        let (fwd, inv) = self.plans.borrow_mut().ensure(n);
        self.scratch
            .ensure(c_in, c_out, n, fwd.get_scratch_len(), inv.get_scratch_len());
        let sc = &mut self.scratch;

        // 1) Copia u para o buffer de comprimento efetivo. Com `n == l` nao ha
        //    padding: zera-se apenas a cauda quando ela existe.
        {
            let time_flat = sc
                .time_in
                .as_slice_mut()
                .expect("time_in contíguo (ordem C)");
            for c in 0..c_in {
                let row = &mut time_flat[c * n..(c + 1) * n];
                let src = u.row(c);
                for (dst, s) in row[..l].iter_mut().zip(src.iter()) {
                    *dst = *s;
                }
                if n > l {
                    for dst in row[l..].iter_mut() {
                        *dst = 0.0;
                    }
                }
            }
        }

        // 2) FFT real canal a canal -> [C_in, nb].
        {
            let fft_scratch = &mut sc.fwd_scratch;
            let time = sc
                .time_in
                .as_slice_mut()
                .expect("time_in contíguo (ordem C)");
            let spec_in = sc
                .spec_in
                .as_slice_mut()
                .expect("spec_in contíguo (ordem C)");
            for c in 0..c_in {
                let input = &mut time[c * n..(c + 1) * n];
                let output = &mut spec_in[c * nb..(c + 1) * nb];
                fwd.process_with_scratch(input, output, fft_scratch)
                    .map_err(fft_error)?;
            }
        }

        // 3) Truncamento modal + contração sobre C_in:
        //    Y[o,k] = sum_c R_phi[o,c,k] * U[c,k], k < min(modes, nb).
        let k_eff = modes.min(nb);
        sc.spec_out.fill(Complex32::new(0.0, 0.0));
        {
            let r = self
                .r_phi
                .as_slice()
                .expect("r_phi contíguo (ordem C) [C_out,C_in,modes]");
            let spec_in = sc.spec_in.as_slice().expect("spec_in contíguo (ordem C)");
            let spec_out = sc
                .spec_out
                .as_slice_mut()
                .expect("spec_out contíguo (ordem C)");
            for o in 0..c_out {
                let out_row = &mut spec_out[o * nb..o * nb + k_eff];
                for c in 0..c_in {
                    let base = (o * c_in + c) * modes;
                    let weight = &r[base..base + k_eff];
                    let in_row = &spec_in[c * nb..c * nb + k_eff];
                    for ((acc, &w), &x) in out_row.iter_mut().zip(weight).zip(in_row) {
                        *acc += w * x;
                    }
                }
            }
        }

        // 4) DC e Nyquist precisam de parte imaginária nula para a irfft.
        //    (DC não tem fase fisicamente significativa para sinal real.)
        {
            let spec_out = sc
                .spec_out
                .as_slice_mut()
                .expect("spec_out contíguo (ordem C)");
            for row in spec_out.chunks_mut(nb) {
                row[0].im = 0.0;
                row[nb - 1].im = 0.0;
            }
        }

        // 5) Publica o espectro transferido [C_out, modes] para o M4.
        {
            self.wave_spectrum.fill(Complex32::new(0.0, 0.0));
            let spec_out = sc.spec_out.as_slice().expect("spec_out contíguo");
            let dst = self
                .wave_spectrum
                .as_slice_mut()
                .expect("wave_spectrum contíguo (ordem C)");
            for (dst_row, src_row) in dst.chunks_mut(modes).zip(spec_out.chunks(nb)) {
                dst_row[..k_eff].copy_from_slice(&src_row[..k_eff]);
            }
        }
        self.last_bins = nb;

        // 6) irfft de volta ao domínio do tempo.
        {
            let inv_scratch = &mut sc.inv_scratch;
            let spec_out = sc
                .spec_out
                .as_slice_mut()
                .expect("spec_out contíguo (ordem C)");
            let time_out = sc
                .time_out
                .as_slice_mut()
                .expect("time_out contíguo (ordem C)");
            for o in 0..c_out {
                let spec = &mut spec_out[o * nb..(o + 1) * nb];
                let dst = &mut time_out[o * n..(o + 1) * n];
                inv.process_with_scratch(spec, dst, inv_scratch)
                    .map_err(fft_error)?;
            }
        }

        // 7) Projeção local 1x1 + ativação tanh (simetria de fase), já
        //    recortando para o L original.
        //
        //    Estrutura escolhida para vetorização: para cada canal de saída, a
        //    projeção vira `C_in` AXPYs contíguas (um `zip` de fatias por canal)
        //    e a ativação vira uma passada separada sobre a linha — sem
        //    indexação `c * n + x` dentro do laço interno.
        {
            let time_in = sc.time_in.as_slice().expect("time_in contíguo (ordem C)");
            let time_out = sc
                .time_out
                .as_slice_mut()
                .expect("time_out contíguo (ordem C)");
            let w_local = self
                .w_local
                .as_slice()
                .expect("w_local contíguo (ordem C) [C_out,C_in]");
            for o in 0..c_out {
                let weights = &w_local[o * c_in..(o + 1) * c_in];
                let row = &mut time_out[o * n..o * n + l];

                if let Some(&w0) = weights.first() {
                    let src = &time_in[..l];
                    for (acc, &x) in row.iter_mut().zip(src.iter()) {
                        *acc += w0 * x;
                    }
                }
                for (c, &wc) in weights.iter().enumerate().skip(1) {
                    let src = &time_in[c * n..c * n + l];
                    for (acc, &x) in row.iter_mut().zip(src.iter()) {
                        *acc += wc * x;
                    }
                }
                for acc in row.iter_mut() {
                    *acc = fast_tanh(*acc);
                }
            }
        }

        // 8) Copia o recorte [C_out, L] para a saída (qualquer layout).
        for (mut dst_row, src_row) in out.outer_iter_mut().zip(self.scratch.time_out.outer_iter()) {
            for (dst, src) in dst_row.iter_mut().zip(src_row.iter()) {
                *dst = *src;
            }
        }
        Ok(())
    }

    /// Entropia espectral normalizada do campo de entrada, em `[0, 1]`:
    ///
    /// `H = -Σ p_k ln p_k / ln K`, `p_k = |U_k|² / Σ|U|²`,
    /// `K = L_eff/2 + 1` (todos os bins). Sinal nulo ⇒ `0.0`.
    ///
    /// Usa o scratch interno pré-alocado (a alocação só ocorre quando aparece
    /// um `L_eff` novo); no laço quente de `L` constante não aloca.
    pub fn compute_spectral_entropy(&self, u: ArrayView2<'_, f32>) -> Result<f64> {
        if self.c_in == 0 {
            return Err(AetherError::Config("wave: c_in deve ser > 0".into()));
        }
        if u.nrows() != self.c_in {
            return Err(AetherError::shape(
                [self.c_in, u.ncols()],
                u.shape().to_vec(),
            ));
        }
        let l = u.ncols();
        if l == 0 {
            return Err(AetherError::Config("wave: L == 0".into()));
        }
        if l == 1 {
            // Um único sample já é "toda a energia no DC".
            return Ok(0.0);
        }

        let n = fft_len_for(l);
        let nb = n / 2 + 1;
        let (fwd, _) = self.plans.borrow_mut().ensure(n);
        let scratch_len = fwd.get_scratch_len();

        let mut guard = self.entropy.borrow_mut();
        let ent: &mut EntropyScratch = &mut guard;
        ent.ensure(self.c_in, n, scratch_len);
        let EntropyScratch {
            time,
            spec,
            scratch: fft_scratch,
            ..
        } = ent;

        time.fill(0.0);
        for (mut dst_row, src_row) in time.outer_iter_mut().zip(u.outer_iter()) {
            for (dst, src) in dst_row.iter_mut().zip(src_row.iter()) {
                *dst = *src;
            }
        }
        {
            let time_slice = time.as_slice_mut().expect("entropy.time contíguo");
            let spec_slice = spec.as_slice_mut().expect("entropy.spec contíguo");
            for c in 0..self.c_in {
                let input = &mut time_slice[c * n..(c + 1) * n];
                let output = &mut spec_slice[c * nb..(c + 1) * nb];
                fwd.process_with_scratch(input, output, fft_scratch)
                    .map_err(fft_error)?;
            }
        }

        // Potência por bin somada sobre os canais: p_k = Σ_c |U_c,k|² / total.
        // A distribuição tem K = nb categorias (bins), não C_in*nb: somar os
        // canais antes de normalizar é o que dá a entropia *espectral*.
        let mut total = 0.0f64;
        for k in 0..nb {
            total += spec.column(k).iter().map(bin_power).sum::<f64>();
        }
        if total <= 0.0 || !total.is_finite() {
            return Ok(0.0);
        }
        let mut h = 0.0f64;
        for k in 0..nb {
            let p = spec.column(k).iter().map(bin_power).sum::<f64>() / total;
            if p > 0.0 {
                h -= p * p.ln();
            }
        }
        let k = nb as f64;
        Ok((h / k.ln()).clamp(0.0, 1.0))
    }

    /// Entropia do espectro **já transferido** (`wave_spectrum`), em `[0, 1]`.
    ///
    /// `p_k = Σ_o |Y_o,k|² / Σ_{o,k}|Y|²` sobre os `modes` bins e denominador
    /// comum `ln K` (`K = L_eff/2 + 1` do último `forward`). Como o caminho de
    /// onda é band-limited, `H_wave <= ln(modes)/ln(K)` e, para ruído de banda
    /// larga, `H_wave <= H_raw` (`INV-W5`). Sem `forward` anterior ⇒ `0.0`.
    pub fn compute_wave_entropy(&self) -> f64 {
        if self.modes == 0 || self.last_bins < 2 {
            return 0.0;
        }
        let k_eff = self.modes.min(self.last_bins);
        // Potência por bin somada sobre os canais de saída: a distribuição tem
        // `k_eff` categorias (bins), não `C_out * modes`.
        let spec = &self.wave_spectrum;

        let total: f64 = (0..k_eff)
            .map(|k| spec.column(k).iter().map(bin_power).sum::<f64>())
            .sum();
        if total <= 0.0 || !total.is_finite() {
            return 0.0;
        }
        let mut h = 0.0f64;
        for k in 0..k_eff {
            let p = spec.column(k).iter().map(bin_power).sum::<f64>() / total;
            if p > 0.0 {
                h -= p * p.ln();
            }
        }
        let k_ref = self.last_bins as f64;
        (h / k_ref.ln()).clamp(0.0, 1.0)
    }

    /// Espectro transferido do último `forward`: `[C_out, modes]`.
    ///
    /// É o caminho de onda (após `R_phi`, antes da projeção local), consumido
    /// pelo Módulo 4 como banco de ressonadores (`spec[[o,k]]`, `k < modes`).
    pub fn wave_spectrum(&self) -> ArrayView2<'_, Complex32> {
        self.wave_spectrum.view()
    }

    /// Pesos da matriz de transferência, ordem C `[C_out, C_in, modes]`
    /// (leitura: prova de `INV-W1`).
    pub fn r_phi(&self) -> &[Complex32] {
        self.r_phi
            .as_slice()
            .expect("r_phi é contíguo (ordem C) por construção")
    }

    /// Pesos da projeção local 1x1, ordem C `[C_out, C_in]` (leitura).
    pub fn w_local(&self) -> &[f32] {
        self.w_local
            .as_slice()
            .expect("w_local é contíguo (ordem C) por construção")
    }

    /// Número de bins espectrais mantidos (truncamento modal).
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
}

#[cfg(test)]
mod tanh_tests {
    use super::fast_tanh;

    /// A otimizacao troca `f32::tanh` (libm) por Pade 7/6 vetorizavel.
    /// Este teste fixa o tamanho do desvio aceito: 1e-4.
    #[test]
    fn fast_tanh_dentro_de_1e4_do_tanh_exato() {
        let mut worst = 0.0f32;
        let mut worst_at = 0.0f32;
        let mut x = -12.0f32;
        while x <= 12.0 {
            let err = (fast_tanh(x) - x.tanh()).abs();
            if err > worst {
                worst = err;
                worst_at = x;
            }
            x += 0.001;
        }
        assert!(worst <= 1e-4, "erro maximo {worst:.3e} em x={worst_at}");
    }

    #[test]
    fn fast_tanh_limitado_monotono_e_simetrico() {
        let mut prev = -1.0f32;
        for i in -400..=400 {
            let x = i as f32 * 0.05;
            let y = fast_tanh(x);
            assert!(y.abs() <= 1.0 + 1e-6, "saida fora de [-1,1] em x={x}: {y}");
            assert!(y >= prev - 1e-6, "nao monotonico em x={x}");
            assert!((fast_tanh(-x) + y).abs() < 1e-6, "assimetria em x={x}");
            prev = y;
        }
    }
}

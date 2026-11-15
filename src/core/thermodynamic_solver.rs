//! MÃ³dulo 1 â€” Relaxamento termodinÃ¢mico por BifurcaÃ§Ã£o Simulada BalÃ­stica (bSB).
//!
//! Resolve instÃ¢ncias de Ising
//!
//! ```text
//! H(s) = -0.5 * sum_ij s_i J_ij s_j - sum_i h_i s_i,   s_i in {-1, +1}
//! ```
//!
//! pela dinÃ¢mica contÃ­nua (`x`, `y` em `f64`)
//!
//! ```text
//! dx/dt = a0 * y
//! dy/dt = -(a0 - a(t)) * x + c0 * (J x + h)
//! a(t)  = a0 * t / Tmax                     (rampa linear 0 -> a0)
//! c0    = 0.5 / (sqrt(N) * std(J))          (std por instÃ¢ncia do lote)
//! ```
//!
//! integrada por Euler simplÃ©tico (`y` antes de `x`) com paredes perfeitamente
//! inelÃ¡sticas (`|x| > 1` => `x = sign(x)`, `y = 0`).
//!
//! # Garantias
//!
//! * **Elitismo** â€” a cada passo o estado discreto `sign(x)` Ã© avaliado e o
//!   melhor da trajetÃ³ria Ã© guardado. `RelaxOutcome::states` Ã© o argmin da
//!   trajetÃ³ria (nunca o estado final bruto).
//! * **Polimento guloso determinÃ­stico** â€” depois do relaxamento, o melhor
//!   estado sofre flips simples enquanto existir um flip que reduza `H`
//!   (`Delta H_i = 2 s_i (g_i + h_i)` com `g = J s`). Isso torna a melhoria
//!   estrita (`INV-T1`) determinÃ­stica e reprodutÃ­vel. Se â€” caso patolÃ³gico â€”
//!   o estado polido ainda nÃ£o for melhor que `H(sign(x0))`, uma busca de fuga
//!   determinÃ­stica testa `N` vizinhos de 1 flip, cada um polido, em ordem de
//!   Ã­ndice fixa e guarda o melhor.
//! * **Zero alocaÃ§Ã£o** â€” [`ThermodynamicIsingSolver::relax_into`] reutiliza
//!   todos os buffers alocados em [`ThermodynamicIsingSolver::new`]; nenhuma
//!   alocaÃ§Ã£o de heap ocorre no caminho quente (`INV-T4`).
//! * **Determinismo bit-a-bit** â€” [`ThermodynamicIsingSolver::reset`] reinicia
//!   o `ChaCha8Rng` e a integraÃ§Ã£o nÃ£o depende de `HashMap`/threads (`INV-T5`).
//!
//! # Layouts exigidos
//!
//! `IsingLandscape::j` Ã© `[B, N, N]` e `IsingLandscape::h` Ã© `[B, N]`; as views
//! precisam ser C-contÃ­guas (sem *strides*), o que Ã© o layout natural de
//! `Array3`/`Array2` e de `row`/`into_shape` usados pelos consumidores. Views
//! nÃ£o contÃ­guas sÃ£o rejeitadas com [`AetherError::Config`] (a validaÃ§Ã£o nÃ£o
//! aloca no caminho feliz). `RelaxOutcome` precisa ter exatamente as formas
//! `[B, N]`, `[B, N]`, `[B, steps + 1]` e `[B]`; divergÃªncia retorna
//! [`AetherError::Shape`] e **nÃ£o** redimensiona nada.

use ndarray::{Array1, Array2, ArrayView2, ArrayView3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::error::{AetherError, Result};

/// AceleraÃ§Ã£o inicial `a_0` da dinÃ¢mica bSB.
const A0: f64 = 1.0;

/// Abaixo deste desvio-padrÃ£o de `J`, `c_0` cai no *clamp* documentado.
const STD_MIN: f64 = 1e-12;

/// TolerÃ¢ncia (estrita) para aceitar um flip como redutor de energia.
const FLIP_EPS: f64 = 1e-12;

/// Semente usada por [`ThermodynamicIsingSolver::new`] antes de qualquer
/// [`ThermodynamicIsingSolver::reset`] explÃ­cito.
const DEFAULT_SEED: u64 = 0xA37E_0001;

/// Paisagem de energia de um lote de instÃ¢ncias de Ising.
///
/// * `j` â€” `[B, N, N]`, simÃ©trica e com diagonal zero (nÃ£o validado: contrato
///   do chamador).
/// * `h` â€” `[B, N]`, campo externo (viÃ©s) por instÃ¢ncia.
#[derive(Clone, Copy, Debug)]
pub struct IsingLandscape<'a> {
    /// Acoplamentos `J` empilhados por instÃ¢ncia do lote.
    pub j: ArrayView3<'a, f64>,
    /// Campo externo `h` por instÃ¢ncia.
    pub h: ArrayView2<'a, f64>,
}

impl<'a> IsingLandscape<'a> {
    /// Empacota as duas views que descrevem a paisagem.
    pub fn new(j: ArrayView3<'a, f64>, h: ArrayView2<'a, f64>) -> Self {
        Self { j, h }
    }
}

/// Resultado do relaxamento termodinÃ¢mico de um lote.
#[derive(Debug, Clone)]
pub struct RelaxOutcome {
    /// `[B, N]` estado emergente = argmin (polido) da trajetÃ³ria, em `{-1, +1}`.
    pub states: Array2<i8>,
    /// `[B, N]` estado inicial `sign(x_0)` efetivamente usado.
    pub initial_states: Array2<i8>,
    /// `[B, steps + 1]` curva `H(s_0), H(s_1), ..., H(s_steps)`.
    pub energy_trace: Array2<f64>,
    /// `[B]` menor energia obtida (`== H(states)`).
    pub best_energy: Array1<f64>,
}

impl RelaxOutcome {
    /// Aloca um resultado com as formas canÃ´nicas `[B, N]`, `[B, N]`,
    /// `[B, steps + 1]`, `[B]`, pronto para [`ThermodynamicIsingSolver::relax_into`].
    pub fn with_capacity(batch: usize, n: usize, steps: usize) -> Self {
        Self {
            states: Array2::<i8>::zeros((batch, n)),
            initial_states: Array2::<i8>::zeros((batch, n)),
            energy_trace: Array2::<f64>::zeros((batch, steps + 1)),
            best_energy: Array1::<f64>::zeros(batch),
        }
    }
}

/// Solver de BifurcaÃ§Ã£o Simulada BalÃ­stica para um lote fixo de instÃ¢ncias.
///
/// Todos os buffers sÃ£o alocados em [`ThermodynamicIsingSolver::new`] e
/// reutilizados; o `N`, `B` e o nÃºmero de passos sÃ£o fixos por construÃ§Ã£o.
pub struct ThermodynamicIsingSolver {
    batch: usize,
    n: usize,
    steps: usize,
    dt: f64,
    a0: f64,
    /// `[B, N]` posiÃ§Ãµes contÃ­nuas.
    x: Array2<f64>,
    /// `[B, N]` momentos contÃ­nuos.
    y: Array2<f64>,
    /// `[B, N]` scratch do gradiente `J x + h`.
    hx: Array2<f64>,
    /// `[B, N]` elite discreta da trajetÃ³ria.
    best_states: Array2<i8>,
    /// Scratch do estado discreto corrente (`N`).
    s_cur: Vec<i8>,
    /// Scratch do estado discreto de fuga (`N`).
    s_try: Vec<i8>,
    /// Scratch do gradiente discreto `J s` (`N`).
    g: Vec<f64>,
    /// Scratch do gradiente discreto de fuga (`N`).
    g_try: Vec<f64>,
    /// RNG determinÃ­stico semeado por `reset`.
    rng: ChaCha8Rng,
}

impl std::fmt::Debug for ThermodynamicIsingSolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThermodynamicIsingSolver")
            .field("batch", &self.batch)
            .field("n", &self.n)
            .field("steps", &self.steps)
            .field("dt", &self.dt)
            .field("a0", &self.a0)
            .finish_non_exhaustive()
    }
}

impl ThermodynamicIsingSolver {
    /// Aloca todos os buffers do solver para um lote fixo.
    ///
    /// `dt` nÃ£o positivo ou nÃ£o finito cai no padrÃ£o `0.05` (documentado);
    /// `batch`/`n` iguais a zero fazem `relax*` retornar
    /// [`AetherError::Config`].
    pub fn new(batch: usize, n: usize, steps: usize, dt: f64) -> Self {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.05 };
        let mut solver = Self {
            batch,
            n,
            steps,
            dt,
            a0: A0,
            x: Array2::<f64>::zeros((batch, n)),
            y: Array2::<f64>::zeros((batch, n)),
            hx: Array2::<f64>::zeros((batch, n)),
            best_states: Array2::<i8>::zeros((batch, n)),
            s_cur: vec![1i8; n],
            s_try: vec![1i8; n],
            g: vec![0.0; n],
            g_try: vec![0.0; n],
            rng: ChaCha8Rng::seed_from_u64(DEFAULT_SEED),
        };
        solver.reset(DEFAULT_SEED);
        solver
    }

    /// Reinicia `x, y ~ U(-0.1, 0.1)` com o RNG semeado em `seed`.
    ///
    /// NÃ£o aloca: o `ChaCha8Rng` Ã© reconstruÃ­do no stack.
    pub fn reset(&mut self, seed: u64) {
        self.rng = ChaCha8Rng::seed_from_u64(seed);
        for b in 0..self.batch {
            for i in 0..self.n {
                self.x[[b, i]] = (self.rng.gen::<f64>() - 0.5) * 0.2;
                self.y[[b, i]] = (self.rng.gen::<f64>() - 0.5) * 0.2;
            }
        }
        self.best_states.fill(1);
    }

    /// ConveniÃªncia: aloca um [`RelaxOutcome`] e delega para `relax_into`.
    pub fn relax(&mut self, land: IsingLandscape<'_>) -> Result<RelaxOutcome> {
        let mut out = RelaxOutcome::with_capacity(self.batch, self.n, self.steps);
        self.relax_into(land, &mut out)?;
        Ok(out)
    }

    /// Caminho quente: relaxa o lote reutilizando `out` **sem alocar**.
    ///
    /// Exige que `out` jÃ¡ tenha as formas canÃ´nicas (`[B, N]`, `[B, N]`,
    /// `[B, steps + 1]`, `[B]`); qualquer divergÃªncia retorna
    /// [`AetherError::Shape`] sem redimensionar buffers.
    pub fn relax_into(&mut self, land: IsingLandscape<'_>, out: &mut RelaxOutcome) -> Result<()> {
        let (batch, n, steps) = (self.batch, self.n, self.steps);
        if batch == 0 || n == 0 {
            return Err(AetherError::Config(
                "solver com batch ou n igual a zero".into(),
            ));
        }

        // --- validaÃ§Ã£o de formas (sem alocar no caminho feliz) ---
        check_shape2(land.h.shape(), batch, n)?;
        check_shape3(land.j.shape(), batch, n)?;
        check_shape2(out.states.shape(), batch, n)?;
        check_shape2(out.initial_states.shape(), batch, n)?;
        check_shape2(out.energy_trace.shape(), batch, steps + 1)?;
        if out.best_energy.len() != batch {
            return Err(AetherError::shape(vec![batch], vec![out.best_energy.len()]));
        }

        // --- layouts C-contÃ­guos (o laÃ§o quente usa slices planos) ---
        let j = land
            .j
            .as_slice()
            .ok_or_else(|| AetherError::Config("IsingLandscape::j deve ser C-contÃ­guo".into()))?;
        let h = land
            .h
            .as_slice()
            .ok_or_else(|| AetherError::Config("IsingLandscape::h deve ser C-contÃ­guo".into()))?;
        let init_all = out
            .initial_states
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.initial_states deve ser C-contÃ­guo".into()))?;
        let trace_all = out
            .energy_trace
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.energy_trace deve ser C-contÃ­guo".into()))?;
        let state_all = out
            .states
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.states deve ser C-contÃ­guo".into()))?;

        for b in 0..batch {
            let j_off = b * n * n;
            let h_off = b * n;
            let t_off = b * (steps + 1);
            let s_off = b * n;
            let best = self.relax_one(
                b,
                &j[j_off..j_off + n * n],
                &h[h_off..h_off + n],
                &mut init_all[s_off..s_off + n],
                &mut trace_all[t_off..t_off + steps + 1],
                &mut state_all[s_off..s_off + n],
            );
            out.best_energy[b] = best;
        }
        Ok(())
    }

    /// `H(s) = -0.5 * s' J s - h' s`, com `J: [N, N]` e `h: [1, N]`.
    ///
    /// NÃ£o aloca. Formas invÃ¡lidas (ou views nÃ£o contÃ­guas) retornam
    /// `f64::NAN` â€” a checagem Ã© Ãºnica e idÃªntica em debug e release (a
    /// assinatura `-> f64` nÃ£o tem canal de erro).
    pub fn energy(&self, j: &ArrayView2<'_, f64>, h: &ArrayView2<'_, f64>, s: &[i8]) -> f64 {
        let n = s.len();
        if j.dim() != (n, n) || h.dim() != (1, n) {
            return f64::NAN;
        }
        let Some(jb) = j.as_slice() else {
            return f64::NAN;
        };
        let Some(hb) = h.as_slice() else {
            return f64::NAN;
        };
        row_energy(jb, n, hb, s)
    }

    /// Relaxa uma instÃ¢ncia do lote. Retorna `H(states)` do estado emergente.
    ///
    /// `init_row` recebe `sign(x_0)`, `trace_row` recebe `steps + 1` energias e
    /// `state_row` recebe o argmin polido.
    fn relax_one(
        &mut self,
        b: usize,
        jb: &[f64],
        hb: &[f64],
        init_row: &mut [i8],
        trace_row: &mut [f64],
        state_row: &mut [i8],
    ) -> f64 {
        let n = self.n;
        let steps = self.steps;
        let dt = self.dt;
        let a0 = self.a0;

        // --- estado inicial efetivamente usado: sign(x_0) ---
        for i in 0..n {
            init_row[i] = sign(self.x[[b, i]]);
            self.best_states[[b, i]] = init_row[i];
        }
        let e0 = row_energy(jb, n, hb, init_row);
        trace_row[0] = e0;
        let mut e_best = e0;

        // --- c_0 por instÃ¢ncia: 0.5 / (sqrt(N) * std(J)), com clamp ---
        let mut sum = 0.0;
        let mut sumsq = 0.0;
        for v in jb.iter() {
            sum += *v;
            sumsq += *v * *v;
        }
        let count = (n * n) as f64;
        let mean = sum / count;
        let var = (sumsq / count - mean * mean).max(0.0);
        let std = var.sqrt();
        let inv_sqrt_n = 1.0 / (n as f64).sqrt();
        let c0 = if std < STD_MIN {
            0.5 * inv_sqrt_n
        } else {
            0.5 * inv_sqrt_n / std
        };

        // --- integraÃ§Ã£o simplÃ©tica do lote b ---
        let x_all = self
            .x
            .as_slice_mut()
            .expect("x Ã© C-contÃ­guo por construÃ§Ã£o");
        let y_all = self
            .y
            .as_slice_mut()
            .expect("y Ã© C-contÃ­guo por construÃ§Ã£o");
        let hx_all = self
            .hx
            .as_slice_mut()
            .expect("hx Ã© C-contÃ­guo por construÃ§Ã£o");
        let xr = &mut x_all[b * n..(b + 1) * n];
        let yr = &mut y_all[b * n..(b + 1) * n];
        let hxr = &mut hx_all[b * n..(b + 1) * n];

        for k in 0..steps {
            // a(t) = a0 * t / Tmax, com t = (k + 1) dt e Tmax = steps * dt.
            let a = a0 * ((k + 1) as f64) / (steps as f64);
            let drift = a0 - a;

            // hx = J x + h (gemv vetorizado em slices planos)
            for i in 0..n {
                let row = &jb[i * n..(i + 1) * n];
                hxr[i] = dot_f64(row, xr) + hb[i];
            }

            // Euler simplÃ©tico: momento primeiro, posiÃ§Ã£o depois, paredes inelÃ¡sticas.
            for i in 0..n {
                yr[i] += dt * (-drift * xr[i] + c0 * hxr[i]);
                let xn = xr[i] + dt * a0 * yr[i];
                if xn > 1.0 {
                    xr[i] = 1.0;
                    yr[i] = 0.0;
                } else if xn < -1.0 {
                    xr[i] = -1.0;
                    yr[i] = 0.0;
                } else {
                    xr[i] = xn;
                }
            }

            // energia do estado discreto corrente + elitismo
            for i in 0..n {
                self.s_cur[i] = sign(xr[i]);
            }
            // `s` convertido uma vez (O(n)); a energia vira um dot f64 puro.
            {
                let sf = &mut self.g[..n];
                for i in 0..n {
                    sf[i] = f64::from(self.s_cur[i]);
                }
                let mut e = 0.0;
                for i in 0..n {
                    let row = &jb[i * n..(i + 1) * n];
                    e -= 0.5 * sf[i] * dot_f64(row, sf);
                }
                for i in 0..n {
                    e -= hb[i] * sf[i];
                }
                trace_row[k + 1] = e;
                if e < e_best {
                    e_best = e;
                    for i in 0..n {
                        self.best_states[[b, i]] = self.s_cur[i];
                    }
                }
            }
        }

        // --- polimento guloso determinÃ­stico do melhor estado ---
        for i in 0..n {
            self.s_cur[i] = self.best_states[[b, i]];
        }
        let e = descend(n, jb, hb, self.s_cur.as_mut_slice(), self.g.as_mut_slice());

        // --- fuga determinÃ­stica (sÃ³ se o polimento nÃ£o bateu H(s_0)) ---
        if e >= e0 {
            for i in 0..n {
                self.best_states[[b, i]] = self.s_cur[i];
            }
            let mut best_e = e;
            for i in 0..n {
                let elite = self.best_states.row(b);
                let base = elite.as_slice().expect("linha do elite Ã© C-contÃ­gua");
                self.s_try.copy_from_slice(base);
                self.s_try[i] = -self.s_try[i];
                let e_try = descend(
                    n,
                    jb,
                    hb,
                    self.s_try.as_mut_slice(),
                    self.g_try.as_mut_slice(),
                );
                if e_try < best_e {
                    best_e = e_try;
                    self.s_cur.copy_from_slice(&self.s_try);
                }
            }
        }

        state_row.copy_from_slice(&self.s_cur);
        // Energia canÃ´nica do estado devolvido: mesma funÃ§Ã£o/formula de `energy`.
        row_energy(jb, n, hb, state_row)
    }
}

/// `sign(0) = +1` (convenÃ§Ã£o documentada; `f64::signum` daria `0`).
#[inline]
fn sign(v: f64) -> i8 {
    if v >= 0.0 {
        1
    } else {
        -1
    }
}

/// `H(s) = -0.5 * sum_i s_i (J s)_i - sum_i h_i s_i` em slices planos.
#[inline]
fn row_energy(jb: &[f64], n: usize, hb: &[f64], s: &[i8]) -> f64 {
    let mut acc = 0.0;
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        acc -= 0.5 * f64::from(s[i]) * dot_i8(row, s);
    }
    for (hi, si) in hb.iter().zip(s.iter()) {
        acc -= *hi * f64::from(*si);
    }
    acc
}

/// Produto interno `aÂ·b` com 16 acumuladores independentes.
///
/// Rust 100% safe: sem intrinsics. Os 16 acumuladores quebram a cadeia de
/// dependÃªncia do FMA (latÃªncia ~4 ciclos, throughput 2/ciclo no AVX2) e dÃ£o ao
/// LLVM material para SLP-vetorizar em 4 registradores YMM. A ordem de reduÃ§Ã£o Ã©
/// fixa (0..16 e depois o resto), logo o resultado Ã© **bit-a-bit reprodutÃ­vel
/// entre execuÃ§Ãµes** â€” embora difira do laÃ§o escalar em ~1 ULP.
#[inline]
fn dot_f64(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    let mut acc = [0.0f64; 16];
    let chunks = n / 16;
    for c in 0..chunks {
        let x = &a[c * 16..c * 16 + 16];
        let y = &b[c * 16..c * 16 + 16];
        for k in 0..16 {
            acc[k] += x[k] * y[k];
        }
    }
    let mut sum = 0.0;
    for v in acc {
        sum += v;
    }
    for i in chunks * 16..n {
        sum += a[i] * b[i];
    }
    sum
}

/// Produto interno `aÂ·s` com `s âˆˆ {-1,+1}` (mesma estrutura de [`dot_f64`]).
#[inline]
fn dot_i8(a: &[f64], s: &[i8]) -> f64 {
    let n = a.len().min(s.len());
    let mut acc = [0.0f64; 16];
    let chunks = n / 16;
    for c in 0..chunks {
        let x = &a[c * 16..c * 16 + 16];
        let y = &s[c * 16..c * 16 + 16];
        for k in 0..16 {
            acc[k] += x[k] * f64::from(y[k]);
        }
    }
    let mut sum = 0.0;
    for v in acc {
        sum += v;
    }
    for i in chunks * 16..n {
        sum += a[i] * f64::from(s[i]);
    }
    sum
}

/// `g = J s` em slices planos (`g` tem `n` posiÃ§Ãµes).
#[inline]
fn fill_g(jb: &[f64], n: usize, s: &[i8], g: &mut [f64]) {
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        g[i] = dot_i8(row, s);
    }
}

/// Descida gulosa determinÃ­stica: a cada iteraÃ§Ã£o aceita o flip simples de
/// menor `Delta H_i = 2 s_i (g_i + h_i)` (empate => menor Ã­ndice), atÃ© nÃ£o
/// existir flip estritamente redutor ou esgotar `8 N` flips.
///
/// Retorna `H(s)` do estado final (mesma fÃ³rmula de [`row_energy`]).
fn descend(n: usize, jb: &[f64], hb: &[f64], s: &mut [i8], g: &mut [f64]) -> f64 {
    fill_g(jb, n, s, g);
    let max_flips = 8 * n;
    for _ in 0..max_flips {
        let mut pick = usize::MAX;
        let mut pick_delta = -FLIP_EPS;
        for i in 0..n {
            let delta = 2.0 * f64::from(s[i]) * (g[i] + hb[i]);
            if delta < pick_delta {
                pick_delta = delta;
                pick = i;
            }
        }
        if pick == usize::MAX {
            break;
        }
        let delta_s = -2.0 * f64::from(s[pick]);
        s[pick] = -s[pick];
        for k in 0..n {
            g[k] += jb[k * n + pick] * delta_s;
        }
    }
    row_energy(jb, n, hb, s)
}

#[inline]
fn check_shape2(shape: &[usize], rows: usize, cols: usize) -> Result<()> {
    if shape.len() != 2 || shape[0] != rows || shape[1] != cols {
        return Err(AetherError::shape(vec![rows, cols], shape.to_vec()));
    }
    Ok(())
}

#[inline]
fn check_shape3(shape: &[usize], batch: usize, n: usize) -> Result<()> {
    if shape.len() != 3 || shape[0] != batch || shape[1] != n || shape[2] != n {
        return Err(AetherError::shape(vec![batch, n, n], shape.to_vec()));
    }
    Ok(())
}



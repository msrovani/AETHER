//! Módulo 1 — Relaxamento termodinâmico por Bifurcação Simulada Balística (bSB).
//!
//! Resolve instâncias de Ising
//!
//! ```text
//! H(s) = -0.5 * sum_ij s_i J_ij s_j - sum_i h_i s_i,   s_i in {-1, +1}
//! ```
//!
//! pela dinâmica contínua (`x`, `y` em `f64`)
//!
//! ```text
//! dx/dt = a0 * y
//! dy/dt = -(a0 - a(t)) * x + c0 * (J x + h)
//! a(t)  = a0 * t / Tmax                     (rampa linear 0 -> a0)
//! c0    = 0.5 / (sqrt(N) * std(J))          (std por instância do lote)
//! ```
//!
//! integrada por Euler simplético (`y` antes de `x`) com paredes perfeitamente
//! inelásticas (`|x| > 1` => `x = sign(x)`, `y = 0`).
//!
//! # Garantias
//!
//! * **Elitismo** — a cada passo o estado discreto `sign(x)` é avaliado e o
//!   melhor da trajetória é guardado. `RelaxOutcome::states` é o argmin da
//!   trajetória (nunca o estado final bruto).
//! * **Polimento guloso determinístico** — depois do relaxamento, o melhor
//!   estado sofre flips simples enquanto existir um flip que reduza `H`
//!   (`Delta H_i = 2 s_i (g_i + h_i)` com `g = J s`). Isso torna a melhoria
//!   estrita (`INV-T1`) determinística e reprodutível. Se — caso patológico —
//!   o estado polido ainda não for melhor que `H(sign(x0))`, uma busca de fuga
//!   determinística testa `N` vizinhos de 1 flip, cada um polido, em ordem de
//!   índice fixa e guarda o melhor.
//! * **Zero alocação** — [`ThermodynamicIsingSolver::relax_into`] reutiliza
//!   todos os buffers alocados em [`ThermodynamicIsingSolver::new`]; nenhuma
//!   alocação de heap ocorre no caminho quente (`INV-T4`).
//! * **Determinismo bit-a-bit** — [`ThermodynamicIsingSolver::reset`] reinicia
//!   o `ChaCha8Rng` e a integração não depende de `HashMap`/threads (`INV-T5`).
//!
//! # Layouts exigidos
//!
//! `IsingLandscape::j` é `[B, N, N]` e `IsingLandscape::h` é `[B, N]`; as views
//! precisam ser C-contíguas (sem *strides*), o que é o layout natural de
//! `Array3`/`Array2` e de `row`/`into_shape` usados pelos consumidores. Views
//! não contíguas são rejeitadas com [`AetherError::Config`] (a validação não
//! aloca no caminho feliz). `RelaxOutcome` precisa ter exatamente as formas
//! `[B, N]`, `[B, N]`, `[B, steps + 1]` e `[B]`; divergência retorna
//! [`AetherError::Shape`] e **não** redimensiona nada.

use ndarray::{Array1, Array2, ArrayView2, ArrayView3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::error::{AetherError, Result};

/// Aceleração inicial `a_0` da dinâmica bSB.
const A0: f64 = 1.0;

/// Abaixo deste desvio-padrão de `J`, `c_0` cai no *clamp* documentado.
const STD_MIN: f64 = 1e-12;

/// Tolerância (estrita) para aceitar um flip como redutor de energia.
const FLIP_EPS: f64 = 1e-12;

/// Semente usada por [`ThermodynamicIsingSolver::new`] antes de qualquer
/// [`ThermodynamicIsingSolver::reset`] explícito.
const DEFAULT_SEED: u64 = 0xA37E_0001;

/// Paisagem de energia de um lote de instâncias de Ising.
///
/// * `j` — `[B, N, N]`, simétrica e com diagonal zero (não validado: contrato
///   do chamador).
/// * `h` — `[B, N]`, campo externo (viés) por instância.
#[derive(Clone, Copy, Debug)]
pub struct IsingLandscape<'a> {
    /// Acoplamentos `J` empilhados por instância do lote.
    pub j: ArrayView3<'a, f64>,
    /// Campo externo `h` por instância.
    pub h: ArrayView2<'a, f64>,
}

impl<'a> IsingLandscape<'a> {
    /// Empacota as duas views que descrevem a paisagem.
    pub fn new(j: ArrayView3<'a, f64>, h: ArrayView2<'a, f64>) -> Self {
        Self { j, h }
    }
}

/// Resultado do relaxamento termodinâmico de um lote.
#[derive(Debug, Clone)]
pub struct RelaxOutcome {
    /// `[B, N]` estado emergente = argmin (polido) da trajetória, em `{-1, +1}`.
    pub states: Array2<i8>,
    /// `[B, N]` estado inicial `sign(x_0)` efetivamente usado.
    pub initial_states: Array2<i8>,
    /// `[B, steps + 1]` curva `H(s_0), H(s_1), ..., H(s_steps)`.
    pub energy_trace: Array2<f64>,
    /// `[B]` menor energia obtida (`== H(states)`).
    pub best_energy: Array1<f64>,
}

impl RelaxOutcome {
    /// Aloca um resultado com as formas canônicas `[B, N]`, `[B, N]`,
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

/// Solver de Bifurcação Simulada Balística para um lote fixo de instâncias.
///
/// Todos os buffers são alocados em [`ThermodynamicIsingSolver::new`] e
/// reutilizados; o `N`, `B` e o número de passos são fixos por construção.
pub struct ThermodynamicIsingSolver {
    batch: usize,
    n: usize,
    steps: usize,
    dt: f64,
    a0: f64,
    /// `[B, N]` posições contínuas.
    x: Array2<f64>,
    /// `[B, N]` momentos contínuos.
    y: Array2<f64>,
    /// `[B, N]` scratch do gradiente `J x + h`.
    hx: Array2<f64>,
    /// `[B, N]` elite discreta da trajetória.
    best_states: Array2<i8>,
    /// Scratch do estado discreto corrente (`N`).
    s_cur: Vec<i8>,
    /// Scratch do estado discreto de fuga (`N`).
    s_try: Vec<i8>,
    /// Scratch do gradiente discreto `J s` (`N`).
    g: Vec<f64>,
    /// Scratch do gradiente discreto de fuga (`N`).
    g_try: Vec<f64>,
    /// RNG determinístico semeado por `reset`.
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
    /// `dt` não positivo ou não finito cai no padrão `0.05` (documentado);
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
    /// Não aloca: o `ChaCha8Rng` é reconstruído no stack.
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

    /// Conveniência: aloca um [`RelaxOutcome`] e delega para `relax_into`.
    pub fn relax(&mut self, land: IsingLandscape<'_>) -> Result<RelaxOutcome> {
        let mut out = RelaxOutcome::with_capacity(self.batch, self.n, self.steps);
        self.relax_into(land, &mut out)?;
        Ok(out)
    }

    /// Caminho quente: relaxa o lote reutilizando `out` **sem alocar**.
    ///
    /// Exige que `out` já tenha as formas canônicas (`[B, N]`, `[B, N]`,
    /// `[B, steps + 1]`, `[B]`); qualquer divergência retorna
    /// [`AetherError::Shape`] sem redimensionar buffers.
    pub fn relax_into(&mut self, land: IsingLandscape<'_>, out: &mut RelaxOutcome) -> Result<()> {
        let (batch, n, steps) = (self.batch, self.n, self.steps);
        if batch == 0 || n == 0 {
            return Err(AetherError::Config(
                "solver com batch ou n igual a zero".into(),
            ));
        }

        // --- validação de formas (sem alocar no caminho feliz) ---
        check_shape2(land.h.shape(), batch, n)?;
        check_shape3(land.j.shape(), batch, n)?;
        check_shape2(out.states.shape(), batch, n)?;
        check_shape2(out.initial_states.shape(), batch, n)?;
        check_shape2(out.energy_trace.shape(), batch, steps + 1)?;
        if out.best_energy.len() != batch {
            return Err(AetherError::shape(vec![batch], vec![out.best_energy.len()]));
        }

        // --- layouts C-contíguos (o laço quente usa slices planos) ---
        let j = land
            .j
            .as_slice()
            .ok_or_else(|| AetherError::Config("IsingLandscape::j deve ser C-contíguo".into()))?;
        let h = land
            .h
            .as_slice()
            .ok_or_else(|| AetherError::Config("IsingLandscape::h deve ser C-contíguo".into()))?;
        let init_all = out
            .initial_states
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.initial_states deve ser C-contíguo".into()))?;
        let trace_all = out
            .energy_trace
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.energy_trace deve ser C-contíguo".into()))?;
        let state_all = out
            .states
            .as_slice_mut()
            .ok_or_else(|| AetherError::Config("out.states deve ser C-contíguo".into()))?;

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
    /// Não aloca. Formas inválidas (ou views não contíguas) retornam
    /// `f64::NAN` — a checagem é única e idêntica em debug e release (a
    /// assinatura `-> f64` não tem canal de erro).
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

    /// Relaxa uma instância do lote. Retorna `H(states)` do estado emergente.
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

        // --- c_0 por instância: 0.5 / (sqrt(N) * std(J)), com clamp ---
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

        // --- integração simplética do lote b ---
        let x_all = self
            .x
            .as_slice_mut()
            .expect("x é C-contíguo por construção");
        let y_all = self
            .y
            .as_slice_mut()
            .expect("y é C-contíguo por construção");
        let hx_all = self
            .hx
            .as_slice_mut()
            .expect("hx é C-contíguo por construção");
        let xr = &mut x_all[b * n..(b + 1) * n];
        let yr = &mut y_all[b * n..(b + 1) * n];
        let hxr = &mut hx_all[b * n..(b + 1) * n];

        for k in 0..steps {
            // a(t) = a0 * t / Tmax, com t = (k + 1) dt e Tmax = steps * dt.
            let a = a0 * ((k + 1) as f64) / (steps as f64);
            let drift = a0 - a;

            // hx = J x + h (gemv manual em slices planos)
            for i in 0..n {
                let row = &jb[i * n..(i + 1) * n];
                let mut acc = 0.0;
                for (jv, xv) in row.iter().zip(xr.iter()) {
                    acc += *jv * *xv;
                }
                hxr[i] = acc + hb[i];
            }

            // Euler simplético: momento primeiro, posição depois, paredes inelásticas.
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
            let e = row_energy(jb, n, hb, &self.s_cur);
            trace_row[k + 1] = e;
            if e < e_best {
                e_best = e;
                for i in 0..n {
                    self.best_states[[b, i]] = self.s_cur[i];
                }
            }
        }

        // --- polimento guloso determinístico do melhor estado ---
        for i in 0..n {
            self.s_cur[i] = self.best_states[[b, i]];
        }
        let e = descend(n, jb, hb, self.s_cur.as_mut_slice(), self.g.as_mut_slice());

        // --- fuga determinística (só se o polimento não bateu H(s_0)) ---
        if e >= e0 {
            for i in 0..n {
                self.best_states[[b, i]] = self.s_cur[i];
            }
            let mut best_e = e;
            for i in 0..n {
                let elite = self.best_states.row(b);
                let base = elite.as_slice().expect("linha do elite é C-contígua");
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
        // Energia canônica do estado devolvido: mesma função/formula de `energy`.
        row_energy(jb, n, hb, state_row)
    }
}

/// `sign(0) = +1` (convenção documentada; `f64::signum` daria `0`).
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
        let mut g = 0.0;
        for (jv, sv) in row.iter().zip(s.iter()) {
            g += *jv * f64::from(*sv);
        }
        acc -= 0.5 * f64::from(s[i]) * g;
    }
    for (hi, si) in hb.iter().zip(s.iter()) {
        acc -= *hi * f64::from(*si);
    }
    acc
}

/// `g = J s` em slices planos (`g` tem `n` posições).
#[inline]
fn fill_g(jb: &[f64], n: usize, s: &[i8], g: &mut [f64]) {
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        let mut acc = 0.0;
        for (jv, sv) in row.iter().zip(s.iter()) {
            acc += *jv * f64::from(*sv);
        }
        g[i] = acc;
    }
}

/// Descida gulosa determinística: a cada iteração aceita o flip simples de
/// menor `Delta H_i = 2 s_i (g_i + h_i)` (empate => menor índice), até não
/// existir flip estritamente redutor ou esgotar `8 N` flips.
///
/// Retorna `H(s)` do estado final (mesma fórmula de [`row_energy`]).
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

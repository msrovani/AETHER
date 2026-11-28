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
    /// `[B, N]` scratch do estado discreto corrente (por batch — R11).
    s_cur: Array2<i8>,
    /// `[B, N]` scratch do estado discreto de fuga (por batch — R11).
    s_try: Array2<i8>,
    /// `[B, N]` scratch do gradiente discreto `J s` (por batch — R11).
    g: Array2<f64>,
    /// `[B, N]` scratch do gradiente discreto de fuga (por batch — R11).
    g_try: Array2<f64>,
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

/// Fatias por-thread de um relaxamento (R11) — movidas para a thread.
/// `b0` é o índice **global** do primeiro batch do chunk (j/h são globais).
struct BatchChunks<'a> {
    b0: usize,
    x: &'a mut [f64],
    y: &'a mut [f64],
    hx: &'a mut [f64],
    best: &'a mut [i8],
    s_cur: &'a mut [i8],
    s_try: &'a mut [i8],
    g: &'a mut [f64],
    g_try: &'a mut [f64],
    init: &'a mut [i8],
    trace: &'a mut [f64],
    states: &'a mut [i8],
    best_energy: &'a mut [f64],
}

/// Divide `all` (flat `[B, per]`) em `chunks` fatias disjuntas e contíguas —
/// uma por thread (R11). `threads <= B` garante chunk não-vazio.
fn split_chunks<'a, T>(
    all: &'a mut [T],
    threads: usize,
    batch: usize,
    per: usize,
) -> Vec<&'a mut [T]> {
    let mut out = Vec::with_capacity(threads);
    let mut rest: &'a mut [T] = all;
    let mut done = 0usize;
    for t in 0..threads {
        let upto = (batch * (t + 1) / threads) * per;
        debug_assert!(upto > done, "thread sem batch (threads > batch?)");
        let (head, tail) = rest.split_at_mut(upto - done);
        out.push(head);
        rest = tail;
        done = upto;
    }
    out
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
            s_cur: Array2::<i8>::from_elem((batch, n), 1),
            s_try: Array2::<i8>::from_elem((batch, n), 1),
            g: Array2::<f64>::zeros((batch, n)),
            g_try: Array2::<f64>::zeros((batch, n)),
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

        // --- R11: paralelismo por batch -----------------------------------
        // Batches são independentes (resultado de b só depende dos buffers de
        // b) → threads scoped com `available_parallelism`; determinismo
        // bit-a-bit preservado (tie-break por índice, ordem fixa por batch).
        let dt = self.dt;
        let a0 = self.a0;
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
        let bs_all = self
            .best_states
            .as_slice_mut()
            .expect("best_states é C-contíguo por construção");
        let sc_all = self
            .s_cur
            .as_slice_mut()
            .expect("s_cur é C-contíguo por construção");
        let st_all = self
            .s_try
            .as_slice_mut()
            .expect("s_try é C-contíguo por construção");
        let g_all = self
            .g
            .as_slice_mut()
            .expect("g é C-contíguo por construção");
        let gt_all = self
            .g_try
            .as_slice_mut()
            .expect("g_try é C-contíguo por construção");
        let be_all = out
            .best_energy
            .as_slice_mut()
            .expect("best_energy é C-contíguo por construção");

        let threads = std::thread::available_parallelism()
            .map_or(1, |p| p.get())
            .min(batch);
        // R11: caminho serial (batch == 1 ou 1 núcleo visível) — zero alocação,
        // mesmo contrato do `alloc_invariants` (contagem de allocs == 0).
        if threads == 1 {
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
            let bs_all = self
                .best_states
                .as_slice_mut()
                .expect("best_states é C-contíguo por construção");
            let sc_all = self
                .s_cur
                .as_slice_mut()
                .expect("s_cur é C-contíguo por construção");
            let st_all = self
                .s_try
                .as_slice_mut()
                .expect("s_try é C-contíguo por construção");
            let g_all = self
                .g
                .as_slice_mut()
                .expect("g é C-contíguo por construção");
            let gt_all = self
                .g_try
                .as_slice_mut()
                .expect("g_try é C-contíguo por construção");
            for b in 0..batch {
                let j_off = b * n * n;
                let h_off = b * n;
                let t_off = b * (steps + 1);
                let s_off = b * n;
                let best = Self::relax_one_batch(
                    n,
                    steps,
                    dt,
                    a0,
                    &j[j_off..j_off + n * n],
                    &h[h_off..h_off + n],
                    &mut x_all[s_off..s_off + n],
                    &mut y_all[s_off..s_off + n],
                    &mut hx_all[s_off..s_off + n],
                    &mut bs_all[s_off..s_off + n],
                    &mut sc_all[s_off..s_off + n],
                    &mut st_all[s_off..s_off + n],
                    &mut g_all[s_off..s_off + n],
                    &mut gt_all[s_off..s_off + n],
                    &mut init_all[s_off..s_off + n],
                    &mut trace_all[t_off..t_off + steps + 1],
                    &mut state_all[s_off..s_off + n],
                );
                out.best_energy[b] = best;
            }
            return Ok(());
        }
        // Fatias pré-computadas, disjuntas por construção (o borrow checker
        // não vê a disjunção de indexações repetidas dentro do loop).
        let x_c = split_chunks(x_all, threads, batch, n);
        let y_c = split_chunks(y_all, threads, batch, n);
        let hx_c = split_chunks(hx_all, threads, batch, n);
        let bs_c = split_chunks(bs_all, threads, batch, n);
        let sc_c = split_chunks(sc_all, threads, batch, n);
        let st_c = split_chunks(st_all, threads, batch, n);
        let g_c = split_chunks(g_all, threads, batch, n);
        let gt_c = split_chunks(gt_all, threads, batch, n);
        let init_c = split_chunks(init_all, threads, batch, n);
        let trc_c = split_chunks(trace_all, threads, batch, steps + 1);
        let stat_c = split_chunks(state_all, threads, batch, n);
        let be_c = split_chunks(be_all, threads, batch, 1);
        // Consome os Vecs em trava, montando um `BatchChunks` por thread —
        // movidos para a thread (sem `unsafe`, sem aliasing por construção).
        let mut chunks: Vec<BatchChunks<'_>> = Vec::with_capacity(threads);
        {
            let mut xi = x_c.into_iter();
            let mut yi = y_c.into_iter();
            let mut hi = hx_c.into_iter();
            let mut bi = bs_c.into_iter();
            let mut si = sc_c.into_iter();
            let mut ti = st_c.into_iter();
            let mut gi = g_c.into_iter();
            let mut gti = gt_c.into_iter();
            let mut ini = init_c.into_iter();
            let mut tri = trc_c.into_iter();
            let mut sti = stat_c.into_iter();
            let mut bei = be_c.into_iter();
            let mut b0 = 0usize;
            loop {
                match (
                    xi.next(),
                    yi.next(),
                    hi.next(),
                    bi.next(),
                    si.next(),
                    ti.next(),
                    gi.next(),
                    gti.next(),
                    ini.next(),
                    tri.next(),
                    sti.next(),
                    bei.next(),
                ) {
                    (
                        Some(x),
                        Some(y),
                        Some(hx),
                        Some(best),
                        Some(s_cur),
                        Some(s_try),
                        Some(g),
                        Some(g_try),
                        Some(init),
                        Some(trace),
                        Some(states),
                        Some(best_energy),
                    ) => {
                        let span = x.len() / n;
                        let chunk = BatchChunks {
                            b0,
                            x,
                            y,
                            hx,
                            best,
                            s_cur,
                            s_try,
                            g,
                            g_try,
                            init,
                            trace,
                            states,
                            best_energy,
                        };
                        b0 += span;
                        chunks.push(chunk);
                    }
                    _ => break,
                }
            }
        }
        std::thread::scope(|s| {
            for ch in chunks {
                let BatchChunks {
                    b0,
                    x: xr,
                    y: yr,
                    hx: hxr,
                    best: br,
                    s_cur: sc,
                    s_try: stx,
                    g: gr,
                    g_try: gtr,
                    init,
                    trace: trc,
                    states: stat,
                    best_energy: be,
                } = ch;
                let j_ref: &[f64] = j;
                let h_ref: &[f64] = h;
                s.spawn(move || {
                    let b_span = xr.len() / n;
                    for b in 0..b_span {
                        // j/h são GLOBAIS: o índice é `b0 + b` (os buffers por
                        // chunk são locais — s_off é o `b` local mesmo).
                        let j_off = (b0 + b) * n * n;
                        let h_off = (b0 + b) * n;
                        let t_off = b * (steps + 1);
                        let s_off = b * n;
                        be[b] = Self::relax_one_batch(
                            n,
                            steps,
                            dt,
                            a0,
                            &j_ref[j_off..j_off + n * n],
                            &h_ref[h_off..h_off + n],
                            &mut xr[s_off..s_off + n],
                            &mut yr[s_off..s_off + n],
                            &mut hxr[s_off..s_off + n],
                            &mut br[s_off..s_off + n],
                            &mut sc[s_off..s_off + n],
                            &mut stx[s_off..s_off + n],
                            &mut gr[s_off..s_off + n],
                            &mut gtr[s_off..s_off + n],
                            &mut init[s_off..s_off + n],
                            &mut trc[t_off..t_off + steps + 1],
                            &mut stat[s_off..s_off + n],
                        );
                    }
                });
            }
        });
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

    /// Relaxa uma instância do lote — função associada **sem `self`** (R11).
    ///
    /// O paralelismo por batch exige buffers por-thread: nunca `&mut self`.
    /// `init_row` recebe `sign(x_0)`, `trace_row` recebe `steps + 1` energias e
    /// `state_row` recebe o argmin polido.
    fn relax_one_batch(
        n: usize,
        steps: usize,
        dt: f64,
        a0: f64,
        jb: &[f64],
        hb: &[f64],
        xr: &mut [f64],
        yr: &mut [f64],
        hxr: &mut [f64],
        best_row: &mut [i8],
        s_cur: &mut [i8],
        s_try: &mut [i8],
        g: &mut [f64],
        g_try: &mut [f64],
        init_row: &mut [i8],
        trace_row: &mut [f64],
        state_row: &mut [i8],
    ) -> f64 {
        // --- estado inicial efetivamente usado: sign(x_0) ---
        for i in 0..n {
            init_row[i] = sign(xr[i]);
            best_row[i] = init_row[i];
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
        // xr/yr/hxr chegam por parâmetro (buffer por batch — R11).

        for k in 0..steps {
            // a(t) = a0 * t / Tmax, com t = (k + 1) dt e Tmax = steps * dt.
            let a = a0 * ((k + 1) as f64) / (steps as f64);
            let drift = a0 - a;

            // hx = J x + h (gemv vetorizado em slices planos)
            for i in 0..n {
                let row = &jb[i * n..(i + 1) * n];
                hxr[i] = dot_f64(row, xr) + hb[i];
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
                s_cur[i] = sign(xr[i]);
            }
            // `s` convertido uma vez (O(n)); a energia vira um dot f64 puro.
            {
                let sf = &mut g[..n];
                for i in 0..n {
                    sf[i] = f64::from(s_cur[i]);
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
                        best_row[i] = s_cur[i];
                    }
                }
            }
        }

        // --- polimento guloso determinístico do melhor estado ---
        for i in 0..n {
            s_cur[i] = best_row[i];
        }
        // F1 (Sprint C): `e` vai ao final do estado devolvido; sem recálculo
        // extra de O(N²) em `relax_one` (escavação redundantes em L436).
        let mut e = descend(n, jb, hb, s_cur, g);

        // --- fuga determinística (só se o polimento não bateu H(s_0)) ---
        if e >= e0 {
            for i in 0..n {
                best_row[i] = s_cur[i];
            }
            let mut best_e = e;
            for i in 0..n {
                s_try.copy_from_slice(best_row);
                s_try[i] = -s_try[i];
                let e_try = descend(n, jb, hb, s_try, g_try);
                if e_try < best_e {
                    best_e = e_try;
                    s_cur.copy_from_slice(s_try);
                }
            }
            // F1 (Sprint C): a energia do estado final devolvido reusamos
            // (`descend` devolve `row_energy` do estado devolvido).
            e = best_e;
        }

        state_row.copy_from_slice(s_cur);
        // F1 (Sprint C): é `e` = `descend`/`best_e` acima — poupamos um
        // recálculo O(N²) de `row_energy(jb, n, hb, state_row)`.
        e
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
        acc -= 0.5 * f64::from(s[i]) * dot_i8(row, s);
    }
    for (hi, si) in hb.iter().zip(s.iter()) {
        acc -= *hi * f64::from(*si);
    }
    acc
}

/// Produto interno `a·b` com 16 acumuladores independentes.
///
/// Rust 100% safe: sem intrinsics. Os 16 acumuladores quebram a cadeia de
/// dependência do FMA (latência ~4 ciclos, throughput 2/ciclo no AVX2) e dão ao
/// LLVM material para SLP-vetorizar em 4 registradores YMM. A ordem de redução é
/// fixa (0..16 e depois o resto), logo o resultado é **bit-a-bit reprodutível
/// entre execuções** — embora difira do laço escalar em ~1 ULP.
///
/// Curva medida (N=64, steps=60, Core 7 240H): **16 → 17,4 GFLOP/s** ·
/// 24 → 10,4 (a cauda de 16 elementos em N=64 cai no laço escalar) ·
/// 32 → 12,3 (spill de registradores). 16 divide todas as potências de dois
/// usadas pelo organismo (64..1024), então não há cauda em nenhum N.
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

/// Produto interno `a·s` com `s ∈ {-1,+1}` (mesma estrutura de [`dot_f64`]).
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

/// `g = J s` em slices planos (`g` tem `n` posições).
#[inline]
fn fill_g(jb: &[f64], n: usize, s: &[i8], g: &mut [f64]) {
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        g[i] = dot_i8(row, s);
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

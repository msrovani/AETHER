//! E5 — currículo por **progresso de aprendizado** (Vol. I §4, Vol. II §5).
//!
//! O baseline do sonhador amostra contrafactuais por surpresa bruta. Em um
//! buffer com amostras não aprendíveis (ruído sem fonte), isso gasta atualizações
//! perseguindo outliers. O currículo por progresso usa `d(erro)/dt`: só reusa
//! a região que ainda está rendendo aprendizado.
//!
//! O gate compara, na mesma memória e no mesmo buffer, o tempo de readaptação
//! pós-ruptura (A → B) das duas políticas contra o teto `T03 = 150` ciclos, e
//! exige **zero alocação** no laço de atualização.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::f32::consts::TAU;

use aether_engine::core::koopman_sindy::{DelayEmbedding, OnlineKoopmanOperator};
use aether_engine::thermodynamic_dreamer::ProgressCurriculum;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const EMB: usize = 16;
const HORIZON: usize = 12;
const SLOTS: usize = 128;
const MAX_CYCLES: usize = 150;
const VALIDATION: usize = 24;

// ---------------------------------------------------------------------------
// Contador de alocação (mesmo padrão de tests/alloc_invariants.rs)
// ---------------------------------------------------------------------------
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
}

// ---------------------------------------------------------------------------
// Séries (mesmas do T03 em tests/cognitive_audit.rs)
// ---------------------------------------------------------------------------
fn vdp_series(n: usize, dt: f64) -> Vec<f64> {
    let mu = 1.0f64;
    let (mut x, mut y) = (2.0f64, 0.0f64);
    let mut xs = Vec::with_capacity(n);
    for _ in 0..n {
        let f = |x: f64, y: f64| (y, mu * (1.0 - x * x) * y - x);
        let (k1x, k1y) = f(x, y);
        let (k2x, k2y) = f(x + 0.5 * dt * k1x, y + 0.5 * dt * k1y);
        let (k3x, k3y) = f(x + 0.5 * dt * k2x, y + 0.5 * dt * k2y);
        let (k4x, k4y) = f(x + dt * k3x, y + dt * k3y);
        x += dt * (k1x + 2.0 * k2x + 2.0 * k3x + k4x) / 6.0;
        y += dt * (k1y + 2.0 * k2y + 2.0 * k3y + k4y) / 6.0;
        xs.push(x);
    }
    xs
}

fn regime_b_series(n: usize) -> Vec<f32> {
    (0..n)
        .map(|t| {
            let t = t as f32;
            0.8 * (TAU * 0.23 * t).sin() + 0.4 * (TAU * 0.07 * t).sin()
        })
        .collect()
}

fn embedding_of(series: &[f32], start: usize) -> [f32; EMB] {
    let mut e = DelayEmbedding::new(EMB).expect("delay");
    for k in 0..EMB {
        e.push(series[start + k]);
    }
    let mut out = [0.0f32; EMB];
    e.vector_into(&mut out).expect("vector");
    out
}

fn pretrain_a() -> OnlineKoopmanOperator {
    let xs = vdp_series(4000, 0.01);
    let regime_a: Vec<f32> = xs.iter().map(|v| (*v / 2.5) as f32).collect();
    let mut op = OnlineKoopmanOperator::with_forgetting(EMB, 1, 0.05, 0.99).expect("koopman");
    let mut emb = DelayEmbedding::new(EMB).expect("delay");
    for k in 0..EMB {
        emb.push(regime_a[k]);
    }
    let mut z = [0.0f32; EMB];
    for t in EMB..(2500 - HORIZON) {
        emb.vector_into(&mut z).expect("z");
        op.observe(&z, &[regime_a[t + HORIZON]]).expect("treino A");
        emb.push(regime_a[t]);
    }
    op
}

fn validation_error(op: &OnlineKoopmanOperator, zs: &[[f32; EMB]], ys: &[f32]) -> f32 {
    let mut out = [0.0f32; 1];
    let mut acc = 0.0f32;
    for (z, y) in zs.iter().zip(ys.iter()) {
        op.predict_into(z, &mut out).expect("predict");
        acc += (out[0] - *y).abs();
    }
    acc / ys.len().max(1) as f32
}

/// Política de seleção de replay.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    /// Baseline: maior surpresa (erro) — o que o T03 faz implicitamente.
    Surprise,
    /// E5: maior progresso positivo `d(erro)/dt`.
    Progress,
}

struct Replay {
    z: Vec<[f32; EMB]>,
    y: Vec<f32>,
    prev_err: Vec<f32>,
    cur_err: Vec<f32>,
    outlier: Vec<bool>,
}

impl Replay {
    fn new(series: &[f32], seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut z = Vec::with_capacity(SLOTS);
        let mut y = Vec::with_capacity(SLOTS);
        let mut outlier = Vec::with_capacity(SLOTS);
        for i in 0..SLOTS {
            let start = EMB + i * 3;
            z.push(embedding_of(series, start));
            let clean = series[start + EMB + HORIZON];
            // 35% de outliers não aprendíveis (ruído sem fonte).
            let corrupt = rng.gen::<f32>() < 0.35;
            let obs = if corrupt {
                clean + (rng.gen::<f32>() - 0.5) * 6.0
            } else {
                clean
            };
            y.push(obs);
            outlier.push(corrupt);
        }
        Self {
            z,
            y,
            prev_err: vec![0.0; SLOTS],
            cur_err: vec![0.0; SLOTS],
            outlier,
        }
    }

    fn scan(&mut self, op: &OnlineKoopmanOperator) {
        let mut out = [0.0f32; 1];
        for i in 0..SLOTS {
            op.predict_into(&self.z[i], &mut out).expect("predict");
            self.cur_err[i] = (out[0] - self.y[i]).abs();
        }
    }

    /// Índice escolhido; no modo progresso, atualiza `prev_err` do escolhido.
    fn choose(&mut self, policy: Policy) -> usize {
        let mut best = 0usize;
        match policy {
            Policy::Surprise => {
                for i in 1..SLOTS {
                    if self.cur_err[i] > self.cur_err[best] {
                        best = i;
                    }
                }
            }
            Policy::Progress => {
                let mut best_prog = f32::NEG_INFINITY;
                for i in 0..SLOTS {
                    let prog = self.prev_err[i] - self.cur_err[i];
                    if prog > best_prog {
                        best_prog = prog;
                        best = i;
                    }
                }
                // Sem progresso positivo em lugar nenhum: cai no erro médio
                // (não persegue o outlier extremo).
                if best_prog <= 0.0 {
                    let mean = self.cur_err.iter().sum::<f32>() / SLOTS as f32;
                    best = 0;
                    let mut best_d = f32::INFINITY;
                    for i in 0..SLOTS {
                        let d = (self.cur_err[i] - mean).abs();
                        if d < best_d {
                            best_d = d;
                            best = i;
                        }
                    }
                }
            }
        }
        best
    }

    /// Atualiza a linha de base de erro de todos os slots (progresso global).
    fn commit(&mut self) {
        for i in 0..SLOTS {
            self.prev_err[i] = self.cur_err[i];
        }
    }
}

/// Roda a readaptação e devolve (ciclos até 80% de redução, updates em outliers).
fn readapt(
    op: &mut OnlineKoopmanOperator,
    replay: &mut Replay,
    zs: &[[f32; EMB]],
    ys: &[f32],
    policy: Policy,
) -> (usize, usize) {
    let before = validation_error(op, zs, ys).max(1e-6);
    let target = before * 0.2;
    let mut outliers = 0usize;
    for cycle in 0..MAX_CYCLES {
        replay.scan(op);
        let idx = replay.choose(policy);
        replay.commit();
        outliers += usize::from(replay.outlier[idx]);
        op.observe(&replay.z[idx], &[replay.y[idx]])
            .expect("adaptacao B");
        if cycle % 4 == 3 && validation_error(op, zs, ys) < target {
            return (cycle + 1, outliers);
        }
    }
    (MAX_CYCLES, outliers)
}

#[test]
fn e5_curriculo_por_progresso_bate_t03_sem_alocar() {
    let b = regime_b_series(4000);
    let mut zs = Vec::with_capacity(VALIDATION);
    let mut ys = Vec::with_capacity(VALIDATION);
    for i in 0..VALIDATION {
        let start = 1500 + i * 7;
        zs.push(embedding_of(&b, start));
        ys.push(b[start + EMB + HORIZON]);
    }

    // Surpresa bruta (baseline do E5).
    let mut op_s = pretrain_a();
    let mut replay_s = Replay::new(&b, 0xA37E_0502);
    let (cycles_surprise, out_surprise) =
        readapt(&mut op_s, &mut replay_s, &zs, &ys, Policy::Surprise);

    // Progresso de aprendizado (E5).
    let mut op_p = pretrain_a();
    let mut replay_p = Replay::new(&b, 0xA37E_0502);
    let (cycles_progress, out_progress) =
        readapt(&mut op_p, &mut replay_p, &zs, &ys, Policy::Progress);

    println!(
        "[E5] readaptacao surpresa={cycles_surprise} progresso={cycles_progress} (T03=150) | updates_outlier surpresa={out_surprise} progresso={out_progress}"
    );
    assert!(
        cycles_progress <= cycles_surprise,
        "progresso ({cycles_progress}) pior que surpresa ({cycles_surprise})"
    );
    assert!(
        cycles_progress < MAX_CYCLES,
        "progresso nao bateu T03: {cycles_progress} >= 150"
    );
    assert!(
        out_progress < out_surprise,
        "progresso gastou {out_progress} updates em outliers (surpresa={out_surprise})"
    );

    // O peso do currículo prefere progresso a surpresa bruta.
    let mut cur = ProgressCurriculum::new();
    for k in 0..64 {
        cur.observe(1.0 - k as f32 / 80.0, 1.0);
    }
    assert!(cur.progress() > 0.0, "progresso deveria ser positivo");
    let w_progress = cur.select_weight(0.1);
    let w_surprise = cur.select_weight(0.9);
    assert!(
        w_progress > w_surprise,
        "peso({w_progress}) <= peso({w_surprise})"
    );

    // Zero alocação no laço de atualização (após warmup).
    let mut op_z = pretrain_a();
    let mut replay_z = Replay::new(&b, 0xA37E_0502);
    let z0 = replay_z.z[0];
    let _ = op_z.predict_into(&z0, &mut [0.0f32; 1]);
    let (_, allocs) = count_allocs(|| {
        for _ in 0..MAX_CYCLES {
            replay_z.scan(&op_z);
            let idx = replay_z.choose(Policy::Progress);
            replay_z.commit();
            op_z.observe(&replay_z.z[idx], &[replay_z.y[idx]])
                .expect("zero alloc");
        }
    });
    println!("[E5] allocs no loop de 150 ciclos = {allocs}");
    assert_eq!(allocs, 0, "laco do curriculo alocou {allocs} vezes");
}

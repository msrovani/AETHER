//! Testes do Módulo 1 — solver termodinâmico bSB (feature `m1`).
//!
//! Cobre:
//! * `INV-T1` — `H(s_final) < H(s_0)` estritamente em 50 instâncias `N = 64`,
//!   200 passos, `h = 0`, com a energia recomputada por [`ThermodynamicIsingSolver::energy`].
//! * `INV-T2` — `states` e `initial_states` em `{-1, +1}`.
//! * `INV-T3` — curva de energia com exatamente `steps + 1` amostras.
//! * `INV-T4` — `relax_into` não aloca após o warmup (alocador contador *por
//!   thread*: outros testes do binário rodando em paralelo não contaminam a
//!   contagem).
//! * `INV-T5` — determinismo bit-a-bit com `reset(seed)`.
//! * Lote `B > 1`, equivalência `relax`/`relax_into` e rejeição de formas inválidas.

#![cfg(feature = "m1")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use aether_engine::AetherError;
use ndarray::{Array1, Array2, Array3, ArrayView1, Axis};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

// ---------------------------------------------------------------------------
// Alocador contador (só para `INV-T4`)
//
// A contagem é *thread-local*: `cargo test` roda os testes deste binário em
// threads paralelas, e um contador global seria contaminado pelas alocações do
// harness e dos outros testes. O `Cell` é const-inicializado, portanto acessá-lo
// dentro de `alloc` não aloca (sem recursão).
// ---------------------------------------------------------------------------

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct ThreadCountingAlloc;

unsafe impl GlobalAlloc for ThreadCountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: ThreadCountingAlloc = ThreadCountingAlloc;

fn allocs_now() -> usize {
    ALLOCS.with(Cell::get)
}

// ---------------------------------------------------------------------------
// Utilitários
// ---------------------------------------------------------------------------

/// `J` simétrica com entradas `U(-1, 1)/sqrt(N)` e diagonal zero; `h` nulo.
fn symmetric_landscape(n: usize, seed: u64) -> (Array2<f64>, Array2<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = Array2::<f64>::zeros((n, n));
    for i in 0..n {
        for k in (i + 1)..n {
            let v = (rng.gen::<f64>() * 2.0 - 1.0) / (n as f64).sqrt();
            j[[i, k]] = v;
            j[[k, i]] = v;
        }
    }
    (j, Array2::<f64>::zeros((1, n)))
}

fn row_slice(row: ArrayView1<'_, i8>) -> &[i8] {
    row.as_slice().expect("linha de estados C-contígua")
}

/// Menor valor da curva de energia.
fn trace_min(row: ArrayView1<'_, f64>) -> f64 {
    row.iter().fold(f64::INFINITY, |acc, v| acc.min(*v))
}

// ---------------------------------------------------------------------------
// INV-T1 — elitismo estrito em 50 instâncias aleatórias N=64 / 200 passos
// ---------------------------------------------------------------------------

#[test]
fn inv_t1_elitismo_estrito_50_casos() {
    const N: usize = 64;
    const CASES: usize = 50;
    const STEPS: usize = 200;

    let mut solver = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E_0001);
    let h = Array2::<f64>::zeros((1, N));

    for case in 0..CASES {
        let mut j = Array2::<f64>::zeros((N, N));
        for i in 0..N {
            for k in (i + 1)..N {
                let v = (rng.gen::<f64>() * 2.0 - 1.0) / (N as f64).sqrt();
                j[[i, k]] = v;
                j[[k, i]] = v;
            }
        }

        solver.reset(0xDEAD_BEEF ^ case as u64);
        let j3 = j.view().into_shape((1, N, N)).expect("shape J");
        let out = solver
            .relax(IsingLandscape::new(j3, h.view()))
            .expect("relax falhou");

        // INV-T3: steps + 1 amostras, todas finitas.
        assert_eq!(out.energy_trace.dim(), (1, STEPS + 1), "caso {case}");
        assert!(
            out.energy_trace.iter().all(|e| e.is_finite()),
            "caso {case}: curva de energia não finita"
        );

        // INV-T2: estados em {-1, +1}.
        assert!(
            out.states.iter().all(|v| *v == -1 || *v == 1),
            "caso {case}: states fora de {{-1,+1}}"
        );
        assert!(
            out.initial_states.iter().all(|v| *v == -1 || *v == 1),
            "caso {case}: initial_states fora de {{-1,+1}}"
        );

        // trace[0] é H(sign(x_0)) do estado inicial gravado.
        let e0 = out.energy_trace[[0, 0]];
        let e0_recalc = solver.energy(&j.view(), &h.view(), row_slice(out.initial_states.row(0)));
        assert!(
            (e0 - e0_recalc).abs() < 1e-9,
            "caso {case}: trace[0]={e0} != H(initial_states)={e0_recalc}"
        );

        // INV-T1: estritamente menor.
        let ef = out.best_energy[0];
        assert!(
            ef < e0,
            "caso {case}: H_final={ef:.6} !< H_inicial={e0:.6} (invariante violado)"
        );
        assert!(ef.is_finite(), "caso {case}: H_final não finito");

        // O estado emergente tem exatamente a energia anunciada.
        let recomputed = solver.energy(&j.view(), &h.view(), row_slice(out.states.row(0)));
        assert!(
            (recomputed - ef).abs() < 1e-9,
            "caso {case}: energia recomputada {recomputed} != best_energy {ef}"
        );

        // best_energy é o mínimo (polimento só reduz; nunca aumenta a elite).
        let traj_min = trace_min(out.energy_trace.row(0));
        assert!(
            ef <= traj_min + 1e-9,
            "caso {case}: best_energy {ef} > mínimo da trajetória {traj_min}"
        );
    }
}

// ---------------------------------------------------------------------------
// INV-T5 — determinismo bit-a-bit
// ---------------------------------------------------------------------------

#[test]
fn inv_t5_determinismo_bit_a_bit() {
    const N: usize = 48;
    const STEPS: usize = 150;

    let (j, h) = symmetric_landscape(N, 0xD37E_7);
    let j3 = j.view().into_shape((1, N, N)).expect("shape J");

    let mut a = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    let mut b = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    a.reset(0xABCD_1234);
    b.reset(0xABCD_1234);

    let oa = a.relax(IsingLandscape::new(j3, h.view())).expect("relax A");
    let ob = b.relax(IsingLandscape::new(j3, h.view())).expect("relax B");

    assert_eq!(oa.states, ob.states, "states divergiram");
    assert_eq!(oa.initial_states, ob.initial_states, "initial_states divergiram");
    assert_eq!(
        oa.best_energy[0].to_bits(),
        ob.best_energy[0].to_bits(),
        "best_energy divergiu"
    );

    let ta: Vec<u64> = oa.energy_trace.iter().map(|v| v.to_bits()).collect();
    let tb: Vec<u64> = ob.energy_trace.iter().map(|v| v.to_bits()).collect();
    assert_eq!(ta, tb, "curvas de energia divergiram bit-a-bit");
}

// ---------------------------------------------------------------------------
// Lote B > 1 com campo externo por instância
// ---------------------------------------------------------------------------

#[test]
fn lote_b_maior_que_um() {
    const N: usize = 32;
    const B: usize = 4;
    const STEPS: usize = 120;

    let mut rng = ChaCha8Rng::seed_from_u64(0xB47C4);
    let mut j = Array3::<f64>::zeros((B, N, N));
    let mut h = Array2::<f64>::zeros((B, N));
    for b in 0..B {
        for i in 0..N {
            for k in (i + 1)..N {
                let v = (rng.gen::<f64>() * 2.0 - 1.0) / (N as f64).sqrt();
                j[[b, i, k]] = v;
                j[[b, k, i]] = v;
            }
        }
        // campo externo fraco, alternado por instância
        for i in 0..N {
            let sign = if (i + b) % 2 == 0 { 1.0 } else { -1.0 };
            h[[b, i]] = 0.1 * sign;
        }
    }

    let mut solver = ThermodynamicIsingSolver::new(B, N, STEPS, 0.05);
    let mut out = RelaxOutcome::with_capacity(B, N, STEPS);
    solver.reset(0xB47C5);
    solver
        .relax_into(IsingLandscape::new(j.view(), h.view()), &mut out)
        .expect("relax_into");

    assert_eq!(out.states.dim(), (B, N));
    assert_eq!(out.initial_states.dim(), (B, N));
    assert_eq!(out.energy_trace.dim(), (B, STEPS + 1));
    assert_eq!(out.best_energy.len(), B);

    for b in 0..B {
        let e0 = out.energy_trace[[b, 0]];
        let ef = out.best_energy[b];
        assert!(ef <= e0 + 1e-9, "lote {b}: H_final={ef} > H_inicial={e0}");
        assert!(ef.is_finite());

        let jb = j.index_axis(Axis(0), b);
        let hb = h.slice(ndarray::s![b..b + 1, ..]);
        let recomputed = solver.energy(&jb, &hb, row_slice(out.states.row(b)));
        assert!(
            (recomputed - ef).abs() < 1e-9,
            "lote {b}: recomputada {recomputed} != best_energy {ef}"
        );
    }

    // Instâncias do lote são independentes: o resultado do lote não é o mesmo
    // estado colapsado (campo h difere por instância).
    assert!(
        (0..B).any(|b| out.states.row(b) != out.states.row(0)),
        "todas as instâncias colapsaram para o mesmo estado"
    );
}

// ---------------------------------------------------------------------------
// relax() (conveniência) == relax_into() (hot path)
// ---------------------------------------------------------------------------

#[test]
fn relax_e_relax_into_equivalentes() {
    const N: usize = 40;
    const STEPS: usize = 80;

    let (j, h) = symmetric_landscape(N, 0xE9A1);
    let j3 = j.view().into_shape((1, N, N)).expect("shape J");

    let mut solver = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    solver.reset(7);
    let alocado = solver.relax(IsingLandscape::new(j3, h.view())).expect("relax");

    let mut solver2 = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    solver2.reset(7);
    let mut reuso = RelaxOutcome::with_capacity(1, N, STEPS);
    solver2
        .relax_into(IsingLandscape::new(j3, h.view()), &mut reuso)
        .expect("relax_into");

    assert_eq!(alocado.states, reuso.states);
    assert_eq!(
        alocado.best_energy[0].to_bits(),
        reuso.best_energy[0].to_bits()
    );
}

// ---------------------------------------------------------------------------
// Formas inválidas -> AetherError::Shape, sem redimensionar
// ---------------------------------------------------------------------------

#[test]
fn shapes_invalidos_retornam_erro() {
    const N: usize = 16;
    const STEPS: usize = 10;
    const B: usize = 2;

    let mut solver = ThermodynamicIsingSolver::new(B, N, STEPS, 0.05);
    let j_ok = Array3::<f64>::zeros((B, N, N));
    let h_ok = Array2::<f64>::zeros((B, N));
    let mut out = RelaxOutcome::with_capacity(B, N, STEPS);

    // J com lote errado
    let j_bad_batch = Array3::<f64>::zeros((B + 1, N, N));
    assert!(matches!(
        solver.relax_into(
            IsingLandscape::new(j_bad_batch.view(), h_ok.view()),
            &mut out
        ),
        Err(AetherError::Shape { .. })
    ));

    // J com N errado
    let j_bad_n = Array3::<f64>::zeros((B, N + 1, N));
    assert!(matches!(
        solver.relax_into(IsingLandscape::new(j_bad_n.view(), h_ok.view()), &mut out),
        Err(AetherError::Shape { .. })
    ));

    // h com N errado
    let h_bad = Array2::<f64>::zeros((B, N + 1));
    assert!(matches!(
        solver.relax_into(IsingLandscape::new(j_ok.view(), h_bad.view()), &mut out),
        Err(AetherError::Shape { .. })
    ));

    // out.energy_trace com passos errados — e o buffer NÃO é redimensionado
    let mut out_trace = RelaxOutcome::with_capacity(B, N, STEPS + 1);
    assert!(matches!(
        solver.relax_into(IsingLandscape::new(j_ok.view(), h_ok.view()), &mut out_trace),
        Err(AetherError::Shape { .. })
    ));
    assert_eq!(out_trace.energy_trace.dim(), (B, STEPS + 2));
    assert_eq!(out_trace.states.dim(), (B, N));

    // out.states com N errado
    let mut out_states = RelaxOutcome::with_capacity(B, N - 1, STEPS);
    assert!(matches!(
        solver.relax_into(IsingLandscape::new(j_ok.view(), h_ok.view()), &mut out_states),
        Err(AetherError::Shape { .. })
    ));
    assert_eq!(out_states.states.dim(), (B, N - 1));

    // energy(): formas inválidas => NaN (contrato não-panicking em release)
    let s = Array1::<i8>::ones(N);
    let j2 = Array2::<f64>::zeros((N, N));
    let h_wrong = Array2::<f64>::zeros((N, N));
    assert!(solver
        .energy(&j2.view(), &h_wrong.view(), row_slice(s.view()))
        .is_nan());
    let j_wrong = Array2::<f64>::zeros((N + 1, N));
    assert!(solver
        .energy(&j_wrong.view(), &h_ok.slice(ndarray::s![0..1, ..]), row_slice(s.view()))
        .is_nan());
}

// ---------------------------------------------------------------------------
// INV-T4 — zero alocação em relax_into após o warmup
// ---------------------------------------------------------------------------

#[test]
fn inv_t4_relax_into_zero_alocacao() {
    const N: usize = 64;
    const STEPS: usize = 200;

    let (j, h) = symmetric_landscape(N, 0xA110C);
    let j3 = j.view().into_shape((1, N, N)).expect("shape J");
    let mut solver = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, N, STEPS);

    // warmup: inicializa estado global de ndarray/rand e aquece o caminho.
    solver.reset(1);
    solver
        .relax_into(IsingLandscape::new(j3, h.view()), &mut out)
        .expect("warmup");

    let before = allocs_now();
    for seed in 0..3u64 {
        solver.reset(seed);
        solver
            .relax_into(IsingLandscape::new(j3, h.view()), &mut out)
            .expect("relax_into quente");
    }
    let after = allocs_now();

    assert_eq!(
        after - before,
        0,
        "relax_into alocou {} vezes após o warmup (deve ser 0)",
        after - before
    );
}

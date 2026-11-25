//! Gate S1 (R1) — Fast-J Gating:
//! 1. ΔR(J) > 0,30 com spins adversários (acoplamento move as bases);
//! 2. zero alocação (pattern `Counting` do `alloc_invariants.rs`);
//! 3. reproducibilidade bit-a-bit entre seeds.

#![cfg(feature = "m7")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

// -- allocator contador (pausa do pattern tests/alloc_invariants.rs) --
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

fn j0(n: usize, seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut v = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let val = rng.gen::<f32>() * 2.0 - 1.0;
            v[i * n + k] = val;
            v[k * n + i] = val;
        }
    }
    v
}

fn adversarial_spins(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| if i % 2 == 0 { 0.9 } else { -0.9 })
        .collect()
}

fn max_offdiag_delta(a: &[f32], b: &[f32], n: usize) -> f32 {
    let mut m = 0.0f32;
    for i in 0..n {
        for k in 0..n {
            if i != k {
                m = m.max((b[i * n + k] - a[i * n + k]).abs());
            }
        }
    }
    m
}

#[test]
fn gate_s1_gating_muda_jacobiano_adversarial() {
    let n = 16;
    let mut field = MorphogeneticField::new(n, &j0(n, 0xB0A), MorphogenesisConfig::default());
    let spins = adversarial_spins(n);
    let j_before = field.j.clone();
    for _ in 0..10 {
        field.try_evolve_topology(1.0, &spins, 0.05).expect("step");
    }
    let delta = max_offdiag_delta(&j_before, &field.j, n);
    assert!(
        delta > 0.30,
        "Fast-J Gating falhou: max|ΔJ| = {delta} (esperado > 0,30)"
    );
}

#[test]
fn gate_s1_gating_zero_alloc() {
    let n = 16;
    let mut field = MorphogeneticField::new(n, &j0(n, 0xC0FFEE), MorphogenesisConfig::default());
    let spins = adversarial_spins(n);
    let g = vec![0.5f32; n];

    // warmup
    field.try_evolve_topology(0.5, &spins, 0.05).expect("warmup");

    let (_, allocs) = count_allocs(|| {
        field.set_fast_g(&g).expect("set_fast_g");
        field.try_evolve_topology(0.5, &spins, 0.05).expect("step");
    });
    assert_eq!(allocs, 0, "Fast-J Gating alocou {allocs} vezes (deve ser 0)");

    let (_, allocs2) = count_allocs(|| {
        field.fast_g_from_wave(&[]);
    });
    assert_eq!(allocs2, 0, "fast_g_from_wave alocou {allocs2} vezes");
}

#[test]
fn gate_s1_gating_reprodutive_bit_a_bit() {
    let n = 8;
    let spins = adversarial_spins(n);
    let g = vec![0.75f32; n];
    let run = |seed: u64| -> Vec<f32> {
        let mut f = MorphogeneticField::new(n, &j0(n, seed), MorphogenesisConfig::default());
        f.set_fast_g(&g).expect("set_fast_g");
        for _ in 0..5 {
            f.try_evolve_topology(0.5, &spins, 0.05).expect("step");
        }
        f.j.clone()
    };
    let a = run(0x1234);
    let b = run(0x1234);
    assert_eq!(a, b, "reprodutividade bit-a-bit violada (Sprint A, R1)");
}

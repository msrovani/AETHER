//! E2c-2 — gates do roof dual completo (max-flow) para QUBO/Ising.
//!
//! O E2c v1 (dominância) é *sound* mas míope: no controle `plant = 0%` ele fixa
//! `0,0%` das variáveis. O roof dual olha a posiform inteira e enxerga
//! persistências que a dominância não vê.
//!
//! Gates:
//! 1. **soundness**: fixar pelo roof dual nunca altera o valor ótimo (força bruta);
//! 2. **alcance**: no vidro de spin puro (`h = 0`) a redução é **estritamente
//!    maior** que a da dominância (que é zero por construção);
//! 3. **caso submodular exato**: com todo `J_ij > 0` o problema é tratável por
//!    corte mínimo — o roof dual resolve no ótimo, sem variável ambígua;
//! 4. **zero alocação**: `solve_into` não aloca após o warmup.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{persistency_qumo, roof_dual_qumo, RoofDualWorkspace};

fn random_glass(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for m in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + m] = w;
            j[m * n + i] = w;
        }
    }
    let mut h = vec![0.0f64; n];
    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k) {
        let row_abs: f64 = j[v * n..(v + 1) * n]
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != v)
            .map(|(_, x)| x.abs())
            .sum();
        let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
        h[v] = sign * (row_abs + delta);
    }
    (j, h)
}

fn brute_force(n: usize, j: &[f64], h: &[f64]) -> f64 {
    let mut best = f64::INFINITY;
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        best = best.min(energy(j, h, n, &s));
    }
    best
}

/// Ótimo com as fixações aplicadas (enumera só o resíduo).
fn optimum_with_fixed(n: usize, j: &[f64], h: &[f64], fixed: &[Option<i8>], res: &[usize]) -> f64 {
    let mut s = vec![0i8; n];
    for (i, f) in fixed.iter().enumerate() {
        if let Some(v) = f {
            s[i] = *v;
        }
    }
    let rn = res.len();
    if rn == 0 {
        return energy(j, h, n, &s);
    }
    let mut best = f64::INFINITY;
    for mask in 0..(1u64 << rn) {
        for (k, &old) in res.iter().enumerate() {
            s[old] = if (mask >> k) & 1 == 0 { -1 } else { 1 };
        }
        best = best.min(energy(j, h, n, &s));
    }
    best
}

#[test]
fn soundness_em_vidro_de_spin_e_com_vies() {
    let n = 12usize;
    let instances = 60usize;
    for regime in [0.0f64, 0.25, 0.45] {
        let k = (regime * n as f64).round() as usize;
        for inst in 0..instances {
            let seed = 0xE2C2_0001u64
                .wrapping_add(inst as u64)
                .wrapping_add((regime * 1000.0) as u64);
            let (j, h) = random_glass(n, k, 0.6, seed);
            let e_star = brute_force(n, &j, &h);
            let pre = roof_dual_qumo(n, &j, &h);
            let e_red = optimum_with_fixed(n, &j, &h, &pre.fixed, &pre.residual);
            assert!(
                (e_red - e_star).abs() < 1e-9,
                "roof dual mudou o ótimo (plant={regime}, inst={inst}): {e_star} -> {e_red}"
            );
        }
    }
}

#[test]
fn roof_dual_ve_o_que_a_dominancia_nao_ve() {
    // Vidro de spin puro: h = 0. Dominância reduz 0,0% por construção.
    let n = 14usize;
    let mut total_roof = 0usize;
    let mut total_dom = 0usize;
    let instances = 20usize;
    for inst in 0..instances {
        let (j, h) = random_glass(n, 0, 0.0, 0xE2C2_1000 + inst as u64);
        let roof = roof_dual_qumo(n, &j, &h);
        let dom = persistency_qumo(n, &j, &h);
        assert_eq!(dom.fixed_count(), 0, "controle: dominância não deve fixar");
        total_roof += roof.fixed_count();
        total_dom += dom.fixed_count();
        // soundness do braço do controle
        let e_star = brute_force(n, &j, &h);
        let e_red = optimum_with_fixed(n, &j, &h, &roof.fixed, &roof.residual);
        assert!((e_red - e_star).abs() < 1e-9, "roof dual insound com h=0");
    }
    assert!(
        total_roof > total_dom,
        "roof dual ({total_roof}) deveria exceder dominância ({total_dom}) sem viés"
    );
}

#[test]
fn caso_submodular_e_resolvido_no_otimo() {
    // Todo J_ij > 0 ⇒ energia submodular em x ⇒ corte mínimo resolve exato.
    let n = 11usize;
    for inst in 0..12usize {
        let mut rng = ChaCha8Rng::seed_from_u64(0xE2C2_2000 + inst as u64);
        let mut j = vec![0.0f64; n * n];
        for i in 0..n {
            for m in (i + 1)..n {
                let w = 0.5 + rng.gen::<f64>();
                j[i * n + m] = w;
                j[m * n + i] = w;
            }
        }
        let h: Vec<f64> = (0..n).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect();
        let e_star = brute_force(n, &j, &h);
        let pre = roof_dual_qumo(n, &j, &h);
        let e_red = optimum_with_fixed(n, &j, &h, &pre.fixed, &pre.residual);
        assert!(
            (e_red - e_star).abs() < 1e-9,
            "submodular: ótimo divergiu ({e_star} vs {e_red})"
        );
        assert_eq!(
            pre.residual.len(),
            0,
            "submodular: nenhuma variável deveria ficar ambígua"
        );
    }
}

#[test]
fn roof_dual_nao_aloca_apos_warmup() {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

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
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }
    #[global_allocator]
    static GLOBAL: Counting = Counting;

    let n = 14usize;
    let (j, h) = random_glass(n, 3, 0.6, 0xE2C2_3000);
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = vec![None::<i8>; n];
    ws.solve_into(n, &j, &h, &mut fixed); // warmup

    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    ws.solve_into(n, &j, &h, &mut fixed);
    ARMED.with(|a| a.set(false));
    let allocs = ALLOCS.with(|c| c.get());
    assert_eq!(allocs, 0, "solve_into alocou {allocs} vezes (deve ser 0)");
}

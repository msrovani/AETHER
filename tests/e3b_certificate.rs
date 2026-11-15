//! E3b — gates do certificado de coerência.
//!
//! Duas frentes:
//! * **matemática**: `energy`, `local_gap`, `spectral_lower_bound` e a matriz 2D;
//! * **empírica**: em MAX-2-SAT (verificação exata), o certificado é
//!   **100% preciso** — e abstém quando não há solução.
//!
//! Referência: `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` §4 e Vol. III §12.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{
    energy, local_gap, local_gap_with, spectral_lower_bound, Certificate, Decision, Provenance,
};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

#[test]
fn ferromagnet_energy_and_gap_are_exact() {
    // J = [[0,1],[1,0]], h = 0, s = (1,1): H = -1; qualquer flip dá +1 ⇒ gap = 2.
    let j = vec![0.0, 1.0, 1.0, 0.0];
    let h = vec![0.0, 0.0];
    let s = [1i8, 1];
    assert!((energy(&j, &h, 2, &s) - (-1.0)).abs() < 1e-12);
    assert!((local_gap(&j, &h, 2, &s) - 2.0).abs() < 1e-12);

    let mut g = vec![0.0f64; 2];
    let gap = local_gap_with(&j, &h, 2, &s, &mut g);
    assert!((gap - 2.0).abs() < 1e-12);

    // O estado antiferrromagnético (-1,+1) também é mínimo local (gap 2).
    let s2 = [-1i8, 1];
    assert!((energy(&j, &h, 2, &s2) - 1.0).abs() < 1e-12);
    assert!((local_gap(&j, &h, 2, &s2) - 2.0).abs() < 1e-12);
}

#[test]
fn spectral_bound_never_exceeds_ground_state() {
    let n = 6usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0xE3B_B0UND);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let w = rng.gen::<f64>() * 2.0 - 1.0;
            j[i * n + k] = w;
            j[k * n + i] = w;
        }
    }
    let h = vec![0.0f64; n];

    // ground state por força bruta
    let mut best = f64::INFINITY;
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        best = best.min(energy(&j, &h, n, &s));
    }

    let lb = spectral_lower_bound(&j, &h, n, 64);
    assert!(
        lb <= best + 1e-9,
        "limite espectral {lb} > ground state {best}"
    );
}

#[test]
fn decision_matrix_has_four_quadrants() {
    let strong = Certificate::new(-10.0, 5.0, 0, 1.0, Provenance::Observed);
    assert!(strong.is_coherent());
    assert_eq!(strong.decide(255, 1.0), Decision::Act);
    assert_eq!(strong.decide(0, 1.0), Decision::Propose);

    // gap abaixo do piso físico ⇒ não é inequívoco
    let weak = Certificate::new(-10.0, 0.0, 0, 1.0, Provenance::Transplanted);
    assert!(!weak.is_coherent());
    assert_eq!(weak.decide(255, 1.0), Decision::Cite);
    assert_eq!(weak.decide(0, 1.0), Decision::Abstain);
    assert_eq!(weak.confidence(1.0), 0.0);
}

#[test]
fn violations_invalidate_the_certificate() {
    let c = Certificate::new(-1.0, 3.0, 2, 1.0, Provenance::Inferred);
    assert!(!c.is_coherent());
    assert_eq!(c.confidence(1.0), 0.0);
    assert_eq!(c.decide(255, 1.0), Decision::Cite);
}

// ---------------------------------------------------------------------------
// Empírico: MAX-2-SAT com verificação exata
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Clause {
    i: usize,
    si: i8,
    j: usize,
    sj: i8,
}

fn make_2sat(n: usize, m: usize, seed: u64) -> Vec<Clause> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..m)
        .map(|_| {
            let i = rng.gen_range(0..n);
            let mut j = rng.gen_range(0..n);
            if j == i {
                j = (j + 1) % n;
            }
            Clause {
                i,
                si: if rng.gen::<bool>() { 1 } else { -1 },
                j,
                sj: if rng.gen::<bool>() { 1 } else { -1 },
            }
        })
        .collect()
}

fn violations(clauses: &[Clause], s: &[i8]) -> u32 {
    clauses
        .iter()
        .filter(|c| s[c.i] != c.si && s[c.j] != c.sj)
        .count() as u32
}

fn ising_from_2sat(n: usize, clauses: &[Clause]) -> (Vec<f64>, Vec<f64>) {
    let mut j = vec![0.0f64; n * n];
    let mut h = vec![0.0f64; n];
    for c in clauses {
        let (si, sj) = (f64::from(c.si), f64::from(c.sj));
        let w = 0.25 * si * sj;
        j[c.i * n + c.j] -= w;
        j[c.j * n + c.i] -= w;
        h[c.i] += 0.25 * si;
        h[c.j] += 0.25 * sj;
    }
    (j, h)
}

#[test]
fn exact_verification_gives_100_percent_precision() {
    let n = 10usize;
    let m = n;
    let instances = 60usize;
    let restarts = 6usize;

    let mut jview = Array3::<f64>::zeros((1, n, n));
    let mut h2 = Array2::<f64>::zeros((1, n));
    let mut solver = ThermodynamicIsingSolver::new(1, n, 20, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, 20);

    let (mut decisive, mut decisive_ok, mut satisfiable) = (0usize, 0usize, 0usize);

    for inst in 0..instances {
        let clauses = make_2sat(n, m, 0xE3B_0001 + inst as u64);
        let (j, hp) = ising_from_2sat(n, &clauses);

        // satisfatibilidade por força bruta
        let mut s = vec![0i8; n];
        let mut sat = false;
        for mask in 0..(1u64 << n) {
            for (i, si) in s.iter_mut().enumerate() {
                *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
            }
            if violations(&clauses, &s) == 0 {
                sat = true;
                break;
            }
        }
        if sat {
            satisfiable += 1;
        }

        for (i, chunk) in j.chunks(n).enumerate() {
            for (k, &v) in chunk.iter().enumerate() {
                jview[[0, i, k]] = v;
            }
        }
        for i in 0..n {
            h2[[0, i]] = hp[i];
        }

        let mut best = u32::MAX;
        let mut best_s = vec![0i8; n];
        for r in 0..restarts {
            solver.reset(0xE3B_0001 ^ (inst as u64).wrapping_mul(31).wrapping_add(r as u64));
            solver.relax_into(IsingLandscape::new(jview.view(), h2.view()), &mut out)
                .expect("relax");
            let mut cur = vec![0i8; n];
            for (i, ci) in cur.iter_mut().enumerate() {
                *ci = out.states[[0, i]];
            }
            let v = violations(&clauses, &cur);
            if v < best {
                best = v;
                best_s.copy_from_slice(&cur);
            }
        }

        if best == 0 {
            decisive += 1;
            // o certificado só é emitido se a verificação recontar 0 violações
            if violations(&clauses, &best_s) == 0 {
                decisive_ok += 1;
                assert!(
                    sat,
                    "estado com 0 violações numa instância insatisfatível (impossível)"
                );
            }
        }
    }

    assert!(satisfiable > 0, "nenhuma instância satisfatível no lote");
    assert!(decisive > 0, "nenhuma resposta decisiva — cobertura zero");
    assert_eq!(
        decisive, decisive_ok,
        "certificado emitiu decisão sem verificação exata"
    );
    let precision = decisive_ok as f64 / decisive as f64;
    assert!(
        (precision - 1.0).abs() < 1e-12,
        "precisão {precision} != 100%"
    );
    let coverage = decisive as f64 / instances as f64;
    assert!(coverage > 0.5, "cobertura {coverage} baixa demais");
}

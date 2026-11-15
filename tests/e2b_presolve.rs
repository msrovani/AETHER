//! E2b — gates do presolve simbólico.
//!
//! * unidades e implicações fixam o que devem fixar;
//! * conflito é detectado;
//! * **soundness**: nenhuma fixação contradiz uma solução real (força bruta).
//!
//! Referência: `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` §2 e Vol. III §12.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::presolve::{presolve_2sat, residual_clauses, Clause2};

fn violations(clauses: &[Clause2], s: &[i8]) -> u32 {
    clauses.iter().filter(|c| !c.satisfied(s)).count() as u32
}

#[test]
fn unit_clause_fixes_variable() {
    let clauses = [Clause2::new(0, 1, 0, 1)];
    let p = presolve_2sat(1, &clauses);
    assert!(!p.conflict);
    assert_eq!(p.fixed[0], Some(1));
    assert!(p.residual.is_empty());

    let clauses = [Clause2::new(1, -1, 1, -1)];
    let p = presolve_2sat(2, &clauses);
    assert_eq!(p.fixed[1], Some(-1));
    assert_eq!(p.fixed[0], None);
}

#[test]
fn implication_chain_forces_literal() {
    // (x ∨ y) e (¬x ∨ y): de ¬y vem x, e de x vem y ⇒ y é verdadeiro.
    let clauses = [
        Clause2::new(0, 1, 1, 1),
        Clause2::new(0, -1, 1, 1),
    ];
    let p = presolve_2sat(2, &clauses);
    assert!(!p.conflict);
    assert_eq!(p.fixed[1], Some(1), "y deveria ser forçado a +1");
    assert_eq!(p.reduction(2), 0.5);
}

#[test]
fn contradiction_is_detected() {
    // (x∨y) ∧ (x∨¬y) ∧ (¬x∨y) ∧ (¬x∨¬y) — insatisfatível.
    let clauses = [
        Clause2::new(0, 1, 1, 1),
        Clause2::new(0, 1, 1, -1),
        Clause2::new(0, -1, 1, 1),
        Clause2::new(0, -1, 1, -1),
    ];
    let p = presolve_2sat(2, &clauses);
    assert!(p.conflict, "conflito não detectado");
}

#[test]
fn residual_clauses_are_over_residual_space() {
    let clauses = [
        Clause2::new(0, 1, 0, 1), // fixa x0 = +1
        Clause2::new(0, -1, 1, 1),
        Clause2::new(1, 1, 2, 1),
    ];
    let p = presolve_2sat(3, &clauses);
    assert_eq!(p.fixed[0], Some(1));
    let res = residual_clauses(&clauses, &p.fixed, &p.residual).expect("residual");
    // a cláusula (¬x0 ∨ x1) é satisfeita por x0=+1? não — vira unidade em x1
    // (o presolve deve ter fixado x1; então sobra só a cláusula de x1/x2)
    assert!(res.iter().all(|c| c.i < p.residual.len() && c.j < p.residual.len()));
}

#[test]
fn soundness_against_brute_force() {
    let n = 8usize;
    let m = 12usize;
    let instances = 200usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0xE2B_0001);

    for _ in 0..instances {
        // instância plantada (sempre satisfatível)
        let mut s_star = vec![1i8; n];
        for v in s_star.iter_mut() {
            *v = if rng.gen::<bool>() { 1 } else { -1 };
        }
        let mut clauses = Vec::with_capacity(m + 2);
        for _ in 0..m {
            let i = rng.gen_range(0..n);
            let mut j = rng.gen_range(0..n);
            if j == i {
                j = (j + 1) % n;
            }
            let si = if rng.gen::<bool>() { 1 } else { -1 };
            let sj = if s_star[i] == si {
                if rng.gen::<bool>() { 1 } else { -1 }
            } else {
                s_star[j]
            };
            clauses.push(Clause2::new(i, si, j, sj));
        }
        clauses.push(Clause2::new(0, s_star[0], 0, s_star[0]));
        clauses.push(Clause2::new(3, s_star[3], 3, s_star[3]));

        let p = presolve_2sat(n, &clauses);
        assert!(!p.conflict, "plantada não pode ser conflito");

        // força bruta: toda solução tem de concordar com as fixações
        let mut s = vec![0i8; n];
        for mask in 0..(1u64 << n) {
            for (i, si) in s.iter_mut().enumerate() {
                *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
            }
            if violations(&clauses, &s) != 0 {
                continue;
            }
            for (i, f) in p.fixed.iter().enumerate() {
                if let Some(v) = f {
                    assert_eq!(
                        s[i], *v,
                        "fixação insound: var {i} fixada em {v}, solução tem {}",
                        s[i]
                    );
                }
            }
        }
    }
}

#[test]
fn presolve_never_increases_violations() {
    // Reconstrução: fixar as variáveis e resolver só o resíduo não pode piorar.
    let n = 6usize;
    let clauses = [
        Clause2::new(0, 1, 0, 1),
        Clause2::new(0, -1, 1, 1),
        Clause2::new(1, 1, 2, 1),
        Clause2::new(2, -1, 3, 1),
        Clause2::new(3, 1, 4, -1),
        Clause2::new(4, 1, 5, 1),
    ];
    let p = presolve_2sat(n, &clauses);
    assert!(!p.conflict);
    let res = residual_clauses(&clauses, &p.fixed, &p.residual).expect("residual");
    // a instância residual tem de preservar a satisfatibilidade
    let mut s = vec![0i8; n];
    for (i, f) in p.fixed.iter().enumerate() {
        if let Some(v) = f {
            s[i] = *v;
        }
    }
    let rn = p.residual.len();
    let mut ok = false;
    for mask in 0..(1u64 << rn) {
        let mut r = vec![0i8; rn];
        for (k, rk) in r.iter_mut().enumerate() {
            *rk = if (mask >> k) & 1 == 0 { -1 } else { 1 };
        }
        if violations(&res, &r) == 0 {
            ok = true;
            break;
        }
    }
    assert!(ok, "resíduo perdeu satisfatibilidade");
}

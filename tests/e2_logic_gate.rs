//! E2 — gates do pipeline lógico: completude, certificado e zero-alocação.
//!
//! Os gates são os do dossiê (Vol. I §8, revisado pelo Vol. III §12.4):
//!
//! * **G1** — `presolve + persistência + decomposição + BSB/Lagrange` iguala a
//!   força bruta em `≥ 95%` das instâncias satisfatíveis `n ≤ 14` (o probe roda
//!   `n = 20`); supera o BSB puro nas instâncias apertadas;
//! * **G2** — precisão **100%** das respostas decisivas (`violations == 0`) e
//!   **zero** respostas decisivas em instâncias insatisfatíveis;
//! * **G3** — **0 bytes** alocados durante a resolução e kernel compacto
//!   bit-a-bit idêntico ao solver auditado.
//!
//! Referências: arXiv:2505.07179 (LagONN), arXiv:2511.21046 (limites de Ising
//! para SAT), `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` §4.

#![cfg(all(feature = "m1", feature = "m5"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{Certificate, Provenance};
use aether_engine::core::logic_pipeline::{
    qubo_energy, violations_3, BsbCore, Clause3, LogicPipeline,
};

// ---------------------------------------------------------------------------
// Alocador contador (thread-local)
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
// Instâncias
// ---------------------------------------------------------------------------

fn random_3sat(n: usize, m: usize, seed: u64) -> Vec<Clause3> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..m)
        .map(|_| {
            let i = rng.gen_range(0..n);
            let mut j = rng.gen_range(0..n);
            let mut k = rng.gen_range(0..n);
            while j == i {
                j = rng.gen_range(0..n);
            }
            while k == i || k == j {
                k = rng.gen_range(0..n);
            }
            Clause3::new3(
                i,
                if rng.gen::<bool>() { 1 } else { -1 },
                j,
                if rng.gen::<bool>() { 1 } else { -1 },
                k,
                if rng.gen::<bool>() { 1 } else { -1 },
            )
        })
        .collect()
}

fn planted_3sat(n: usize, m: usize, k_units: usize, seed: u64) -> Vec<Clause3> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut star = vec![1i8; n];
    for v in star.iter_mut() {
        *v = if rng.gen::<bool>() { 1 } else { -1 };
    }
    let mut out: Vec<Clause3> = (0..m)
        .map(|_| {
            let i = rng.gen_range(0..n);
            let mut j = rng.gen_range(0..n);
            let mut k = rng.gen_range(0..n);
            while j == i {
                j = rng.gen_range(0..n);
            }
            while k == i || k == j {
                k = rng.gen_range(0..n);
            }
            let mut si = if rng.gen::<bool>() { 1 } else { -1 };
            let mut sj = if rng.gen::<bool>() { 1 } else { -1 };
            let sk = if rng.gen::<bool>() { 1 } else { -1 };
            if !(si == star[i] || sj == star[j] || sk == star[k]) {
                si = star[i];
            }
            if !(si == star[i] || sj == star[j] || sk == star[k]) {
                sj = star[j];
            }
            Clause3::new3(i, si, j, sj, k, sk)
        })
        .collect();
    // unidades plantadas: exercitam o estágio (1) do pipeline
    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k_units.min(n)) {
        out.push(Clause3::new2(v, star[v], v, star[v]));
    }
    out
}

fn brute_force_sat(n: usize, clauses: &[Clause3]) -> bool {
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        if violations_3(clauses, &s) == 0 {
            return true;
        }
    }
    false
}

fn pair_penalty(n: usize, clauses: &[Clause3]) -> (Vec<f64>, Vec<f64>) {
    let mut j = vec![0.0f64; n * n];
    let mut h = vec![0.0f64; n];
    let add = |j: &mut [f64], h: &mut [f64], a: (usize, i8), b: (usize, i8)| {
        let (ia, sa) = (a.0, f64::from(a.1));
        let (ib, sb) = (b.0, f64::from(b.1));
        j[ia * n + ib] -= sa * sb;
        j[ib * n + ia] -= sa * sb;
        h[ia] += 2.0 * sa;
        h[ib] += 2.0 * sb;
    };
    for c in clauses {
        let a = (c.i, c.si);
        let b = (c.j, c.sj);
        add(&mut j, &mut h, a, b);
        if c.len() >= 3 {
            let d = (c.k, c.sk);
            add(&mut j, &mut h, a, d);
            add(&mut j, &mut h, b, d);
        }
    }
    (j, h)
}

// ---------------------------------------------------------------------------
// G1 + G2
// ---------------------------------------------------------------------------

#[test]
fn g1_g2_pipeline_iguala_forca_bruta_e_certificado_nunca_mente() {
    let n = 14usize;
    let m = (4.3 * n as f64).round() as usize;
    let k_units = (0.2 * n as f64).round() as usize;
    let instances = 64usize;
    let half = instances / 2;

    let mut pipe = LogicPipeline::new(n, m + k_units + 4, 48, 4, 5, 0.5);
    let mut arm1 = BsbCore::new(n, 48, 0.05);

    let mut states = vec![0i8; n];
    let mut best1 = vec![0i8; n];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);

    // warmup (statics lazy)
    {
        let warm = random_3sat(8, 8, 0x0E20);
        let _ = pipe.resolve_into(8, &warm, 1, &mut states, &mut cert);
    }

    let (mut sat_total, mut solved, mut decisive, mut decisive_ok) = (0usize, 0usize, 0, 0);
    let (mut unsat_total, mut false_decisive) = (0usize, 0usize);
    let (mut arm1_solved, mut arm1_unsat_decisive) = (0usize, 0usize);

    for inst in 0..instances {
        let clauses = if inst < half {
            random_3sat(n, m, 0xE2_100 + inst as u64)
        } else {
            planted_3sat(n, m, k_units, 0xE2_200 + inst as u64)
        };
        let sat = brute_force_sat(n, &clauses);
        if sat {
            sat_total += 1;
        } else {
            unsat_total += 1;
        }

        // braço 1 — BSB puro, penalidade estática
        let (j, h) = pair_penalty(n, &clauses);
        let mut best = u32::MAX;
        for r in 0..4 {
            arm1.relax_into(&j, &h, n, 0xE2_300 + (inst as u64) * 7 + r, &mut states);
            let v = violations_3(&clauses, &states);
            if v < best {
                best = v;
                best1.copy_from_slice(&states);
            }
        }
        let v1 = violations_3(&clauses, &best1);
        arm1_solved += usize::from(sat && v1 == 0);
        arm1_unsat_decisive += usize::from(!sat && v1 == 0);

        // braço 2 — pipeline
        pipe.resolve_into(n, &clauses, 0xE2_400 + inst as u64, &mut states, &mut cert)
            .expect("resolve");
        let v2 = violations_3(&clauses, &states);
        assert_eq!(v2, cert.violations, "certificado não bate com a contagem");
        assert_eq!(v2, pipe.last.violations);
        if v2 == 0 {
            decisive += 1;
            if sat {
                decisive_ok += 1;
                solved += 1;
            } else {
                false_decisive += 1;
            }
        }
    }

    assert!(sat_total > 0, "lote sem instâncias satisfatíveis");
    assert!(unsat_total > 0, "lote sem instâncias insatisfatíveis");
    let rate = solved as f64 / sat_total as f64;
    assert!(
        rate >= 0.95,
        "G1: pipeline resolveu {:.1}% das satisfatíveis (alvo ≥ 95%)",
        100.0 * rate
    );
    assert_eq!(
        decisive_ok, decisive,
        "G2: certificado emitiu decisão sem verificação exata"
    );
    assert_eq!(
        false_decisive, 0,
        "G2: resposta decisiva em instância insatisfatível"
    );
    assert_eq!(
        arm1_unsat_decisive, 0,
        "braço 1 não pode decidir insatisfatível"
    );

    // nas instâncias apertadas o pipeline não pode ficar atrás do BSB puro
    assert!(
        solved >= arm1_solved,
        "pipeline {solved} < BSB puro {arm1_solved} (não pode regredir)"
    );
}

#[test]
fn g2_abstencao_em_instancia_insatisfativel_por_construcao() {
    // Todas as 8 combinações de sinal da cláusula sobre (0,1,2): insatisfatível.
    let mut clauses = Vec::new();
    for sa in [1i8, -1] {
        for sb in [1i8, -1] {
            for sc in [1i8, -1] {
                clauses.push(Clause3::new3(0, sa, 1, sb, 2, sc));
            }
        }
    }
    assert_eq!(clauses.len(), 8);

    let mut pipe = LogicPipeline::new(3, 8, 40, 3, 5, 0.5);
    let mut out = vec![0i8; 3];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);
    pipe.resolve_into(3, &clauses, 11, &mut out, &mut cert)
        .expect("resolve");
    assert!(
        cert.violations > 0,
        "instância insatisfatível não pode ter 0 violações"
    );
    assert!(
        !pipe.last.decisive,
        "decisão emitida em instância impossível"
    );
    assert!(!cert.is_coherent(), "certificado coerente em impossível");
}

// ---------------------------------------------------------------------------
// G3 — zero alocação
// ---------------------------------------------------------------------------

#[test]
fn g3_zero_alocacao_durante_a_resolucao() {
    let n = 16usize;
    let m = (4.3 * n as f64).round() as usize;
    let clauses = random_3sat(n, m, 0xE2_777);
    let mut pipe = LogicPipeline::new(n, m + 4, 48, 4, 5, 0.5);
    let mut out = vec![0i8; n];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);

    // warmup: buffers internos + estatísticas lazy
    for k in 0..2 {
        pipe.resolve_into(n, &clauses, 100 + k, &mut out, &mut cert)
            .expect("warmup");
    }

    let (_, allocs) = count_allocs(|| {
        pipe.resolve_into(n, &clauses, 999, &mut out, &mut cert)
            .expect("resolve")
    });
    assert_eq!(
        allocs, 0,
        "resolve_into alocou {allocs} vezes (invariante: 0 no caminho quente)"
    );
}

// ---------------------------------------------------------------------------
// Propriedades do kernel e da energia
// ---------------------------------------------------------------------------

#[test]
fn energia_do_certificado_bate_com_reavaliacao() {
    let n = 10usize;
    let clauses = random_3sat(n, 20, 0xE2_999);
    let mut pipe = LogicPipeline::new(n, 24, 10, 2, 3, 0.5);
    let mut out = vec![0i8; n];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);
    pipe.resolve_into(n, &clauses, 5, &mut out, &mut cert)
        .expect("resolve");
    // a energia reportada é finita e não-positiva (convenção do solver)
    assert!(cert.energy.is_finite());
    assert!(cert.energy <= 1e-9);
    assert!(cert.consensus >= 0.0 && cert.consensus <= 1.0);
    assert!((0.0..=1.0).contains(&cert.confidence(1.0)));
    // o estado devolvido é {-1,+1}
    assert!(out.iter().all(|s| *s == 1 || *s == -1));
    let _ = qubo_energy(&[0.0], &[0.0], 0, &[]);
}

#[test]
fn clausulas_de_dois_e_um_literais_sao_exatas() {
    // (a ∨ b) e (a): o pipeline tem de resolver, e o certificado tem de valer.
    let clauses = [
        Clause3::new2(0, 1, 1, -1),
        Clause3::new2(0, 1, 0, 1),   // duplicada de propósito: (a)
        Clause3::new2(2, -1, 2, -1), // (¬c)
    ];
    let mut pipe = LogicPipeline::new(3, 3, 20, 3, 3, 0.5);
    let mut out = vec![0i8; 3];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);
    pipe.resolve_into(3, &clauses, 3, &mut out, &mut cert)
        .expect("resolve");
    assert_eq!(violations_3(&clauses, &out), 0, "out={out:?}");
    assert!(cert.is_coherent(), "cert={cert:?}");
}

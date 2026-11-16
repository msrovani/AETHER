//! E2c-3 — o que vem **depois** do roof dual em QUBO denso.
//!
//! O E2c-2 fechou o roof dual e mediu: no vidro puro (`h=0`) ele fixa 0, e sob
//! viés estende a dominância (+9,1 pp em plant=45%). A literatura de 2026 diz
//! por quê e o que fazer:
//!
//! * **Naumann & Andres (arXiv:2610.20346, 2026):** todo posiforme ótimo de roof
//!   dual já dá o **maior domínio de autarkia possível**. Nenhum autarky de
//!   posiforme quadrático fixa mais que o roof dual — para ir além é preciso
//!   sair de autarkias.
//! * **Wang & Hahn (Front. Comput. Sci. 2026):** *probing* (fixar variáveis e
//!   recomputar o roof dual) aumenta a probabilidade de atingir o ótimo global;
//!   com um subconjunto pequeno, o resíduo vira 2-SAT solúvel e o ótimo é
//!   **certificado**. Em QUBO aleatório denso, ~metade das variáveis basta.
//! * **Rother et al. (CVPR 2007):** QPBO-P, a versão prática do probing.
//!
//! Este probe mede, em `n=14` (mesmo gerador do E2c): fixações do roof dual,
//! fixações por **união de persistências dos dois ramos** (probing sem 2-SAT) e
//! o custo de uma **busca exata guiada por roof dual** (nós/folhas) — com
//! soundness contra força bruta.
//!
//! ```text
//! cargo run --release --example e2c3_probe
//! ```
//!
//! Saída: tabela + `target/aether_e2c3_probe.json`.

#![allow(clippy::needless_range_loop)]

use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::roof_dual_qumo;

const N: usize = 14;
const DELTA: f64 = 0.6;
const INSTANCES: usize = 200;
const INSTANCES_DFS_PLANT0: usize = 20;
const SEED0: u64 = 0xE2C_2026;

fn make_instance(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
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
        let e = energy(j, h, n, &s);
        if e < best {
            best = e;
        }
    }
    best
}

/// Reduz o QUBO às variáveis livres, substituindo as fixas em `h`.
fn reduce(
    n: usize,
    j: &[f64],
    h: &[f64],
    assign: &[Option<i8>],
) -> (Vec<usize>, Vec<f64>, Vec<f64>) {
    let free: Vec<usize> = (0..n).filter(|&i| assign[i].is_none()).collect();
    let m = free.len();
    let mut js = vec![0.0f64; m * m];
    let mut hs = vec![0.0f64; m];
    for (a, &i) in free.iter().enumerate() {
        let mut hi = h[i];
        for (k, ak) in assign.iter().enumerate() {
            if let Some(v) = ak {
                if k != i {
                    hi += j[i * n + k] * f64::from(*v);
                }
            }
        }
        hs[a] = hi;
        for (b, &k) in free.iter().enumerate() {
            js[a * m + b] = j[i * n + k];
        }
    }
    (free, js, hs)
}

fn full_energy(n: usize, j: &[f64], h: &[f64], assign: &[Option<i8>]) -> f64 {
    let s: Vec<i8> = assign.iter().map(|x| x.expect("completo")).collect();
    energy(j, h, n, &s)
}

/// Fixações de uma passada de roof dual.
fn root_fixed(n: usize, j: &[f64], h: &[f64]) -> usize {
    roof_dual_qumo(n, j, h).fixed_count()
}

/// Probing por união de persistências: se os dois ramos de `p` fixam `q` no
/// mesmo valor, `q` é persistente no problema original. Itera até fixpoint.
fn probe_fixed(n: usize, j: &[f64], h: &[f64], max_rounds: usize) -> usize {
    let mut assign = vec![None::<i8>; n];
    for _ in 0..max_rounds {
        let (free, js, hs) = reduce(n, j, h, &assign);
        if free.is_empty() {
            break;
        }
        let p = roof_dual_qumo(free.len(), &js, &hs);
        let mut changed = false;
        for (a, &i) in free.iter().enumerate() {
            if let Some(v) = p.fixed[a] {
                assign[i] = Some(v);
                changed = true;
            }
        }

        let free: Vec<usize> = (0..n).filter(|&i| assign[i].is_none()).collect();
        if free.len() <= 1 {
            if changed {
                continue;
            }
            break;
        }
        let mut agree = vec![None::<i8>; n];
        let mut disagree = vec![false; n];
        for &pv in &free {
            let mut bf = [vec![None::<i8>; n], vec![None::<i8>; n]];
            for (bi, &v) in [-1i8, 1i8].iter().enumerate() {
                let mut a2 = assign.clone();
                a2[pv] = Some(v);
                let (f2, js2, hs2) = reduce(n, j, h, &a2);
                let q = roof_dual_qumo(f2.len(), &js2, &hs2);
                for (a, &i) in f2.iter().enumerate() {
                    if let Some(x) = q.fixed[a] {
                        bf[bi][i] = Some(x);
                    }
                }
            }
            for i in 0..n {
                if i == pv {
                    continue;
                }
                match (bf[0][i], bf[1][i]) {
                    (Some(x), Some(y)) if x == y => {
                        if let Some(old) = agree[i] {
                            if old != x {
                                disagree[i] = true;
                            }
                        } else {
                            agree[i] = Some(x);
                        }
                    }
                    _ => disagree[i] = true,
                }
            }
        }
        let mut applied = false;
        for i in 0..n {
            if !disagree[i] {
                if let Some(v) = agree[i] {
                    if assign[i].is_none() {
                        assign[i] = Some(v);
                        applied = true;
                    }
                }
            }
        }
        if !changed && !applied {
            break;
        }
    }
    assign.iter().filter(|x| x.is_some()).count()
}

#[derive(Default)]
struct SearchStats {
    nodes: usize,
    leaves: usize,
    root_fixed: usize,
}

/// Busca exata: em cada nó, o roof dual propaga fixações; quando não há mais
/// nada a fixar, ramifica numa variável. A folha é uma atribuição completa.
///
/// Cada nó trabalha numa **cópia local** da atribuição: as fixações de um ramo
/// não vazam para o irmão (bug clássico de backtracking).
fn dfs(
    n: usize,
    j: &[f64],
    h: &[f64],
    assign: &[Option<i8>],
    st: &mut SearchStats,
    is_root: bool,
) -> f64 {
    st.nodes += 1;
    let mut a = assign.to_vec();
    let free: Vec<usize> = (0..n).filter(|&i| a[i].is_none()).collect();
    if free.is_empty() {
        st.leaves += 1;
        return full_energy(n, j, h, &a);
    }
    let (_, js, hs) = reduce(n, j, h, &a);
    let p = roof_dual_qumo(free.len(), &js, &hs);
    if is_root {
        st.root_fixed = p.fixed_count();
    }
    let mut applied = false;
    for (k, &i) in free.iter().enumerate() {
        if let Some(v) = p.fixed[k] {
            a[i] = Some(v);
            applied = true;
        }
    }
    if applied {
        return dfs(n, j, h, &a, st, false);
    }

    let pv = free[0];
    let mut best = f64::INFINITY;
    for v in [-1i8, 1] {
        a[pv] = Some(v);
        let e = dfs(n, j, h, &a, st, false);
        if e < best {
            best = e;
        }
        a[pv] = None;
    }
    best
}

fn main() {
    println!("E2c-3 probe | n={N} inst={INSTANCES} delta={DELTA} seed={SEED0:#x}");
    println!(
        "{:>6} {:>9} {:>10} {:>7} {:>10} {:>9} {:>9} {:>7} {:>8}",
        "plant",
        "root_fix",
        "probe_fix",
        "extra",
        "dfs_nodes",
        "dfs_leaf",
        "leaf_frac",
        "ms",
        "sound"
    );

    let mut json = String::from("[\n");
    let regimes = [0.0f64, 0.25, 0.45];

    for (ri, &plant) in regimes.iter().enumerate() {
        let k = (plant * N as f64).round() as usize;
        let dfs_n = if plant == 0.0 {
            INSTANCES_DFS_PLANT0
        } else {
            INSTANCES
        };

        let mut root_sum = 0usize;
        let mut probe_sum = 0usize;
        let mut nodes_sum = 0usize;
        let mut leaves_sum = 0usize;
        let mut sound = 0usize;
        let mut dfs_runs = 0usize;

        let t0 = Instant::now();
        for inst in 0..INSTANCES {
            let seed = SEED0.wrapping_add(inst as u64);
            let (j, h) = make_instance(N, k, DELTA, seed);

            root_sum += root_fixed(N, &j, &h);
            probe_sum += probe_fixed(N, &j, &h, 8);

            if inst < dfs_n {
                let assign = vec![None::<i8>; N];
                let mut st = SearchStats::default();
                let e_dfs = dfs(N, &j, &h, &assign, &mut st, true);
                let e_bf = brute_force(N, &j, &h);
                if (e_dfs - e_bf).abs() < 1e-9 {
                    sound += 1;
                }
                nodes_sum += st.nodes;
                leaves_sum += st.leaves;
                dfs_runs += 1;
            }
        }
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let leaf_frac = leaves_sum as f64 / 2f64.powi(N as i32) / dfs_runs as f64;

        println!(
            "{:>5.0}% {:>9.2} {:>10.2} {:>7.2} {:>10.1} {:>9.1} {:>9.5} {:>7.1} {:>4}/{:<3}",
            plant * 100.0,
            root_sum as f64 / INSTANCES as f64,
            probe_sum as f64 / INSTANCES as f64,
            (probe_sum as f64 - root_sum as f64) / INSTANCES as f64,
            nodes_sum as f64 / dfs_runs as f64,
            leaves_sum as f64 / dfs_runs as f64,
            leaf_frac,
            dt,
            sound,
            dfs_runs
        );

        json.push_str(&format!(
            "  {{\"plant\": {plant}, \"k\": {k}, \"root_fix\": {:.3}, \"probe_fix\": {:.3}, \"extra\": {:.3}, \"dfs_nodes\": {:.1}, \"dfs_leaves\": {:.1}, \"leaf_fraction\": {:.6}, \"sound\": {sound}, \"dfs_runs\": {dfs_runs}, \"ms\": {:.1}}}{}\n",
            root_sum as f64 / INSTANCES as f64,
            probe_sum as f64 / INSTANCES as f64,
            (probe_sum as f64 - root_sum as f64) / INSTANCES as f64,
            nodes_sum as f64 / dfs_runs as f64,
            leaves_sum as f64 / dfs_runs as f64,
            leaf_frac,
            dt,
            if ri + 1 < regimes.len() { "," } else { "" }
        ));
    }
    json.push_str("]\n");
    let _ = std::fs::write("target/aether_e2c3_probe.json", json);
    println!("\nartefato: target/aether_e2c3_probe.json");
}

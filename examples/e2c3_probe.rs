//! E2c-3 â€” o que vem **depois** do roof dual em QUBO denso.
//!
//! O E2c-2 fechou o roof dual e mediu: no vidro puro (`h=0`) ele fixa 0, e sob
//! viÃ©s estende a dominÃ¢ncia (+9,1 pp em plant=45%). A literatura de 2026 diz
//! por quÃª e o que fazer:
//!
//! * **Naumann & Andres (arXiv:2610.20346, 2026):** todo posiforme Ã³timo de roof
//!   dual jÃ¡ dÃ¡ o **maior domÃ­nio de autarkia possÃ­vel**. Nenhum autarky de
//!   posiforme quadrÃ¡tico fixa mais que o roof dual â€” para ir alÃ©m Ã© preciso
//!   sair de autarkias.
//! * **Wang & Hahn (Front. Comput. Sci. 2026):** *probing* (fixar variÃ¡veis e
//!   recomputar o roof dual) aumenta a probabilidade de atingir o Ã³timo global;
//!   com um subconjunto pequeno, o resÃ­duo vira 2-SAT solÃºvel e o Ã³timo Ã©
//!   **certificado**. Em QUBO aleatÃ³rio denso, ~metade das variÃ¡veis basta.
//! * **Rother et al. (CVPR 2007):** QPBO-P, a versÃ£o prÃ¡tica do probing.
//!
//! Este probe mede, em `n=14` (mesmo gerador do E2c): fixaÃ§Ãµes do roof dual,
//! fixaÃ§Ãµes por **uniÃ£o de persistÃªncias dos dois ramos** (probing sem 2-SAT) e
//! o custo de uma **busca exata guiada por roof dual** (nÃ³s/folhas) â€” com
//! soundness contra forÃ§a bruta.
//!
//! ```text
//! cargo run --release --example e2c3_probe
//! ```
//!
//! SaÃ­da: tabela + `target/aether_e2c3_probe.json`.

#![allow(clippy::needless_range_loop)]

use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::persistency_search::ExactSearch;
use aether_engine::core::presolve::{roof_dual_qumo, RoofDualWorkspace, ROOF_MAX_N};

const N: usize = 14;
const DELTA: f64 = 0.6;
const INSTANCES: usize = 200;
const INSTANCES_DFS_PLANT0: usize = 20;
const SEED0: u64 = 0xE2C_2026;
/// OrÃ§amento de nÃ³s por instÃ¢ncia no modo `--scale`.
const SCALE_BUDGET: u32 = 500_000;

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

/// Reduz o QUBO Ã s variÃ¡veis livres, substituindo as fixas em `h`.
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

/// FixaÃ§Ãµes de uma passada de roof dual.
fn root_fixed(n: usize, j: &[f64], h: &[f64]) -> usize {
    roof_dual_qumo(n, j, h).fixed_count()
}

/// Probing por uniÃ£o de persistÃªncias: se os dois ramos de `p` fixam `q` no
/// mesmo valor, `q` Ã© persistente no problema original. Itera atÃ© fixpoint.
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

/// Escala: atÃ© onde a busca exata fecha **sem** bound? (plant 45%)
fn scale_table() {
    println!("E2c-3 scale | plant=45% delta={DELTA} budget={SCALE_BUDGET}");
    println!(
        "{:>4} {:>4} {:>9} {:>10} {:>9} {:>8} {:>8}",
        "n", "k", "closed", "mean_nodes", "max_nodes", "mean_ms", "total_ms"
    );
    let mut json = String::from("[\n");
    let ns = [20usize, 24, 28, 32, 40, 48, 64];
    let instances = 10u64;
    for (i, &n) in ns.iter().enumerate() {
        let k = (0.45 * n as f64).round() as usize;
        let mut search = ExactSearch::new(SCALE_BUDGET);
        let mut closed = 0usize;
        let mut nodes_sum = 0u64;
        let mut nodes_max = 0u32;
        let t0 = Instant::now();
        for inst in 0..instances {
            let (j, h) = make_instance(n, k, DELTA, SEED0.wrapping_add(inst));
            let r = search.solve(n, &j, &h);
            if !r.exhausted {
                closed += 1;
                nodes_sum += u64::from(r.nodes);
                nodes_max = nodes_max.max(r.nodes);
            }
        }
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        let mean_nodes = if closed > 0 {
            nodes_sum / closed as u64
        } else {
            0
        };
        let mean_ms = dt / instances as f64;
        println!(
            "{:>4} {:>4} {:>4}/{:<4} {:>10} {:>9} {:>8.1} {:>8.1}",
            n, k, closed, instances, mean_nodes, nodes_max, mean_ms, dt
        );
        json.push_str(&format!(
            "  {{\"n\": {n}, \"k\": {k}, \"closed\": {closed}, \"instances\": {instances}, \"mean_nodes\": {mean_nodes}, \"max_nodes\": {nodes_max}, \"mean_ms\": {mean_ms:.1}, \"total_ms\": {dt:.1}}}{}\n",
            if i + 1 < ns.len() { "," } else { "" }
        ));
    }
    json.push_str("]\n");
    let _ = std::fs::write("target/aether_e2c3_scale.json", json);
    println!("\nartefato: target/aether_e2c3_scale.json");
}

/// n=64 com orçamento maior: o bound é obrigatório ou é só custo?
fn deep_n64() {
    let n = 64usize;
    let k = (0.45 * n as f64).round() as usize;
    let instances = 3u64;
    let budget = 5_000_000u32;
    println!("E2c-3 deep | n={n} k={k} budget={budget}");
    let mut search = ExactSearch::new(budget);
    for inst in 0..instances {
        let (j, h) = make_instance(n, k, DELTA, SEED0.wrapping_add(inst));
        let t0 = Instant::now();
        let r = search.solve(n, &j, &h);
        println!(
            "inst={inst} closed={} nodes={} leaves={} root={} ms={:.1}",
            !r.exhausted,
            r.nodes,
            r.leaves,
            r.root_fixed,
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// O `pass_into` público devolve o fluxo. Testa se `c0 + flow` é um lower bound
/// válido, comparando com a força bruta.
fn bound_check() {
    let n = 14usize;
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = [None::<i8>; ROOF_MAX_N];
    println!("E2c-3 bound | n={n} c0+flow vs bruto");
    for inst in 0..10u64 {
        let k = (0.45 * n as f64).round() as usize;
        let (j, h) = make_instance(n, k, DELTA, SEED0.wrapping_add(inst));
        let flow = ws.pass_into(n, &j, &h, &mut fixed[..n]);
        let mut c0 = 0.0;
        for i in 0..n {
            c0 += h[i];
            for kk in (i + 1)..n {
                c0 -= j[i * n + kk];
            }
        }
        let lb = c0 + flow;
        let opt = brute_force(n, &j, &h);
        println!(
            "inst={inst} flow={flow:.3} c0={c0:.3} lb={lb:.3} opt={opt:.3} gap={:.3} valid={}",
            (opt - lb).max(0.0),
            lb <= opt + 1e-9
        );
    }
}

fn main() {
    if std::env::args().any(|a| a == "--scale") {
        scale_table();
        return;
    }
    if std::env::args().any(|a| a == "--deep") {
        deep_n64();
        return;
    }
    if std::env::args().any(|a| a == "--bound") {
        bound_check();
        return;
    }
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
    let mut search = ExactSearch::new(u32::MAX);
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
                let rep = search.solve(N, &j, &h);
                let e_bf = brute_force(N, &j, &h);
                if rep.energy.is_finite() && (rep.energy - e_bf).abs() < 1e-9 {
                    sound += 1;
                }
                nodes_sum += rep.nodes as usize;
                leaves_sum += rep.leaves as usize;
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

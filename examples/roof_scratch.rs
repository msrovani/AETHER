//! Scratch: diagnóstico do roof dual em instâncias mínimas.
use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{persistency_qumo, roof_dual_qumo, RoofDualWorkspace};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn bf(n: usize, j: &[f64], h: &[f64]) -> (f64, Vec<i8>) {
    let mut best = f64::INFINITY;
    let mut bs = vec![0i8; n];
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        let e = energy(j, h, n, &s);
        if e < best {
            best = e;
            bs.copy_from_slice(&s);
        }
    }
    (best, bs)
}

fn report(name: &str, n: usize, j: Vec<f64>, h: Vec<f64>) {
    let (e, s) = bf(n, &j, &h);
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = vec![None::<i8>; n];
    let flow = ws.pass_into(n, &j, &h, &mut fixed);
    let pre = roof_dual_qumo(n, &j, &h);
    println!("\n=== {name} === flow={flow:.3}");
    println!("  brute: E*={e:.4} s*={s:?}");
    println!("  pass_into fixeds={fixed:?}");
    println!(
        "  roof_dual_qumo fixeds={:?} residual={:?}",
        pre.fixed, pre.residual
    );
    // soundness do braço completo
    let mut s2 = vec![0i8; n];
    for (i, f) in pre.fixed.iter().enumerate() {
        if let Some(v) = f {
            s2[i] = *v;
        }
    }
    let rn = pre.residual.len();
    let mut best = f64::INFINITY;
    if rn == 0 {
        best = energy(&j, &h, n, &s2);
    } else {
        for mask in 0..(1u64 << rn) {
            for (k, &old) in pre.residual.iter().enumerate() {
                s2[old] = if (mask >> k) & 1 == 0 { -1 } else { 1 };
            }
            best = best.min(energy(&j, &h, n, &s2));
        }
    }
    println!("  E_reduzido={best:.4}  sound={}", (best - e).abs() < 1e-9);
}

fn glass(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
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

fn main() {
    report("ferro J=+1", 2, vec![0.0, 1.0, 1.0, 0.0], vec![0.0, 0.0]);
    report(
        "antiferro J=-1",
        2,
        vec![0.0, -1.0, -1.0, 0.0],
        vec![0.0, 0.0],
    );
    report(
        "antiferro J=-1 + h0=2",
        2,
        vec![0.0, -1.0, -1.0, 0.0],
        vec![2.0, 0.0],
    );
    report("viés h0=+2", 2, vec![0.0, 1.0, 1.0, 0.0], vec![2.0, 0.0]);

    // resumo por regime
    for plant in [0.0f64, 0.25, 0.45] {
        let n = 14usize;
        let k = (plant * n as f64).round() as usize;
        let mut roof_sum = 0.0;
        let mut dom_sum = 0.0;
        let instances = 60usize;
        let mut unsound = 0usize;`n        let mut roof_only_sum = 0.0f64;
        for inst in 0..instances {
            let (j, h) = glass(n, k, 0.6, 0xE2C_0001 + inst as u64);
            let roof = roof_dual_qumo(n, &j, &h);
            let dom = persistency_qumo(n, &j, &h);
            roof_sum += roof.reduction(n);
            dom_sum += dom.reduction(n);
            // roof puro (sem a pré-passada de dominância)
            let mut ws = RoofDualWorkspace::new();
            let mut rf = vec![None::<i8>; n];
            let nfix = ws.solve_into(n, &j, &h, &mut rf);
            roof_only_sum += nfix as f64 / n as f64;
            // soundness
            let (e_star, _) = bf(n, &j, &h);
            let mut s2 = vec![0i8; n];
            for (i, f) in roof.fixed.iter().enumerate() {
                if let Some(v) = f {
                    s2[i] = *v;
                }
            }
            let rn = roof.residual.len();
            let mut best = f64::INFINITY;
            if rn == 0 {
                best = energy(&j, &h, n, &s2);
            } else {
                for mask in 0..(1u64 << rn) {
                    for (kk, &old) in roof.residual.iter().enumerate() {
                        s2[old] = if (mask >> kk) & 1 == 0 { -1 } else { 1 };
                    }
                    best = best.min(energy(&j, &h, n, &s2));
                }
            }
            if (best - e_star).abs() > 1e-9 {
                unsound += 1;
            }
        }
        println!(
            "\nplant={:.0}%  dominância={:.1}%  roof={:.1}%  unsound={}",
            100.0 * plant,
            100.0 * dom_sum / instances as f64,
            100.0 * roof_sum / instances as f64,`n            100.0 * roof_only_sum / instances as f64,
            unsound
        );
    }
}

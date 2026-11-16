//! Scratch: diagnóstico do roof dual em instâncias mínimas.
use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{roof_dual_qumo, RoofDualWorkspace};

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
}

fn main() {
    // 1) termo ferromagnético puro: J12 = +1, h = 0 → ótimo s = (1,1) ou (-1,-1)
    report(
        "ferro J=+1",
        2,
        vec![0.0, 1.0, 1.0, 0.0],
        vec![0.0, 0.0],
    );
    // 2) antiferro puro: J12 = -1 → ótimo s = (1,-1) / (-1,1)
    report(
        "antiferro J=-1",
        2,
        vec![0.0, -1.0, -1.0, 0.0],
        vec![0.0, 0.0],
    );
    // 3) viés forte em 0 (dominância fixa)
    report(
        "viés h0=+2",
        2,
        vec![0.0, 1.0, 1.0, 0.0],
        vec![2.0, 0.0],
    );
    // 4) triângulo frustrado
    let mut j = vec![0.0f64; 9];
    for (a, b, w) in [(0, 1, -1.0), (1, 2, -1.0), (0, 2, -1.0)] {
        j[a * 3 + b] = w;
        j[b * 3 + a] = w;
    }
    report("triângulo frustrado", 3, j, vec![0.0; 3]);
    // 5) 4 spins aleatórios
    let n = 4;
    let mut j = vec![0.0f64; n * n];
    let vals = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
    let mut k = 0;
    for i in 0..n {
        for m in (i + 1)..n {
            j[i * n + m] = vals[k];
            j[m * n + i] = vals[k];
            k += 1;
        }
    }
    report("4 spins ±1", n, j, vec![0.0; n]);
}

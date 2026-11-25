//! Gate SB-5 (L8, G-MORPH) — síntese de bacias emergentes:
//! 1. `new_basin_count` sobe (> 0) com spins adversários + surpresa
//!    (cristalização de bacias híbridas);
//! 2. `take_new_basin_count` zera o contador (consumidor interface).

#![cfg(feature = "m7")]

use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

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

#[test]
fn gate_s5_new_basin_events() {
    let n = 16;
    let mut field = MorphogeneticField::new(n, &j0(n, 0xCA5), MorphogenesisConfig::default());
    let spins: Vec<f32> = (0..n).map(|i| if i % 2 == 0 { 0.9 } else { -0.9 }).collect();
    // Evolui até cristalização da bacia (densidade > 0,30, capacidade 20 frames).
    let mut steps = 0usize;
    loop {
        field.try_evolve_topology(1.0, &spins, 0.05).expect("step");
        steps += 1;
        if field.density() > 0.30 || steps >= 20 {
            break;
        }
    }
    assert!(
        field.new_basin_count() >= 1,
        "bacia nascente nao detectada (contagem = {}, densidade = {:.3})",
        field.new_basin_count(),
        field.density()
    );
    let c = field.take_new_basin_count();
    assert!(c >= 1);
    assert_eq!(field.new_basin_count(), 0, "take nao zerou");
}

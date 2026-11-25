//! Gate S5 (R6/F23) — Assinaturas macro (G-ISO cross-modal):
//! 1. `macro_signature()` determinística (2 chamadas, mesma fase/ordem);
//! 2. Collision rate: ≤ 2 colisões em 10 assinaturas distintas (campo adversarial).

#![cfg(feature = "m1")]

use std::collections::HashSet;

use aether_engine::core::renormalization::RenormalizationHierarchy;

#[test]
fn gate_s5_signature_deterministic() {
    let mut rg = RenormalizationHierarchy::new(64).expect("rg64");
    let f = vec![0.3f32; 64];
    rg.forward(&f).expect("forward");
    assert_eq!(
        rg.macro_signature(),
        rg.macro_signature(),
        "macro_signature nao e deterministica"
    );
}

#[test]
fn gate_s5_collision_rate() {
    let mut rg = RenormalizationHierarchy::new(64).expect("rg64");
    let mut sigs = Vec::new();
    for t in 0..10i32 {
        let mut f = vec![0.0f32; 64];
        for i in 0..64 {
            f[i] = 0.4 + 0.008 * t as f32 * (1.0 + (i % 5) as f32);
        }
        rg.forward(&f).expect("forward");
        sigs.push(rg.macro_signature());
    }
    let uniq = sigs.iter().collect::<HashSet<_>>().len();
    let collisions = 10 - uniq as i32;
    assert!(
        collisions <= 2,
        "colisões {collisions}/10 (expectativa ≤ 2, R6/F23)"
    );
}

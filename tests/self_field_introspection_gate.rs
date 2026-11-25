//! Gate SB-6 (L9) — `observe_topology` (SelfField introspecção):
//! 1. `topo_margin_ema` decai para 0 com `rho_bound = 1.0` (estável);
//! 2. Sobe com `rho_bound = 0.0` (margem máxima); sempre finito em [0, 1].

#![cfg(feature = "m1")]

use aether_engine::core::self_field::SelfField;

#[test]
fn gate_s6_topo_stability_ema() {
    let mut sf = SelfField::new();
    assert!(sf.topo_stability_ema() >= 0.0 && sf.topo_stability_ema() <= 1.0);

    for _ in 0..200 {
        sf.observe_j_stability(1.0);
    }
    assert!(
        sf.topo_stability_ema().abs() < 1e-6,
        "margem com rho_bound=1.0 deveria empuxar para 0: {:.4}",
        sf.topo_stability_ema()
    );

    for _ in 0..200 {
        sf.observe_j_stability(0.0);
    }
    let m = sf.topo_stability_ema();
    assert!(m > 1e-3, "margem deveria subir (rho_bound=0): {m:.4}");
    assert!(m <= 1.0, "margem fora de [0,1]: {m}");

    // Defesas: entrada não finita/negativa não corrumpem o estado.
    sf.observe_j_stability(-1.0);
    sf.observe_j_stability(f32::NAN);
    sf.observe_j_stability(f32::INFINITY);
    assert!(sf.topo_stability_ema().is_finite());
}

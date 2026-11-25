//! Gate S3 (R3) — RG spine (L12):
//! 1. `rg_spine_applied > 0` (loops aplicam ascensão + torque);
//! 2. `cbf_violations == 0` (invariante do reflexo);
//! 3. `rg_macro_order > 0` com eventos adversários (macro somente existe
//!    quando há energia ascendente).

#![cfg(feature = "m1")]

use aether_engine::core::aether_governor::{Governor, GovernorConfig, ReflexEvent, LATENT_DIM};
use aether_engine::core::renormalization::RenormalizationHierarchy;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[test]
fn gate_s3_rg_spine_aplicado() {
    let mut cfg = GovernorConfig::default();
    // micro=16 (RenormalizationHierarchy exige múltiplo de 16).
    cfg.rg = Some(RenormalizationHierarchy::new(16).expect("rg16"));

    let mut gov = Governor::spawn(cfg).expect("spawn");

    let mut rng = ChaCha8Rng::seed_from_u64(0xC0DE);
    for _ in 0..200 {
        let mut z = [0.0f32; LATENT_DIM];
        for v in z.iter_mut() {
            *v = rng.gen::<f32>() * 2.0 - 1.0;
        }
        assert!(gov.submit(ReflexEvent { t_ns: 0, z }), "fila cheia");
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = gov.shutdown().expect("shutdown");

    assert_eq!(report.cbf_violations, 0, "CBF violations do R3");
    assert!(
        report.rg_spine_applied > 0,
        "RG spine nunca aplicado (expectativa: 200)"
    );
    assert!(
        report.rg_macro_order > 0.0,
        "macro_order == 0 com eventos não-nulos"
    );
}

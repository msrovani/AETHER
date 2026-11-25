//! Gate S4 (R4) — Dream → E2 commit (L3/L4):
//! 1. `actuator_applied > 0` (EmaActuator suaviza a política decidida);
//! 2. `cbf_violations == 0` (invariante do reflexo);
//! 3. `last_smoothed_policy ∈ [0, 3)` (política validada no domínio).

#![cfg(feature = "m1")]

use aether_engine::core::active_inference::EmaActuator;
use aether_engine::core::aether_governor::{
    Governor, GovernorConfig, ReflexEvent, LATENT_DIM,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[test]
fn gate_s4_dream_e2_commit_via_ema() {
    let mut cfg = GovernorConfig::default();
    let actuator = EmaActuator::new(1, 0.5).expect("ema");
    cfg.actuator = Some(actuator);

    let mut gov = Governor::spawn(cfg).expect("spawn");

    let mut rng = ChaCha8Rng::seed_from_u64(0xD2E2);
    for _ in 0..100 {
        let mut z = [0.0f32; LATENT_DIM];
        for v in z.iter_mut() {
            *v = rng.gen::<f32>() * 2.0 - 1.0;
        }
        assert!(gov.submit(ReflexEvent { t_ns: 0, z }), "fila cheia");
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    let report = gov.shutdown().expect("shutdown");

    assert!(
        report.actuator_applied > 0,
        "EmaActuator nunca aplicou (expectativa: 100)"
    );
    assert_eq!(report.cbf_violations, 0, "CBF violations do R4");
    assert!(
        report.last_smoothed_policy >= 0.0 && report.last_smoothed_policy < 3.0,
        "smoothed policy fora de [0, 3): {}",
        report.last_smoothed_policy
    );
}

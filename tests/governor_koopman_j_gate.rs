//! Gate SB-2 (Sprint B, L4) — Governor publica Koopman J:
//! `last_koopman_j()` devolve a matriz W (len = dim*dim) após eventos.

#![cfg(feature = "m1")]

use aether_engine::core::aether_governor::{Governor, GovernorConfig, ReflexEvent, LATENT_DIM};
use aether_engine::core::koopman_sindy::{KOOPMAN_D, KOOPMAN_P};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[test]
fn gate_sb2_governor_exposes_koopman_j() {
    let mut gov = Governor::spawn(GovernorConfig::default()).expect("spawn");
    // (Sem assert de "None inicial" — a thread koopman pode já ter processado
    // o primeiro evento no intervalo de agendamento; o contrato é acesso
    // idempotente após eventos, gate S3 verifica disponibilidade após 200.)

    let mut rng = ChaCha8Rng::seed_from_u64(0x5A00_5A00);
    for _ in 0..200 {
        let mut z = [0.0f32; LATENT_DIM];
        for v in z.iter_mut() {
            *v = rng.gen::<f32>() * 2.0 - 1.0;
        }
        assert!(gov.submit(ReflexEvent { t_ns: 0, z }), "fila cheia");
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    let j = gov.last_koopman_j().expect("Koopman J disponível");
    // `w` do operador lifted: `out_dim × dim` (KOOPMAN_P²) — SB-2 documenta
    // as adaptações de forma para o consumidor no Dreamer (Sprint B).
    assert_eq!(j.len(), KOOPMAN_P * KOOPMAN_P);
    assert!(j.iter().all(|v| v.is_finite()), "J com valores não finitos");
    let _ = KOOPMAN_D;
    assert!(j.iter().all(|v| v.is_finite()), "J com valores não finitos");
    gov.shutdown().expect("shutdown");
}

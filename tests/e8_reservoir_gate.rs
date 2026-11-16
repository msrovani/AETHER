//! E8 — gate do reservatório congelado (Vol. II §6/§10).
//!
//! Gates:
//! 1. regressão de acurácia do readout congelado frente à física plástica
//!    `< 5%`;
//! 2. treino de `10^6` amostras `< 60 s` em 1 core (amostra reduzida em debug
//!    para o teste continuar utilizável; em release o gate é integral);
//! 3. throughput de inferência do congelado `≥` baseline (dentro de 5%).

use aether_engine::core::reservoir::{bench_frozen, bench_joint, narma_like, ReservoirConfig};

#[test]
fn e8_reservatorio_congelado_bate_gates() {
    let samples = if cfg!(debug_assertions) {
        40_000usize
    } else {
        1_000_000usize
    };
    let predict_n = 256usize;
    let series = narma_like(samples + predict_n + 64, 0xA37E_0E83);
    let cfg = ReservoirConfig {
        n: 32,
        spectral_radius: 0.9,
        leak: 0.3,
        input_scale: 0.5,
        seed: 0xA37E_0E84,
    };

    let frozen = bench_frozen(&series, cfg, 32, predict_n).expect("frozen");
    let joint = bench_joint(&series, cfg, 32, predict_n, 1e-4).expect("joint");
    let smin = series.iter().cloned().fold(f32::INFINITY, f32::min);
    let smax = series.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    println!("[E8] serie min={smin:.4} max={smax:.4} len={}", series.len());
    let regression = ((frozen.nrmse - joint.nrmse) / joint.nrmse.max(1e-12)).max(0.0);
    let speedup = joint.train_secs / frozen.train_secs.max(1e-12);

    println!(
        "[E8] samples={samples} frozen_nrmse={:.4} joint_nrmse={:.4} regression={:.4} frozen_s={:.3} joint_s={:.3} speedup={:.2}x",
        frozen.nrmse, joint.nrmse, regression, frozen.train_secs, joint.train_secs, speedup
    );
    println!(
        "[E8] predict/s frozen={:.0} joint={:.0}",
        frozen.predict_per_sec, joint.predict_per_sec
    );

    assert!(
        frozen.nrmse.is_finite() && joint.nrmse.is_finite(),
        "NRMSE nao finito"
    );
    assert!(
        regression < 0.05,
        "regressao {regression:.4} >= 0.05 (frozen={:.4} joint={:.4})",
        frozen.nrmse,
        joint.nrmse
    );
    assert!(
        frozen.train_secs < 60.0,
        "treino congelado {:.3}s >= 60s",
        frozen.train_secs
    );
    assert!(
        frozen.predict_per_sec >= joint.predict_per_sec * 0.95,
        "throughput congelado {:.0} < baseline {:.0}",
        frozen.predict_per_sec,
        joint.predict_per_sec
    );
}

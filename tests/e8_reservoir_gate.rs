//! E8 — gate do reservatório esparso congelado (Vol. II §6/§10).
//!
//! Gates:
//! 1. Mackey–Glass τ=17: NRMSE `< 0,01`, treino de `10^6` amostras `< 60 s`
//!    (reduzido em debug), throughput de inferência congelada acima do piso;
//! 2. NARMA-10: NRMSE `< 0,15` com N=400;
//! 3. ajuste conjunto (40 passadas sem gradiente): o congelado é `≥ 20×` mais
//!    rápido e não regride mais de 5% em acurácia.

use aether_engine::core::reservoir::{
    bench_frozen, bench_frozen_supervised, evaluate, joint_search, mackey_glass, narma10,
    FrozenModel, ReservoirConfig,
};

#[test]
fn e8_mackey_glass_acuracy_e_tempo_de_um_milhao() {
    let train_len = if cfg!(debug_assertions) {
        100_000usize
    } else {
        1_000_000usize
    };
    let test_len = 2_000usize;
    let series = mackey_glass(train_len + test_len + 4_000, 17, 0.1);
    let cfg = ReservoirConfig {
        n: 100,
        degree: 10,
        spectral_radius: 1.0,
        leak: 0.8,
        input_scale: 1.0,
        seed: 0xA37E_0E91,
    };
    let b = bench_frozen(&series, cfg, 0, train_len, test_len, 200, 1e-8).expect("mg");
    println!(
        "[E8] MG samples={train_len} train_s={:.3} nrmse={:.8} pps={:.0}",
        b.train_secs, b.nrmse, b.predict_per_sec
    );
    assert!(b.nrmse.is_finite() && b.nrmse < 0.01, "NRMSE MG {:.6}", b.nrmse);
    assert!(b.train_secs < 60.0, "treino MG {:.3}s >= 60s", b.train_secs);
    let pps_min = if cfg!(debug_assertions) {
        100_000.0
    } else {
        500_000.0
    };
    assert!(
        b.predict_per_sec > pps_min,
        "throughput MG {:.0}/s <= {pps_min:.0}/s",
        b.predict_per_sec
    );
}

#[test]
fn e8_narma10_acuracy() {
    let train_len = 16_000usize;
    let test_len = 2_000usize;
    let (u, y) = narma10(train_len + test_len + 2_000, 0xA37E_0E90);
    let cfg = ReservoirConfig {
        n: 400,
        degree: 20,
        spectral_radius: 0.8,
        leak: 1.0,
        input_scale: 0.4,
        seed: 0xA37E_0E92,
    };
    let b = bench_frozen_supervised(&u, &y, cfg, 0, train_len, test_len, 200, 1e-10)
        .expect("narma");
    println!(
        "[E8] NARMA train_s={:.3} nrmse={:.6} pps={:.0}",
        b.train_secs, b.nrmse, b.predict_per_sec
    );
    assert!(b.nrmse.is_finite() && b.nrmse < 0.15, "NRMSE NARMA {:.6}", b.nrmse);
}

#[test]
fn e8_congelado_vence_ajuste_conjunto_em_tempo() {
    let train_len = 4_000usize;
    let val_len = 1_000usize;
    let test_len = 2_000usize;
    let series = mackey_glass(train_len + val_len + test_len + 2_000, 17, 0.1);
    let cfg = ReservoirConfig {
        n: 100,
        degree: 10,
        spectral_radius: 1.0,
        leak: 0.8,
        input_scale: 1.0,
        seed: 0xA37E_0E91,
    };

    let (mut frozen, frozen_secs) =
        FrozenModel::train(cfg, &series, 0, train_len, 200, 1e-8).expect("frozen");
    let (frozen_test, _) = evaluate(&mut frozen, &series, train_len + val_len, test_len)
        .expect("eval frozen");

    let joint = joint_search(&series, cfg, 0, train_len, val_len, 200, 40, 1e-8)
        .expect("joint search");
    // Comparação no **teste**, não na validação usada para selecionar.
    let (mut joint_model, _) =
        FrozenModel::train(joint.best, &series, 0, train_len, 200, 1e-8).expect("joint best");
    let (joint_test, _) = evaluate(&mut joint_model, &series, train_len + val_len, test_len)
        .expect("eval joint");
    let speedup = joint.search_secs / frozen_secs.max(1e-12);
    println!(
        "[E8] joint passes={} val={:.6} test={:.6} secs={:.3} | frozen test={:.6} secs={:.4} | speedup={:.1}x",
        joint.training_passes,
        joint.val_nrmse,
        joint_test,
        joint.search_secs,
        frozen_test,
        frozen_secs,
        speedup
    );

    assert_eq!(joint.training_passes, 40);
    assert!(speedup >= 20.0, "speedup {speedup:.1}x < 20x");
    assert!(
        frozen_test <= joint_test * 1.05,
        "regressao congelada: frozen={frozen_test:.6} joint={joint_test:.6}"
    );
}

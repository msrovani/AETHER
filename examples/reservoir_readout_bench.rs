//! E8 — benchmark do reservatório congelado contra física plástica.
//!
//! Uso: `cargo run --release --example reservoir_readout_bench`
//!
//! Escreve `target/aether_e8_reservoir.json` com os números medidos.

use aether_engine::core::reservoir::{
    bench_frozen, bench_joint, narma_like, ReservoirConfig,
};

fn main() {
    let samples = if cfg!(debug_assertions) {
        60_000usize
    } else {
        1_000_000usize
    };
    let predict_n = 512usize;
    let series = narma_like(samples + predict_n + 64, 0xA37E_0E81);
    let cfg = ReservoirConfig {
        n: 32,
        spectral_radius: 0.9,
        leak: 0.3,
        input_scale: 0.5,
        seed: 0xA37E_0E82,
    };
    let frozen = bench_frozen(&series, cfg, 32, predict_n).expect("frozen");
    let joint = bench_joint(&series, cfg, 32, predict_n, 1e-4).expect("joint");
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
    let json = format!(
        "{{\"samples\":{samples},\"frozen_nrmse\":{:.6},\"joint_nrmse\":{:.6},\"regression\":{:.6},\"frozen_train_s\":{:.6},\"joint_train_s\":{:.6},\"speedup\":{:.6},\"frozen_predict_per_s\":{:.1},\"joint_predict_per_s\":{:.1}}}",
        frozen.nrmse,
        joint.nrmse,
        regression,
        frozen.train_secs,
        joint.train_secs,
        speedup,
        frozen.predict_per_sec,
        joint.predict_per_sec
    );
    let _ = std::fs::create_dir_all("target");
    if let Err(e) = std::fs::write("target/aether_e8_reservoir.json", json) {
        eprintln!("nao foi possivel escrever o artefato: {e}");
    }
}

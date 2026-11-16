//! E7 — probe do corpo `S0` por `10^5` ciclos com telemetria real.
//!
//! Uso: `cargo run --release --example s0_body_probe`
//! Escreve `target/aether_e7_body.json`.

use aether_engine::core::s0_body::run_s0;

fn main() {
    let cycles = 100_000u64;
    let on = run_s0(cycles, true, 0xA37E_0E76);
    let off = run_s0(cycles, false, 0xA37E_0E77);
    let drop = 1.0 - off.entropy / on.entropy.max(1e-9);
    let spearman = on.spearman_inverted_u();

    println!(
        "[E7] cycles={cycles} entropy_on={:.4} entropy_off={:.4} drop={:.3} spearman={:.4}",
        on.entropy, off.entropy, drop, spearman
    );
    println!(
        "[E7] throttle={} afinidade={} pausa={} alarmes={}",
        on.actuator_counts.0, on.actuator_counts.1, on.actuator_counts.2, on.alarms
    );
    let freq: Vec<String> = on.frequency.iter().map(|v| format!("{:.4}", v)).collect();
    let json = format!(
        "{{\"cycles\":{cycles},\"entropy_on\":{:.6},\"entropy_off\":{:.6},\"drop\":{:.6},\"spearman\":{:.6},\"throttle\":{},\"affinity\":{},\"pause\":{},\"alarms\":{},\"frequency\":[{}]}}",
        on.entropy,
        off.entropy,
        drop,
        spearman,
        on.actuator_counts.0,
        on.actuator_counts.1,
        on.actuator_counts.2,
        on.alarms,
        freq.join(",")
    );
    let _ = std::fs::create_dir_all("target");
    if let Err(e) = std::fs::write("target/aether_e7_body.json", json) {
        eprintln!("nao foi possivel escrever o artefato: {e}");
    }
}

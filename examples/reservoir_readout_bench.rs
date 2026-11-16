//! E8 — benchmark do reservatório esparso congelado contra ajuste da dinâmica.
//!
//! Uso: `cargo run --release --example reservoir_readout_bench`
//! Escreve `target/aether_e8_reservoir.json`.
//!
//! O que é medido:
//! * Mackey–Glass τ=17 (caos com memória longa) — NRMSE, treino e inferência;
//! * NARMA-10 (memória não-linear) — NRMSE;
//! * varredura de `λ` **sem reexecutar** o reservatório (`recalibrate`);
//! * RLS online vs ridge (custo/acurácia);
//! * baseline “conjunto”: 40 passadas de busca sobre `(ρ, α, input_scale)`.

use aether_engine::core::reservoir::{
    bench_frozen, bench_frozen_supervised, evaluate, evaluate_rls, joint_search, mackey_glass,
    narma10, FrozenModel, ReservoirConfig, RlsModel,
};

fn main() {
    let train_len = if cfg!(debug_assertions) {
        100_000usize
    } else {
        1_000_000usize
    };
    let washout = 200usize;
    let test_len = 2_000usize;

    // ------------------------------------------------------------------
    // Mackey-Glass: configuração escolhida por validação (não pelo teste).
    // ------------------------------------------------------------------
    let mg = mackey_glass(train_len + test_len + 4_000, 17, 0.1);
    let mg_cfg = ReservoirConfig {
        n: 100,
        degree: 10,
        spectral_radius: 1.0,
        leak: 0.8,
        input_scale: 1.0,
        seed: 0xA37E_0E91,
    };
    let mg_bench = bench_frozen(&mg, mg_cfg, 0, train_len, test_len, washout, 1e-10)
        .expect("mackey-glass frozen");
    println!(
        "[E8] MG train={}s nrmse={:.6} pps={:.0}",
        mg_bench.train_secs, mg_bench.nrmse, mg_bench.predict_per_sec
    );

    // Varredura de λ no MESMO acumulador (sem reexecutar o reservatório).
    let small = train_len.min(20_000);
    let (mut sweep_model, _) =
        FrozenModel::train(mg_cfg, &mg, 0, small, washout, 1e-10).expect("sweep");
    let mut lambda_rows = String::new();
    for lambda in [1e-10f64, 1e-9, 1e-8, 1e-7, 1e-6, 1e-5] {
        sweep_model.recalibrate(lambda).expect("recalibrate");
        let (nrmse, _) = evaluate(&mut sweep_model, &mg, small, test_len).expect("eval sweep");
        if !lambda_rows.is_empty() {
            lambda_rows.push(',');
        }
        lambda_rows.push_str(&format!("{{\"lambda\":{lambda:.0e},\"nrmse\":{nrmse:.8}}}"));
    }
    println!("[E8] lambda sweep: [{lambda_rows}]");

    // Múltiplas sementes: média ± desvio do NRMSE em 100k amostras.
    let seed_train = train_len.min(100_000);
    let mut mg_seed_errs = Vec::new();
    for seed in [1u64, 2, 3, 4, 5] {
        let cfg = ReservoirConfig { seed, ..mg_cfg };
        let b = bench_frozen(&mg, cfg, 0, seed_train, test_len, washout, 1e-10).expect("seed mg");
        mg_seed_errs.push(b.nrmse);
    }
    let (mg_mean, mg_std) = mean_std(&mg_seed_errs);
    println!("[E8] MG seeds mean={mg_mean:.8} std={mg_std:.8}");

    // Ablação de esparsidade: 10 conexões/nó vs denso (N-1).
    let dense_cfg = ReservoirConfig {
        degree: mg_cfg.n.saturating_sub(1).max(1),
        ..mg_cfg
    };
    let dense =
        bench_frozen(&mg, dense_cfg, 0, seed_train, test_len, washout, 1e-10).expect("dense mg");
    let sparse =
        bench_frozen(&mg, mg_cfg, 0, seed_train, test_len, washout, 1e-10).expect("sparse mg");
    println!(
        "[E8] esparsidade sparse={:.3}s/{:.8} dense={:.3}s/{:.8} speedup={:.1}x",
        sparse.train_secs,
        sparse.nrmse,
        dense.train_secs,
        dense.nrmse,
        dense.train_secs / sparse.train_secs.max(1e-9)
    );

    // RLS online vs ridge no mesmo trecho.
    let (mut rls, rls_secs) =
        RlsModel::train_supervised(mg_cfg, &mg, &mg[1..], 0, small, washout, 0.05, 1.0)
            .expect("rls");
    let (rls_nrmse, _) = evaluate_rls(&mut rls, &mg, &mg[1..], small, test_len).expect("eval rls");
    println!("[E8] RLS train={rls_secs:.3}s nrmse={rls_nrmse:.6}");

    // Baseline conjunto: 40 passadas completas de treino (sem backprop).
    let joint = joint_search(&mg, mg_cfg, 0, small, 1_000, washout, 40, 1e-10).expect("joint");
    let ratio = joint.search_secs / (mg_bench.train_secs * small as f64 / train_len as f64);
    println!(
        "[E8] joint passes={} secs={:.3} val={:.6} speedup_vs_frozen={:.1}x",
        joint.training_passes, joint.search_secs, joint.val_nrmse, ratio
    );

    // ------------------------------------------------------------------
    // NARMA-10 supervisionado: N=400, ρ=0.8, α=1.0, input=0.4, λ=1e-10.
    // ------------------------------------------------------------------
    let narma_train = 16_000usize;
    let (u, y) = narma10(narma_train + test_len + 2_000, 0xA37E_0E90);
    let narma_cfg = ReservoirConfig {
        n: 400,
        degree: 20,
        spectral_radius: 0.8,
        leak: 1.0,
        input_scale: 0.4,
        seed: 0xA37E_0E92,
    };
    let narma_bench =
        bench_frozen_supervised(&u, &y, narma_cfg, 0, narma_train, test_len, washout, 1e-10)
            .expect("narma frozen");
    println!(
        "[E8] NARMA train={}s nrmse={:.6} pps={:.0}",
        narma_bench.train_secs, narma_bench.nrmse, narma_bench.predict_per_sec
    );

    let json = format!(
        "{{\"mg\":{{\"train_samples\":{train_len},\"train_s\":{:.6},\"nrmse\":{:.8},\"predict_per_s\":{:.1},\"n\":{},\"degree\":{},\"rho\":{},\"leak\":{},\"input\":{},\"lambda\":1e-10}},\"mg_seeds\":{{\"mean\":{mg_mean:.8},\"std\":{mg_std:.8}}},\"sparsity\":{{\"sparse_s\":{:.6},\"dense_s\":{:.6},\"speedup\":{:.3},\"sparse_nrmse\":{:.8},\"dense_nrmse\":{:.8}}},\"narma\":{{\"train_samples\":{narma_train},\"train_s\":{:.6},\"nrmse\":{:.8},\"predict_per_s\":{:.1},\"n\":{},\"degree\":{},\"rho\":{},\"leak\":{},\"input\":{},\"lambda\":1e-10}},\"lambda_sweep\":[{}],\"rls\":{{\"train_s\":{rls_secs:.6},\"nrmse\":{rls_nrmse:.8}}},\"joint\":{{\"passes\":{},\"secs\":{:.6},\"val_nrmse\":{:.8},\"speedup\":{ratio:.3}}}}}",
        mg_bench.train_secs,
        mg_bench.nrmse,
        mg_bench.predict_per_sec,
        mg_cfg.n,
        mg_cfg.degree,
        mg_cfg.spectral_radius,
        mg_cfg.leak,
        mg_cfg.input_scale,
        sparse.train_secs,
        dense.train_secs,
        dense.train_secs / sparse.train_secs.max(1e-9),
        sparse.nrmse,
        dense.nrmse,
        narma_bench.train_secs,
        narma_bench.nrmse,
        narma_bench.predict_per_sec,
        narma_cfg.n,
        narma_cfg.degree,
        narma_cfg.spectral_radius,
        narma_cfg.leak,
        narma_cfg.input_scale,
        lambda_rows,
        joint.training_passes,
        joint.search_secs,
        joint.val_nrmse,
    );
    let _ = std::fs::create_dir_all("target");
    if let Err(e) = std::fs::write("target/aether_e8_reservoir.json", json) {
        eprintln!("nao foi possivel escrever o artefato: {e}");
    }
}

fn mean_std(v: &[f32]) -> (f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f32;
    let mean = v.iter().sum::<f32>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n;
    (mean, var.sqrt())
}

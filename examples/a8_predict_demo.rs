//! Demo A8 — o que `predict.series` / metacog / OOD fazem (números no stdout).
#![cfg(feature = "agent")]

use aether_engine::agent::{forecast_series, metacog_should_abstain, NRMSE_OOD};
use aether_engine::core::reservoir::mackey_glass;

fn main() {
    let series = mackey_glass(500, 17, 0.1);
    let rep = forecast_series(&series, 3);
    println!("=== A8 predict.series (Mackey–Glass τ=17, n=500, H=3) ===");
    println!("NRMSE holdout = {:.6}  (limite OOD = {NRMSE_OOD})", rep.nrmse);
    println!("abstain       = {:?}", rep.abstain);
    println!("yhat[H]       = {:?}", rep.yhat);
    println!();
    println!("=== OOD (série curta 3 pts) ===");
    let ood = forecast_series(&[1.0, 2.0, 3.0], 1);
    println!("abstain = {:?}  yhat_len = {}", ood.abstain, ood.yhat.len());
    println!();
    println!("=== metacog self_field ===");
    println!("nrmse=0.001 → abstain={}", metacog_should_abstain(0.001));
    println!("nrmse=0.12  → abstain={}", metacog_should_abstain(0.12));
}

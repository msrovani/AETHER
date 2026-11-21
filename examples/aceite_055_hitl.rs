//! Aceite 0.5.5 — cutover/revert HITL explícito (não auto).
//!
//! Uso:
//!   cargo run --example aceite_055_hitl --features agent -- prepare-sentinel
//!   cargo run --example aceite_055_hitl --features agent -- approve-cutover
//!   cargo run --example aceite_055_hitl --features agent -- revert
//!   cargo run --example aceite_055_hitl --features agent -- ensure-scribe

use aether_engine::agent::{
    apply_smoke_result, begin_upgrade, cutover_promote, ensure_scribe_fields_on_active_lock,
    mark_download_approved, revert_previous, DecSlot, DecUpgradePhase, DecUpgradeState, LampColor,
};
use aether_engine::json::Json;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    match arg.as_str() {
        "prepare-sentinel" => prepare_sentinel(),
        "approve-cutover" => approve_cutover(),
        "revert" => revert(),
        "ensure-scribe" => ensure_scribe(),
        "roundtrip" => roundtrip(),
        "status" => status(),
        _ => {
            eprintln!(
                "uso: aceite_055_hitl <prepare-sentinel|approve-cutover|revert|ensure-scribe|roundtrip|status>"
            );
            std::process::exit(2);
        }
    }
}

fn prepare_sentinel() {
    let mut st = begin_upgrade(DecSlot::Sentinel, "gemma-3-1b-prose").expect("begin");
    if st.phase == DecUpgradePhase::AbortRollback {
        eprintln!("abort: {}", st.reason);
        std::process::exit(3);
    }
    // Simula Approve download (HITL) → Fetch.
    if st.phase == DecUpgradePhase::HitlApproveDownload {
        mark_download_approved(&mut st).expect("download approve");
    }
    // Smoke verde sintético (CI); live fica no Python.
    st.phase = DecUpgradePhase::Smoke;
    st.lamps.fetch = LampColor::Green;
    let smoke = Json::obj([
        ("boot", Json::str("green")),
        ("fit", Json::str("green")),
        ("t1", Json::str("green")),
        ("t2", Json::str("green")),
        ("c1", Json::str("green")),
        ("and", Json::str("green")),
        ("reason", Json::str("aceite_055 dry smoke")),
    ]);
    apply_smoke_result(&mut st, &smoke).expect("smoke");
    // Garante candidate lock para cutover real.
    let _ = std::fs::create_dir_all(".vega/profiles/candidate");
    let lock = Json::obj([
        ("schema", Json::str("engine.lock/1")),
        ("bank", Json::str("candidate")),
        (
            "primary",
            Json::obj([("id", Json::str("vega-u-0.8b"))]),
        ),
        (
            "sentinel",
            Json::obj([("id", Json::str("gemma-3-1b-prose"))]),
        ),
        (
            "engine_label",
            Json::str("vega-u-0.8b AND gemma-3-1b-prose"),
        ),
        ("fingerprint", Json::str("aceite-055-candidate")),
    ]);
    std::fs::write(
        ".vega/profiles/candidate/engine.lock.json",
        lock.to_string(),
    )
    .expect("candidate lock");
    println!(
        "{}",
        Json::obj([
            ("ok", Json::Bool(true)),
            ("phase", Json::str(st.phase.label())),
            (
                "hitl_cutover_id",
                Json::Num(st.hitl_cutover_id.unwrap_or(0) as f64),
            ),
            ("reason", Json::str(&st.reason)),
        ])
    );
}

fn approve_cutover() {
    let mut st = DecUpgradeState::load();
    cutover_promote(&mut st).unwrap_or_else(|e| {
        eprintln!("cutover: {e}");
        std::process::exit(4);
    });
    println!(
        "{}",
        Json::obj([
            ("ok", Json::Bool(true)),
            ("phase", Json::str(st.phase.label())),
            ("reason", Json::str(&st.reason)),
        ])
    );
}

fn revert() {
    let mut st = DecUpgradeState::load();
    revert_previous(&mut st).unwrap_or_else(|e| {
        eprintln!("revert: {e}");
        std::process::exit(5);
    });
    println!(
        "{}",
        Json::obj([
            ("ok", Json::Bool(true)),
            ("phase", Json::str(st.phase.label())),
            ("reason", Json::str(&st.reason)),
        ])
    );
}

fn ensure_scribe() {
    ensure_scribe_fields_on_active_lock().expect("scribe");
    let path = aether_engine::agent::active_scribe_path().unwrap_or_default();
    println!(
        "{}",
        Json::obj([
            ("ok", Json::Bool(true)),
            ("scribe_path", Json::str(&path)),
        ])
    );
}

fn status() {
    let st = DecUpgradeState::load();
    println!(
        "{}",
        Json::obj([
            ("phase", Json::str(st.phase.label())),
            ("bank", Json::str(&st.product_bank)),
            ("fallback", Json::Bool(st.fallback_armed)),
            ("reason", Json::str(&st.reason)),
            (
                "candidate",
                Json::str(st.candidate_id.as_deref().unwrap_or("-"))
            ),
        ])
    );
}

/// HITL explícito num único processo: prepare → cutover → ensure scribe → revert.
fn roundtrip() {
    prepare_sentinel();
    let mut st = DecUpgradeState::load();
    if st.phase != DecUpgradePhase::HitlCutover {
        eprintln!("roundtrip: esperado hitl_cutover após prepare, atual={}", st.phase.label());
        std::process::exit(4);
    }
    cutover_promote(&mut st).unwrap_or_else(|e| {
        eprintln!("cutover: {e}");
        std::process::exit(4);
    });
    ensure_scribe_fields_on_active_lock().expect("scribe post-cutover");
    let path = aether_engine::agent::active_scribe_path().unwrap_or_default();
    if path != "gemma-3-1b-prose" {
        eprintln!("scribe_path inesperado: {path}");
        std::process::exit(6);
    }
    revert_previous(&mut st).unwrap_or_else(|e| {
        eprintln!("revert: {e}");
        std::process::exit(5);
    });
    let _ = ensure_scribe_fields_on_active_lock();
    println!(
        "{}",
        Json::obj([
            ("ok", Json::Bool(true)),
            ("roundtrip", Json::str("prepare→cutover→scribe→revert")),
            ("scribe_path", Json::str(&path)),
            ("final_phase", Json::str(st.phase.label())),
            ("reason", Json::str(&st.reason)),
        ])
    );
}

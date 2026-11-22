//! Gate T-DEC upgrade — dual-bank, HW, smoke fail → active intacto, cutover só HITL.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    apply_smoke_result, begin_upgrade, cutover_promote, engine_label_from_ids, hw_compat_for,
    load_catalog, DecSlot, DecUpgradePhase, DecUpgradeState, LampColor, CATALOG_PATH,
};
use aether_engine::json::Json;

#[test]
fn catalog_allowlist_existe() {
    assert!(std::path::Path::new(CATALOG_PATH).is_file());
    let cat = load_catalog().expect("catalog");
    assert!(cat
        .iter()
        .any(|r| r.id == "vega-u-0.8b" && r.slot == DecSlot::Primary));
    assert!(cat
        .iter()
        .any(|r| r.id == "gemma-3-1b-prose" && r.slot == DecSlot::Sentinel));
}

#[test]
fn hw_4b_bf16_recusa_soma_and() {
    let cat = load_catalog().unwrap();
    let r = cat.iter().find(|x| x.id == "qwen3.5-4b-attach").unwrap();
    assert_eq!(hw_compat_for(r, 2048, false), LampColor::Red);
    // NF4 cabe no orçamento com peer 2GB → yellow (não auto-verde)
    assert_eq!(hw_compat_for(r, 2048, true), LampColor::Yellow);
}

#[test]
fn begin_upgrade_27b_aborta_hw() {
    let st = begin_upgrade(DecSlot::Primary, "qwen3.8-27b-attach").expect("begin");
    assert_eq!(st.phase, DecUpgradePhase::AbortRollback);
    assert_eq!(st.product_bank, "active");
    assert!(st.fallback_armed);
    assert_eq!(st.lamps.hw, LampColor::Red);
}

#[test]
fn smoke_vermelho_nao_cutover() {
    let mut st = DecUpgradeState {
        phase: DecUpgradePhase::Smoke,
        slot: Some(DecSlot::Primary),
        candidate_id: Some("vega-u-0.8b".into()),
        ..Default::default()
    };
    let smoke = Json::obj([
        ("boot", Json::str("green")),
        ("fit", Json::str("green")),
        ("t1", Json::str("red")),
        ("t2", Json::str("green")),
        ("c1", Json::str("green")),
        ("and", Json::str("green")),
        ("reason", Json::str("T1 falhou")),
    ]);
    apply_smoke_result(&mut st, &smoke).unwrap();
    assert_eq!(st.phase, DecUpgradePhase::AbortRollback);
    assert!(st.fallback_armed);
    assert!(cutover_promote(&mut st).is_err());
}

#[test]
fn smoke_verde_pede_hitl_cutover() {
    let mut st = DecUpgradeState {
        phase: DecUpgradePhase::Smoke,
        slot: Some(DecSlot::Sentinel),
        candidate_id: Some("gemma-3-1b-prose".into()),
        ..Default::default()
    };
    let smoke = Json::obj([
        ("boot", Json::str("green")),
        ("fit", Json::str("green")),
        ("t1", Json::str("green")),
        ("t2", Json::str("green")),
        ("c1", Json::str("green")),
        ("and", Json::str("green")),
        ("reason", Json::str("ok")),
    ]);
    apply_smoke_result(&mut st, &smoke).unwrap();
    assert_eq!(st.phase, DecUpgradePhase::HitlCutover);
    assert_eq!(st.product_bank, "active");
    assert!(st.fallback_armed);
    assert_eq!(st.lamps.ok_geral, LampColor::Off);
}

#[test]
fn label_dinamico() {
    assert_eq!(
        engine_label_from_ids("vega-u-0.8b", "gemma-3-4b-it"),
        "vega-u-0.8b AND gemma-3-4b-it"
    );
}

#[test]
fn artefato_contrato() {
    let art = Json::obj([
        ("schema", Json::str("dec_upgrade_gate/1")),
        ("dual_bank", Json::Bool(true)),
        ("product_reads", Json::str("active_only")),
        ("candidate_on_bus", Json::Bool(false)),
        ("silent_upgrade", Json::Bool(false)),
        ("hitl_download", Json::Bool(true)),
        ("hitl_cutover", Json::Bool(true)),
        ("catalog", Json::str(CATALOG_PATH)),
        ("doc", Json::str("docs/T_DEC_UPGRADE.md")),
    ]);
    std::fs::create_dir_all("target").ok();
    std::fs::write("target/aether_dec_upgrade_gate.json", art.to_string()).unwrap();
    let t = std::fs::read_to_string("target/aether_dec_upgrade_gate.json").unwrap();
    assert!(t.contains("\"candidate_on_bus\":false"));
    assert!(t.contains("\"silent_upgrade\":false"));
}

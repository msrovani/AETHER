//! Fase 8 — Hier MoE/LSH fora do default; não bloqueia T-NL-IN.

#![cfg(feature = "agent")]

use aether_engine::agent::{lsh_route_refuse, HierMoeStatus};

#[test]
fn hier_moe_posicao_fila() {
    let s = HierMoeStatus::from_env(false);
    assert!(!s.may_enable());
    assert!(!s.default_path);
    let j = s.to_json().to_string();
    assert!(j.contains("\"blocks_tnl_in\":false"));
    assert!(lsh_route_refuse("k").is_err());
    let path = aether_engine::agent::HIER_MOE_STATUS_PATH;
    std::fs::create_dir_all("target").ok();
    std::fs::write(path, j).unwrap();
}

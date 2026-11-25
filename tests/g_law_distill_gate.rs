//! Gate SB-4 (L7) — Sonho SGDB -> SINDy -> Law (G-LAW):
//! 1. SINDy fits a harmonic series (`x' = v, v' = -omega^2 x`) com residual
//!    pequeno (< 0.25 — fit válido);
//! 2. LawRecord encodado com k/n_samples/residual finitos;
//! 3. `SgdbLedger::stage_law` + `commit_run` + `recall_scope` devolve o law
//!    (authority <= 120).

#![cfg(feature = "m1")]

use aether_engine::core::koopman_sindy::{SindyEquationDiscoverer, SINDY_WINDOW};
use aether_engine::knowledge::sgdb::ledger::{LawRecord, ORGANISM_LAW_AUTH, SgdbLedger};
use aether_engine::knowledge::sgdb::{MockTransport, SgdbConfig};
use aether_engine::core::knowledge_bridge_f1::ScopeId;

#[test]
fn gate_s4_sgdb_sindy_law() {
    const OMEGA2: f32 = 0.25;
    let scope = ScopeId::new(1, 0, 3);
    let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);

    // Série harmônica: x' = v, v' = -OMEGA2 * x (2 estados: [x, v]).
    let mut sindy = SindyEquationDiscoverer::streaming(1.0, 0.02).expect("sindy");
    let mut x = 0.7f32;
    let mut v = 0.0f32;
    for _ in 0..(SINDY_WINDOW + 16) {
        sindy.push([x, v]);
        let (nx, nv) = (x + v, v - OMEGA2 * x);
        x = nx.clamp(-1.0, 1.0);
        v = nv;
    }
    let residual = sindy.fit_window().expect("fit");
    assert!(residual < 0.25, "fit válido esperado: residual={residual}");

    let law = LawRecord {
        k: 8,
        n_samples: (SINDY_WINDOW + 16) as u32,
        dt: 1.0,
        residual: residual as f32,
        authority: ORGANISM_LAW_AUTH,
        scope,
    };
    assert!(law.residual.is_finite() && law.k > 0 && law.n_samples > 0);

    ledger.stage_law(law);
    assert_eq!(ledger.commit_run(), 1, "lei estagiada");

    let hits = ledger
        .client_mut()
        .recall_scope(scope, 16)
        .expect("recall no scope");
    assert!(
        hits.iter().any(|h| h.authority <= ORGANISM_LAW_AUTH),
        "lei not found in scope recall (auth cap)"
    );
}

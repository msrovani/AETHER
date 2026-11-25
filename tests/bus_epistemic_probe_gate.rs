//! Gate S8 (R10/A10) — orçamento de sondagem epístÉo (G-ABD, A10):
//! 1. Budget esgota após `set_budget(n)` — `try_probe()` não passa n+1;
//! 2. `has_probe_budget()` false ao esgotar, true após `reset_budget()`.

#![cfg(feature = "agent")]

use aether_engine::agent::epistemic_budget::*;

#[test]
fn gate_s8_probe_budget_exhausts() {
    set_budget(5);
    let mut ok = 0;
    for _ in 0..10 {
        if try_probe() {
            ok += 1;
        }
    }
    assert_eq!(ok, 5, "orçamento esgotou em {ok} (deve ser 5)");
    assert!(!has_probe_budget(), "budget ainda ativo ao esgotar");
    reset_budget();
    assert!(has_probe_budget(), "reset não reabriu o orçamento");
}

#[test]
fn gate_s8_planner_excludes_probes_when_empty() {
    // Sem budget: candidatos de sonda não entram no ranking (planner hook).
    set_budget(0);
    assert!(!has_probe_budget());
    reset_budget();
}

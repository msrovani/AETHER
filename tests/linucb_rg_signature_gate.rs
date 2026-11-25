//! Gate SB-3 (L6, Sprint A) — `ContextualTools` carrega assinatura macro do RG:
//! 1. `set_rg_macro_signature` aceita 4 palavras `u16`;
//! 2. assinatura acessível via `rg_macro_signature` (determinística);
//! 3. scores do bandit não alteram com a assinatura (wiring isolado;
//!    consumo por bonus é Sprint B).

#![cfg(feature = "agent")]

use aether_engine::agent::linucb::ContextualTools;

#[test]
fn gate_s3b_contextual_tools_carries_rg_signature() {
    let mut ct = ContextualTools::new(vec!["tool_a", "tool_b"], 0.5);
    assert!(ct.rg_macro_signature().is_none(), "sem assinatura inicial");

    let sig = [0x14fd, 0xfd20, 0x2026, 0x2614];
    ct.set_rg_macro_signature(&sig);
    assert_eq!(ct.rg_macro_signature(), Some(&sig));

    let before = ct.rg_macro_signature().copied();
    ct.set_rg_macro_signature(&sig);
    assert_eq!(
        ct.rg_macro_signature().copied(),
        before,
        "assinatura idempotente"
    );
}

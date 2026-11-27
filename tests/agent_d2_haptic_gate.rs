//! Gate **D2** — T-HAPTIC render tátil; banda verde/amarela/vermelha; só pós-triagem;
//! no-op honesto; `AETHER_HAPTIC_CMD`. NOTA: teste único (muta env).

#![cfg(feature = "agent")]

use aether_engine::agent::haptic::{band_from_action, HapticBand, HapticPort};
use aether_engine::agent::Action;

#[test]
fn d2_haptic_banda_verde_amarela_vermelha() {
    let p = HapticPort::off();
    // banda triagem
    assert_eq!(band_from_action(Action::Act), HapticBand::Green);
    assert_eq!(band_from_action(Action::Cite), HapticBand::Green);
    assert_eq!(band_from_action(Action::Propose), HapticBand::Yellow);
    assert_eq!(band_from_action(Action::Abstain), HapticBand::Red);
    // render tipado + padrões
    let g = p.render(Action::Cite);
    assert_eq!(g.band, HapticBand::Green);
    assert_eq!(g.pattern, "tap-once");
    assert_eq!(p.render(Action::Propose).pattern, "tap-tap");
    assert_eq!(p.render(Action::Abstain).pattern, "long-buzz");
    // off → no-op honesto Ok; backend ausente → no-op
    assert!(p.emit(&g).is_ok());

    std::env::remove_var("AETHER_HAPTIC_CMD");
    let on = HapticPort { enabled: true };
    assert!(on.emit(&g).is_ok(), "backend ausente → no-op honesto");
    // comando inexistente + ligado → Err, não panic
    std::env::set_var("AETHER_HAPTIC_CMD", "definitely-not-a-real-binary-xyz");
    assert!(on.emit(&g).is_err());
    std::env::remove_var("AETHER_HAPTIC_CMD");
}

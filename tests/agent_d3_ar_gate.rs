//! Gate **D3** — T-AR overlay; deps duras C3 + T-COH; só âncora pós-evidência; auth≤120;
//! `AETHER_AR_CMD`; no-op honesto. NOTA: teste único (muta env).

#![cfg(feature = "agent")]

use aether_engine::agent::ar::{ArAnchor, ArPort};

fn anchor() -> ArAnchor {
    ArAnchor {
        id: "a1".into(),
        label: "brasil".into(),
        pos: [0.0, 0.0, 0.0],
        auth: 120,
    }
}

#[test]
fn d3_ar_overlay_pos_evidencia_deps_c3_tcoh() {
    std::env::remove_var("AETHER_AR_CMD");
    // off → Err
    assert!(ArPort::off().overlay(&anchor()).is_err());

    let mut p = ArPort {
        enabled: true,
        vis_ready: true,
        coh_ready: true,
    };
    // âncora inválida → reject; auth>120 → reject; sem evidência label vazio → reject
    let mut bad = anchor();
    bad.label = "".into();
    assert!(p.overlay(&bad).is_err());
    let mut badauth = anchor();
    badauth.auth = 255;
    assert!(p.overlay(&badauth).is_err());
    let mut nan = anchor();
    nan.pos = [f32::NAN, 0.0, 0.0];
    assert!(p.overlay(&nan).is_err());

    // deps duras C3 + T-COH
    let no_c3 = ArPort {
        enabled: true,
        vis_ready: false,
        coh_ready: true,
    };
    assert!(no_c3.overlay(&anchor()).is_err());
    let no_coh = ArPort {
        enabled: true,
        vis_ready: true,
        coh_ready: false,
    };
    assert!(no_coh.overlay(&anchor()).is_err());

    // backend ausente → no-op honesto; âncora validada
    assert!(p.overlay(&anchor()).is_ok());
    // comando inexistente + ligado → Err, não panic
    std::env::set_var("AETHER_AR_CMD", "definitely-not-a-real-binary-xyz");
    assert!(p.overlay(&anchor()).is_err());
    std::env::remove_var("AETHER_AR_CMD");
    let _ = &mut p;
}

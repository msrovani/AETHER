//! Gate **C3** — T-VIS backend real via sidecar externo `AETHER_VIS_CMD`.
//! `VisionObs` tipado (labels, não pixels); conf não inventada; `Err` honesto sem backend;
//! assinatura RG 64→16→4 reusa A6; auth≤120; não libera `act` sozinha.
//! Contrato: docs/TOMADAS_FUTURO §C3. NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::vision::VisionPort;

#[test]
fn c3_vis_sidecar_honesto_e_assinatura() {
    let port = VisionPort { enabled: true };
    let img = "img.png"; // image-ref; pixels não cruzam o T0

    std::env::remove_var("AETHER_VIS_CMD");
    std::env::remove_var("AETHER_VIS_CONF");
    assert!(port.observe_sidecar(img, None).is_err());
    std::env::set_var("AETHER_VIS_CMD", "definitely-not-a-real-binary-xyz");
    assert!(port.observe_sidecar(img, None).is_err());
    std::env::remove_var("AETHER_VIS_CMD");

    #[cfg(windows)]
    {
        // sidecar stub imprime labels tipadas `nome conf`
        std::env::set_var("AETHER_VIS_CMD", "cmd /c echo brasil 0.9 & rem");
        let obs = port.observe_sidecar(img, None).expect("stub backend");
        assert!(obs.labels.iter().any(|(n, _)| n == "brasil"), "label tipada");
        assert!(obs.entities.iter().any(|e| e == "brasil"));
        assert!(obs.auth_cap <= 120, "auth≤120");
        // C3: assinatura RG 64→16→4
        let sig = port.signature(img, &obs, 0);
        assert_eq!(sig.len(), 4);
        // determinística
        assert_eq!(sig, port.signature(img, &obs, 0));
    }

    std::env::remove_var("AETHER_VIS_CMD");
    std::env::remove_var("AETHER_VIS_CONF");
}

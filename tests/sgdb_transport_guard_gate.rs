//! Gate **T2** — guarda de spawn stdio do SGDB (sem shell, sem `-Command`).
//!
//! O `AETHER_SGDB_STDIO` é config confiável, mas a invariante "sem shell"
//! (W1/W8) vale também aqui: shells duros são rejeitados e `powershell` só
//! aceita o shape do launcher documentado (`-NoProfile -File <caminho.ps1>`).
//! Este gate valida **apenas a rejeição** (não spawna nada).

#![cfg(feature = "sgdb")]

use aether_engine::knowledge::sgdb::client::open_transport;

#[test]
fn t2_stdio_rejeita_shells_duros() {
    let cfg = aether_engine::knowledge::sgdb::SgdbConfig::default();
    for spec in [
        "stdio:sh -c echo hi",
        "stdio:bash -c 'echo hi'",
        "stdio:cmd /c echo hi",
        "stdio:cmd.exe /c dir",
        "stdio:pwsh -NoProfile -Command dir",
        "stdio:zsh",
    ] {
        let err = open_transport(spec, &cfg).expect_err(spec);
        assert!(
            err.to_string().contains("shell não permitido"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn t2_stdio_rejeita_powershell_fora_do_shape() {
    let cfg = aether_engine::knowledge::sgdb::SgdbConfig::default();
    for spec in [
        "stdio:powershell -Command dir",
        "stdio:powershell -NoProfile -Command Get-Date",
        "stdio:powershell -EncodedCommand AAAA",
        "stdio:powershell -NoProfile -ExecutionPolicy Bypass -File x.ps1",
        "stdio:powershell.exe -File x.ps1",
    ] {
        let err = open_transport(spec, &cfg).expect_err(spec);
        assert!(
            err.to_string().contains("só aceita") || err.to_string().contains("não permitido"),
            "{spec}: {err}"
        );
    }
}

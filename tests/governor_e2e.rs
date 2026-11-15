//! Integração ponta a ponta do `aether-governor` (Fase 6).
//!
//! Executa o binário real em modo headless por 1 s e valida o artefato que o
//! consumo externo vê: `aether_state.bin` com exatamente 32 bytes, CRC-32 IEEE
//! válido e campos coerentes. Também prova que corrupção de 1 byte é rejeitada.

#![cfg(all(feature = "m1", feature = "m2", feature = "m3", feature = "m5"))]

use std::path::PathBuf;
use std::process::Command;

use aether_engine::platform::state_bridge::{
    crc32_ieee, decode_state, read_state_file, STATE_BYTES, STATE_CRC_SPAN,
};

fn unique_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aether_state_e2e_{}_{}_{}.bin",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

#[test]
fn headless_1s_publica_frame_32b_com_crc_valido() {
    let path = unique_path("headless");
    let _ = std::fs::remove_file(&path);

    let output = Command::new(env!("CARGO_BIN_EXE_aether-governor"))
        .args([
            "--headless",
            "--duration-secs",
            "1",
            "--no-lan",
            "--quiet",
            "--state-path",
        ])
        .arg(&path)
        .output()
        .expect("executar aether-governor");

    assert!(
        output.status.success(),
        "exit={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr deveria estar limpo: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let raw = std::fs::read(&path).expect("frame binario gravado");
    assert_eq!(raw.len(), STATE_BYTES, "frame deve ter 32 bytes");

    // CRC-32 IEEE confere no span de 28 bytes.
    let stored = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
    assert_eq!(
        crc32_ieee(&raw[..STATE_CRC_SPAN]),
        stored,
        "CRC-32 do frame invalido"
    );

    let state = decode_state(&raw).expect("frame decodifica");
    assert_eq!(state, read_state_file(&path).expect("read_state_file"));
    assert!(
        (0.0..=1.0).contains(&state.coherence_r),
        "coerencia fora de faixa: {}",
        state.coherence_r
    );
    assert!(
        (0.0..=1.0).contains(&state.identity_confidence),
        "identidade fora de faixa: {}",
        state.identity_confidence
    );
    assert!(
        state.free_energy.is_finite(),
        "energia livre nao finita: {}",
        state.free_energy
    );
    assert!(
        state.flow_regime <= 3,
        "regime invalido: {}",
        state.flow_regime
    );
    assert!(
        state.niche_role <= 2,
        "papel invalido: {}",
        state.niche_role
    );
    assert!(state.cycle_us > 0, "latencia do ciclo deve ser > 0");

    // Corrupcao de 1 byte tem de ser rejeitada (checksum de verdade).
    let mut bad = raw.clone();
    bad[5] ^= 0x40;
    assert!(decode_state(&bad).is_none(), "CRC aceitou frame corrompido");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn headless_com_lan_nao_quebra_e_reporta_peers() {
    let path = unique_path("lan");
    let _ = std::fs::remove_file(&path);

    let output = Command::new(env!("CARGO_BIN_EXE_aether-governor"))
        .args([
            "--headless",
            "--duration-secs",
            "1",
            "--quiet",
            "--node-id",
            "4242",
            "--state-path",
        ])
        .arg(&path)
        .output()
        .expect("executar aether-governor");

    assert!(
        output.status.success(),
        "exit={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let state = read_state_file(&path).expect("frame com LAN");
    // Sem peers reais na sala de CI, `lan_peers` pode ser 0; o contrato e que o
    // valor seja coerente e o processo nao falhe quando o multicast esta ativo.
    assert!(state.lan_peers < 1024);
    assert!(state.cycle_us > 0);
    let _ = std::fs::remove_file(&path);
}

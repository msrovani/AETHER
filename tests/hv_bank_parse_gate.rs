//! Gate **R5** — parse de `HvBank` com aritmética verificada.
//!
//! Antes: header hostil (`count`/`words` gigantes de um `bank.bin` truncado)
//! wrapava em release (`overflow-checks=false`) e o `Vec::with_capacity`
//! podia ir a OOM/abort com sintoma ininteligível. Contrato: header fora dos
//! caps ou com overflow → `Err` de string, **nunca panic, nunca alocação
//! gigante**.

#![cfg(feature = "agent")]

use std::path::PathBuf;

use aether_engine::core::hv::HvBank;

const HEADER_SIZE: usize = 128;

fn header_bin(count: u64, words: u32) -> Vec<u8> {
    let mut raw = vec![0u8; HEADER_SIZE];
    raw[0..8].copy_from_slice(b"AETHV1\0\0");
    // version=1
    raw[8..12].copy_from_slice(&1u32.to_le_bytes());
    // dim_bits, words, dim_in
    raw[12..16].copy_from_slice(&1024u32.to_le_bytes());
    raw[16..20].copy_from_slice(&words.to_le_bytes());
    raw[20..24].copy_from_slice(&64u32.to_le_bytes());
    raw[28..36].copy_from_slice(&count.to_le_bytes());
    raw[36..44].copy_from_slice(&42u64.to_le_bytes());
    raw
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("target/r5_hv_gate_{name}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn r5_count_gigante_e_err_sem_alloc_nem_panic() {
    let dir = tmp_dir("gigante");
    let bank = dir.join("bank.bin");
    let idx = dir.join("idx.bin");
    std::fs::write(&bank, header_bin(u64::MAX / 8, 8)).unwrap();
    std::fs::write(&idx, b"").unwrap();
    let err = HvBank::load(&bank, &idx).expect_err("header hostil é Err");
    assert!(
        err.contains("caps") || err.contains("mismatch") || err.contains("overflow"),
        "{err}"
    );
}

#[test]
fn r5_payload_size_mismatch_e_err() {
    let dir = tmp_dir("mismatch");
    let bank = dir.join("bank.bin");
    let idx = dir.join("idx.bin");
    // count=10, words=4 → payload esperado 320 B; escrevemos 64 B.
    let mut raw = header_bin(10, 4);
    raw.extend(std::iter::repeat(0u8).take(64));
    std::fs::write(&bank, raw).unwrap();
    std::fs::write(&idx, vec![0u8; 240]).unwrap();
    let err = HvBank::load(&bank, &idx).expect_err("payload curto é Err");
    assert!(err.contains("payload"), "{err}");
}

#[test]
fn r5_banco_valido_carrega() {
    let dir = tmp_dir("ok");
    let bank = dir.join("bank.bin");
    let idx = dir.join("idx.bin");
    let count = 3u64;
    let words = 4u32;
    let mut raw = header_bin(count, words);
    // 3 registros × 4 words × 8 B = 96 B de bits.
    raw.extend(std::iter::repeat(0xABu8).take((count * words as u64 * 8) as usize));
    std::fs::write(&bank, &raw).unwrap();
    // idx: 24 B por registro (lo+hi u128 key).
    let mut idx_bytes = Vec::new();
    for k in 0..count {
        idx_bytes.extend_from_slice(&k.to_le_bytes());
        idx_bytes.extend_from_slice(&0u64.to_le_bytes());
        idx_bytes.extend_from_slice(&k.to_le_bytes());
    }
    std::fs::write(&idx, idx_bytes).unwrap();
    let loaded = HvBank::load(&bank, &idx).expect("banco válido carrega");
    assert_eq!(loaded.count(), count as usize);
    assert_eq!(loaded.words, words as usize);
}

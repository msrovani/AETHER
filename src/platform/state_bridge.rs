//! Atuador atômico de estado: frame binário de 32 bytes com CRC-32 IEEE.
//!
//! O governador grava `aether_state.bin` a cada 250 ms usando um buffer
//! pré-alocado e `rename` atômico (arquivo temporário no mesmo diretório).
//! Qualquer processo local (PowerShell, AutoHotkey, script) pode ler o frame
//! sem risco de ver escrita parcial.
//!
//! # Layout (little-endian)
//!
//! | Offset | Bytes | Campo | Tipo |
//! |---:|---:|---|---|
//! | 0 | 4 | `coherence_r` | f32 |
//! | 4 | 4 | `identity_confidence` | f32 |
//! | 8 | 4 | `free_energy` | f32 |
//! | 12 | 1 | `flow_regime` | u8 (0..=3) |
//! | 13 | 1 | `niche_role` | u8 (0..=2) |
//! | 14 | 2 | `lan_peers` | u16 |
//! | 16 | 4 | `cycle_us` | u32 |
//! | 20 | 8 | reservado (zeros) | — |
//! | 28 | 4 | `crc32` de `[0..28]` | u32 |
//!
//! Total: **32 bytes**.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{AetherError, Result};

/// Tamanho fixo do frame publicado.
pub const STATE_BYTES: usize = 32;
/// Bytes cobertos pelo CRC.
pub const STATE_CRC_SPAN: usize = 28;
/// Intervalo de publicação.
pub const STATE_INTERVAL: Duration = Duration::from_millis(250);

/// CRC-32 IEEE (polinômio refletido `0xEDB8_8320`), sem tabela e sem alocação.
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Entregáveis reais expostos ao consumo externo.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GovernorState {
    /// Sincronia Kuramoto local + LAN em `[0, 1]`.
    pub coherence_r: f32,
    /// Similaridade da assinatura de cadência com a memória BAM.
    pub identity_confidence: f32,
    /// Surpresa variacional atual (energia livre esperada).
    pub free_energy: f32,
    /// `0 = Basal`, `1 = DeepFocus`, `2 = Vigilante`, `3 = Critico`.
    pub flow_regime: u8,
    /// `0 = Sentinel`, `1 = Architect`, `2 = Regulator`.
    pub niche_role: u8,
    /// Nós ativos descobertos por multicast na LAN.
    pub lan_peers: u16,
    /// Latência real do ciclo em microssegundos.
    pub cycle_us: u32,
}

impl GovernorState {
    /// Nome legível do regime de fluxo.
    pub const fn flow_label(&self) -> &'static str {
        match self.flow_regime {
            0 => "Basal",
            1 => "DeepFocus",
            2 => "Vigilante",
            _ => "Critico",
        }
    }

    /// Nome legível do papel ecológico.
    pub const fn role_label(&self) -> &'static str {
        match self.niche_role {
            0 => "Sentinel",
            1 => "Architect",
            _ => "Regulator",
        }
    }

    /// Serializa o frame de 32 bytes (CRC nos últimos 4).
    pub fn encode(&self) -> [u8; STATE_BYTES] {
        let mut buf = [0u8; STATE_BYTES];
        buf[0..4].copy_from_slice(&self.coherence_r.to_le_bytes());
        buf[4..8].copy_from_slice(&self.identity_confidence.to_le_bytes());
        buf[8..12].copy_from_slice(&self.free_energy.to_le_bytes());
        buf[12] = self.flow_regime;
        buf[13] = self.niche_role;
        buf[14..16].copy_from_slice(&self.lan_peers.to_le_bytes());
        buf[16..20].copy_from_slice(&self.cycle_us.to_le_bytes());
        // buf[20..28] permanece zerado (reservado).
        let crc = crc32_ieee(&buf[..STATE_CRC_SPAN]);
        buf[STATE_CRC_SPAN..STATE_BYTES].copy_from_slice(&crc.to_le_bytes());
        buf
    }
}

/// Decodifica e valida o CRC de um frame de 32 bytes.
pub fn decode_state(buf: &[u8]) -> Option<GovernorState> {
    if buf.len() != STATE_BYTES {
        return None;
    }
    let want = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
    if crc32_ieee(&buf[..STATE_CRC_SPAN]) != want {
        return None;
    }
    let f32_at = |o: usize| f32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
    let st = GovernorState {
        coherence_r: f32_at(0),
        identity_confidence: f32_at(4),
        free_energy: f32_at(8),
        flow_regime: buf[12],
        niche_role: buf[13],
        lan_peers: u16::from_le_bytes([buf[14], buf[15]]),
        cycle_us: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
    };
    let finite = st.coherence_r.is_finite()
        && st.identity_confidence.is_finite()
        && st.free_energy.is_finite();
    finite.then_some(st)
}

/// Caminho canônico do frame (`%TEMP%/aether_state.bin`).
pub fn default_state_path() -> PathBuf {
    std::env::temp_dir().join("aether_state.bin")
}

/// Lê e valida o frame de um caminho.
pub fn read_state_file(path: &Path) -> Result<GovernorState> {
    let raw = fs::read(path).map_err(AetherError::Io)?;
    decode_state(&raw).ok_or_else(|| {
        AetherError::Config(format!(
            "state bridge: {} nao tem {} bytes ou CRC-32 invalido",
            path.display(),
            STATE_BYTES
        ))
    })
}

/// Ponte de escrita atômica com buffer pré-alocado.
#[derive(Debug)]
pub struct StateBridge {
    dest: PathBuf,
    tmp: PathBuf,
    buf: [u8; STATE_BYTES],
    last_write: Option<Instant>,
    interval: Duration,
    writes: u64,
}

impl StateBridge {
    /// Cria a ponte para `dest` (ou o caminho canônico).
    pub fn new(dest: Option<PathBuf>) -> Result<Self> {
        let dest = dest.unwrap_or_else(default_state_path);
        let file = dest
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AetherError::Config("state bridge: caminho sem nome".into()))?;
        let tmp = dest.with_file_name(format!("{file}.tmp"));
        Ok(Self {
            dest,
            tmp,
            buf: [0u8; STATE_BYTES],
            last_write: None,
            interval: STATE_INTERVAL,
            writes: 0,
        })
    }

    /// Caminho de destino.
    pub fn path(&self) -> &Path {
        &self.dest
    }

    /// Quantas escritas atômicas já foram feitas.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// Ajusta o intervalo de publicação (padrão 250 ms).
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// Grava se o intervalo decorreu. `Ok(true)` = arquivo publicado.
    pub fn maybe_write(&mut self, state: &GovernorState) -> Result<bool> {
        let due = match self.last_write {
            None => true,
            Some(t) => t.elapsed() >= self.interval,
        };
        if !due {
            return Ok(false);
        }
        self.write_now(state)?;
        Ok(true)
    }

    /// Grava imediatamente (buffer + `rename` atômico).
    pub fn write_now(&mut self, state: &GovernorState) -> Result<()> {
        self.buf = state.encode();
        fs::write(&self.tmp, self.buf).map_err(AetherError::Io)?;
        // `rename` substitui o destino de forma atômica (MoveFileEx no Windows).
        fs::rename(&self.tmp, &self.dest).map_err(AetherError::Io)?;
        self.last_write = Some(Instant::now());
        self.writes += 1;
        Ok(())
    }

    /// Remove um temporário órfão de execução anterior.
    pub fn clean_stale_tmp(&self) {
        let _ = fs::remove_file(&self.tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GovernorState {
        GovernorState {
            coherence_r: 0.87,
            identity_confidence: 0.93,
            free_energy: 0.041,
            flow_regime: 1,
            niche_role: 2,
            lan_peers: 3,
            cycle_us: 412,
        }
    }

    #[test]
    fn crc32_ieee_vetor_conhecido() {
        // "123456789" ⇒ 0xCBF43926 (vetor canônico do CRC-32/ISO-HDLC).
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn frame_32b_roundtrip_e_deteccao_de_corrupcao() {
        let st = sample();
        let buf = st.encode();
        assert_eq!(buf.len(), STATE_BYTES);
        assert_eq!(decode_state(&buf), Some(st));
        assert_eq!(&buf[20..28], &[0u8; 8], "reservado deve ficar zerado");

        let mut bad = buf;
        bad[3] ^= 0x40;
        assert_eq!(decode_state(&bad), None, "CRC deve rejeitar corrupcao");
        assert_eq!(decode_state(&buf[..31]), None, "tamanho errado");
    }

    #[test]
    fn ponte_grava_atomicamente_e_respeita_intervalo() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("aether_state_unit_{}.bin", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut bridge = StateBridge::new(Some(path.clone())).expect("bridge");
        bridge.set_interval(Duration::from_millis(10));
        assert!(bridge.maybe_write(&sample()).expect("write"));
        assert_eq!(read_state_file(&path).expect("read"), sample());
        // Imediatamente depois, o intervalo ainda não venceu.
        assert!(!bridge.maybe_write(&sample()).expect("skip"));
        std::thread::sleep(Duration::from_millis(15));
        assert!(bridge.maybe_write(&sample()).expect("write 2"));
        assert_eq!(bridge.writes(), 2);
        let _ = fs::remove_file(&path);
    }
}

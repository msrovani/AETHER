//! Especiação de enxame: nicho ecológico + frames holográficos de 72 bytes.
//!
//! * [`SwarmSpeciationEngine`] — cada nó carrega um *genoma* contínuo
//!   (`GENOME_DIM` floats). O valor de nicho premia a divergência:
//!   `Φ_niche = Φ_auto + α_div · min_{j≠i} ‖g_i − g_j‖₂`.
//!   Os papéis [`NicheRole`] (`Sentinel`/`Architect`/`Regulator`) saem de
//!   ranking dinâmico (raio espectral, plasticidade), desempatado por `Φ_niche`.
//! * [`HolographicFrame`] — quadro binário **fixo de 72 bytes** com CRC-32 IEEE
//!   (tabela `const`, sem alocação) para a rede P2P; **nenhum texto** no fio.
//!
//! Layout do quadro (little-endian):
//!
//! | bytes | campo |
//! |---|---|
//! | 0..2 | magic `0xA37E` |
//! | 2 | versão |
//! | 3 | papel (`NicheRole as u8`) |
//! | 4..8 | `node_id` |
//! | 8..12 | `seq` |
//! | 12..20 | `t_ms` |
//! | 20..24 | `Φ_auto` |
//! | 24..28 | `Φ_niche` |
//! | 28..32 | coerência |
//! | 32..64 | genoma (8 × f32) |
//! | 64..68 | reservado (zero) |
//! | 68..72 | CRC-32 IEEE de `[0..68]` |
//!
//! Zero alocação em `step`, `encode_into` e `decode` (buffers pré-alocados em
//! `new`; ordenação in-place com `sort_unstable_by`).

use crate::error::{AetherError, Result};

/// Dimensão do genoma de nicho.
pub const GENOME_DIM: usize = 8;
/// Tamanho do frame holográfico.
pub const FRAME_BYTES: usize = 72;
/// Magic do frame.
pub const FRAME_MAGIC: u16 = 0xA37E;
/// Versão do formato.
pub const FRAME_VERSION: u8 = 1;

const CRC_OFFSET: usize = FRAME_BYTES - 4;

/// Papel ecológico no nicho.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NicheRole {
    /// Vigilância: maior raio espectral / excitabilidade.
    Sentinel = 0,
    /// Plasticidade estrutural: crescimento e exploração.
    Architect = 1,
    /// Homeostase: mantém ρ no alvo e poda o excesso.
    Regulator = 2,
}

impl NicheRole {
    /// Decodifica o byte do fio.
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Sentinel),
            1 => Some(Self::Architect),
            2 => Some(Self::Regulator),
            _ => None,
        }
    }
}

/// CRC-32 IEEE 802.3 (polinômio refletido `0xEDB88320`), tabela em `const`.
const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// CRC-32 IEEE (zlib/PNG) sem alocação.
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// Estado de um nó para fins de especiação.
#[derive(Clone, Copy, Debug)]
pub struct NodeState {
    /// Genoma contínuo.
    pub genome: [f32; GENOME_DIM],
    /// Autonomia/integração local `Φ_auto`.
    pub phi_auto: f32,
    /// Raio espectral estimado.
    pub spectral_radius: f32,
    /// Plasticidade recente.
    pub plasticity: f32,
    /// Coerência de fase local `R ∈ [0,1]`.
    pub coherence: f32,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            genome: [0.0; GENOME_DIM],
            phi_auto: 0.0,
            spectral_radius: 0.0,
            plasticity: 0.0,
            coherence: 0.0,
        }
    }
}

/// Frame holográfico (forma decodificada).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HolographicFrame {
    /// Identificador do nó emissor.
    pub node_id: u32,
    /// Sequência do emissor.
    pub seq: u32,
    /// Timestamp (ms).
    pub t_ms: u64,
    /// Papel de nicho.
    pub role: NicheRole,
    /// `Φ_auto`.
    pub phi_auto: f32,
    /// `Φ_niche`.
    pub phi_niche: f32,
    /// Coerência local.
    pub coherence: f32,
    /// Genoma.
    pub genome: [f32; GENOME_DIM],
}

#[inline]
fn put_f32(buf: &mut [u8; FRAME_BYTES], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn get_f32(buf: &[u8; FRAME_BYTES], off: usize) -> f32 {
    f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

impl HolographicFrame {
    /// Serializa em `out` (72 B) e grava o CRC-32 nos 4 últimos bytes.
    pub fn encode_into(&self, out: &mut [u8; FRAME_BYTES]) {
        out.fill(0);
        out[0..2].copy_from_slice(&FRAME_MAGIC.to_le_bytes());
        out[2] = FRAME_VERSION;
        out[3] = self.role as u8;
        out[4..8].copy_from_slice(&self.node_id.to_le_bytes());
        out[8..12].copy_from_slice(&self.seq.to_le_bytes());
        out[12..20].copy_from_slice(&self.t_ms.to_le_bytes());
        put_f32(out, 20, self.phi_auto);
        put_f32(out, 24, self.phi_niche);
        put_f32(out, 28, self.coherence);
        for (k, g) in self.genome.iter().enumerate() {
            put_f32(out, 32 + 4 * k, *g);
        }
        let crc = crc32_ieee(&out[..CRC_OFFSET]);
        out[CRC_OFFSET..].copy_from_slice(&crc.to_le_bytes());
    }

    /// Valida magic, versão, CRC e papel; devolve o frame decodificado.
    pub fn decode(buf: &[u8; FRAME_BYTES]) -> Result<Self> {
        if u16::from_le_bytes([buf[0], buf[1]]) != FRAME_MAGIC || buf[2] != FRAME_VERSION {
            return Err(AetherError::Config(
                "frame holografico: magic/versao".into(),
            ));
        }
        let want = u32::from_le_bytes([buf[68], buf[69], buf[70], buf[71]]);
        if crc32_ieee(&buf[..CRC_OFFSET]) != want {
            return Err(AetherError::Degenerate(
                "frame holografico: CRC invalido".into(),
            ));
        }
        let role = NicheRole::from_u8(buf[3])
            .ok_or_else(|| AetherError::Config("frame holografico: papel invalido".into()))?;
        let mut genome = [0.0f32; GENOME_DIM];
        for (k, g) in genome.iter_mut().enumerate() {
            *g = get_f32(buf, 32 + 4 * k);
        }
        Ok(Self {
            node_id: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            seq: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            t_ms: u64::from_le_bytes([
                buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
            ]),
            role,
            phi_auto: get_f32(buf, 20),
            phi_niche: get_f32(buf, 24),
            coherence: get_f32(buf, 28),
            genome,
        })
    }
}

/// Motor de especiação do enxame.
#[derive(Debug)]
pub struct SwarmSpeciationEngine {
    alpha_div: f32,
    eta_repel: f32,
    nodes: Vec<NodeState>,
    phi_niche: Vec<f32>,
    roles: Vec<NicheRole>,
    min_dist: Vec<f32>,
    nearest: Vec<usize>,
    order: Vec<usize>,
    seq: u32,
}

impl SwarmSpeciationEngine {
    /// Cria o motor para `n ≥ 2` nós, com peso de divergência `alpha_div ≥ 0`.
    pub fn new(n: usize, alpha_div: f32) -> Result<Self> {
        if n < 2 {
            return Err(AetherError::Config("Speciation: n >= 2".into()));
        }
        if !alpha_div.is_finite() || alpha_div < 0.0 {
            return Err(AetherError::Config("Speciation: alpha_div >= 0".into()));
        }
        Ok(Self {
            alpha_div,
            eta_repel: 0.05,
            nodes: vec![NodeState::default(); n],
            phi_niche: vec![0.0; n],
            roles: vec![NicheRole::Regulator; n],
            min_dist: vec![0.0; n],
            nearest: vec![0; n],
            order: (0..n).collect(),
            seq: 0,
        })
    }

    /// Número de nós.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Sempre `false` (`n ≥ 2` garantido no construtor).
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Define o passo de repulsão genômica (`0` desliga a deriva).
    pub fn set_repulsion(&mut self, eta: f32) {
        self.eta_repel = if eta.is_finite() {
            eta.clamp(0.0, 0.5)
        } else {
            0.05
        };
    }

    /// Escreve o estado do nó `i` (valores não finitos são rejeitados).
    pub fn set_node(&mut self, i: usize, s: NodeState) -> Result<()> {
        if i >= self.nodes.len() {
            return Err(AetherError::shape([self.nodes.len()], [i]));
        }
        let finite = s.genome.iter().all(|g| g.is_finite())
            && s.phi_auto.is_finite()
            && s.spectral_radius.is_finite()
            && s.plasticity.is_finite()
            && s.coherence.is_finite();
        if !finite {
            return Err(AetherError::Degenerate(
                "Speciation: estado nao finito".into(),
            ));
        }
        self.nodes[i] = s;
        Ok(())
    }

    /// Estado do nó `i`.
    pub fn node(&self, i: usize) -> &NodeState {
        &self.nodes[i]
    }

    /// `Φ_niche` por nó (válido após [`step`](Self::step)).
    pub fn phi_niche(&self) -> &[f32] {
        &self.phi_niche
    }

    /// Papéis por nó (válidos após [`step`](Self::step)).
    pub fn roles(&self) -> &[NicheRole] {
        &self.roles
    }

    /// Menor distância de genoma de cada nó ao seu vizinho mais próximo.
    pub fn min_distances(&self) -> &[f32] {
        &self.min_dist
    }

    /// Menor distância de genoma do enxame (diversidade mínima).
    pub fn swarm_min_distance(&self) -> f32 {
        self.min_dist.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// Quantos papéis distintos existem no enxame.
    pub fn distinct_roles(&self) -> usize {
        let mut seen = [false; 3];
        for r in &self.roles {
            seen[*r as usize] = true;
        }
        seen.iter().filter(|b| **b).count()
    }

    /// Um passo: `min_dist`, `Φ_niche`, repulsão genômica e papéis.
    pub fn step(&mut self) {
        self.compute_niche();
        self.repel_genomes();
        self.assign_roles();
    }

    fn compute_niche(&mut self) {
        let n = self.nodes.len();
        for i in 0..n {
            let mut best = f32::INFINITY;
            let mut arg = i;
            for j in 0..n {
                if j == i {
                    continue;
                }
                let mut d2 = 0.0f32;
                for k in 0..GENOME_DIM {
                    let d = self.nodes[i].genome[k] - self.nodes[j].genome[k];
                    d2 += d * d;
                }
                if d2 < best {
                    best = d2;
                    arg = j;
                }
            }
            let d = best.sqrt();
            self.min_dist[i] = d;
            self.nearest[i] = arg;
            self.phi_niche[i] = self.nodes[i].phi_auto + self.alpha_div * d;
        }
    }

    fn repel_genomes(&mut self) {
        if self.eta_repel == 0.0 {
            return;
        }
        for i in 0..self.nodes.len() {
            let j = self.nearest[i];
            let inv = 1.0 / self.min_dist[i].max(1e-6);
            for k in 0..GENOME_DIM {
                let away = (self.nodes[i].genome[k] - self.nodes[j].genome[k]) * inv;
                let g = self.nodes[i].genome[k] + self.eta_repel * away;
                self.nodes[i].genome[k] = g.clamp(-1.0, 1.0);
            }
        }
    }

    fn assign_roles(&mut self) {
        let n = self.nodes.len();
        let quota = n.div_ceil(3);
        for (k, o) in self.order.iter_mut().enumerate() {
            *o = k;
        }
        for r in self.roles.iter_mut() {
            *r = NicheRole::Regulator;
        }
        let nodes = &self.nodes;
        let niche = &self.phi_niche;
        self.order.sort_unstable_by(|&a, &b| {
            nodes[b]
                .spectral_radius
                .total_cmp(&nodes[a].spectral_radius)
                .then_with(|| niche[b].total_cmp(&niche[a]))
                .then_with(|| a.cmp(&b))
        });
        for &i in self.order.iter().take(quota) {
            self.roles[i] = NicheRole::Sentinel;
        }
        self.order[quota..].sort_unstable_by(|&a, &b| {
            nodes[b]
                .plasticity
                .total_cmp(&nodes[a].plasticity)
                .then_with(|| niche[b].total_cmp(&niche[a]))
                .then_with(|| a.cmp(&b))
        });
        for &i in self.order[quota..].iter().take(quota) {
            self.roles[i] = NicheRole::Architect;
        }
        if self.roles.iter().all(|r| *r == self.roles[0]) {
            self.roles[n - 1] = NicheRole::Regulator;
            self.roles[0] = NicheRole::Sentinel;
        }
    }

    /// Serializa o frame holográfico do nó `i` em `out` (zero alocação).
    pub fn encode_into(&mut self, i: usize, t_ms: u64, out: &mut [u8; FRAME_BYTES]) -> Result<()> {
        if i >= self.nodes.len() {
            return Err(AetherError::shape([self.nodes.len()], [i]));
        }
        self.seq = self.seq.wrapping_add(1);
        HolographicFrame {
            node_id: i as u32,
            seq: self.seq,
            t_ms,
            role: self.roles[i],
            phi_auto: self.nodes[i].phi_auto,
            phi_niche: self.phi_niche[i],
            coherence: self.nodes[i].coherence,
            genome: self.nodes[i].genome,
        }
        .encode_into(out);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(n: usize) -> SwarmSpeciationEngine {
        let mut e = SwarmSpeciationEngine::new(n, 0.5).expect("engine");
        for i in 0..n {
            let mut s = NodeState::default();
            for k in 0..GENOME_DIM {
                s.genome[k] = 0.01 * ((i * 7 + k * 3) % 11) as f32;
            }
            s.phi_auto = 0.1 * i as f32;
            s.spectral_radius = 1.0 - 0.05 * i as f32;
            s.plasticity = ((i * 5) % 7) as f32 / 7.0;
            s.coherence = 0.5;
            e.set_node(i, s).expect("set");
        }
        e
    }

    #[test]
    fn crc32_bate_com_vetor_de_referencia_ieee() {
        // "123456789" -> 0xCBF43926 (CRC-32/ISO-HDLC).
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn frame_roundtrip_72_bytes_e_detecta_corrupcao() {
        let mut e = engine(6);
        e.step();
        let mut buf = [0u8; FRAME_BYTES];
        e.encode_into(3, 1234, &mut buf).expect("enc");
        let f = HolographicFrame::decode(&buf).expect("dec");
        assert_eq!(f.node_id, 3);
        assert_eq!(f.t_ms, 1234);
        assert_eq!(f.role, e.roles()[3]);
        buf[40] ^= 0x01;
        assert!(HolographicFrame::decode(&buf).is_err());
    }

    #[test]
    fn nicho_premia_divergencia_e_gera_tres_papeis() {
        let mut e = engine(9);
        let before = {
            e.step();
            e.min_distances()[0]
        };
        for _ in 0..50 {
            e.step();
        }
        assert!(e.swarm_min_distance() > 0.0);
        assert!(e.min_distances()[0] >= before - 1e-6);
        let expected = e.node(0).phi_auto + 0.5 * e.min_distances()[0];
        assert!((e.phi_niche()[0] - expected).abs() < 0.05 + 1e-3);
        assert_eq!(e.distinct_roles(), 3);
    }
}

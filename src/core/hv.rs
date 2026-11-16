//! HvBank — geometria transplantada (10.240 bits), camada T1/T2.
//!
//! Formato AETHV1 (`docs/INTAKE_PLAN.md`). Não entra no hot path T0.
//! CLAIM fórum MSG 0229 / 0244. Feature `m1`.

use std::fs;
use std::path::Path;

const MAGIC: &[u8; 8] = b"AETHV1\0\0";
const HEADER_SIZE: usize = 128;

/// Banco de hipervetores binários (1,28 KB/átomo @ 10.240 bits).
#[derive(Debug, Clone)]
pub struct HvBank {
    pub dim: usize,
    pub words: usize,
    pub dim_in: usize,
    pub seed: u64,
    pub bits: Vec<u64>,
    pub keys: Vec<u128>,
    /// LSH: 4 tabelas × 16 bits → buckets (preenchido se count > 10_000).
    pub lsh_tables: Vec<Vec<Vec<u32>>>,
}

impl HvBank {
    pub fn load(bank_path: &Path, idx_path: &Path) -> Result<Self, String> {
        let raw = fs::read(bank_path).map_err(|e| e.to_string())?;
        if raw.len() < HEADER_SIZE {
            return Err("bank.bin curto".into());
        }
        if &raw[0..8] != MAGIC {
            return Err("magic inválido".into());
        }
        let version = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        if version != 1 {
            return Err(format!("version {version}"));
        }
        let dim_bits = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
        let words = u32::from_le_bytes(raw[16..20].try_into().unwrap()) as usize;
        let dim_in = u32::from_le_bytes(raw[20..24].try_into().unwrap()) as usize;
        let count = u64::from_le_bytes(raw[28..36].try_into().unwrap()) as usize;
        let seed = u64::from_le_bytes(raw[36..44].try_into().unwrap());
        let payload = &raw[HEADER_SIZE..];
        if payload.len() != count * words * 8 {
            return Err("payload size mismatch".into());
        }
        let mut bits = Vec::with_capacity(count * words);
        for chunk in payload.chunks_exact(8) {
            bits.push(u64::from_le_bytes(chunk.try_into().unwrap()));
        }
        let idx = fs::read(idx_path).map_err(|e| e.to_string())?;
        if idx.len() != count * 24 {
            return Err("idx size mismatch".into());
        }
        let mut keys = Vec::with_capacity(count);
        for rec in idx.chunks_exact(24) {
            let lo = u64::from_le_bytes(rec[0..8].try_into().unwrap());
            let hi = u64::from_le_bytes(rec[8..16].try_into().unwrap());
            keys.push((u128::from(hi) << 64) | u128::from(lo));
        }
        let mut bank = Self {
            dim: dim_bits,
            words,
            dim_in,
            seed,
            bits,
            keys,
            lsh_tables: Vec::new(),
        };
        if bank.count() > 10_000 {
            bank.build_lsh(4, 16);
        }
        Ok(bank)
    }

    pub fn count(&self) -> usize {
        self.keys.len()
    }

    pub fn atom_words(&self, i: usize) -> &[u64] {
        let o = i * self.words;
        &self.bits[o..o + self.words]
    }

    pub fn hamming(a: &[u64], b: &[u64]) -> u32 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x ^ y).count_ones())
            .sum()
    }

    /// Top-k por menor Hamming; `out` pré-alocado `(dist, key)`.
    pub fn top_k_into(&self, q: &[u64], k: usize, out: &mut [(u32, u128)]) -> usize {
        let k = k.min(out.len()).min(self.count());
        if k == 0 {
            return 0;
        }
        if !self.lsh_tables.is_empty() {
            return self.top_k_lsh(q, k, out);
        }
        for slot in out.iter_mut().take(k) {
            *slot = (u32::MAX, 0);
        }
        let mut filled = 0usize;
        for i in 0..self.count() {
            let d = Self::hamming(q, self.atom_words(i));
            let key = self.keys[i];
            if filled < k {
                out[filled] = (d, key);
                filled += 1;
                if filled == k {
                    out[..k].sort_by_key(|t| t.0);
                }
            } else if d < out[k - 1].0 {
                out[k - 1] = (d, key);
                out[..k].sort_by_key(|t| t.0);
            }
        }
        filled
    }

    /// LSH: `n_tables` tabelas, cada uma usa `bits_per` bits do hipervetor como chave.
    pub fn build_lsh(&mut self, n_tables: usize, bits_per: usize) {
        let n_tables = n_tables.max(1);
        let bits_per = bits_per.clamp(1, 16);
        let buckets = 1usize << bits_per;
        self.lsh_tables = Vec::with_capacity(n_tables);
        for t in 0..n_tables {
            let mut table = vec![Vec::new(); buckets];
            let word = (t * 3) % self.words;
            let shift = (t * 5) % (64usize.saturating_sub(bits_per).max(1));
            let mask = (1u64 << bits_per) - 1;
            for i in 0..self.count() {
                let w = self.atom_words(i)[word];
                let code = ((w >> shift) & mask) as usize;
                table[code].push(i as u32);
            }
            self.lsh_tables.push(table);
        }
    }

    fn top_k_lsh(&self, q: &[u64], k: usize, out: &mut [(u32, u128)]) -> usize {
        let bits_per = {
            let buckets = self.lsh_tables[0].len();
            (buckets.trailing_zeros()) as usize
        };
        let mask = (1u64 << bits_per) - 1;
        let mut cand = Vec::with_capacity(256);
        for (t, table) in self.lsh_tables.iter().enumerate() {
            let word = (t * 3) % self.words;
            let shift = (t * 5) % (64usize.saturating_sub(bits_per).max(1));
            let code = ((q[word] >> shift) & mask) as usize;
            for &idx in &table[code] {
                cand.push(idx);
            }
        }
        cand.sort_unstable();
        cand.dedup();
        for slot in out.iter_mut().take(k) {
            *slot = (u32::MAX, 0);
        }
        let mut filled = 0usize;
        for idx in cand {
            let i = idx as usize;
            let d = Self::hamming(q, self.atom_words(i));
            let key = self.keys[i];
            if filled < k {
                out[filled] = (d, key);
                filled += 1;
                if filled == k {
                    out[..k].sort_by_key(|t| t.0);
                }
            } else if d < out[k - 1].0 {
                out[k - 1] = (d, key);
                out[..k].sort_by_key(|t| t.0);
            }
        }
        filled
    }
}

/// Reconstrói plano de shards a partir de fatos (≤48/relação) — view materializada.
pub fn shard_plan_from_relations(rel_counts: &[(String, usize)]) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    for (rel, c) in rel_counts {
        let mut off = 0usize;
        while off < *c {
            let n = (*c - off).min(48);
            out.push((rel.clone(), off, n));
            off += n;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hamming_zero() {
        let a = [0u64; 160];
        assert_eq!(HvBank::hamming(&a, &a), 0);
    }
}

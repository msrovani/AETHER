//! E3 — transplantação geométrica (Johnson–Lindenstrauss) + proveniência.
//!
//! O E3 responde a uma pergunta específica: **um espaço semântico externo
//! (embeddings densos) pode ser transplantado para os hipervetores do motor
//! sem virar RAG?** A resposta deste módulo é operacional:
//!
//! 1. [`JlProjector`] projeta `d` dimensões densas em `D` **bits** por uma
//!    matriz Gaussiana semeada e binarização por sinal. O resultado tem
//!    `D/8` bytes por átomo (`D = 10.240` ⇒ **1,28 KB**), o mesmo orçamento de
//!    um vetor FHRR `D = 160` em `f32` complexo.
//! 2. [`PhaseEncoder`] projeta o mesmo embedding em `D` **fases** (random
//!    Fourier features **lineares**, sem viés). A linearidade é o que faz
//!    `a ⊗ b⁻¹ ⊗ c` funcionar: a soma de fases é a projeção da soma dos
//!    vetores, não uma aproximação.
//! 3. [`BinaryBam`] é a memória de cleanup binária (distância de Hamming).
//! 4. [`StructuralVerifier`] cruza consistência relacional com proveniência
//!    **fora do hot path** (`≤ 20 ms`, zero alocação), emitindo eventos em um
//!    [`KnowledgeSink`] quando ligado.
//!
//! # O que a geometria **não** garante
//!
//! Johnson–Lindenstrauss preserva **distâncias**, não **fatos**. Um átomo
//! transplantado carrega similaridade, não verdade: por isso todo átomo e toda
//! resposta carregam [`SourceTag`] e a decodificação tem abstenção por margem.
//! Chamar a projeção de "conhecimento" seria exatamente o erro que o Vol. III
//! §7/R3 denuncia.
//!
//! # Orçamento de memória (o desenho importa)
//!
//! A matriz JL **não** é materializada inteira (`d × D` em `f32` = 15,7 MB).
//! [`JlProjector::project_into`] materializa um bloco de `block × d` (786 KB
//! para `block = 512`, `d = 384`) e o aplica a **todas** as linhas antes de
//! passar ao próximo bloco: o bloco fica residente na L2 e a matriz grande
//! nunca existe. É o mesmo raciocínio do *working set* de 1,25 MB do dossiê.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::core::knowledge_bridge::{EventKind, KnowledgeEvent, KnowledgeSink, SourceTag, Verdict};

/// Bloco default de bits materializado por vez (`512 × 384 f32` = 786 KB).
pub const DEFAULT_BLOCK: usize = 512;

/// Gerador Gaussiano determinístico (Box–Muller sobre `ChaCha8Rng`).
#[inline]
fn gauss(rng: &mut ChaCha8Rng) -> f32 {
    let u1 = rng.gen::<f32>().max(f32::MIN_POSITIVE);
    let u2 = rng.gen::<f32>();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Preenche `out` com N(0,1) determinístico.
pub fn gaussian_vec_into(rng: &mut ChaCha8Rng, out: &mut [f32]) {
    for v in out.iter_mut() {
        *v = gauss(rng);
    }
}

/// Normaliza `v` para norma euclidiana 1 (no-op se degenerado).
pub fn normalize_in_place(v: &mut [f32]) {
    let n2: f32 = v.iter().map(|x| x * x).sum();
    if n2 > 0.0 && n2.is_finite() {
        let inv = n2.sqrt().recip();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

/// Projetor Johnson–Lindenstrauss com binarização por sinal.
///
/// `d` = dimensão de entrada, `bits` = dimensão de saída em bits.
#[derive(Debug)]
pub struct JlProjector {
    d: usize,
    bits: usize,
    words: usize,
    block: usize,
    seed: u64,
    /// `block × d` — único buffer grande, reutilizado por bloco.
    mat: Vec<f32>,
    /// `block` acumuladores por linha.
    acc: Vec<f32>,
}

impl JlProjector {
    /// Aloca o bloco de projeção. `block` é limitado a `bits`.
    pub fn new(d: usize, bits: usize, block: usize, seed: u64) -> Self {
        assert!(d > 0 && bits > 0, "JL: dimensoes devem ser > 0");
        let block = block.clamp(1, bits);
        Self {
            d,
            bits,
            words: bits.div_ceil(64),
            block,
            seed,
            mat: vec![0.0; block * d],
            acc: vec![0.0; block],
        }
    }

    /// Dimensão de entrada.
    pub fn dim(&self) -> usize {
        self.d
    }
    /// Dimensão de saída em bits.
    pub fn bits(&self) -> usize {
        self.bits
    }
    /// Palavras de 64 bits por átomo.
    pub fn words(&self) -> usize {
        self.words
    }
    /// Bytes por átomo binarizado.
    pub fn bytes_per_atom(&self) -> usize {
        self.words * 8
    }
    /// Bytes do bloco de projeção (não a matriz inteira).
    pub fn scratch_bytes(&self) -> usize {
        self.mat.len() * std::mem::size_of::<f32>() + self.acc.len() * 4
    }

    /// Projeta `n_rows` embeddings (`rows` row-major, `d` cada) em `bits` bits
    /// por linha, gravados em `out` (`n_rows × words`).
    ///
    /// Zero alocação após [`JlProjector::new`]. A matriz é regenerada por
    /// bloco a partir de `seed`, então o resultado **não** depende da ordem de
    /// chamada nem do número de linhas.
    pub fn project_into(&mut self, rows: &[f32], n_rows: usize, out: &mut [u64]) {
        assert!(rows.len() >= n_rows * self.d, "JL: rows curto");
        assert!(out.len() >= n_rows * self.words, "JL: out curto");
        for w in out[..n_rows * self.words].iter_mut() {
            *w = 0;
        }
        let d = self.d;
        let block = self.block;
        let mut start = 0usize;
        while start < self.bits {
            let b = block.min(self.bits - start);
            // Matriz do bloco: determinística por (seed, start).
            let mut rng = ChaCha8Rng::seed_from_u64(
                self.seed ^ (start as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
            );
            for k in 0..b {
                let row = &mut self.mat[k * d..(k + 1) * d];
                for v in row.iter_mut() {
                    *v = gauss(&mut rng);
                }
            }
            // Aplica o bloco residente a todas as linhas.
            for r in 0..n_rows {
                let x = &rows[r * d..(r + 1) * d];
                for k in 0..b {
                    let prow = &self.mat[k * d..(k + 1) * d];
                    let mut acc = 0.0f32;
                    for j in 0..d {
                        acc += prow[j] * x[j];
                    }
                    self.acc[k] = acc;
                }
                for k in 0..b {
                    if self.acc[k] >= 0.0 {
                        let bit = start + k;
                        out[r * self.words + (bit >> 6)] |= 1u64 << (bit & 63);
                    }
                }
            }
            start += b;
        }
    }
}

/// Distância de Hamming entre dois átomos de `words` palavras, contando apenas
/// os `bits` primeiros bits.
#[inline]
pub fn hamming(a: &[u64], b: &[u64], words: usize, bits: usize) -> u32 {
    let mut acc = 0u32;
    let last = (bits - 1) / 64;
    let mask = if bits % 64 == 0 {
        u64::MAX
    } else {
        (1u64 << (bits % 64)) - 1
    };
    for w in 0..words {
        let mut x = a[w] ^ b[w];
        if w == last {
            x &= mask;
        }
        acc += x.count_ones();
    }
    acc
}

/// Memória binária de cleanup (BAM): banco de átomos + busca por Hamming.
///
/// Consulta sem alocação: `nearest` percorre o banco com `popcount`.
#[derive(Debug)]
pub struct BinaryBam {
    dim: usize,
    words: usize,
    cap: usize,
    atoms: Vec<u64>,
}

impl BinaryBam {
    /// Banco com capacidade fixa `cap` (não realoca em `push` até encher).
    pub fn with_capacity(dim: usize, cap: usize) -> Self {
        assert!(dim > 0, "BAM: dim > 0");
        let words = dim.div_ceil(64);
        Self {
            dim,
            words,
            cap,
            atoms: Vec::with_capacity(cap * words),
        }
    }

    /// Dimensão em bits.
    pub fn dim(&self) -> usize {
        self.dim
    }
    /// Palavras por átomo.
    pub fn words(&self) -> usize {
        self.words
    }
    /// Número de átomos.
    pub fn len(&self) -> usize {
        self.atoms.len() / self.words
    }
    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }
    /// Bytes ocupados.
    pub fn memory_bytes(&self) -> usize {
        self.atoms.len() * 8
    }
    /// Bytes por átomo.
    pub fn bytes_per_atom(&self) -> usize {
        self.words * 8
    }

    /// Copia um átomo para o banco. Retorna o índice (ou `None` se cheio).
    pub fn push(&mut self, bits: &[u64]) -> Option<usize> {
        assert!(bits.len() >= self.words);
        if self.len() >= self.cap {
            return None;
        }
        self.atoms.extend_from_slice(&bits[..self.words]);
        Some(self.len() - 1)
    }

    /// Átomo `i`.
    pub fn atom(&self, i: usize) -> &[u64] {
        let b = i * self.words;
        &self.atoms[b..b + self.words]
    }

    /// Similaridade `1 − 2·hamming/D` em `[-1, 1]`.
    pub fn similarity(&self, a: usize, b: usize) -> f32 {
        1.0 - 2.0 * hamming(self.atom(a), self.atom(b), self.words, self.dim) as f32
            / self.dim as f32
    }

    /// Vizinho mais próximo por Hamming. Retorna `(índice, distância)`.
    pub fn nearest(&self, cue: &[u64]) -> (usize, u32) {
        let mut best = 0usize;
        let mut best_d = u32::MAX;
        for i in 0..self.len() {
            let dd = hamming(self.atom(i), cue, self.words, self.dim);
            if dd < best_d {
                best_d = dd;
                best = i;
            }
        }
        (best, best_d)
    }
}

/// `a ⊕ b` (binding/unbinding binário: o mesmo XOR).
#[inline]
pub fn bind_xor_into(a: &[u64], b: &[u64], out: &mut [u64]) {
    for i in 0..out.len() {
        out[i] = a[i] ^ b[i];
    }
}

/// Codificador de fases **linear** (random Fourier features sem viés).
///
/// `θ(x) = s·W·x (mod 2π)` com `W ~ N(0, 1)` e `s` a escala. Como não há viés,
/// `θ(a) − θ(b) + θ(c) = θ(a − b + c)` **exatamente** (módulo `2π`): é o que
/// permite analogia por aritmética de fase.
#[derive(Debug)]
pub struct PhaseEncoder {
    d: usize,
    dim: usize,
    scale: f32,
    mat: Vec<f32>,
}

impl PhaseEncoder {
    /// Matriz `dim × d` (ex.: `160 × 384 f32` = 240 KB, cabe na L2).
    pub fn new(d: usize, dim: usize, seed: u64, scale: f32) -> Self {
        assert!(d > 0 && dim > 0, "PhaseEncoder: dims > 0");
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut mat = vec![0.0f32; dim * d];
        for v in mat.iter_mut() {
            *v = gauss(&mut rng) * scale;
        }
        Self { d, dim, scale, mat }
    }

    /// Dimensão de entrada.
    pub fn dim_in(&self) -> usize {
        self.d
    }
    /// Dimensão de saída (fasores).
    pub fn dim(&self) -> usize {
        self.dim
    }
    /// Escala aplicada.
    pub fn scale(&self) -> f32 {
        self.scale
    }
    /// Bytes da matriz.
    pub fn memory_bytes(&self) -> usize {
        self.mat.len() * std::mem::size_of::<f32>()
    }

    /// Codifica `x` (len `d`) em `out` (len `dim`), fases em `[0, 2π)`.
    /// Zero alocação.
    pub fn encode_into(&self, x: &[f32], out: &mut [f32]) {
        assert!(x.len() >= self.d && out.len() >= self.dim);
        for (k, o) in out[..self.dim].iter_mut().enumerate() {
            let row = &self.mat[k * self.d..(k + 1) * self.d];
            let mut acc = 0.0f32;
            for j in 0..self.d {
                acc += row[j] * x[j];
            }
            *o = acc.rem_euclid(std::f32::consts::TAU);
        }
    }
}

/// Similaridade média de cosseno entre dois vetores de fase.
#[inline]
pub fn phase_similarity(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut acc = 0.0f32;
    for k in 0..n {
        acc += (a[k] - b[k]).cos();
    }
    acc / n as f32
}

/// `a ⊗ b⁻¹ ⊗ c` em fase: `out[k] = a[k] − b[k] + c[k] (mod 2π)`.
pub fn phase_analogy(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
    let n = out.len().min(a.len()).min(b.len()).min(c.len());
    for k in 0..n {
        out[k] = (a[k] - b[k] + c[k]).rem_euclid(std::f32::consts::TAU);
    }
}

/// Correlação de Spearman entre duas amostras de mesmo tamanho.
///
/// Aloca (ordenação); **não** é caminho quente. Empates recebem posto médio.
pub fn spearman(xs: &[f64], ys: &[f64]) -> f64 {
    let n = xs.len().min(ys.len());
    if n < 3 {
        return f64::NAN;
    }
    let rx = ranks(&xs[..n]);
    let ry = ranks(&ys[..n]);
    let nf = n as f64;
    let mx = rx.iter().sum::<f64>() / nf;
    let my = ry.iter().sum::<f64>() / nf;
    let mut num = 0.0;
    let mut dx = 0.0;
    let mut dy = 0.0;
    for i in 0..n {
        let a = rx[i] - mx;
        let b = ry[i] - my;
        num += a * b;
        dx += a * a;
        dy += b * b;
    }
    if dx <= 0.0 || dy <= 0.0 {
        return f64::NAN;
    }
    num / (dx.sqrt() * dy.sqrt())
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut k = i;
        while k + 1 < n && (v[order[k + 1]] - v[order[i]]).abs() < 1e-15 {
            k += 1;
        }
        let avg = ((i + 1) as f64 + (k + 1) as f64) / 2.0;
        for idx in i..=k {
            out[order[idx]] = avg;
        }
        i = k + 1;
    }
    out
}

/// Fato observado/transplantado/inferido — entrada do verificador.
///
/// `margin` é a margem de cleanup da consulta que produziu o fato (0 quando o
/// fato veio de uma gravação direta, sem consulta).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FactRecord {
    /// Identificador da entidade.
    pub entity: u32,
    /// Identificador da relação.
    pub relation: u32,
    /// Identificador do valor.
    pub value: u32,
    /// Origem declarada.
    pub tag: SourceTag,
    /// Identificador da fonte externa (`0` = sem fonte ⇒ inválido para `Transplanted`).
    pub source: u32,
    /// Margem da consulta (`0.0` para gravação direta).
    pub margin: f32,
}

/// Resultado do verificador estrutural.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructuralVerdict {
    /// Fatos avaliados.
    pub checked: usize,
    /// Pares `(entidade, relação)` com dois valores diferentes.
    pub value_conflicts: usize,
    /// Proveniência inválida (transplantado sem fonte / inferido sem lastro).
    pub provenance_violations: usize,
    /// Fatos com margem abaixo do piso (não aceitos como resposta).
    pub ambiguous: usize,
    /// Eventos entregues ao sink.
    pub emitted: u64,
    /// Eventos descartados pelo sink (fila cheia).
    pub dropped: u64,
}

impl StructuralVerdict {
    /// O grafo é estruturalmente consistente e toda proveniência tem lastro.
    pub fn is_valid(&self) -> bool {
        self.value_conflicts == 0 && self.provenance_violations == 0
    }
}

/// Verificador de consistência de grafo **fora do hot path**.
///
/// Duas checagens, ambas `O(n)` com tabelas pré-alocadas:
///
/// 1. **Funcionalidade** — `(entidade, relação)` não pode apontar para dois
///    valores distintos.
/// 2. **Proveniência** — `Transplanted` exige `source != 0`; `Inferred` exige
///    ao menos um fato `Observed`/`Transplanted` para a mesma entidade.
///
/// Tabelas indexadas por `entity · relations + relation`: nenhum `HashMap`,
/// nenhuma alocação em [`StructuralVerifier::verify_into`].
#[derive(Debug)]
pub struct StructuralVerifier {
    entities: usize,
    relations: usize,
    slot: Vec<u32>,
    slot_stamp: Vec<u32>,
    support: Vec<u32>,
    support_stamp: Vec<u32>,
    stamp_now: u32,
}

impl StructuralVerifier {
    /// Aloca as tabelas para `entities × relations`.
    pub fn new(entities: usize, relations: usize) -> Self {
        let cells = entities.saturating_mul(relations);
        Self {
            entities,
            relations,
            slot: vec![0; cells],
            slot_stamp: vec![0; cells],
            support: vec![0; entities],
            support_stamp: vec![0; entities],
            stamp_now: 0,
        }
    }

    /// Bytes das tabelas.
    pub fn memory_bytes(&self) -> usize {
        (self.slot.len() + self.slot_stamp.len() + self.support.len() + self.support_stamp.len())
            * 4
    }

    fn next_stamp(&mut self) -> u32 {
        self.stamp_now = self.stamp_now.wrapping_add(1);
        if self.stamp_now == 0 {
            self.slot_stamp.fill(0);
            self.support_stamp.fill(0);
            self.stamp_now = 1;
        }
        self.stamp_now
    }

    /// Verifica `facts`; emite um [`KnowledgeEvent`] por fato quando `sink`
    /// está ligado. Zero alocação.
    pub fn verify_into<S: KnowledgeSink>(
        &mut self,
        facts: &[FactRecord],
        min_margin: f32,
        mut sink: Option<&mut S>,
    ) -> StructuralVerdict {
        let stamp = self.next_stamp();
        let mut out = StructuralVerdict {
            checked: facts.len(),
            ..Default::default()
        };

        // Passo 1 — lastro por entidade (evidência observada/transplantada).
        for f in facts {
            if f.tag.is_evidence() {
                let e = f.entity as usize;
                if e < self.entities {
                    if self.support_stamp[e] != stamp {
                        self.support_stamp[e] = stamp;
                        self.support[e] = 0;
                    }
                    self.support[e] += 1;
                }
            }
        }

        // Passo 2 — consistência + proveniência.
        for f in facts {
            let e = f.entity as usize;
            let r = f.relation as usize;
            let in_range = e < self.entities && r < self.relations;
            let mut verdict = Verdict::Ok;

            if in_range {
                let cell = e * self.relations + r;
                if self.slot_stamp[cell] != stamp {
                    self.slot_stamp[cell] = stamp;
                    self.slot[cell] = f.value;
                } else if self.slot[cell] != f.value {
                    out.value_conflicts += 1;
                    verdict = Verdict::Violation;
                }
            } else {
                out.provenance_violations += 1;
                verdict = Verdict::Violation;
            }

            let provenance_ok = match f.tag {
                SourceTag::Transplanted => f.source != 0,
                SourceTag::Inferred => {
                    e < self.entities && self.support_stamp[e] == stamp && self.support[e] > 0
                }
                SourceTag::Observed | SourceTag::Axiomatic | SourceTag::Human => true,
            };
            if !provenance_ok {
                out.provenance_violations += 1;
                verdict = Verdict::Violation;
            }

            if f.margin < min_margin {
                out.ambiguous += 1;
                if verdict == Verdict::Ok {
                    verdict = Verdict::Abstain;
                }
            }

            if let Some(s) = sink.as_mut() {
                let kind = if verdict == Verdict::Abstain {
                    EventKind::Abstention
                } else {
                    EventKind::Verification
                };
                let ev =
                    KnowledgeEvent::new(0, kind, f.tag, verdict, f.entity, f.relation, f.value)
                        .with_score(f.margin);
                if s.try_emit(ev) {
                    out.emitted += 1;
                } else {
                    out.dropped += 1;
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::knowledge_bridge::SpscKnowledgeBridge;

    #[test]
    fn jl_preserva_ordem_de_distancias() {
        // 3 pontos: A e B próximos, C longe. Hamming deve refletir isso.
        let d = 48usize;
        let bits = 2048usize;
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut rows = vec![0.0f32; 3 * d];
        for v in rows.iter_mut() {
            *v = gauss(&mut rng);
        }
        // B = A + ruído pequeno; C = -A.
        for j in 0..d {
            rows[d + j] = rows[j] + 0.15 * gauss(&mut rng);
            rows[2 * d + j] = -rows[j];
        }
        let mut proj = JlProjector::new(d, bits, 256, 0xE3);
        let mut out = vec![0u64; 3 * proj.words()];
        proj.project_into(&rows, 3, &mut out);
        let w = proj.words();
        let dab = hamming(&out[0..w], &out[w..2 * w], w, bits);
        let dac = hamming(&out[0..w], &out[2 * w..3 * w], w, bits);
        assert!(
            dab < dac,
            "vizinho deveria estar mais perto: {dab} vs {dac}"
        );
        assert_eq!(proj.bytes_per_atom(), bits / 8);
    }

    #[test]
    fn fase_e_linear_e_analogia_fecha() {
        let d = 64usize;
        let dim = 128usize;
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let mut a = vec![0.0f32; d];
        let mut b = vec![0.0f32; d];
        let mut c = vec![0.0f32; d];
        for j in 0..d {
            a[j] = gauss(&mut rng);
            b[j] = gauss(&mut rng);
            c[j] = gauss(&mut rng);
        }
        // d = a - b + c
        let mut dd = vec![0.0f32; d];
        for j in 0..d {
            dd[j] = a[j] - b[j] + c[j];
        }
        let enc = PhaseEncoder::new(d, dim, 0xE3, 1.0);
        let (mut pa, mut pb, mut pc, mut pd) = (
            vec![0.0f32; dim],
            vec![0.0f32; dim],
            vec![0.0f32; dim],
            vec![0.0f32; dim],
        );
        enc.encode_into(&a, &mut pa);
        enc.encode_into(&b, &mut pb);
        enc.encode_into(&c, &mut pc);
        enc.encode_into(&dd, &mut pd);
        let mut rec = vec![0.0f32; dim];
        phase_analogy(&pa, &pb, &pc, &mut rec);
        assert!(
            (phase_similarity(&rec, &pd) - 1.0).abs() < 1e-4,
            "aritmetica de fase nao reproduziu o vetor alvo"
        );
    }

    #[test]
    fn verificador_pega_conflito_e_proveniencia_sem_fonte() {
        let mut v = StructuralVerifier::new(4, 2);
        let facts = [
            FactRecord {
                entity: 1,
                relation: 0,
                value: 5,
                tag: SourceTag::Transplanted,
                source: 9,
                margin: 0.4,
            },
            // mesmo (entidade, relação), valor diferente ⇒ conflito
            FactRecord {
                entity: 1,
                relation: 0,
                value: 6,
                tag: SourceTag::Observed,
                source: 0,
                margin: 0.4,
            },
            // transplantado sem fonte ⇒ violação de proveniência
            FactRecord {
                entity: 2,
                relation: 1,
                value: 7,
                tag: SourceTag::Transplanted,
                source: 0,
                margin: 0.4,
            },
        ];
        let out = v.verify_into(
            &facts,
            0.1,
            None::<&mut crate::core::knowledge_bridge::NullSink>,
        );
        assert_eq!(out.value_conflicts, 1);
        assert_eq!(out.provenance_violations, 1);
        assert!(!out.is_valid());
    }

    #[test]
    fn verificador_emite_no_bridge_e_nao_aloca() {
        let mut v = StructuralVerifier::new(8, 4);
        let facts: Vec<FactRecord> = (0..8)
            .map(|i| FactRecord {
                entity: i,
                relation: 0,
                value: i,
                tag: SourceTag::Observed,
                source: 0,
                margin: 0.5,
            })
            .collect();
        let (mut bridge, rx) = SpscKnowledgeBridge::<64>::new();
        let out = v.verify_into(&facts, 0.1, Some(&mut bridge));
        assert_eq!(out.emitted, 8);
        assert!(out.is_valid());
        drop(bridge);
        assert_eq!(rx.len(), 8);
    }

    #[test]
    fn spearman_de_ordem_inversa_e_menos_um() {
        let xs: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..50).map(|i| -(i as f64)).collect();
        assert!((spearman(&xs, &ys) + 1.0).abs() < 1e-12);
    }
}

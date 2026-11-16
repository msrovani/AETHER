//! E2d — aquisição CRAM-style de custos pairwise para o Logos.
//!
//! Treina `K` compartilhado (linha/coluna/bloco) por pseudolikelihood **sem**
//! BPTT pela relaxação; emite `J,h` na convenção do certificado e resolve via
//! [`crate::core::logic_pipeline::BsbCore`]. Ver `docs/CRAM_LOGOS.md`.
//! Feature `m1`.

use crate::core::certificate::{self, Certificate, Provenance};
use crate::core::logic_pipeline::BsbCore;
use crate::core::sudoku_csp::{
    decode_spins, relation, violations, CellRelation, SudokuBoard, SudokuShape,
};

/// Energia pairwise compacta: três matrizes `C×C` compartilhadas + bias por dígito.
#[derive(Clone, Debug)]
pub struct PairwiseEnergy {
    pub shape: SudokuShape,
    /// `K_row`, `K_col`, `K_box` — cada uma `C×C` row-major.
    pub k_row: Vec<f64>,
    pub k_col: Vec<f64>,
    pub k_box: Vec<f64>,
    /// Bias por dígito (`C`).
    pub bias: Vec<f64>,
}

impl PairwiseEnergy {
    pub fn zeros(shape: SudokuShape) -> Self {
        let c = shape.order;
        let z = vec![0.0; c * c];
        Self {
            shape,
            k_row: z.clone(),
            k_col: z.clone(),
            k_box: z,
            bias: vec![0.0; c],
        }
    }

    /// Hand-set axiomatic: custo alto no mesmo dígito dentro da unidade.
    pub fn hand_set(shape: SudokuShape, same_digit_cost: f64) -> Self {
        let c = shape.order;
        let mut e = Self::zeros(shape);
        for a in 0..c {
            e.k_row[a * c + a] = same_digit_cost;
            e.k_col[a * c + a] = same_digit_cost;
            e.k_box[a * c + a] = same_digit_cost;
        }
        e
    }

    #[inline]
    fn k_mut(&mut self, rel: CellRelation) -> &mut [f64] {
        match rel {
            CellRelation::SameRow => &mut self.k_row,
            CellRelation::SameCol => &mut self.k_col,
            CellRelation::SameBox => &mut self.k_box,
        }
    }

    #[inline]
    fn k_ref(&self, rel: CellRelation) -> &[f64] {
        match rel {
            CellRelation::SameRow => &self.k_row,
            CellRelation::SameCol => &self.k_col,
            CellRelation::SameBox => &self.k_box,
        }
    }

    /// Número de parâmetros treináveis (3·C² + C).
    pub fn n_params(&self) -> usize {
        let c = self.shape.order;
        3 * c * c + c
    }

    /// Energia Potts one-hot do tabuleiro completo (sem termo de entropia).
    pub fn potts_energy(&self, board: &SudokuBoard) -> f64 {
        let c = self.shape.order;
        let mut e = 0.0;
        for i in 0..self.shape.n_cells() {
            let Some(di) = board.cells[i] else {
                continue;
            };
            e += self.bias[di as usize];
            for (j, cell) in board.cells.iter().enumerate().skip(i + 1) {
                let Some(dj) = *cell else {
                    continue;
                };
                if let Some(rel) = relation(self.shape, i, j) {
                    let k = self.k_ref(rel);
                    e += k[(di as usize) * c + (dj as usize)];
                }
            }
        }
        e
    }
}

/// Converte custos pairwise + one-hot + pistas → QUBO Ising (`J` row-major, `h`).
///
/// Convenção: `H = −½ sᵀ J s − hᵀ s` ([`certificate::energy`]).
/// `x = (1+s)/2` mapeia binário → spin.
pub fn pairwise_to_qubo(
    energy: &PairwiseEnergy,
    clues: &SudokuBoard,
    onehot_lambda: f64,
    clue_field: f64,
) -> (Vec<f64>, Vec<f64>) {
    let shape = energy.shape;
    let c = shape.order;
    let n = shape.n_spins();
    let mut coeff = vec![0.0; n * n]; // c_uv em x_u x_v (u≤v simétrico acumulado)
    let mut b = vec![0.0; n]; // linear em x

    // Pairwise K entre células relacionadas
    for i in 0..shape.n_cells() {
        for j in (i + 1)..shape.n_cells() {
            let Some(rel) = relation(shape, i, j) else {
                continue;
            };
            let k = energy.k_ref(rel);
            for a in 0..c {
                for bb in 0..c {
                    let u = shape.spin(i, a);
                    let v = shape.spin(j, bb);
                    let cost = k[a * c + bb];
                    coeff[u * n + v] += cost;
                    coeff[v * n + u] += cost;
                }
            }
        }
    }

    // Bias por dígito
    for i in 0..shape.n_cells() {
        for a in 0..c {
            b[shape.spin(i, a)] += energy.bias[a];
        }
    }

    // One-hot: λ (Σ_a x_a − 1)² = λ(−Σ x_a + 2 Σ_{a<b} x_a x_b) + const
    for i in 0..shape.n_cells() {
        for a in 0..c {
            b[shape.spin(i, a)] -= onehot_lambda;
            for bb in (a + 1)..c {
                let u = shape.spin(i, a);
                let v = shape.spin(i, bb);
                coeff[u * n + v] += 2.0 * onehot_lambda;
                coeff[v * n + u] += 2.0 * onehot_lambda;
            }
        }
    }

    // Pistas: campo forte empurrando o dígito correto
    for (i, cell) in clues.cells.iter().enumerate() {
        if let Some(d) = *cell {
            let d = d as usize;
            for a in 0..c {
                let u = shape.spin(i, a);
                if a == d {
                    b[u] -= clue_field;
                } else {
                    b[u] += clue_field;
                }
            }
        }
    }

    // x_u x_v → spins: c_uv/4 s_u s_v + ... ; J_uv = −c_uv/4 (u≠v)
    // h_u = −(b_u/2 + Σ_v c_uv/4)
    let mut j = vec![0.0; n * n];
    let mut h = vec![0.0; n];
    for u in 0..n {
        let mut neigh = 0.0;
        for v in 0..n {
            if u == v {
                continue;
            }
            let c_uv = coeff[u * n + v];
            j[u * n + v] = -c_uv / 4.0;
            neigh += c_uv / 4.0;
        }
        h[u] = -(b[u] / 2.0 + neigh);
    }
    (j, h)
}

/// Pseudolikelihood local: para cada célula, softmax do campo dos vizinhos.
pub fn train_pseudolikelihood(
    energy: &mut PairwiseEnergy,
    solutions: &[SudokuBoard],
    steps: usize,
    lr: f64,
) {
    let shape = energy.shape;
    let c = shape.order;
    if solutions.is_empty() || steps == 0 {
        return;
    }
    let mut logits = vec![0.0; c];
    for step in 0..steps {
        let board = &solutions[step % solutions.len()];
        for i in 0..shape.n_cells() {
            let Some(true_d) = board.cells[i] else {
                continue;
            };
            let true_d = true_d as usize;
            // Campo local L_a = −bias_a − Σ_{j,b} K[a,b] 1[digit_j=b]
            for a in 0..c {
                let mut field = -energy.bias[a];
                for (j, cell) in board.cells.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    let Some(dj) = *cell else {
                        continue;
                    };
                    let Some(rel) = relation(shape, i, j) else {
                        continue;
                    };
                    let k = energy.k_ref(rel);
                    field -= k[a * c + (dj as usize)];
                }
                logits[a] = field;
            }
            // softmax
            let max_l = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mut sum = 0.0;
            for x in logits.iter_mut() {
                *x = (*x - max_l).exp();
                sum += *x;
            }
            for x in logits.iter_mut() {
                *x /= sum;
            }
            // ∇: (p_a − 1[a=true]) * ∂field/∂θ ; field = −bias − K·…
            // Maximizar log p_true ⇒ gradiente em bias_a: +(p_a − 1_true)
            // em K[a,dj]: +(p_a − 1_true)  (porque field diminui com K)
            for a in 0..c {
                let err = logits[a] - if a == true_d { 1.0 } else { 0.0 };
                energy.bias[a] += lr * err;
                for (j, cell) in board.cells.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    let Some(dj) = *cell else {
                        continue;
                    };
                    let Some(rel) = relation(shape, i, j) else {
                        continue;
                    };
                    let idx = a * c + (dj as usize);
                    energy.k_mut(rel)[idx] += lr * err;
                }
            }
        }
    }
}

/// Limiar de early-stop: max energia Potts das soluções + ε.
pub fn energy_stop_threshold(energy: &PairwiseEnergy, solutions: &[SudokuBoard], eps: f64) -> f64 {
    solutions
        .iter()
        .map(|b| energy.potts_energy(b))
        .fold(f64::NEG_INFINITY, f64::max)
        + eps
}

/// Resolve um puzzle com reinícios BSB; atesta com oráculo de violações.
pub fn solve_with_certificate(
    energy: &PairwiseEnergy,
    clues: &SudokuBoard,
    core: &mut BsbCore,
    restarts: usize,
    base_seed: u64,
    provenance: Provenance,
) -> (SudokuBoard, Certificate, f64) {
    let (j, h) = pairwise_to_qubo(energy, clues, 4.0, 8.0);
    let n = energy.shape.n_spins();
    let mut states = vec![1i8; n];
    let mut best_states = states.clone();
    let mut best_e = f64::INFINITY;
    let mut best_viol = u32::MAX;
    let mut coherent_hits = 0u32;

    for r in 0..restarts {
        let e = core.relax_into(&j, &h, n, base_seed.wrapping_add(r as u64), &mut states);
        let board = decode_spins(energy.shape, &states);
        let viol = violations(&board);
        if viol < best_viol || (viol == best_viol && e < best_e) {
            best_viol = viol;
            best_e = e;
            best_states.copy_from_slice(&states);
        }
        if viol == 0 {
            coherent_hits += 1;
        }
    }

    let board = decode_spins(energy.shape, &best_states);
    let viol = violations(&board);
    let gap_raw = certificate::local_gap(&j, &h, n, &best_states);
    let gap = if gap_raw.is_finite() {
        gap_raw.max(0.0)
    } else {
        0.0
    };
    let consensus = if restarts > 0 {
        coherent_hits as f64 / restarts as f64
    } else {
        0.0
    };
    let e_final = certificate::energy(&j, &h, n, &best_states);
    let cert = Certificate::new(e_final, gap, viol, consensus, provenance);
    (board, cert, best_e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sudoku_csp::shidoku_solutions;

    #[test]
    fn hand_set_params_count() {
        let shape = SudokuShape::new(4).unwrap();
        let e = PairwiseEnergy::hand_set(shape, 2.0);
        assert_eq!(e.n_params(), 3 * 16 + 4);
    }

    #[test]
    fn qubo_dims() {
        let shape = SudokuShape::new(4).unwrap();
        let e = PairwiseEnergy::hand_set(shape, 2.0);
        let clues = SudokuBoard::empty(shape);
        let (j, h) = pairwise_to_qubo(&e, &clues, 4.0, 8.0);
        let n = shape.n_spins();
        assert_eq!(j.len(), n * n);
        assert_eq!(h.len(), n);
    }

    #[test]
    fn pl_runs_without_panic() {
        let shape = SudokuShape::new(4).unwrap();
        let mut e = PairwiseEnergy::zeros(shape);
        let sols = shidoku_solutions();
        train_pseudolikelihood(&mut e, &sols, 32, 0.05);
        assert!(e.k_row.iter().any(|x| *x != 0.0));
    }
}

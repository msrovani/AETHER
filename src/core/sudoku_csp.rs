//! CSP Sudoku-like para E2d (CRAM→Logos).
//!
//! Oráculo de violações all-diff + geração sintética 4×4 (lab) e hook 9×9.
//! Feature `m1`. Ver `docs/CRAM_LOGOS.md`.

/// Ordem do tabuleiro: lado `side = order`, bloco `box = sqrt(order)`.
/// Lab usa `order = 4`; `--full` pode usar `9`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SudokuShape {
    pub order: usize,
}

impl SudokuShape {
    pub const fn new(order: usize) -> Option<Self> {
        match order {
            4 | 9 => Some(Self { order }),
            _ => None,
        }
    }

    #[inline]
    pub fn side(self) -> usize {
        self.order
    }

    #[inline]
    pub fn box_n(self) -> usize {
        match self.order {
            4 => 2,
            9 => 3,
            _ => 0,
        }
    }

    #[inline]
    pub fn n_cells(self) -> usize {
        self.order * self.order
    }

    /// Spins one-hot: uma variável por (célula, dígito).
    #[inline]
    pub fn n_spins(self) -> usize {
        self.n_cells() * self.order
    }

    #[inline]
    pub fn cell(self, row: usize, col: usize) -> usize {
        row * self.order + col
    }

    #[inline]
    pub fn spin(self, cell: usize, digit: usize) -> usize {
        cell * self.order + digit
    }

    #[inline]
    pub fn row_col(self, cell: usize) -> (usize, usize) {
        (cell / self.order, cell % self.order)
    }
}

/// Puzzle: dígitos `0..order` nas células preenchidas; `None` = vazio.
#[derive(Clone, Debug)]
pub struct SudokuBoard {
    pub shape: SudokuShape,
    /// `cells[i] = Some(d)` com `d ∈ 0..order`, ou `None`.
    pub cells: Vec<Option<u8>>,
}

impl SudokuBoard {
    pub fn empty(shape: SudokuShape) -> Self {
        Self {
            shape,
            cells: vec![None; shape.n_cells()],
        }
    }

    pub fn from_digits(shape: SudokuShape, digits: &[u8]) -> Option<Self> {
        if digits.len() != shape.n_cells() {
            return None;
        }
        let mut cells = Vec::with_capacity(digits.len());
        for &d in digits {
            if (d as usize) >= shape.order {
                return None;
            }
            cells.push(Some(d));
        }
        Some(Self { shape, cells })
    }

    /// Máscara de pistas: `true` = célula clampada.
    pub fn with_clues(solution: &Self, clue_mask: &[bool]) -> Option<Self> {
        if clue_mask.len() != solution.cells.len() {
            return None;
        }
        let mut cells = vec![None; solution.cells.len()];
        for i in 0..cells.len() {
            if clue_mask[i] {
                cells[i] = solution.cells[i];
            }
        }
        Some(Self {
            shape: solution.shape,
            cells,
        })
    }
}

/// Conta violações all-diff (pares na mesma unidade com o mesmo dígito) +
/// células vazias / dígito inválido. `0` = tabuleiro completo e factível.
pub fn violations(board: &SudokuBoard) -> u32 {
    let n = board.shape.order;
    let box_n = board.shape.box_n();
    let mut v = 0u32;

    for cell in &board.cells {
        match cell {
            None => v += 1,
            Some(d) if (*d as usize) >= n => v += 1,
            _ => {}
        }
    }

    for r in 0..n {
        v += unit_pair_violations(board, (0..n).map(|c| board.shape.cell(r, c)));
    }
    for c in 0..n {
        v += unit_pair_violations(board, (0..n).map(|r| board.shape.cell(r, c)));
    }
    for br in 0..(n / box_n) {
        for bc in 0..(n / box_n) {
            let cells = (0..box_n).flat_map(|dr| {
                (0..box_n).map(move |dc| board.shape.cell(br * box_n + dr, bc * box_n + dc))
            });
            v += unit_pair_violations(board, cells);
        }
    }
    v
}

fn unit_pair_violations(board: &SudokuBoard, cells: impl Iterator<Item = usize>) -> u32 {
    let idxs: Vec<usize> = cells.collect();
    let mut v = 0u32;
    for a in 0..idxs.len() {
        let da = match board.cells[idxs[a]] {
            Some(d) => d,
            None => continue,
        };
        for b in (a + 1)..idxs.len() {
            if board.cells[idxs[b]] == Some(da) {
                v += 1;
            }
        }
    }
    v
}

/// Decodifica spins ±1 one-hot → tabuleiro (argmax por célula).
pub fn decode_spins(shape: SudokuShape, spins: &[i8]) -> SudokuBoard {
    let n = shape.order;
    let mut cells = Vec::with_capacity(shape.n_cells());
    for cell in 0..shape.n_cells() {
        let mut best_d = 0usize;
        let mut best_s = i8::MIN;
        for d in 0..n {
            let s = spins.get(shape.spin(cell, d)).copied().unwrap_or(-1);
            if s > best_s {
                best_s = s;
                best_d = d;
            }
        }
        cells.push(Some(best_d as u8));
    }
    SudokuBoard { shape, cells }
}

/// Relação entre duas células distintas (para pesos compartilhados CRAM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellRelation {
    SameRow,
    SameCol,
    SameBox,
}

/// Classifica o par `(i,j)` de células; `None` se não compartilham unidade.
pub fn relation(shape: SudokuShape, i: usize, j: usize) -> Option<CellRelation> {
    if i == j {
        return None;
    }
    let (ri, ci) = shape.row_col(i);
    let (rj, cj) = shape.row_col(j);
    let box_n = shape.box_n();
    if ri == rj {
        return Some(CellRelation::SameRow);
    }
    if ci == cj {
        return Some(CellRelation::SameCol);
    }
    if ri / box_n == rj / box_n && ci / box_n == cj / box_n {
        return Some(CellRelation::SameBox);
    }
    None
}

/// Soluções 4×4 canônicas (dígitos 0..3) para treino/teste determinístico.
pub fn shidoku_solutions() -> Vec<SudokuBoard> {
    let shape = SudokuShape::new(4).expect("4×4");
    let boards: &[&[u8]] = &[
        &[0, 1, 2, 3, 2, 3, 0, 1, 1, 0, 3, 2, 3, 2, 1, 0],
        &[0, 1, 3, 2, 2, 3, 1, 0, 1, 0, 2, 3, 3, 2, 0, 1],
        &[1, 0, 2, 3, 2, 3, 1, 0, 0, 1, 3, 2, 3, 2, 0, 1],
        &[0, 1, 2, 3, 3, 2, 1, 0, 1, 0, 3, 2, 2, 3, 0, 1],
        &[1, 2, 0, 3, 0, 3, 1, 2, 2, 1, 3, 0, 3, 0, 2, 1],
        &[2, 3, 0, 1, 0, 1, 2, 3, 3, 2, 1, 0, 1, 0, 3, 2],
        &[3, 2, 1, 0, 1, 0, 3, 2, 2, 3, 0, 1, 0, 1, 2, 3],
        &[0, 2, 1, 3, 1, 3, 0, 2, 2, 0, 3, 1, 3, 1, 2, 0],
    ];
    boards
        .iter()
        .filter_map(|d| SudokuBoard::from_digits(shape, d))
        .filter(|b| violations(b) == 0)
        .collect()
}

/// Gera máscara de pistas determinística: mantém `keep` células da solução.
pub fn clue_mask(n_cells: usize, keep: usize, seed: u64) -> Vec<bool> {
    let keep = keep.min(n_cells);
    let mut mask = vec![false; n_cells];
    let mut order: Vec<usize> = (0..n_cells).collect();
    let mut state = seed | 1;
    for i in (1..n_cells).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state as usize) % (i + 1);
        order.swap(i, j);
    }
    for &idx in order.iter().take(keep) {
        mask[idx] = true;
    }
    mask
}

/// Instâncias 4×4 (pista, solução) com seed fixa.
pub fn shidoku_instances(n: usize, clues_per: usize, seed: u64) -> Vec<(SudokuBoard, SudokuBoard)> {
    let sols = shidoku_solutions();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let sol = &sols[i % sols.len()];
        let mask = clue_mask(sol.shape.n_cells(), clues_per, seed.wrapping_add(i as u64));
        if let Some(puzzle) = SudokuBoard::with_clues(sol, &mask) {
            out.push((puzzle, sol.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solutions_are_valid() {
        let sols = shidoku_solutions();
        assert!(sols.len() >= 4, "got {}", sols.len());
        for s in &sols {
            assert_eq!(violations(s), 0);
        }
    }

    #[test]
    fn empty_has_violations() {
        let b = SudokuBoard::empty(SudokuShape::new(4).unwrap());
        assert!(violations(&b) > 0);
    }
}

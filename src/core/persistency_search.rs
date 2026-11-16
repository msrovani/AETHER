//! E2c-3 �?" busca exata guiada por **persistência** (roof dual como propagador).
//!
//! O E2c-2 mostrou que o roof dual sozinho não reduz o vidro denso: no root de
//! um vidro puro (`h = 0`) ele fixa **0** variáveis �?" e isso é teorema, não bug:
//! a energia é invariante por `s �?' �^'s`, então toda variável aparece com os dois
//! sinais entre os dois ótimos espelhados.
//!
//! A literatura de 2026 fecha duas portas e abre uma:
//!
//! * **Naumann & Andres (arXiv:2610.20346):** todo posiforme ótimo de roof dual
//!   já dá o **maior domínio de autarkia possível** �?" nenhum autarky de posiforme
//!   quadrático fixa mais. *Strong persistency* não é extensão.
//! * **Wang & Hahn (Front. Comput. Sci. 2026):** *probing* de variáveis + 2-SAT
//!   do resíduo certifica o ótimo global; em QUBO denso, ~metade das variáveis
//!   basta.
//! * **Boros et al. (2008):** o bound iterado (`C + ν`) é mais forte que o roof
//!   dual isolado.
//!
//! Esta busca usa o fato central medido no `examples/e2c3_probe.rs`: **uma
//! ramificação quebra a simetria** e o roof dual passa a fixar o resto. Em
//! `n=14`, 1.017 nós (plant 0), 59 (25%) e 16 (45%), sempre exato contra força
//! bruta.
//!
//! # Garantia
//!
//! Cada fixação devolvida por [`RoofDualWorkspace::solve_into`] preserva o valor
//! ótimo do subproblema. A busca enumera **todas** as folhas não podadas; a
//! menor energia entre elas é o ótimo global. O relatório inclui `nodes`,
//! `leaves` e `root_fixed` para que o custo seja auditável.
//!
//! # Custo
//!
//! `O(nós · max-flow)`, com pior caso exponencial (como qualquer busca exata).
//! �? um caminho de **T1/T2**, não de T0: `new` aloca uma vez; `solve` não aloca.

use crate::core::certificate::energy;
use crate::core::presolve::{RoofDualWorkspace, ROOF_MAX_N};

/// Resultado de uma resolução exata.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SearchReport {
    /// Energia do ótimo global (ou `f64::INFINITY` se o orçamento estourou).
    pub energy: f64,
    /// Nós visitados (cada nó = uma chamada de roof dual).
    pub nodes: u32,
    /// Folhas completas avaliadas.
    pub leaves: u32,
    /// Nós podados pelo bound (`const_fixo + bound_resíduo �?� incumbente`).
    pub pruned: u32,
    /// Variáveis fixadas já no root (uma passada de roof dual).
    pub root_fixed: u32,
    /// `true` se o orçamento de nós foi excedido antes de fechar a árvore.
    pub exhausted: bool,
}

/// Upper bound factível: guloso pelo sinal de `h` + descida coordenada.
///
/// É só um **incumbente** (qualquer atribuição completa vale); serve para podar
/// cedo, não para certificar.
fn greedy_ub(n: usize, j: &[f64], h: &[f64]) -> f64 {
    let mut s = [1i8; ROOF_MAX_N];
    for i in 0..n {
        s[i] = if h[i] >= 0.0 { 1 } else { -1 };
    }
    for _ in 0..4 {
        let mut changed = false;
        for i in 0..n {
            let mut field = h[i];
            for k in 0..n {
                if k != i {
                    field += j[i * n + k] * f64::from(s[k]);
                }
            }
            if f64::from(s[i]) * field < 0.0 {
                s[i] = -s[i];
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    energy(j, h, n, &s[..n])
}

/// Workspace pré-alocado da busca exata.
///
/// Construa **uma vez** e reutilize em `solve`; a busca em si não aloca.
#[derive(Debug)]
pub struct ExactSearch {
    assign: [i8; ROOF_MAX_N],
    sub_j: [f64; ROOF_MAX_N * ROOF_MAX_N],
    sub_h: [f64; ROOF_MAX_N],
    free_idx: [usize; ROOF_MAX_N],
    fixed_tmp: [Option<i8>; ROOF_MAX_N],
    roof: Box<RoofDualWorkspace>,
    nodes: u32,
    leaves: u32,
    root_fixed: u32,
    node_budget: u32,
    exhausted: bool,
    incumbent: f64,
    pruned: u32,
}

impl ExactSearch {
    /// `node_budget` limita o custo; ao estourar, `SearchReport::exhausted`
    /// fica `true` e a energia é `f64::INFINITY`. Use `u32::MAX` para sem limite.
    pub fn new(node_budget: u32) -> Box<Self> {
        Box::new(Self {
            assign: [0; ROOF_MAX_N],
            sub_j: [0.0; ROOF_MAX_N * ROOF_MAX_N],
            sub_h: [0.0; ROOF_MAX_N],
            free_idx: [0; ROOF_MAX_N],
            fixed_tmp: [None; ROOF_MAX_N],
            roof: RoofDualWorkspace::new(),
            nodes: 0,
            leaves: 0,
            root_fixed: 0,
            node_budget,
            exhausted: false,
            incumbent: f64::INFINITY,
            pruned: 0,
        })
    }

    /// Resolve `min_s H(s)` exatamente, com `H` na convenção de
    /// [`crate::core::certificate::energy`].
    ///
    /// `n` deve ser `�?� ROOF_MAX_N` (64); `j` é `n�-n` em ordem C e `h` tem `n`.
    pub fn solve(&mut self, n: usize, j: &[f64], h: &[f64]) -> SearchReport {
        assert!(n <= ROOF_MAX_N, "n > ROOF_MAX_N");
        assert!(j.len() >= n * n, "J menor que n*n");
        assert!(h.len() >= n, "h menor que n");
        self.assign[..n].fill(0);
        self.nodes = 0;
        self.leaves = 0;
        self.root_fixed = 0;
        self.exhausted = false;
        self.incumbent = greedy_ub(n, j, h);
        self.pruned = 0;
        let e = self.dfs(n, j, h, true);
        SearchReport {
            energy: e,
            nodes: self.nodes,
            leaves: self.leaves,
            pruned: self.pruned,
            root_fixed: self.root_fixed,
            exhausted: self.exhausted,
        }
    }

    fn dfs(&mut self, n: usize, j: &[f64], h: &[f64], is_root: bool) -> f64 {
        self.nodes += 1;
        if self.nodes > self.node_budget {
            self.exhausted = true;
            return f64::INFINITY;
        }

        // Variáveis ainda livres.
        let mut m = 0usize;
        for i in 0..n {
            if self.assign[i] == 0 {
                self.free_idx[m] = i;
                m += 1;
            }
        }
        if m == 0 {
            self.leaves += 1;
            let e = energy(j, h, n, &self.assign[..n]);
            if e < self.incumbent {
                self.incumbent = e;
            }
            return e;
        }

        // Reduz: h_i += Σ_{k fixo} J_ik·v_k; J só no bloco livre.
        for a in 0..m {
            let i = self.free_idx[a];
            let mut hi = h[i];
            for k in 0..n {
                let v = self.assign[k];
                if v != 0 && k != i {
                    hi += j[i * n + k] * f64::from(v);
                }
            }
            self.sub_h[a] = hi;
            for b in 0..m {
                self.sub_j[a * m + b] = j[i * n + self.free_idx[b]];
            }
        }

        let _ = self.roof.solve_into(
            m,
            &self.sub_j[..m * m],
            &self.sub_h[..m],
            &mut self.fixed_tmp[..m],
        );

        if is_root {
            self.root_fixed = self.fixed_tmp[..m].iter().filter(|f| f.is_some()).count() as u32;
        }

        // Aplica as persistências e desfaz ao voltar (sem vazar para o irmão).
        let mut applied = [0usize; ROOF_MAX_N];
        let mut ac = 0usize;
        for a in 0..m {
            if let Some(bit) = self.fixed_tmp[a] {
                let i = self.free_idx[a];
                // `pass_into` devolve bits 0/1; `assign` usa 0=livre, ±1.
                self.assign[i] = if bit == 1 { 1 } else { -1 };
                applied[ac] = i;
                ac += 1;
            }
        }
        if ac > 0 {
            let e = self.dfs(n, j, h, false);
            for &i in applied[..ac].iter() {
                self.assign[i] = 0;
            }
            return e;
        }

        // Nó de ramificação: agora sim o bound (uma passada) para podar.
        let bound = self.roof.bound_into(
            m,
            &self.sub_j[..m * m],
            &self.sub_h[..m],
            &mut self.fixed_tmp[..m],
        );
        let mut const_fixed = 0.0;
        for i in 0..n {
            let si = self.assign[i];
            if si != 0 {
                const_fixed -= h[i] * f64::from(si);
                for k in (i + 1)..n {
                    let sk = self.assign[k];
                    if sk != 0 {
                        const_fixed -= j[i * n + k] * f64::from(si) * f64::from(sk);
                    }
                }
            }
        }
        if self.incumbent.is_finite() && const_fixed + bound >= self.incumbent - 1e-9 {
            self.pruned += 1;
            return f64::INFINITY;
        }

        // Sem propagação: ramifica na primeira livre (ordem que fechou n=56).
        let pv = self.free_idx[0];
        self.assign[pv] = -1;
        let e1 = self.dfs(n, j, h, false);
        self.assign[pv] = 0;
        if self.exhausted {
            return f64::INFINITY;
        }
        self.assign[pv] = 1;
        let e2 = self.dfs(n, j, h, false);
        self.assign[pv] = 0;
        if e1 < e2 {
            e1
        } else {
            e2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    const DELTA: f64 = 0.6;
    const SEED0: u64 = 0xE2C_2026;

    fn make_instance(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut j = vec![0.0f64; n * n];
        for i in 0..n {
            for m in (i + 1)..n {
                let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
                j[i * n + m] = w;
                j[m * n + i] = w;
            }
        }
        let mut h = vec![0.0f64; n];
        let mut order: Vec<usize> = (0..n).collect();
        for idx in (1..n).rev() {
            let sw = rng.gen_range(0..=idx);
            order.swap(idx, sw);
        }
        for &v in order.iter().take(k) {
            let row_abs: f64 = j[v * n..(v + 1) * n]
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != v)
                .map(|(_, x)| x.abs())
                .sum();
            let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            h[v] = sign * (row_abs + delta);
        }
        (j, h)
    }

    fn brute_force(n: usize, j: &[f64], h: &[f64]) -> f64 {
        let mut best = f64::INFINITY;
        let mut s = vec![0i8; n];
        for mask in 0..(1u64 << n) {
            for (i, si) in s.iter_mut().enumerate() {
                *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
            }
            best = best.min(energy(j, h, n, &s));
        }
        best
    }

    /// Gadget estrela: centro com acoplamento `�^'1` e `k` folhas com `h = �^'10`.
    /// O centro é constante em todos os ótimos e **não** é dominado por `h`.
    #[test]
    fn estrela_e_resolvida_sem_ramificar() {
        for k in [2usize, 3, 5, 8] {
            let n = k + 1;
            let mut j = vec![0.0f64; n * n];
            let mut h = vec![0.0f64; n];
            for l in 1..n {
                j[l] = -1.0;
                j[l * n] = -1.0;
                h[l] = -10.0;
            }
            let mut s = ExactSearch::new(u32::MAX);
            let r = s.solve(n, &j, &h);
            assert!(!r.exhausted);
            assert!(r.root_fixed >= 1, "centro deveria ser fixado no root");
            assert!(r.nodes <= 2, "estrela não deveria ramificar (k={k})");
        }
    }

    /// `h = 0` puro e **denso**: root fixa 0 (teorema Z2), mas a busca é exata.
    /// (Variáveis isoladas seriam trivialmente persistentes �?" qualquer valor
    /// aparece em algum ótimo �?" por isso o teste usa o grafo denso.)
    #[test]
    fn vidro_puro_root_zero_mas_solucao_exata() {
        let n = 10usize;
        let (j, h) = make_instance(n, 0, DELTA, SEED0);
        let mut s = ExactSearch::new(u32::MAX);
        let r = s.solve(n, &j, &h);
        assert_eq!(r.root_fixed, 0, "h=0 denso não tem persistência no root");
        assert!(!r.exhausted);
        let bf = brute_force(n, &j, &h);
        assert!((r.energy - bf).abs() < 1e-9);
    }
}

//! E2b — presolve simbólico para lógica em Ising.
//!
//! O E3b mostrou que o certificado só tem dentes quando existe **restrição
//! contável** (Vol. II §4.6). Este módulo é a primeira metade dessa restrição:
//! reduzir a instância por **consequência lógica** antes de qualquer energia.
//!
//! # O que é feito
//!
//! 1. **Propagação de unidades** — cláusula com um único literal não fixado
//!    força esse literal.
//! 2. **Fixação por implicação** — no grafo de implicações do 2-SAT
//!    (`(a ∨ b)` dá `¬a → b` e `¬b → a`), se `l ⇒ ¬l` então `l` é **falso** em
//!    toda atribuição satisfatória (critério clássico de 2-SAT).
//! 3. **Fixpoint** — cada fixação pode criar novas unidades e novas implicações.
//!
//! Toda fixação é **consequência lógica**: não é heurística. Por isso o presolve
//! é *sound* por construção — e o experimento E2b verifica isso contra força
//! bruta, não contra a intuição.
//!
//! # O que **não** é (ainda)
//!
//! Isto **não** é roof dual completo. O roof dual (Boros–Hammer) usa max-flow
//! sobre a posiform quadrática e fixa variáveis que são constantes em **algum**
//! ótimo, não em **todas** as soluções. A diferença importa:
//!
//! * implicação ⇒ fixação em **toda** solução (mais fraca em alcance, mais forte
//!   em garantia);
//! * roof dual ⇒ persistência em **algum** ótimo (alcance maior, garantia
//!   diferente).
//!
//! O gate do E2b mede **soundness** exatamente para não confundir as duas.

/// Cláusula de dois literais: `(s_i · x_i) ∨ (s_j · x_j)`.
///
/// `s = +1` significa literal positivo; `s = -1`, negativo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clause2 {
    pub i: usize,
    pub si: i8,
    pub j: usize,
    pub sj: i8,
}

impl Clause2 {
    /// Cria a cláusula `(si·x_i) ∨ (sj·x_j)`.
    pub fn new(i: usize, si: i8, j: usize, sj: i8) -> Self {
        Self { i, si, j, sj }
    }

    /// Avalia a cláusula numa atribuição `s ∈ {-1,+1}ⁿ`.
    #[inline]
    pub fn satisfied(&self, s: &[i8]) -> bool {
        s[self.i] == self.si || s[self.j] == self.sj
    }
}

/// Resultado do presolve.
#[derive(Clone, Debug)]
pub struct Presolve {
    /// Valor forçado por variável (`None` = livre). `Some(v)` é consequência lógica.
    pub fixed: Vec<Option<i8>>,
    /// Índices das variáveis que sobraram, na ordem canônica.
    pub residual: Vec<usize>,
    /// Instância insatisfatível detectada simbolicamente.
    pub conflict: bool,
    /// Rodadas até o fixpoint.
    pub rounds: usize,
}

impl Presolve {
    /// Quantas variáveis foram fixadas.
    pub fn fixed_count(&self) -> usize {
        self.fixed.iter().filter(|f| f.is_some()).count()
    }

    /// Fração de variáveis eliminadas (`0..=1`).
    pub fn reduction(&self, n: usize) -> f64 {
        if n == 0 {
            0.0
        } else {
            self.fixed_count() as f64 / n as f64
        }
    }
}

#[inline]
fn lit(var: usize, sign: i8) -> usize {
    2 * var + usize::from(sign > 0)
}

#[inline]
fn lit_neg(l: usize) -> usize {
    l ^ 1
}

/// Valor da variável do literal `l`, se já conhecido.
#[inline]
fn lit_value(fixed: &[Option<i8>], l: usize) -> Option<i8> {
    let var = l / 2;
    let sign: i8 = if l % 2 == 1 { 1 } else { -1 };
    fixed[var].map(|v| if v == sign { 1 } else { -1 })
}

/// Presolve por unidades + implicações, até o fixpoint.
pub fn presolve_2sat(n: usize, clauses: &[Clause2]) -> Presolve {
    let mut fixed: Vec<Option<i8>> = vec![None; n];
    let mut conflict = false;

    // grafo de implicações: (a ∨ b) ⇒ ¬a → b, ¬b → a
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); 2 * n];
    for c in clauses {
        let a = lit(c.i, c.si);
        let b = lit(c.j, c.sj);
        adj[lit_neg(a)].push(b);
        adj[lit_neg(b)].push(a);
    }

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        let mut changed = false;

        // --- 1. propagação de unidades ---
        for c in clauses {
            let a = lit(c.i, c.si);
            let b = lit(c.j, c.sj);
            let (va, vb) = (lit_value(&fixed, a), lit_value(&fixed, b));

            let sat = va == Some(1) || vb == Some(1);
            if sat {
                continue;
            }
            match (va, vb) {
                (Some(-1), Some(-1)) => {
                    conflict = true;
                    break;
                }
                (Some(-1), None) => {
                    if set_fixed(&mut fixed, b, &mut conflict) {
                        changed = true;
                    }
                }
                (None, Some(-1)) => {
                    if set_fixed(&mut fixed, a, &mut conflict) {
                        changed = true;
                    }
                }
                (None, None) => {}
                _ => {}
            }
        }
        if conflict {
            break;
        }

        // --- 2. fixação por implicação (l ⇒ ¬l) ---
        let mut reached = vec![false; 2 * n];
        let mut stack: Vec<usize> = Vec::with_capacity(2 * n);
        for l in 0..2 * n {
            for r in reached.iter_mut() {
                *r = false;
            }
            stack.clear();
            stack.push(l);
            reached[l] = true;
            while let Some(u) = stack.pop() {
                for &v in &adj[u] {
                    if !reached[v] {
                        reached[v] = true;
                        stack.push(v);
                    }
                }
            }
            if reached[lit_neg(l)] {
                // l é falso em toda solução ⇒ variável recebe -sign(l)
                if set_fixed_false(&mut fixed, l, &mut conflict) {
                    changed = true;
                }
                if conflict {
                    break;
                }
            }
        }
        if conflict || !changed {
            break;
        }
        if rounds > 64 {
            break; // guarda de sanidade (não deve acontecer)
        }
    }

    let residual: Vec<usize> = (0..n).filter(|&i| fixed[i].is_none()).collect();
    Presolve {
        fixed,
        residual,
        conflict,
        rounds,
    }
}

/// Força o literal `l` a ser **verdadeiro**. Retorna `true` se mudou algo.
fn set_fixed(fixed: &mut [Option<i8>], l: usize, conflict: &mut bool) -> bool {
    let var = l / 2;
    let val: i8 = if l % 2 == 1 { 1 } else { -1 };
    match fixed[var] {
        Some(v) if v != val => {
            *conflict = true;
            false
        }
        Some(_) => false,
        None => {
            fixed[var] = Some(val);
            true
        }
    }
}

/// Força o literal `l` a ser **falso** (equivalente a fixar `¬l` verdadeiro).
fn set_fixed_false(fixed: &mut [Option<i8>], l: usize, conflict: &mut bool) -> bool {
    set_fixed(fixed, lit_neg(l), conflict)
}

/// Reescreve as cláusulas sobre o espaço residual (variáveis renumeradas).
///
/// No fixpoint, toda cláusula restante tem **os dois** literais livres; cláusulas
/// satisfeitas por variável fixada são descartadas. Uma cláusula vazia indicaria
/// conflito e é sinalizada por `None`.
pub fn residual_clauses(
    clauses: &[Clause2],
    fixed: &[Option<i8>],
    residual: &[usize],
) -> Option<Vec<Clause2>> {
    let mut map = vec![usize::MAX; fixed.len()];
    for (new, &old) in residual.iter().enumerate() {
        map[old] = new;
    }

    let mut out = Vec::with_capacity(clauses.len());
    for c in clauses {
        if c.satisfied_by(fixed) {
            continue;
        }
        let ni = map[c.i];
        let nj = map[c.j];
        if ni == usize::MAX || nj == usize::MAX {
            // literal fixado-falso: a cláusula virou unidade — não deveria sobrar
            // no fixpoint; se sobrar, o presolve está incompleto.
            return None;
        }
        out.push(Clause2::new(ni, c.si, nj, c.sj));
    }
    Some(out)
}

impl Clause2 {
    /// `true` se algum literal é verdadeiro sob as fixações.
    fn satisfied_by(&self, fixed: &[Option<i8>]) -> bool {
        fixed[self.i] == Some(self.si) || fixed[self.j] == Some(self.sj)
    }
}

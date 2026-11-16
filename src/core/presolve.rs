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

// ---------------------------------------------------------------------------
// E2c — persistência de primeira ordem para QUBO/Ising
// ---------------------------------------------------------------------------

/// Resultado da persistência de primeira ordem.
#[derive(Clone, Debug)]
pub struct QuboPresolve {
    /// Valor ótimo-preservante por variável (`Some(v)` = existe ótimo com `s_i = v`).
    pub fixed: Vec<Option<i8>>,
    /// Variáveis que sobraram.
    pub residual: Vec<usize>,
    /// Campo efetivo após substituir as fixações (`h_j += J_ij·v_i`).
    pub h_eff: Vec<f64>,
    /// Rodadas até o fixpoint.
    pub rounds: usize,
}

impl QuboPresolve {
    /// Quantas variáveis foram fixadas.
    pub fn fixed_count(&self) -> usize {
        self.fixed.iter().filter(|f| f.is_some()).count()
    }

    /// Fração eliminada.
    pub fn reduction(&self, n: usize) -> f64 {
        if n == 0 {
            0.0
        } else {
            self.fixed_count() as f64 / n as f64
        }
    }
}

/// **Persistência de primeira ordem** (regra de dominância) para
/// `H(s) = −½·sᵀJs − hᵀs`, `J` simétrica e de diagonal zero.
///
/// Para a variável `i`, o ganho de virar o spin é
/// `Δ_i(s) = 2·s_i·(h_i + (Js)_i)`. Sobre **todas** as atribuições dos outros:
///
/// * partindo de `s_i = +1`, `max Δ_i = 2(h_i + Σ_j |J_ij|)`. Se for `< 0`,
///   virar para `−1` nunca piora ⇒ existe ótimo com `s_i = −1`;
/// * partindo de `s_i = −1`, `max Δ_i = 2(Σ_j |J_ij| − h_i)`. Se for `< 0`,
///   existe ótimo com `s_i = +1`.
///
/// Fixar assim **preserva o valor ótimo** (pode descartar ótimos alternativos,
/// nunca o valor). É rigoroso, `O(n²)` por rodada, e **não** é roof dual
/// completo: o roof dual (max-flow sobre a posiform) alcança persistências que
/// esta regra não vê.
///
/// A fixação atualiza `h_j += J_ij·v` nos remanescentes, o que pode disparar
/// novas fixações (cascata) — daí o fixpoint.
pub fn persistency_qumo(n: usize, j: &[f64], h: &[f64]) -> QuboPresolve {
    let mut fixed: Vec<Option<i8>> = vec![None; n];
    let mut h_eff: Vec<f64> = vec![0.0; n];
    let mut residual: Vec<usize> = Vec::with_capacity(n);
    let rounds =
        persistency_qumo_into(n, j, h, &mut fixed, &mut h_eff, &mut residual);
    QuboPresolve {
        fixed,
        residual,
        h_eff,
        rounds,
    }
}

/// Versão **sem alocação** de [`persistency_qumo`]: os buffers são do chamador.
///
/// `fixed` e `h_eff` precisam ter pelo menos `n` posições; `residual` é limpo e
/// reutilizado (não realoca enquanto a capacidade for `>= n`). Devolve o número
/// de rodadas até o fixpoint. Semântica idêntica à do alocante — o wrapper
/// acima é apenas a construção dos buffers.
pub fn persistency_qumo_into(
    n: usize,
    j: &[f64],
    h: &[f64],
    fixed: &mut [Option<i8>],
    h_eff: &mut [f64],
    residual: &mut Vec<usize>,
) -> usize {
    assert!(j.len() >= n * n, "persistency: J menor que n*n");
    assert!(h.len() >= n, "persistency: h menor que n");
    assert!(fixed.len() >= n, "persistency: fixed menor que n");
    assert!(h_eff.len() >= n, "persistency: h_eff menor que n");

    for f in fixed[..n].iter_mut() {
        *f = None;
    }
    h_eff[..n].copy_from_slice(&h[..n]);

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        let mut changed = false;

        for i in 0..n {
            if fixed[i].is_some() {
                continue;
            }
            let mut s_abs = 0.0;
            for k in 0..n {
                if k != i && fixed[k].is_none() {
                    s_abs += j[i * n + k].abs();
                }
            }
            let hi = h_eff[i];
            let v = if hi + s_abs < 0.0 {
                -1i8
            } else if hi - s_abs > 0.0 {
                1i8
            } else {
                continue;
            };

            fixed[i] = Some(v);
            changed = true;
            let vi = f64::from(v);
            for k in 0..n {
                if fixed[k].is_none() {
                    h_eff[k] += j[k * n + i] * vi;
                }
            }
        }

        if !changed || rounds > 64 {
            break;
        }
    }

    residual.clear();
    for (i, f) in fixed[..n].iter().enumerate() {
        if f.is_none() {
            residual.push(i);
        }
    }
    rounds
}

// ---------------------------------------------------------------------------
// E2c-2 — roof dual completo (max-flow sobre a posiform quadrática)
// ---------------------------------------------------------------------------
//
// # Por que existe
//
// `persistency_qumo` (E2c v1) só enxerga viés **explícito** em `h`: no controle
// `plant = 0%` ela reduz `0,0%` das variáveis. O roof dual de Boros–Hammer vê
// mais longe porque não raciocina sobre uma variável isolada, e sim sobre a
// **posiform** inteira:
//
// ```text
// H(x) = c₀ + Σ_i l_i x_i + Σ_{i<j} Q_ij x_i x_j        (x ∈ {0,1}ⁿ)
// ```
//
// Cada coeficiente negativo é reescrito com literais (`1 − x = x̄`, e
// `(1 − x_u x_v) = x_u x̄_v + x̄_u x_v + x̄_u x̄_v`) até sobrar apenas
// coeficientes **não negativos** — a posiform quadrática. Sobre ela:
//
// * termo `β·u·v` (β > 0) = "pague β se os dois literais forem verdadeiros"
//   ⇒ arestas de implicação `u → v̄` e `v → ū`, de capacidade β (rede
//   simétrica de implicação);
// * termo `β·u` (β > 0) = "pague β se `u` for verdadeiro" ⇒ aresta `u → ū`.
//
// O fluxo máximo nessa rede é o **roof dual**: o corte mínimo é o maior
// `c₀` que se consegue extrair por reparametrização (`β_∅` máximo). A
// alcançabilidade residual a partir da fonte `S` rotula cada literal e, pela
// comparação dos dois lados da variável (`x_i` e `x̄_i`), decide persistência:
//
// * `x_i` alcançável e `x̄_i` não ⇒ `x_i = 1` em **algum** ótimo;
// * o caso simétrico ⇒ `x_i = 0`;
// * os dois do mesmo lado ⇒ variável **não rotulada** (não se fixa nada).
//
// # Garantia
//
// A persistência é do tipo "existe ótimo com `x_i = v`" — mais fraca que a
// consequência lógica do E2b (que vale em **toda** solução), mas de alcance
// maior. O valor ótimo é preservado; o experimento verifica isso por força
// bruta em 600 instâncias.
//
// # Custo e alocação
//
// Toda a rede (nós, arestas, níveis, filas) vive em [`RoofDualWorkspace`], com
// capacidade fixa (`ROOF_MAX_N = 64`) e **zero alocação** durante a resolução.
// A única alocação acontece em [`RoofDualWorkspace::new`].

/// Maior instância que o roof dual resolve (nós fixos ⇒ buffers fixos).
pub const ROOF_MAX_N: usize = 64;

/// Nós totais da rede: `x_i`, `x̄_i`, `S`, `T`.
const ROOF_NODES: usize = 2 * ROOF_MAX_N + 2;

/// Índice da fonte.
const ROOF_S: usize = 2 * ROOF_MAX_N;

/// Índice do sumidouro.
const ROOF_T: usize = ROOF_S + 1;

/// Arestas residuais: 8 por par (2 cópias × 2 sentidos × reverso) + folga.
const ROOF_ARCS: usize = 8 * (ROOF_MAX_N * (ROOF_MAX_N - 1) / 2) + 4 * ROOF_MAX_N + 64;

/// Tolerância do fluxo (capacidades são `f64` com ordem de grandeza ~10²).
const ROOF_EPS: f64 = 1e-9;

/// Aresta residual da rede de fluxo.
#[derive(Clone, Copy, Debug)]
struct RoofArc {
    to: u16,
    next: i32,
    cap: f64,
}

/// Resultado de uma passada do roof dual.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoofDualStats {
    /// Variáveis fixadas nesta passada.
    pub fixed: usize,
    /// Valor do fluxo máximo (corte mínimo) — diagnóstico do bound.
    pub flow: f64,
}

/// Buffers pré-alocados da rede de fluxo (roof dual / max-flow).
///
/// Construa **uma vez** com [`RoofDualWorkspace::new`] e reutilize: a partir daí
/// [`RoofDualWorkspace::solve_into`] não aloca nem faz *syscall*.
#[derive(Debug)]
pub struct RoofDualWorkspace {
    first: [i32; ROOF_NODES],
    arcs: [RoofArc; ROOF_ARCS],
    arc_count: usize,
    level: [i32; ROOF_NODES],
    iter: [i32; ROOF_NODES],
    queue: [u16; ROOF_NODES],
    reach: [bool; ROOF_NODES],
    // Kosaraju (persistência fraca): ordem de término, componentes e visita.
    iter2: [i32; ROOF_NODES],
    order: [u16; ROOF_NODES],
    stack: [u16; ROOF_NODES],
    region: [i32; ROOF_NODES],
    alive: [bool; ROOF_NODES],
    visited: [bool; ROOF_NODES],
}

impl RoofDualWorkspace {
    /// Aloca a rede **uma única vez** (fora do caminho quente).
    ///
    /// A estrutura é grande (~300 KB), então vai para o heap de propósito: um
    /// `RoofDualWorkspace` na pilha arriscaria estourar a pilha de teste.
    pub fn new() -> Box<Self> {
        Box::new(Self {
            first: [-1; ROOF_NODES],
            arcs: [RoofArc {
                to: 0,
                next: -1,
                cap: 0.0,
            }; ROOF_ARCS],
            arc_count: 0,
            level: [0; ROOF_NODES],
            iter: [0; ROOF_NODES],
            queue: [0; ROOF_NODES],
            reach: [false; ROOF_NODES],
            iter2: [0; ROOF_NODES],
            order: [0; ROOF_NODES],
            stack: [0; ROOF_NODES],
            region: [0; ROOF_NODES],
            alive: [false; ROOF_NODES],
            visited: [false; ROOF_NODES],
        })
    }

    /// Limpa a rede (mantém a memória).
    fn reset(&mut self) {
        for f in self.first.iter_mut() {
            *f = -1;
        }
        self.arc_count = 0;
    }

    #[inline]
    fn add_arc(&mut self, from: usize, to: usize, cap: f64) {
        if cap <= 0.0 || self.arc_count + 2 > ROOF_ARCS {
            return;
        }
        let idx = self.arc_count;
        self.arcs[idx] = RoofArc {
            to: to as u16,
            next: self.first[from],
            cap,
        };
        self.first[from] = idx as i32;
        self.arcs[idx + 1] = RoofArc {
            to: from as u16,
            next: self.first[to],
            cap: 0.0,
        };
        self.first[to] = (idx + 1) as i32;
        self.arc_count = idx + 2;
    }

    /// Aresta com capacidade nos dois sentidos (como na rede de implicação).
    #[inline]
    fn add_edge(&mut self, from: usize, to: usize, fwd: f64, rev: f64) {
        self.add_arc(from, to, fwd);
        self.add_arc(to, from, rev);
    }

    /// `tr_cap` de QPBO: positivo ⇒ `S → nó`; negativo ⇒ `nó → T`.
    #[inline]
    fn add_terminal(&mut self, node: usize, tr: f64) {
        if tr > 0.0 {
            self.add_arc(ROOF_S, node, tr);
        } else if tr < 0.0 {
            self.add_arc(node, ROOF_T, -tr);
        }
    }

    fn bfs(&mut self, s: usize, t: usize) -> bool {
        for l in self.level.iter_mut() {
            *l = -1;
        }
        let mut head = 0usize;
        let mut tail = 0usize;
        self.level[s] = 0;
        self.queue[tail] = s as u16;
        tail += 1;
        while head < tail {
            let v = self.queue[head] as usize;
            head += 1;
            let mut i = self.first[v];
            while i >= 0 {
                let a = self.arcs[i as usize];
                let to = a.to as usize;
                if a.cap > ROOF_EPS && self.level[to] < 0 {
                    self.level[to] = self.level[v] + 1;
                    self.queue[tail] = a.to;
                    tail += 1;
                }
                i = a.next;
            }
        }
        self.level[t] >= 0
    }

    fn dfs(&mut self, v: usize, t: usize, limit: f64) -> f64 {
        if v == t {
            return limit;
        }
        let mut i = self.iter[v];
        while i >= 0 {
            let idx = i as usize;
            let a = self.arcs[idx];
            let to = a.to as usize;
            if a.cap > ROOF_EPS && self.level[to] == self.level[v] + 1 {
                let pushed = self.dfs(to, t, if limit < a.cap { limit } else { a.cap });
                if pushed > ROOF_EPS {
                    self.arcs[idx].cap -= pushed;
                    self.arcs[idx ^ 1].cap += pushed;
                    return pushed;
                }
            }
            i = a.next;
            self.iter[v] = i;
        }
        self.level[v] = -1;
        0.0
    }

    /// Fluxo máximo (Dinic determinístico) na rede corrente.
    fn max_flow(&mut self) -> f64 {
        let mut flow = 0.0f64;
        while self.bfs(ROOF_S, ROOF_T) {
            for (i, f) in self.first.iter().enumerate() {
                self.iter[i] = *f;
            }
            loop {
                let pushed = self.dfs(ROOF_S, ROOF_T, f64::INFINITY);
                if pushed <= ROOF_EPS {
                    break;
                }
                flow += pushed;
            }
        }
        flow
    }

    /// Alcançabilidade residual a partir de `S`.
    fn residual_from_source(&mut self) {
        for r in self.reach.iter_mut() {
            *r = false;
        }
        let mut head = 0usize;
        let mut tail = 0usize;
        self.reach[ROOF_S] = true;
        self.queue[tail] = ROOF_S as u16;
        tail += 1;
        while head < tail {
            let v = self.queue[head] as usize;
            head += 1;
            let mut i = self.first[v];
            while i >= 0 {
                let a = self.arcs[i as usize];
                let to = a.to as usize;
                if a.cap > ROOF_EPS && !self.reach[to] {
                    self.reach[to] = true;
                    self.queue[tail] = a.to;
                    tail += 1;
                }
                i = a.next;
            }
        }
    }

    /// Componentes fortemente conexas do grafo residual (Kosaraju).
    ///
    /// Participam apenas os nós de variáveis **ambíguas** (`alive`). Duas
    /// passadas: ordem de término no grafo residual, depois DFS no grafo
    /// reverso na ordem inversa. Numera as componentes em ordem topológica.
    fn scc_regions(&mut self, n: usize) {
        let nodes = 2 * n;
        for r in self.region.iter_mut() {
            *r = 0;
        }
        for v in self.visited.iter_mut() {
            *v = false;
        }

        // --- 1ª passada: ordem de término sobre o residual -------------------
        let mut m = 0usize;
        for s in 0..nodes {
            if !self.alive[s] || self.visited[s] {
                continue;
            }
            self.visited[s] = true;
            self.iter2[s] = self.first[s];
            self.stack[0] = s as u16;
            let mut sp = 1usize;
            while sp > 0 {
                let v = self.stack[sp - 1] as usize;
                let mut a = self.iter2[v];
                while a >= 0 {
                    let ia = a as usize;
                    let to = self.arcs[ia].to as usize;
                    if self.arcs[ia].cap > ROOF_EPS && self.alive[to] && !self.visited[to] {
                        break;
                    }
                    a = self.arcs[ia].next;
                }
                self.iter2[v] = a;
                if a < 0 {
                    self.order[m] = v as u16;
                    m += 1;
                    sp -= 1;
                } else {
                    let to = self.arcs[a as usize].to as usize;
                    self.visited[to] = true;
                    self.iter2[to] = self.first[to];
                    self.stack[sp] = to as u16;
                    sp += 1;
                }
            }
        }

        // --- 2ª passada: grafo reverso, ordem inversa ------------------------
        let mut comp = 0i32;
        for idx in (0..m).rev() {
            let s = self.order[idx] as usize;
            if !self.alive[s] || self.region[s] > 0 {
                continue;
            }
            comp += 1;
            self.region[s] = comp;
            self.iter2[s] = self.first[s];
            self.stack[0] = s as u16;
            let mut sp = 1usize;
            while sp > 0 {
                let v = self.stack[sp - 1] as usize;
                let mut a = self.iter2[v];
                while a >= 0 {
                    let ia = a as usize;
                    let to = self.arcs[ia].to as usize;
                    // aresta reversa residual (to → v) ⇒ v → to em Gᵀ
                    if self.arcs[ia ^ 1].cap > ROOF_EPS && self.alive[to] && self.region[to] <= 0 {
                        break;
                    }
                    a = self.arcs[ia].next;
                }
                self.iter2[v] = a;
                if a < 0 {
                    sp -= 1;
                } else {
                    let to = self.arcs[a as usize].to as usize;
                    self.region[to] = comp;
                    self.iter2[to] = self.first[to];
                    self.stack[sp] = to as u16;
                    sp += 1;
                }
            }
        }
    }

    /// Uma passada do roof dual sobre `n` variáveis (`j` plano `n×n`, `h`).
    ///
    /// `fixed` recebe `Some(0|1)` para as variáveis persistentes. Devolve o
    /// fluxo máximo (corte mínimo) da rede.
    pub fn pass_into(&mut self, n: usize, j: &[f64], h: &[f64], fixed: &mut [Option<i8>]) -> f64 {
        self.reset();
        for f in fixed.iter_mut() {
            *f = None;
        }
        if n == 0 || n > ROOF_MAX_N {
            return 0.0;
        }

        // --- 1. Ising → QUBO (`H = c₀ + Σ l_i x_i + Σ Q_ij x_i x_j`) --------
        // H(s) = −½·sᵀJs − hᵀs com s = 2x−1:
        //   c₀ = −Σ_{i<j} J_ij + Σ_i h_i
        //   l_i = 2·(Σ_{j≠i} J_ij − h_i)
        //   Q_ij = −4·J_ij
        let mut tr = [0.0f64; 2 * ROOF_MAX_N];
        for i in 0..n {
            let mut row = 0.0f64;
            for k in 0..n {
                if k != i {
                    row += j[i * n + k];
                }
            }
            tr[i] += 2.0 * (row - h[i]); // termo linear `l_i·x_i`
        }
        // mate recebe o simétrico (rede dobrada, QPBO §"second stage").
        for i in 0..n {
            tr[ROOF_MAX_N + i] = -tr[i];
        }

        for i in 0..n {
            for k in (i + 1)..n {
                let q = -4.0 * j[i * n + k];
                if q == 0.0 {
                    continue;
                }
                // Tabela do termo: E00 = E01 = E10 = 0, E11 = q.
                let (ci, cj, cij, cji): (f64, f64, f64, f64);
                if 0.0 >= q {
                    // submodular: E01 + E10 ≥ E00 + E11
                    let (a, b, c, d) = (0.0, 0.0, 0.0, q);
                    let w = roof_weights(a, b, c, d);
                    ci = w.0;
                    cj = w.1;
                    cij = w.2;
                    cji = w.3;
                    tr[i] += ci;
                    tr[ROOF_MAX_N + i] -= ci;
                    tr[k] += cj;
                    tr[ROOF_MAX_N + k] -= cj;
                    // cópia 0: x_i ↔ x_k ; cópia 1: x̄_i ↔ x̄_k
                    // (na cópia 1 as capacidades trocam de lado: espelho QPBO)
                    self.add_edge(i, k, cij, cji);
                    self.add_edge(ROOF_MAX_N + i, ROOF_MAX_N + k, cji, cij);
                } else {
                    // supermodular: usa o literal complementar de `k`
                    let (a, b, c, d) = (0.0, 0.0, q, 0.0);
                    let w = roof_weights(a, b, c, d);
                    ci = w.0;
                    cj = w.1;
                    cij = w.2;
                    cji = w.3;
                    tr[i] += ci;
                    tr[ROOF_MAX_N + i] -= ci;
                    tr[ROOF_MAX_N + k] += cj;
                    tr[k] -= cj;
                    // cópia 0: x_i ↔ x̄_k ; cópia 1: x_k ↔ x̄_i
                    self.add_edge(i, ROOF_MAX_N + k, cij, cji);
                    self.add_edge(k, ROOF_MAX_N + i, cij, cji);
                }
            }
        }

        for i in 0..n {
            self.add_terminal(i, tr[i]);
            self.add_terminal(ROOF_MAX_N + i, tr[ROOF_MAX_N + i]);
        }

        // --- 2. Fluxo máximo e rótulos pela alcançabilidade residual ---------
        let flow = self.max_flow();
        self.residual_from_source();

        // Rótulo por segmento (persistência forte do roof dual).
        for i in 0..ROOF_NODES {
            self.alive[i] = false;
        }
        let mut ambig = [false; ROOF_MAX_N];
        for i in 0..n {
            // `what_segment`: 0 = lado da fonte, 1 = lado do sumidouro; o rótulo
            // é o valor de `x_i`. Ambíguo quando os dois literais caem no mesmo
            // lado (é o "unknown" do QPBO).
            let seg0: i8 = if self.reach[i] { 0 } else { 1 };
            let seg1: i8 = if self.reach[ROOF_MAX_N + i] { 0 } else { 1 };
            if seg0 != seg1 {
                fixed[i] = Some(seg0);
            } else {
                ambig[i] = true;
                self.alive[i] = true;
                self.alive[ROOF_MAX_N + i] = true;
            }
        }

        // Refinamento por SCC (persistência fraca): em QPBO é o passo
        // `ComputeWeakPersistencies`. Só decide o que a segmentação deixou
        // ambíguo; a comparação é entre as componentes dos dois literais.
        if ambig.iter().any(|a| *a) {
            self.scc_regions(n);
            for i in 0..n {
                if !ambig[i] {
                    continue;
                }
                let r0 = self.region[i];
                let r1 = self.region[ROOF_MAX_N + i];
                if r0 > r1 {
                    fixed[i] = Some(0);
                } else if r0 < r1 {
                    fixed[i] = Some(1);
                }
            }
        }
        flow
    }

    /// Roof dual **com cascata**: fixa, repara o Hamiltoniano e repete no
    /// resíduo até o fixpoint (ou `ROOF_MAX_ROUNDS`).
    ///
    /// Todas as fixações vêm de uma única sequência de reduções: cada rodada é
    /// um roof dual da instância já reduzida, então a persistência continua
    /// valendo para o ótimo **original**.
    pub fn solve_into(&mut self, n: usize, j: &[f64], h: &[f64], fixed: &mut [Option<i8>]) -> usize {
        for f in fixed.iter_mut() {
            *f = None;
        }
        if n == 0 || n > ROOF_MAX_N {
            return 0;
        }

        let mut count = 0usize;
        let mut tmp = [None::<i8>; ROOF_MAX_N];
        let mut red_orig = [0usize; ROOF_MAX_N];
        let mut red_j = [0.0f64; ROOF_MAX_N * ROOF_MAX_N];
        let mut red_h = [0.0f64; ROOF_MAX_N];

        for _round in 0..ROOF_MAX_ROUNDS {
            // Lista de variáveis livres e campo efetivo (as fixações viram
            // `h_j += J_ji·s_i`, exatamente como em `persistency_qumo`).
            let mut m = 0usize;
            for i in 0..n {
                if fixed[i].is_none() {
                    red_orig[m] = i;
                    red_h[m] = h[i];
                    m += 1;
                }
            }
            if m == 0 {
                break;
            }
            for a in 0..m {
                let ia = red_orig[a];
                for i in 0..n {
                    if let Some(v) = fixed[i] {
                        red_h[a] += j[ia * n + i] * f64::from(v);
                    }
                }
                for b in 0..m {
                    red_j[a * m + b] = j[ia * n + red_orig[b]];
                }
            }

            let flow = self.pass_into(m, &red_j[..m * m], &red_h[..m], &mut tmp[..m]);
            let _ = flow;

            let mut changed = 0usize;
            for a in 0..m {
                if let Some(x) = tmp[a] {
                    let v: i8 = if x == 1 { 1 } else { -1 };
                    fixed[red_orig[a]] = Some(v);
                    changed += 1;
                }
            }
            if changed == 0 {
                break;
            }
            count += changed;
        }
        count
    }
}

/// Rodadas da cascata do roof dual.
const ROOF_MAX_ROUNDS: usize = 8;

/// Reparametrização de um termo par (QPBO `ComputeWeights`).
///
/// Decompõe `E = [[A,B],[C,D]]` como `unária_i + unária_j + aresta dupla` com
/// coeficientes **não negativos** — é a posiform quadrática do termo. Devolve
/// `(ci, cj, cij, cji)`.
fn roof_weights(a: f64, b: f64, c: f64, d: f64) -> (f64, f64, f64, f64) {
    let mut ci = d - a;
    let b2 = b - a;
    let c2 = c - d;
    let (cj, cij, cji);
    if b2 < 0.0 {
        ci += -b2;
        cj = b2;
        cji = b2 + c2;
        cij = 0.0;
    } else if c2 < 0.0 {
        ci += c2;
        cj = -c2;
        cij = b2 + c2;
        cji = 0.0;
    } else {
        cj = 0.0;
        cij = b2;
        cji = c2;
    }
    (ci, cj, cij.max(0.0), cji.max(0.0))
}

/// **Roof dual completo** para `H(s) = −½·sᵀJs − hᵀs`.
///
/// Versão de conveniência de [`RoofDualWorkspace::solve_into`]: aloca o
/// workspace (uma vez, fora do caminho quente) e devolve o mesmo tipo do E2c v1
/// para que os dois possam ser comparados lado a lado.
///
/// Ordem (fixpoint do presolve):
/// 1. dominância (E2c v1) — garante que o roof dual **contém** o que a
///    dominância já via (monotonicidade do alcance);
/// 2. roof dual com cascata no resíduo (max-flow);
/// 3. dominância de novo no resíduo (persistência fraca remanescente).
///
/// Cada etapa preserva o valor ótimo, então a composição também preserva.
pub fn roof_dual_qumo(n: usize, j: &[f64], h: &[f64]) -> QuboPresolve {
    if n == 0 {
        return QuboPresolve {
            fixed: Vec::new(),
            residual: Vec::new(),
            h_eff: Vec::new(),
            rounds: 0,
        };
    }

    // --- 1. dominância (E2c v1) -------------------------------------------
    let dom = persistency_qumo(n, j, h);
    let mut fixed = dom.fixed.clone();

    // --- 2. roof dual no resíduo ------------------------------------------
    let mut residual: Vec<usize> = (0..n).filter(|&i| fixed[i].is_none()).collect();
    if !residual.is_empty() && residual.len() <= ROOF_MAX_N {
        let m = residual.len();
        let mut sub_j = vec![0.0f64; m * m];
        let mut sub_h = vec![0.0f64; m];
        for (a, &ia) in residual.iter().enumerate() {
            sub_h[a] = dom.h_eff[ia];
            for (b, &ib) in residual.iter().enumerate() {
                sub_j[a * m + b] = j[ia * n + ib];
            }
        }
        let mut ws = RoofDualWorkspace::new();
        let mut sub_fixed = vec![None::<i8>; m];
        ws.solve_into(m, &sub_j, &sub_h, &mut sub_fixed);
        for (a, &ia) in residual.iter().enumerate() {
            if let Some(v) = sub_fixed[a] {
                fixed[ia] = Some(v);
            }
        }
    }

    // --- 3. dominância no resíduo final -----------------------------------
    residual = (0..n).filter(|&i| fixed[i].is_none()).collect();
    if !residual.is_empty() {
        let m = residual.len();
        let mut sub_j = vec![0.0f64; m * m];
        let mut sub_h = vec![0.0f64; m];
        for (a, &ia) in residual.iter().enumerate() {
            sub_h[a] = h[ia];
            for k in 0..n {
                if let Some(v) = fixed[k] {
                    sub_h[a] += j[ia * n + k] * f64::from(v);
                }
            }
            for (b, &ib) in residual.iter().enumerate() {
                sub_j[a * m + b] = j[ia * n + ib];
            }
        }
        let weak = persistency_qumo(m, &sub_j, &sub_h);
        for (a, &ia) in residual.iter().enumerate() {
            if let Some(v) = weak.fixed[a] {
                fixed[ia] = Some(v);
            }
        }
    }

    // --- h_eff final, já com todas as fixações ----------------------------
    let mut h_final: Vec<f64> = h.to_vec();
    for i in 0..n {
        if let Some(v) = fixed[i] {
            for k in 0..n {
                if fixed[k].is_none() {
                    h_final[k] += j[k * n + i] * f64::from(v);
                }
            }
        }
    }

    let residual: Vec<usize> = (0..n).filter(|&i| fixed[i].is_none()).collect();
    QuboPresolve {
        fixed,
        residual,
        h_eff: h_final,
        rounds: 1 + dom.rounds,
    }
}

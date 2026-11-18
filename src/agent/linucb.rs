//! LinUCB contextual com fallback para o chamador (EXP3 continua o default).
//!
//! Dimensão pequena e inversa explícita: sem dependência nova e sem gradiente.
//! O roteador padrão não usa este tipo até o host ligar o caminho.

/// Braço linear com intervalo de confiança.
#[derive(Clone, Debug)]
pub struct LinUcb {
    d: usize,
    arms: usize,
    alpha: f64,
    /// `A` por braço, linha-maior `d×d`. Começa na identidade.
    a: Vec<Vec<f64>>,
    /// `b` por braço.
    b: Vec<Vec<f64>>,
}

impl LinUcb {
    /// `arms` braços em dimensão `d`. `alpha` é a largura do intervalo.
    pub fn new(arms: usize, d: usize, alpha: f64) -> Self {
        let eye = identity(d);
        Self {
            d,
            arms,
            alpha,
            a: vec![eye; arms],
            b: vec![vec![0.0; d]; arms],
        }
    }

    /// Escolhe o braço de maior UCB para o contexto `x`.
    pub fn choose(&self, x: &[f64]) -> usize {
        let mut best = 0usize;
        let mut best_score = f64::MIN;
        for arm in 0..self.arms {
            let theta = solve(&self.a[arm], &self.b[arm]);
            let mean = dot(&theta, x);
            let conf = self.alpha * x_a_inv_x(&self.a[arm], x).sqrt();
            let score = mean + conf;
            if score > best_score {
                best_score = score;
                best = arm;
            }
        }
        best
    }

    /// Atualiza o braço com a recompensa observada.
    pub fn update(&mut self, arm: usize, x: &[f64], reward: f64) {
        let a = &mut self.a[arm];
        let b = &mut self.b[arm];
        for i in 0..self.d {
            b[i] += reward * x[i];
            for j in 0..self.d {
                a[i * self.d + j] += x[i] * x[j];
            }
        }
    }

    /// Média prevista do braço (para ECE).
    pub fn predict(&self, arm: usize, x: &[f64]) -> f64 {
        let theta = solve(&self.a[arm], &self.b[arm]);
        dot(&theta, x).clamp(0.0, 1.0)
    }
}

fn identity(d: usize) -> Vec<f64> {
    let mut m = vec![0.0; d * d];
    for i in 0..d {
        m[i * d + i] = 1.0;
    }
    m
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn x_a_inv_x(a: &[f64], x: &[f64]) -> f64 {
    let inv = invert(a);
    let mut ax = vec![0.0; x.len()];
    let d = x.len();
    for i in 0..d {
        for j in 0..d {
            ax[i] += inv[i * d + j] * x[j];
        }
    }
    dot(x, &ax).max(0.0)
}

fn solve(a: &[f64], b: &[f64]) -> Vec<f64> {
    let inv = invert(a);
    let d = b.len();
    let mut out = vec![0.0; d];
    for i in 0..d {
        for j in 0..d {
            out[i] += inv[i * d + j] * b[j];
        }
    }
    out
}

/// Inversa por Gauss–Jordan. `d` é pequeno (o gate usa 4).
fn invert(a: &[f64]) -> Vec<f64> {
    let d = (a.len() as f64).sqrt() as usize;
    let mut m = vec![0.0; d * d * 2];
    for i in 0..d {
        for j in 0..d {
            m[i * d * 2 + j] = a[i * d + j];
        }
        m[i * d * 2 + d + i] = 1.0;
    }
    for col in 0..d {
        let mut pivot = col;
        let mut best = m[col * d * 2 + col].abs();
        for r in (col + 1)..d {
            let v = m[r * d * 2 + col].abs();
            if v > best {
                best = v;
                pivot = r;
            }
        }
        if pivot != col {
            for c in 0..(d * 2) {
                m.swap(col * d * 2 + c, pivot * d * 2 + c);
            }
        }
        let div = m[col * d * 2 + col];
        if div.abs() < 1e-12 {
            continue;
        }
        for c in 0..(d * 2) {
            m[col * d * 2 + c] /= div;
        }
        for r in 0..d {
            if r == col {
                continue;
            }
            let f = m[r * d * 2 + col];
            for c in 0..(d * 2) {
                let v = m[col * d * 2 + c];
                m[r * d * 2 + c] -= f * v;
            }
        }
    }
    let mut inv = vec![0.0; d * d];
    for i in 0..d {
        for j in 0..d {
            inv[i * d + j] = m[i * d * 2 + d + j];
        }
    }
    inv
}

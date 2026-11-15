//! E3b — Certificado de **coerência** (não de verdade).
//!
//! Um estado emergente do solver não é uma resposta: é uma resposta **com
//! atestado**. Este módulo define o atestado e a política de decisão que o
//! consome.
//!
//! ```text
//! Certificate { energy, gap, violations, consensus, provenance }
//!                 │      │        │          │          │
//!                 │      │        │          │          └─ de onde veio
//!                 │      │        │          └─ quantas reinicializações concordam
//!                 │      │        └─ restrições violadas (0 = factível)
//!                 │      └─ custo do melhor flip simples (0 = não é mínimo local)
//!                 └─ H(s*) na convenção do solver
//! ```
//!
//! # O que o certificado garante — e o que **não** garante
//!
//! Garante **coerência**: o estado é mínimo local (`gap > 0`), respeita as
//! restrições (`violations == 0`) e é reprodutível. **Não** garante verdade: a
//! geometria/energia não sabe se o mundo é assim. Chamar isto de "anti-alucinação"
//! seria o erro que o dossiê Vol. II §4.2 denuncia.
//!
//! # Matriz 2D (Vol. III §6)
//!
//! O atestado é **físico** (`gap`); a autoridade é **social** (HITL, 0..=255).
//! Nenhum dos dois, sozinho, evita erro — juntos, separam quatro casos:
//!
//! ```text
//!                     gap ALTO              gap BAIXO
//!   authority ALTA    ATUA + CITA           CITA, NÃO ATUA
//!   authority BAIXA   PROPÕE               ABSTÉM
//! ```

/// Origem de um conhecimento. Não é decoração: entra na resposta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Medido por um transdutor físico (sensor).
    Observed,
    /// Derivado por regra/energia a partir de outra memória.
    Inferred,
    /// Importado de um espaço semântico externo (ex.: transplantação JL).
    Transplanted,
    /// Definido por contrato/axioma do sistema.
    Axiomatic,
    /// Aprovado por humano (HITL).
    Human,
}

impl Provenance {
    /// Código estável de 1 byte (para logs binários e o bridge do SGDB).
    pub fn code(self) -> u8 {
        match self {
            Provenance::Observed => 0,
            Provenance::Inferred => 1,
            Provenance::Transplanted => 2,
            Provenance::Axiomatic => 3,
            Provenance::Human => 4,
        }
    }

    /// Rótulo estável (só para exibição; não atravessa o sinal).
    pub fn label(self) -> &'static str {
        match self {
            Provenance::Observed => "observed",
            Provenance::Inferred => "inferred",
            Provenance::Transplanted => "transplanted",
            Provenance::Axiomatic => "axiomatic",
            Provenance::Human => "human",
        }
    }
}

/// Ação recomendada pela política de decisão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Confiável e inequívoco: usar e citar a proveniência.
    Act,
    /// Confiável, mas ambíguo aqui: citar sem agir.
    Cite,
    /// Coerente sem fonte forte: propor como hipótese.
    Propose,
    /// Ambíguo e sem fonte: não responder.
    Abstain,
}

/// Atestado que acompanha toda resposta do motor.
#[derive(Clone, Copy, Debug)]
pub struct Certificate {
    /// `H(s*)` na convenção do solver (`-0.5 sᵀJs - hᵀs`).
    pub energy: f64,
    /// Custo do melhor flip simples, `min_i ΔH_i ≥ 0`. `0` significa que `s*`
    /// **não** é mínimo local (há flip que reduz a energia) — atestado inválido.
    pub gap: f64,
    /// Restrições violadas (0 = factível).
    pub violations: u32,
    /// Fração de reinicializações que chegaram à mesma energia (`0..=1`).
    pub consensus: f64,
    /// Origem do conhecimento que sustentou a resposta.
    pub provenance: Provenance,
}

impl Certificate {
    /// Constrói um atestado, normalizando `gap` para `≥ 0`.
    pub fn new(
        energy: f64,
        gap: f64,
        violations: u32,
        consensus: f64,
        provenance: Provenance,
    ) -> Self {
        Self {
            energy,
            gap: if gap.is_finite() { gap.max(0.0) } else { 0.0 },
            violations,
            consensus: consensus.clamp(0.0, 1.0),
            provenance,
        }
    }

    /// O atestado está válido? (mínimo local, factível, finito)
    pub fn is_coherent(&self) -> bool {
        self.energy.is_finite() && self.gap > 0.0 && self.violations == 0
    }

    /// Confiança em `0..=1`, combinando gap normalizado e consenso.
    ///
    /// `gap_scale` é a escala natural do problema (ex.: `|energy|` médio ou o
    /// gap mediano da calibração); gaps muito acima dela saturam em 1.
    pub fn confidence(&self, gap_scale: f64) -> f64 {
        if !self.is_coherent() {
            return 0.0;
        }
        let g = if gap_scale > 0.0 {
            (self.gap / gap_scale).min(1.0)
        } else {
            1.0
        };
        // média geométrica: exige os dois sinais, não permite um compensar o outro
        (g * self.consensus).sqrt()
    }

    /// Matriz 2D de garantia (Vol. III §6).
    ///
    /// `authority ≥ 128` conta como garantia social (HITL). `gap_min` é o piso
    /// físico de inequivocidade, calibrado por experimento.
    pub fn decide(&self, authority: u8, gap_min: f64) -> Decision {
        let social = authority >= 128;
        let physical = self.is_coherent() && self.gap >= gap_min;
        match (social, physical) {
            (true, true) => Decision::Act,
            (true, false) => Decision::Cite,
            (false, true) => Decision::Propose,
            (false, false) => Decision::Abstain,
        }
    }
}

/// `H(s) = -0.5 · Σ_i s_i (J s)_i - Σ_i h_i s_i`, com `J` em `[N, N]` row-major.
///
/// Mesma convenção e mesma ordem de soma de [`crate::core::thermodynamic_solver`].
pub fn energy(j: &[f64], h: &[f64], n: usize, s: &[i8]) -> f64 {
    let mut acc = 0.0;
    for i in 0..n {
        let row = &j[i * n..(i + 1) * n];
        let mut g = 0.0;
        for (jv, sv) in row.iter().zip(s.iter()) {
            g += *jv * f64::from(*sv);
        }
        acc -= 0.5 * f64::from(s[i]) * g;
    }
    for (hi, si) in h.iter().zip(s.iter()) {
        acc -= *hi * f64::from(*si);
    }
    acc
}

/// `g = J s` em `O(N²)`.
pub fn field(j: &[f64], n: usize, s: &[i8]) -> Vec<f64> {
    let mut g = vec![0.0; n];
    for i in 0..n {
        let row = &j[i * n..(i + 1) * n];
        let mut acc = 0.0;
        for (jv, sv) in row.iter().zip(s.iter()) {
            acc += *jv * f64::from(*sv);
        }
        g[i] = acc;
    }
    g
}

/// Gap local: `min_i ΔH_i` com `ΔH_i = 2 s_i (g_i + h_i)`.
///
/// `0` significa que existe flip não-improving? Não: se algum `ΔH_i < 0`, há
/// flip que **reduz** `H` e o estado não é mínimo local — nesse caso o gap é
/// devolvido negativo e o chamador deve rejeitar o atestado
/// ([`Certificate::is_coherent`] normaliza para 0 e invalida).
pub fn local_gap(j: &[f64], h: &[f64], n: usize, s: &[i8]) -> f64 {
    let mut g = field(j, n, s);
    local_gap_with(j, h, n, s, &mut g)
}

/// Versão sem alocação: `g` é scratch pré-alocado de tamanho `n` e é
/// sobrescrito com `J s`.
pub fn local_gap_with(j: &[f64], h: &[f64], n: usize, s: &[i8], g: &mut [f64]) -> f64 {
    for i in 0..n {
        let row = &j[i * n..(i + 1) * n];
        let mut acc = 0.0;
        for (jv, sv) in row.iter().zip(s.iter()) {
            acc += *jv * f64::from(*sv);
        }
        g[i] = acc;
    }
    let mut best = f64::INFINITY;
    for i in 0..n {
        let delta = 2.0 * f64::from(s[i]) * (g[i] + h[i]);
        if delta < best {
            best = delta;
        }
    }
    best
}

/// Limite inferior **rigoroso** para `H`.
///
/// Para qualquer `x` com `‖x‖² = N`, vale `xᵀJx ≤ λ_max·N`; como `s ∈ {±1}ᴺ`
/// satisfaz `‖s‖² = N`, segue
/// `H(s) = −½ sᵀJs − hᵀs ≥ −½·λ_max·N − ‖h‖₁`.
///
/// `λ_max` é limitado **por cima** com Gershgorin sobre `|J|`:
/// `λ_max(J) ≤ ρ(|J|) ≤ max_i Σ_j |J_ij|`. É um limite válido (nunca abaixo do
/// real), portanto `E − LB ≥ 0` é um *optimality gap* verificável — ainda que
/// frouxo para vidros de spin, e é justamente por isso que ele **não** prediz
/// acerto (ver `examples/e3b_certificate.rs`).
pub fn spectral_lower_bound(j: &[f64], h: &[f64], n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let mut row_max = 0.0f64;
    for i in 0..n {
        let row = &j[i * n..(i + 1) * n];
        let s: f64 = row.iter().map(|x| x.abs()).sum();
        if s > row_max {
            row_max = s;
        }
    }
    let h1: f64 = h.iter().map(|x| x.abs()).sum();
    -0.5 * row_max * n as f64 - h1
}

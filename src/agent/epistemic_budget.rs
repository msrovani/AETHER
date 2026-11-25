//! R10/A10 (Sprint A) — orçamento de sondagem epístéo (G-ABD, A10).
//!
//! Estado processo-local (statics atômicos; nenhum efeito fora do processo):
//! `try_probe()` devolve `false` quando o orçamento esgota; o planner usa
//! [`has_probe_budget`] para cortar candidatos de sonda; [`reset_budget`]
//! reabre o orçamento (turno novo). O default é pequeno (16) e cresce apenas
//! via [`set_budget`] (gate Sprint B p/ phenomeno de anomalia).

use std::sync::atomic::{AtomicU64, Ordering};

pub const TOOL_PROBE_BUDGET_DEFAULT: u64 = 16;

static BUDGET: AtomicU64 = AtomicU64::new(TOOL_PROBE_BUDGET_DEFAULT);
static USED: AtomicU64 = AtomicU64::new(0);

/// Define o orçamento de sondas (zera a contabilidade).
pub fn set_budget(n: u64) {
    BUDGET.store(n.min(10_000), Ordering::Relaxed);
    USED.store(0, Ordering::Relaxed);
}

/// Reabre o orçamento (zera o usado).
pub fn reset_budget() {
    USED.store(0, Ordering::Relaxed);
}

/// Orçamento total corrente.
pub fn budget() -> u64 {
    BUDGET.load(Ordering::Relaxed)
}

/// Usado esta janela.
pub fn used() -> u64 {
    USED.load(Ordering::Relaxed)
}

/// Orçamento restante.
pub fn remaining() -> u64 {
    BUDGET
        .load(Ordering::Relaxed)
        .saturating_sub(USED.load(Ordering::Relaxed))
}

/// True se há orçamento para uma sonda nova.
pub fn has_probe_budget() -> bool {
    remaining() > 0
}

/// Tenta consumir uma sonda; `false` quando esgotado.
pub fn try_probe() -> bool {
    let prev = USED.fetch_add(1, Ordering::Relaxed);
    prev < BUDGET.load(Ordering::Relaxed)
}

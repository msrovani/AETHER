//! Afinidade de núcleo e orçamento de residência em L1d.
//!
//! A thread reflexiva é fixada no primeiro núcleo físico detectado
//! (`CorePlan::detect`). O módulo também expõe um contador de orçamento para
//! provar que o estado rápido cabe (ou não) nos 32 KB de L1d.
//!
//! Tudo é API segura: o pinning usa `core_affinity` por baixo do
//! `aether_governor` e nenhum `unsafe` aparece neste arquivo. Sem a feature
//! `affinity`, as funções viram no-op e o relatório diz `pinned=false`.

use crate::core::core_topology::{pin_current_thread, CorePlan};

/// Capacidade nominal de L1d por núcleo físico (Intel Core 7 240H).
pub const L1D_BYTES: usize = 32 * 1024;

/// Contador de orçamento do estado reflexivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct L1Budget {
    used: usize,
}

impl L1Budget {
    /// Começa vazio.
    pub const fn new() -> Self {
        Self { used: 0 }
    }

    /// Contabiliza `bytes` (satura no `usize::MAX`).
    pub fn account(&mut self, bytes: usize) {
        self.used = self.used.saturating_add(bytes);
    }

    /// Bytes já contabilizados.
    pub const fn used(&self) -> usize {
        self.used
    }

    /// Bytes restantes (0 se estourou).
    pub const fn remaining(&self) -> usize {
        L1D_BYTES.saturating_sub(self.used)
    }

    /// Cabe inteiro em L1d?
    pub const fn fits(&self) -> bool {
        self.used <= L1D_BYTES
    }
}

impl Default for L1Budget {
    fn default() -> Self {
        Self::new()
    }
}

/// Relatório de afinidade e residência.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffinityReport {
    /// Núcleo físico escolhido para o reflexo.
    pub core: Option<usize>,
    /// `true` se o pinning foi aplicado nesta thread.
    pub pinned: bool,
    /// Bytes do estado reflexivo contabilizados.
    pub l1d_bytes: usize,
    /// `true` se `l1d_bytes <= 32 KB`.
    pub l1d_fits: bool,
    /// Núcleos lógicos detectados.
    pub logical: usize,
    /// Núcleos físicos detectados.
    pub physical: usize,
    /// Irmãos SMT reservados (devem ficar ociosos).
    pub reserved_siblings: Vec<usize>,
}

/// Plano de núcleos detectado.
pub fn core_plan() -> CorePlan {
    CorePlan::detect()
}

/// Ids lógicos disponíveis para pinning (vazio se indisponível).
pub fn core_ids() -> Vec<usize> {
    #[cfg(feature = "affinity")]
    {
        core_affinity::get_core_ids()
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect()
    }
    #[cfg(not(feature = "affinity"))]
    {
        Vec::new()
    }
}

/// Fixa a **thread corrente** em `core`; `false` se indisponível.
pub fn pin_to_core(core: usize) -> bool {
    pin_current_thread(Some(core))
}

/// Fixa a thread corrente no núcleo reflexivo e mede o orçamento L1d.
pub fn pin_reflex_core(active_bytes: usize) -> AffinityReport {
    let plan = core_plan();
    let pinned = pin_current_thread(plan.reflex);
    let mut budget = L1Budget::new();
    budget.account(active_bytes);
    AffinityReport {
        core: plan.reflex,
        pinned,
        l1d_bytes: budget.used(),
        l1d_fits: budget.fits(),
        logical: plan.logical,
        physical: plan.physical,
        reserved_siblings: plan.reserved_siblings.clone(),
    }
}

/// Relatório sem pinning (baseline).
pub fn report_only(active_bytes: usize) -> AffinityReport {
    let plan = core_plan();
    let mut budget = L1Budget::new();
    budget.account(active_bytes);
    AffinityReport {
        core: plan.reflex,
        pinned: false,
        l1d_bytes: budget.used(),
        l1d_fits: budget.fits(),
        logical: plan.logical,
        physical: plan.physical,
        reserved_siblings: plan.reserved_siblings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orcamento_l1d_e_relatorio() {
        let mut b = L1Budget::new();
        b.account(16 * 1024);
        assert!(b.fits());
        assert_eq!(b.remaining(), 16 * 1024);
        b.account(16 * 1024);
        assert!(b.fits());
        b.account(1);
        assert!(!b.fits());
        assert_eq!(b.remaining(), 0);

        let r = report_only(24 * 1024);
        assert!(r.l1d_fits);
        assert_eq!(r.l1d_bytes, 24 * 1024);
        assert!(!r.pinned);
    }

    #[test]
    fn pinning_e_noop_quando_indisponivel() {
        // Não exige sucesso: em CI sem afinidade o contrato é false, sem panic.
        let r = pin_reflex_core(8 * 1024);
        assert!(r.l1d_fits);
        let _ = r.pinned;
    }
}

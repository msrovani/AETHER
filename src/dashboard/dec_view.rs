//! T-DEC — linha de Detalhe no hub (DecisionPort / Vega).

use crate::agent::DecTomadaStatus;

/// Snapshot carregado para a UI.
#[derive(Clone, Debug)]
pub struct DecView {
    /// Status tipado.
    pub status: DecTomadaStatus,
}

/// Carrega status a partir do ambiente (sem PyO3 no hot path do hub).
pub fn load_dec_view() -> DecView {
    DecView {
        status: DecTomadaStatus::from_env(),
    }
}

/// Linha de programa (lista / cartão).
pub fn dec_program_line(view: &DecView) -> String {
    format!(
        "T-DEC {} · {}",
        if view.status.enabled {
            "opt-in"
        } else {
            "off"
        },
        view.status.backend
    )
}

/// Texto da coluna Detalhe.
pub fn dec_detail(view: &DecView) -> String {
    view.status.detail_line()
}

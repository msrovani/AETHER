//! T-DEC — linha de Detalhe no hub (DecisionPort / Vega ∧ gemma + upgrade dual-bank).

use crate::agent::{
    begin_upgrade, detail_block, load_catalog, revert_previous, DecSlot, DecTomadaStatus,
    DecUpgradePhase, DecUpgradeState, LampColor,
};

/// Snapshot carregado para a UI.
#[derive(Clone, Debug)]
pub struct DecView {
    /// Status tipado da tomada.
    pub status: DecTomadaStatus,
    /// Máquina de estados do upgrade.
    pub upgrade: DecUpgradeState,
    /// Resumo do catálogo (id · slot · hw).
    pub catalog_lines: Vec<String>,
    /// Label do motor active (lock ou default).
    pub engine_label: String,
}

/// Carrega status + upgrade + catálogo (sem PyO3 no hot path).
pub fn load_dec_view() -> DecView {
    let upgrade = DecUpgradeState::load();
    let catalog_lines = load_catalog()
        .map(|cat| {
            cat.into_iter()
                .map(|r| {
                    format!(
                        "{} [{}] {} VRAM≈{}MB hw6g={}",
                        r.id, r.slot.label(), r.hf_id, r.vram_mb_est, r.hw_6gb
                    )
                })
                .collect()
        })
        .unwrap_or_else(|e| vec![format!("catalog erro: {e}")]);
    let engine_label = crate::agent::load_active_lock()
        .and_then(|j| {
            j.get("engine_label")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "vega-u-0.8b AND gemma-prose".into());
    DecView {
        status: DecTomadaStatus::from_env(),
        upgrade,
        catalog_lines,
        engine_label,
    }
}

/// Linha de programa (lista / cartão).
pub fn dec_program_line(view: &DecView) -> String {
    let up = &view.upgrade;
    let fb = if up.fallback_armed { "FB" } else { "live" };
    let scribe = if crate::agent::scribe_opt_in() {
        "scribe=gemma"
    } else {
        "scribe=off"
    };
    format!(
        "T-DEC {} · {} · up={} · {} · {} · {}",
        if view.status.enabled {
            "opt-in"
        } else {
            "off"
        },
        view.status.backend,
        up.phase.label(),
        fb,
        scribe,
        view.engine_label
    )
}

/// Texto da coluna Detalhe.
pub fn dec_detail(view: &DecView) -> String {
    let scribe_path = crate::agent::active_scribe_path().unwrap_or_else(|| "-".into());
    let scribe_on = crate::agent::scribe_opt_in();
    let mut lines = vec![
        "T-DEC = tomada de decisão (DecisionPort), não um chatbot.".into(),
        "Core AETHER ≠ modelo. Motor Vega-U = motor fixo; readers = LMs plugáveis.".into(),
        "Gemma sentinela = mesmo artefato que a escriba T-NL-OUT (1 download · 2 papéis).".into(),
        format!(
            "chip: 1 artefato · 2 papéis · scribe={} · path={}",
            if scribe_on { "ON (AETHER_SCRIBE=gemma)" } else { "off (template)" },
            scribe_path
        ),
        "Up no hub troca o READER; cutover atualiza sentinela e escriba. Fallback = active.".into(),
        "Guia: docs/USER_GUIDE.md §7c · docs/GEMMA_SENTINEL_SCRIBE.md".into(),
        view.status.detail_line(),
        format!("engine (active)={}", view.engine_label),
        detail_block(&view.upgrade),
        "— catálogo allowlist (readers; sentinel tem roles scribe) —".into(),
    ];
    lines.extend(view.catalog_lines.iter().cloned());
    lines.join("\n")
}

/// Lâmpadas numa linha compacta.
pub fn dec_lamps_line(view: &DecView) -> String {
    let l = &view.upgrade.lamps;
    format!(
        "HW:{} Fetch:{} Boot:{} Fit:{} T1:{} T2:{} C1:{} AND:{} Q:{} OK:{}",
        lamp_glyph(l.hw),
        lamp_glyph(l.fetch),
        lamp_glyph(l.boot),
        lamp_glyph(l.fit),
        lamp_glyph(l.t1),
        lamp_glyph(l.t2),
        lamp_glyph(l.c1),
        lamp_glyph(l.and_gate),
        lamp_glyph(l.quarantine),
        lamp_glyph(l.ok_geral),
    )
}

fn lamp_glyph(c: LampColor) -> &'static str {
    match c {
        LampColor::Green => "G",
        LampColor::Yellow => "Y",
        LampColor::Red => "R",
        LampColor::Off => "-",
    }
}

/// Dispara begin_upgrade no slot (enfileira HITL download se HW ok).
pub fn request_upgrade(slot: DecSlot, candidate_id: &str) -> Result<DecUpgradeState, String> {
    begin_upgrade(slot, candidate_id)
}

/// Reverter previous → active.
pub fn request_revert() -> Result<DecUpgradeState, String> {
    let mut st = DecUpgradeState::load();
    revert_previous(&mut st)?;
    Ok(st)
}

/// True se o job DecUpgrade deve/pode rodar (fase Fetch..Smoke).
pub fn upgrade_job_pending(view: &DecView) -> bool {
    matches!(
        view.upgrade.phase,
        DecUpgradePhase::Fetch
            | DecUpgradePhase::Attach
            | DecUpgradePhase::Fit
            | DecUpgradePhase::Smoke
    )
}

//! E6 — gate de instrumentação: os estágios medidos têm de cobrir o quadro.
//!
//! Não é teste de performance (não compara tempos absolutos): é teste de
//! **cobertura do instrumento**. Se alguém inserir um estágio novo no
//! `step_impl` sem marcá-lo, a cobertura cai e este teste avisa.
//!
//! Referência: `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` §10 (E6) e
//! `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md` §11.1.

use std::time::Duration;

use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};
use aether_engine::simulation::profile::{StageProfile, STAGE_COUNT};

#[test]
fn profile_covers_the_frame() {
    // Configuração pequena para o teste ser rápido (não é o regime do bench).
    let cfg = OrganismConfig {
        decision_dim: 32,
        window: 64,
        steps: 16,
        perturb_after: Duration::from_secs(3_600),
        ..OrganismConfig::default()
    };

    let mut org = ClosedLoopOrganism::new(cfg.clone()).expect("organismo");
    let mut frame = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);

    // Warmup: planos FFT, arena e publicação inicial dos nós Kuramoto.
    for _ in 0..10 {
        org.step_into(&mut frame).expect("warmup");
    }

    let mut prof = StageProfile::default();
    let mut worst = 1.0f64;
    let mut seen_total = 0u64;

    for _ in 0..40 {
        prof.reset();
        org.step_profiled_into(&mut frame, &mut prof).expect("step");
        let cov = prof.coverage();
        if cov < worst {
            worst = cov;
        }
        seen_total = prof.total_ns;
    }

    assert!(seen_total > 0, "total_ns não foi preenchido");
    assert!(
        worst >= 0.95,
        "cobertura mínima {:.4} < 0.95 — algum estágio do step_impl não foi marcado",
        worst
    );

    // Sanidade: se os 10 estágios têm tempo, o acumulado é positivo.
    let touched = prof.ns.iter().filter(|&&v| v > 0).count();
    assert!(
        touched >= STAGE_COUNT - 2,
        "apenas {touched}/{STAGE_COUNT} estágios registraram tempo"
    );
}

#[test]
fn profiled_frame_is_consistent() {
    // NOTA DELIBERADA: não se asserta igualdade bit-a-bit entre dois organismos.
    // O quadro depende de `NodeHandle::latest()`, que lê o último frame publicado
    // por threads Kuramoto reais (UDP + agendamento do SO). Dois organismos com a
    // mesma seed divergem por design — o determinismo bit-a-bit do crate vale para
    // codec/solver, não para a amostragem assíncrona. Medir o organismo é medir
    // distribuição, nunca uma execução única.
    let cfg = OrganismConfig {
        decision_dim: 32,
        window: 64,
        steps: 16,
        perturb_after: Duration::from_secs(3_600),
        ..OrganismConfig::default()
    };

    let mut org = ClosedLoopOrganism::new(cfg.clone()).expect("organismo");
    let mut frame = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);
    let mut prof = StageProfile::default();

    for _ in 0..25 {
        prof.reset();
        org.step_profiled_into(&mut frame, &mut prof).expect("step");
        assert_eq!(frame.states.len(), cfg.decision_dim);
        assert_eq!(frame.energy_trace.len(), cfg.steps + 1);
        assert!(frame.energy.is_finite(), "energia não finita");
        assert!(
            prof.total_ns > 0,
            "total_ns zerado no caminho instrumentado"
        );
        assert!(
            prof.accounted_ns() <= prof.total_ns * 101 / 100,
            "soma dos estágios excedeu o total em >1%"
        );
    }
}

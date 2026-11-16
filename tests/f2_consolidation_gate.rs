//! F2 — consolidação `T1`, *commit_run*, `supersede` e retenção em *gist*.
//!
//! Gates (Vol. III §13, pergunta #6):
//! 1. `0` eventos perdidos sob taxa nominal e `0` chamadas ao ledger no `T0`;
//! 2. contradição gera `supersede` e o fato antigo vira *gist* antes da poda;
//! 3. a habilidade de reconhecer o padrão sobrevive à poda (retenção ≥ 95%);
//! 4. `R9 > 0`: abstenção causada por proveniência é mensurável.

use std::f32::consts::TAU;

use aether_engine::core::knowledge_bridge_f1::{
    Certificate, DraftRecord, KnowledgeEvent, KnowledgeSink, Layer, ScopeId,
    SpscKnowledgePhaseBridge,
};
use aether_engine::core::knowledge_fusion::{FusionAction, FusionLedger, GIST_DIM};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 16;

fn payload(phases: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(phases.len() * 8);
    for p in phases {
        out.extend_from_slice(&p.cos().to_le_bytes());
        out.extend_from_slice(&p.sin().to_le_bytes());
    }
    out
}

fn noisy(phases: &[f32], amp: f32, rng: &mut ChaCha8Rng) -> Vec<f32> {
    phases
        .iter()
        .map(|p| p + (rng.gen::<f32>() - 0.5) * 2.0 * amp)
        .collect()
}

fn interleaved(phases: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(phases.len() * 2);
    for p in phases {
        out.push(p.cos());
        out.push(p.sin());
    }
    out
}

#[test]
fn f2_consolidacao_sem_perda_e_gist_preserva_habilidade() {
    let scope = ScopeId::new(1, 7, 3);
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<64>::new();
    let mut ledger = FusionLedger::new();

    // 10 rodadas nominais de 32 eventos: o T1 drena antes de o anel encher.
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E_0F20);
    let base: Vec<Vec<f32>> = (0..32)
        .map(|_| (0..N).map(|_| rng.gen::<f32>() * TAU).collect())
        .collect();
    let mut total = 0usize;
    for round in 0..10u64 {
        for (i, phases) in base.iter().enumerate() {
            let key = round * 32 + i as u64;
            let pl = payload(phases);
            let ev = KnowledgeEvent::new(
                u128::from(key),
                Layer::L4Semantic,
                scope,
                key,
                1_000 + key,
            )
            .with_certificate(Certificate {
                energy: -1.0 - 0.01 * i as f32,
                gap: 0.1,
                violations: 0,
            })
            .with_authority(200)
            .with_payload(&pl);
            tx.record(&ev).expect("record nominal");
            total += 1;
        }
        let mut batch = Vec::with_capacity(32);
        let drained = rx.drain_batch(&mut batch, 32);
        assert_eq!(drained, 32, "rodada {round} perdeu eventos");
        ledger.stage_batch(batch);
        ledger.commit_run();
    }

    assert_eq!(rx.metrics().t0_ledger_calls(), 0, "T0 falou com o ledger");
    assert_eq!(rx.metrics().recorded(), total as u64);
    assert_eq!(rx.metrics().drained(), total as u64);
    assert_eq!(rx.metrics().dropped(), 0, "eventos perdidos no T0");
    assert_eq!(ledger.len(), total);
    assert_eq!(ledger.runs(), 10);

    // Latência do ciclo T0: o produtor só enfileira; nenhuma chamada nova de
    // ledger acontece quando o T1 consolida. A prova é estrutural + métrica.
    let before_calls = rx.metrics().t0_ledger_calls();
    let mut batch = Vec::new();
    let pl = payload(&base[0]);
    let ev = KnowledgeEvent::new(99, Layer::L2Episodic, scope, 99, 99).with_payload(&pl);
    tx.record(&ev).expect("record pos-consolidacao");
    rx.drain_batch(&mut batch, 8);
    assert_eq!(rx.metrics().t0_ledger_calls(), before_calls);
    assert_eq!(batch.len(), 1);

    // ------------------------------------------------------------------
    // Supersede: mesma key, payload diferente → antigo vira gist.
    // ------------------------------------------------------------------
    let old_pattern = base[0].clone();
    let mut contradict = old_pattern.clone();
    contradict[0] += 1.0;
    let rec = DraftRecord {
        key: 0,
        layer: Layer::L4Semantic,
        certificate: Certificate {
            energy: -2.5,
            gap: 0.05,
            violations: 0,
        },
        authority: 255,
        scope,
        tick: 9_999,
        wall_ms: 9_999,
        payload: payload(&contradict),
    };
    ledger.stage(rec.clone());
    assert_eq!(ledger.commit_run(), 0);
    assert_eq!(ledger.superseded(), 1);
    assert_eq!(ledger.gist_count(), 1);
    assert_eq!(ledger.len(), total);

    // ------------------------------------------------------------------
    // Retenção da habilidade após poda: o gist reconhece o padrão antigo.
    // ------------------------------------------------------------------
    let mut hits = 0usize;
    let trials = 40usize;
    for _ in 0..trials {
        let cue = noisy(&old_pattern, 0.25, &mut rng);
        if ledger.gist_cue(scope, &interleaved(&cue), 0.60) == Some(0) {
            hits += 1;
        }
    }
    let retention = hits as f32 / trials as f32;
    println!("[F2] retencao_gist={retention:.3} ({hits}/{trials}) gists={}", ledger.gist_count());
    assert!(
        retention >= 0.95,
        "retencao no gist {retention:.3} < 0.95"
    );

    // Poda explícita de outro atrator também preserva gist.
    let pruned = ledger.prune_to_gist(scope, 5).expect("prune");
    assert_eq!(pruned.key, 5);
    assert_eq!(ledger.pruned(), 1);
    assert_eq!(ledger.len(), total - 1);
    assert!(ledger.gist_cue(scope, &interleaved(&base[5]), 0.60) == Some(5));

    // ------------------------------------------------------------------
    // R9: abstenção por proveniência > 0.
    // ------------------------------------------------------------------
    let d = ledger.decide(
        0,
        Certificate {
            energy: 0.0,
            gap: 0.9,
            violations: 2,
        },
    );
    assert_eq!(d.action, FusionAction::Abstain);
    println!("[F2] R9_abstencoes_proveniencia={}", ledger.provenance_abstentions());
    assert!(ledger.provenance_abstentions() > 0, "R9 vazio");

    // Sanidade do formato do gist.
    assert_eq!(ledger.gists()[0].signature.len(), GIST_DIM);
    let _ = rec; // o registro novo permanece consultável
}

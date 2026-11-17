//! F2/F3 — gates **adversariais** (auditoria do que os gates felizes não testam).
//!
//! Os gates `f2_consolidation_gate` e `f3_fusion_2d_gate` verificam o caminho
//! nominal: 0 perdas sob taxa nominal, matriz 2D, round-trip < 5%, R8 = 0.
//! Aqui testamos os **modos de falha**:
//!
//! 1. **Tabela 2D e fronteiras**: `authority = 127/128` e `gap = 0.25/0.26`
//!    precisam cair em lados opostos, a ação tem de ser função pura de
//!    `(trusted, feasible)` e `violations > 0` tem de forçar infactível.
//! 2. **Honestidade sob sobrecarga**: o anel SPSC **descarta por desenho**.
//!    O que não pode acontecer é a ponte *contar* como entregue o que foi
//!    descartado — `recorded + dropped == tentativas`, `commit_run` == drenado,
//!    e `t0_ledger_calls == 0` mesmo no pior caso.
//! 3. **Poda cirúrgica**: `prune_to_gist` de um escopo não pode tocar a mesma
//!    `key` de outro escopo, e o *gist* tem de carregar o escopo de origem.

use aether_engine::core::knowledge_bridge_f1::{
    Certificate, KnowledgeEvent, KnowledgeSink, Layer, ScopeId, SpscKnowledgePhaseBridge,
};
use aether_engine::core::knowledge_fusion::{FusionAction, FusionLedger, Quadrant};

const N: usize = 16;

fn payload(seed: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(N * 8);
    let mut x = seed | 1;
    for _ in 0..N {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        let re = ((x >> 33) as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0;
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        let im = ((x >> 33) as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0;
        out.extend_from_slice(&re.to_le_bytes());
        out.extend_from_slice(&im.to_le_bytes());
    }
    out
}

fn cert(gap: f32, violations: u32) -> Certificate {
    Certificate {
        energy: -1.0,
        gap,
        violations,
    }
}

/// Tabela do Vol. III §6: a ação é função **pura** de `(trusted, feasible)`.
#[test]
fn f23_tabela_2d_e_fronteiras() {
    let mut ledger = FusionLedger::new(); // limiares: gap ≤ 0,25 · autoridade ≥ 128
    let cases = [
        // (authority, gap, violations, quadrant esperado, ação esperada)
        (255, 0.00, 0, Quadrant::TrustedFeasible, FusionAction::ActAndCite),
        (128, 0.25, 0, Quadrant::TrustedFeasible, FusionAction::ActAndCite),
        (127, 0.00, 0, Quadrant::UnvettedFeasible, FusionAction::Propose),
        (0, 0.10, 0, Quadrant::UnvettedFeasible, FusionAction::Propose),
        (255, 0.26, 0, Quadrant::TrustedAmbiguous, FusionAction::CiteNoAct),
        (200, 1.00, 0, Quadrant::TrustedAmbiguous, FusionAction::CiteNoAct),
        (255, 0.00, 1, Quadrant::TrustedAmbiguous, FusionAction::CiteNoAct),
        (0, 0.26, 0, Quadrant::UnvettedAmbiguous, FusionAction::Abstain),
        (127, 0.00, 2, Quadrant::UnvettedAmbiguous, FusionAction::Abstain),
    ];
    for (authority, gap, violations, want_q, want_a) in cases {
        let d = ledger.decide(authority, cert(gap, violations));
        assert_eq!(
            d.quadrant, want_q,
            "authority={authority} gap={gap} viol={violations}"
        );
        assert_eq!(
            d.action, want_a,
            "authority={authority} gap={gap} viol={violations}"
        );
        // coerência interna do atestado
        assert_eq!(d.feasible, violations == 0 && gap <= 0.25);
        assert!((0.0..=1.0).contains(&d.gap), "gap precisa ser normalizado");
    }

    // fronteiras exatas: 127 vs 128 e 0,25 vs 0,26 têm de separar
    assert_eq!(
        ledger.decide(127, cert(0.0, 0)).action,
        FusionAction::Propose
    );
    assert_eq!(
        ledger.decide(128, cert(0.0, 0)).action,
        FusionAction::ActAndCite
    );
    assert_eq!(
        ledger.decide(255, cert(0.25, 0)).action,
        FusionAction::ActAndCite
    );
    assert_eq!(
        ledger.decide(255, cert(0.250_001, 0)).action,
        FusionAction::CiteNoAct
    );

    // R9: abstenção causada por proveniência (violações) é contabilizada
    let before = ledger.provenance_abstentions();
    let _ = ledger.decide(10, cert(0.0, 3));
    assert!(ledger.provenance_abstentions() > before);
}

/// Sobrecarga: o que cai no anel tem de aparecer como *descartado*, nunca como
/// entregue — e o `T0` continua sem falar com o ledger.
#[test]
fn f23_ponte_honesta_sob_sobrecarga() {
    const RING: usize = 8;
    const ATTEMPTS: u64 = 200;
    let scope = ScopeId::new(9, 1, 1);
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<RING>::new();
    let body = payload(0xBEEF);

    for k in 0..ATTEMPTS {
        let ev = KnowledgeEvent::new(u128::from(k), Layer::L4Semantic, scope, k, 1_000 + k)
            .with_certificate(cert(0.1, 0))
            .with_authority(200)
            .with_payload(&body);
        // o anel enche: `record` devolve Err e conta descarte
        let _ = tx.record(&ev);
    }

    let accepted = tx.metrics().recorded();
    let dropped = tx.metrics().dropped();
    assert_eq!(
        accepted + dropped,
        ATTEMPTS,
        "toda tentativa precisa ser aceita **ou** contada como descartada"
    );
    assert!(dropped > 0, "o anel de 8 deveria ter descartado");
    assert!(accepted <= RING as u64, "aceitas {accepted} > capacidade");
    assert_eq!(
        tx.metrics().t0_ledger_calls(),
        0,
        "R7: o T0 nunca fala com o ledger"
    );

    // T1: drena exatamente o que foi aceito e consolida sem inventar
    let mut out = Vec::new();
    let drained = rx.drain_batch(&mut out, RING);
    assert_eq!(drained as u64, accepted, "drenado != aceito");
    let mut ledger = FusionLedger::new();
    ledger.stage_batch(out);
    let committed = ledger.commit_run();
    assert_eq!(
        committed as u64, accepted,
        "commit_run inventou/perdeu registros"
    );
    assert_eq!(ledger.len() as u64, accepted);

    // segunda passada com as mesmas chaves: nada novo entra, carimbos renovam
    let mut again = Vec::new();
    for k in 0..accepted {
        let ev = KnowledgeEvent::new(u128::from(k), Layer::L4Semantic, scope, k, 2_000 + k)
            .with_certificate(cert(0.2, 0))
            .with_authority(201)
            .with_payload(&body);
        assert!(tx.record(&ev).is_ok());
    }
    let n = rx.drain_batch(&mut again, RING);
    assert_eq!(n as u64, accepted);
    ledger.stage_batch(again);
    assert_eq!(
        ledger.commit_run(),
        0,
        "reentrega idêntica não deve contar como fato novo"
    );
    assert_eq!(ledger.len() as u64, accepted);
    assert_eq!(ledger.superseded(), 0, "payload idêntico não é contradição");
}

/// A poda de um escopo não pode apagar a mesma `key` de outro escopo.
#[test]
fn f23_poda_nao_vaza_escopo() {
    let a = ScopeId::new(1, 0, 1);
    let b = ScopeId::new(2, 0, 1);
    let key = 0xDEAD_BEEF_u128;
    let mut ledger = FusionLedger::new();

    for scope in [a, b] {
        ledger.stage(
            aether_engine::core::knowledge_bridge_f1::DraftRecord {
                key,
                layer: Layer::L4Semantic,
                certificate: cert(0.1, 0),
                authority: 200,
                scope,
                tick: 0,
                wall_ms: 1_000,
                payload: payload(u64::from(scope.organism_id)),
            },
        );
    }
    assert_eq!(ledger.commit_run(), 2, "chaves iguais em escopos distintos");

    let gist = ledger.prune_to_gist(a, key).expect("gist de A");
    assert_eq!(gist.scope, a, "gist precisa carregar o escopo de origem");
    assert!(!ledger.key_in_scope(a, key), "A deveria ter sido podado");
    assert!(
        ledger.key_in_scope(b, key),
        "R8: podar A não pode apagar B"
    );
    assert_eq!(ledger.pruned(), 1);
    assert_eq!(ledger.gist_count(), 1);

    // o gist de A serve A e **não** é anunciado para B
    let sig = gist.signature;
    assert_eq!(ledger.gist_cue(a, &sig, 0.5), Some(key));
    assert_eq!(
        ledger.gist_cue(b, &sig, 0.5),
        None,
        "R8: gist de A não pode ser recuperado por B"
    );
    assert!(ledger.cross_scope_hits() == 0);
}

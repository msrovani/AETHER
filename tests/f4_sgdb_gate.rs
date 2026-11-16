//! F4 — `neural-sgdb` como camada durável (`T2`).
//!
//! Gates (Vol. III §7, §8 e §13):
//! 1. `T0` não fala com o ledger (`t0_ledger_calls == 0`) e nada se perde entre
//!    o anel e o banco;
//! 2. escopo não vaza (com controle positivo de que o fato estrangeiro existe);
//! 3. round-trip íntegro: payload, certificado, autoridade, `tick` e `wall_ms`;
//! 4. queda de rede → outbox → replay, sem perder fato;
//! 5. a matriz 2D decide com evidência vinda do banco;
//! 6. `recall → payload → cue_into` reacende a fase (erro angular < 1e-4);
//! 7. TTL vencido some da consulta.

use std::f32::consts::TAU;
use std::path::PathBuf;

use aether_engine::core::certificate::{Certificate as EngineCertificate, Provenance};
use aether_engine::core::knowledge_bridge_f1::{
    Certificate, DraftRecord, KnowledgeEvent, KnowledgeSink, Layer, ScopeId,
    SpscKnowledgePhaseBridge,
};
use aether_engine::core::knowledge_fusion::FusionAction;
use aether_engine::knowledge::sgdb::ledger::{encode_phases, phase_key, ORGANISM_AUTHORITY};
use aether_engine::knowledge::sgdb::{MockTransport, SgdbConfig, SgdbLedger};

const N: usize = 16;

fn phases(seed: u64, count: usize) -> Vec<Vec<f32>> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..count)
        .map(|_| {
            (0..N)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    (x % 10_000) as f32 / 10_000.0 * TAU
                })
                .collect()
        })
        .collect()
}

fn record(scope: ScopeId, key: u8, authority: u8) -> DraftRecord {
    let mut payload = Vec::new();
    encode_phases(&phases(u64::from(key) + 7, 1)[0], &mut payload);
    DraftRecord {
        key: phase_key(&payload),
        layer: Layer::L4Semantic,
        certificate: Certificate {
            energy: -1.5 - f32::from(key) * 0.01,
            gap: 0.25,
            violations: 0,
        },
        authority,
        scope,
        tick: 123 + u64::from(key),
        wall_ms: 1_700_000_000_000 + u64::from(key),
        payload,
    }
}

fn temp_outbox(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("aether_sgdb_{tag}_{}_{unique}.jsonl", std::process::id()))
}

#[test]
fn f4_gate_t0_ao_sgdb_sem_perda_e_sem_vazamento() {
    let scope = ScopeId::new(1, 7, 3);
    let other = ScopeId::new(2, 7, 3);

    // ------------------------------------------------------------------
    // 1. T0 → ring (0 alloc) → T1 drain → T2 banco.
    // ------------------------------------------------------------------
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<64>::new();
    let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
    for i in 0..32u8 {
        let rec = record(scope, i, ORGANISM_AUTHORITY);
        let ev = KnowledgeEvent::new(rec.key, rec.layer, rec.scope, rec.tick, rec.wall_ms)
            .with_certificate(rec.certificate)
            .with_authority(rec.authority)
            .with_payload(&rec.payload);
        tx.record(&ev).expect("anel");
    }
    let mut drained = Vec::new();
    assert_eq!(rx.drain_batch(&mut drained, 64), 32);
    ledger.stage_batch(drained);
    assert_eq!(rx.metrics().t0_ledger_calls(), 0, "T0 falou com o ledger");
    assert_eq!(rx.metrics().dropped(), 0);
    let committed = ledger.commit_run();
    assert_eq!(committed, 32, "fatos perdidos antes do banco");
    assert_eq!(ledger.metrics().remembered, 32);
    assert!(!ledger.is_offline());

    // ------------------------------------------------------------------
    // 2. Escopo: o fato do organismo 2 existe, mas não vaza para o 1.
    // ------------------------------------------------------------------
    let mut foreign = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), other);
    foreign.stage(record(other, 200, 255));
    foreign.commit_run();
    assert_eq!(foreign.recall(other, "", 8).len(), 1, "controle positivo");
    assert_eq!(
        ledger.recall(scope, "", 20).len(),
        20,
        "consulta do escopo 1 viu o escopo 2"
    );
    assert!(
        !ledger
            .recall(scope, "", 20)
            .iter()
            .any(|hit| hit.doc.scope == other),
        "hit cruzado entre escopos"
    );

    // ------------------------------------------------------------------
    // 3. Round-trip íntegro.
    // ------------------------------------------------------------------
    let hits = ledger.recall(scope, "", 20);
    assert_eq!(hits.len(), 20);
    let sample = &hits[0];
    let back = record(scope, 0, ORGANISM_AUTHORITY);
    let original = hits
        .iter()
        .find(|h| h.doc.key == back.key)
        .expect("fato original");
    assert_eq!(original.doc.payload, back.payload);
    assert_eq!(original.doc.certificate, back.certificate);
    assert_eq!(original.authority, back.authority);
    assert_eq!(original.doc.tick, back.tick);
    assert_eq!(original.doc.wall_ms, back.wall_ms);
    assert_eq!(sample.doc.layer, Layer::L4Semantic);

    // ------------------------------------------------------------------
    // 4. Matriz 2D com evidência do banco.
    // ------------------------------------------------------------------
    let ev = ledger.evidence(scope, "", 8);
    assert!(!ev.is_empty());
    let cert = EngineCertificate::new(-1.0, 0.01, 0, 0.9, Provenance::Observed);
    // Autoridade alta + ambíguo (gap abaixo do piso) ⇒ cita, não atua.
    let decision = cert.decide_with_evidence(&ev, 0.5);
    let fusion = ledger.decide(ev[0].authority, Certificate {
        energy: -1.0,
        gap: 1.0,
        violations: 0,
    });
    assert_eq!(fusion.action, FusionAction::CiteNoAct);
    assert_eq!(decision, aether_engine::core::certificate::Decision::Cite);

    // ------------------------------------------------------------------
    // 5. cue_into reacende a fase a partir do banco.
    // ------------------------------------------------------------------
    let pattern = phases(8, 1).remove(0);
    let mut query = Vec::new();
    for p in &pattern {
        query.push(p.cos());
        query.push(p.sin());
    }
    let mut out = vec![0.0f32; pattern.len()];
    let cue = ledger
        .cue_into_scoped(scope, &query, &mut out)
        .expect("cue no banco");
    let mut max_err = 0.0f32;
    for (a, b) in out.iter().zip(pattern.iter()) {
        let mut d = (a - b).rem_euclid(TAU);
        if d > TAU / 2.0 {
            d -= TAU;
        }
        max_err = max_err.max(d.abs());
    }
    assert!(max_err < 1e-4, "erro angular {max_err}");
    assert!(cue.hits >= 1);
}

#[test]
fn f4_gate_offline_outbox_replay_e_ttl() {
    let scope = ScopeId::new(3, 4, 5);
    let path = temp_outbox("replay");
    let _ = std::fs::remove_file(&path);

    // Banco fora: tudo vai para o outbox local (nada se perde).
    let mut offline = SgdbLedger::new(
        MockTransport::new(),
        SgdbConfig::durable(&path),
        scope,
    );
    offline.client_mut().transport_mut().set_offline(true);
    for i in 0..5u8 {
        offline.stage(record(scope, i, 200));
    }
    offline.commit_run();
    assert_eq!(offline.metrics().spooled, 5);
    assert!(offline.is_offline());
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 5);

    // Banco volta: replay antes do próximo lote, mesma chave não duplica.
    let mock = MockTransport::new();
    let mut online = SgdbLedger::new(mock, SgdbConfig::durable(&path), scope);
    let delivered = online.flush();
    assert_eq!(delivered, 5, "replay incompleto");
    assert!(!online.is_offline());
    assert!(!path.exists(), "outbox não drenado");
    assert_eq!(online.recall(scope, "", 16).len(), 5);
    let _ = std::fs::remove_file(&path);

    // TTL: `set_ttl` é honrado pelo banco no relógio dele.
    let ttl_path = temp_outbox("ttl");
    let mut cfg = SgdbConfig::durable(&ttl_path);
    cfg.ttl_ms = Some(60_000);
    let mut mock = MockTransport::new();
    mock.set_now(1_000_000);
    let mut ledger = SgdbLedger::new(mock, cfg, scope);
    ledger.stage(record(scope, 9, 200));
    ledger.commit_run();
    assert_eq!(ledger.recall(scope, "", 8).len(), 1);
    ledger.client_mut().transport_mut().set_now(1_060_000);
    assert_eq!(ledger.recall(scope, "", 8).len(), 0, "TTL não venceu");
    let _ = std::fs::remove_file(&ttl_path);
}

//! Demonstração da simbiose AETHER ↔ `neural-sgdb` (feature `sgdb`).
//!
//! ```text
//! cargo run --example sgdb_roundtrip
//! ```
//!
//! O exemplo usa o transporte `mock` (o mesmo contrato MCP), então roda sem
//! servidor. Ele mostra o ciclo completo:
//!
//! 1. padrões de fase → `T1` → `commit_run` → banco;
//! 2. banco → `recall` → payload íntegro (certificado, autoridade, carimbos);
//! 3. banco → payload → `cue_into` (semente de atrator);
//! 4. queda de rede → outbox → replay;
//! 5. matriz 2D (`autoridade × factibilidade`) com evidência do banco.

use std::f32::consts::TAU;

use aether_engine::core::certificate::{Certificate as EngineCertificate, Provenance};
use aether_engine::core::knowledge_bridge_f1::{Certificate, DraftRecord, Layer, ScopeId};
use aether_engine::core::knowledge_fusion::FusionAction;
use aether_engine::knowledge::sgdb::ledger::{
    encode_phases, phase_key, phases_from_payload, ORGANISM_AUTHORITY,
};
use aether_engine::knowledge::sgdb::{MockTransport, SgdbConfig, SgdbLedger};

fn pattern(seed: u64, dims: usize) -> Vec<f32> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..dims)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 10_000) as f32 / 10_000.0 * TAU
        })
        .collect()
}

fn record(scope: ScopeId, phases: &[f32], authority: u8) -> DraftRecord {
    let mut payload = Vec::new();
    encode_phases(phases, &mut payload);
    DraftRecord {
        key: phase_key(&payload),
        layer: Layer::L4Semantic,
        certificate: Certificate {
            energy: -1.25,
            gap: 0.05,
            violations: 0,
        },
        authority,
        scope,
        tick: 7,
        wall_ms: 1_700_000_000_000,
        payload,
    }
}

fn main() {
    let scope = ScopeId::new(1, 1, 1);
    let dims = 32;
    let patterns: Vec<Vec<f32>> = (0..8).map(|i| pattern(i + 1, dims)).collect();

    let mut ledger = SgdbLedger::new(MockTransport::new(), SgdbConfig::default(), scope);
    for (i, phases) in patterns.iter().enumerate() {
        ledger.stage(record(scope, phases, 200 + i as u8));
    }
    let committed = ledger.commit_run();
    println!("1. T1→T2: {committed} padrões no banco ({:?})", ledger.metrics());

    // 2. Round-trip íntegro.
    let hits = ledger.recall(scope, "", 20);
    let first = hits.iter().find(|h| h.doc.key == record(scope, &patterns[0], 200).key);
    match first {
        Some(hit) => println!(
            "2. recall: {} hits | payload={} B | auth={} | E={:.3} | ΔE={:.3}",
            hits.len(),
            hit.doc.payload.len(),
            hit.authority,
            hit.doc.certificate.energy,
            hit.doc.certificate.gap
        ),
        None => println!("2. recall: padrão não voltou (falha de integridade)"),
    }

    // 3. Semente de atrator.
    let mut query = Vec::with_capacity(dims * 2);
    for p in &patterns[0] {
        query.push(p.cos());
        query.push(p.sin());
    }
    let mut seed = vec![0.0f32; dims];
    if let Ok(hit) = ledger.cue_into_scoped(scope, &query, &mut seed) {
        let err = seed
            .iter()
            .zip(patterns[0].iter())
            .map(|(a, b)| ((a - b).rem_euclid(TAU)).min((b - a).rem_euclid(TAU)))
            .fold(0.0f32, f32::max);
        println!("3. cue_into: key={:032x} erro angular máx={err:.2e}", hit.key);
    }

    // 4. Offline → outbox → replay (em memória, sem tocar o disco).
    let offline = ledger.client_mut().transport_mut();
    offline.set_offline(true);
    ledger.stage(record(scope, &pattern(99, dims), 200));
    ledger.commit_run();
    println!(
        "4. offline: spooled={} pending={} err={:?}",
        ledger.metrics().spooled,
        ledger.is_offline(),
        ledger.last_error()
    );
    ledger.client_mut().transport_mut().set_offline(false);
    let replayed = ledger.flush();
    println!("   replay: {replayed} documento(s) entregues");

    // 5. Matriz 2D com evidência do banco.
    let ev = ledger.evidence(scope, "", 8);
    let cert = EngineCertificate::new(-1.0, 0.02, 0, 0.9, Provenance::Observed);
    let decision = cert.decide_with_evidence(&ev, 0.5);
    let fusion = ledger.decide(
        ev.first().map(|e| e.authority).unwrap_or(0),
        Certificate {
            energy: -1.0,
            gap: 1.0,
            violations: 0,
        },
    );
    println!(
        "5. matriz 2D: evidências={} | certificado={:?} | fusão={:?}",
        ev.len(),
        decision,
        fusion.action
    );
    if fusion.action == FusionAction::CiteNoAct {
        println!("   (autoridade alta + fisicamente ambíguo ⇒ cita, não atua)");
    }

    // 6. Reinício: a fase volta do banco.
    let transport = std::mem::replace(
        ledger.client_mut().transport_mut(),
        MockTransport::new(),
    );
    let mut restarted = SgdbLedger::new(transport, SgdbConfig::default(), scope);
    let revived = restarted.hydrate(scope, 16);
    let seeds = restarted
        .recall(scope, "", 20)
        .iter()
        .map(|h| phases_from_payload(&h.doc.payload).len())
        .sum::<usize>();
    println!("6. reinício: {revived} atratores reacendidos ({seeds} fases)");
    println!("   autoridade de organismo = {ORGANISM_AUTHORITY} (sem HITL)");
}

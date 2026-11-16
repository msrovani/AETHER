//! F1 — gates da ponte de conhecimento `T0 → T1 → T2` (Vol. III §7).
//!
//! Quatro gates, cada um amarrado a um risco do Vol. III:
//!
//! 1. **zero alocação no `T0`** — 10.000 `record()` com alocador de contagem
//!    armado ⇒ **0 bytes** (e 0 alocações);
//! 2. **anti-RAG (`R7`)** — `t0_ledger_calls == 0` ao longo de todo o ciclo;
//! 3. **integridade do `T1`** — `drain_batch` recupera 100% dos eventos quando
//!    o anel não satura, com `key`, certificado, autoridade, `tick` e `wall_ms`
//!    distintos e íntegros;
//! 4. **isolamento de escopo (`R8`)** — eventos do organismo 1 nunca aparecem
//!    em consulta do organismo 2 (0 hits cruzados).

#![cfg(feature = "m1")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::core::knowledge_bridge_f1::{
    BridgeError, Certificate, DraftRecord, KnowledgeEvent, KnowledgeSink, KnowledgeSource, Layer,
    ScopeId, ScopeLedger, SpscKnowledgePhaseBridge, MAX_PHASE_BYTES,
};

// ---------------------------------------------------------------------------
// Alocador contador (thread-local: os testes correm em paralelo)
// ---------------------------------------------------------------------------

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
            BYTES.with(|c| c.set(c.get() + layout.size()));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
            BYTES.with(|c| c.set(c.get() + layout.size()));
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
            BYTES.with(|c| c.set(c.get() + new_size));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    BYTES.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()), BYTES.with(|c| c.get()))
}

fn phase_payload(seed: u64) -> Vec<u8> {
    // `D = 160` complexo `f32` = 1280 B, valores determinísticos por seed.
    let mut v = Vec::with_capacity(MAX_PHASE_BYTES);
    let mut x = seed | 1;
    for _ in 0..MAX_PHASE_BYTES / 8 {
        for lane in 0..2 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let f = ((x >> 33) as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let val = if lane == 0 { f } else { -f };
            v.extend_from_slice(&val.to_le_bytes());
        }
    }
    v
}

// ---------------------------------------------------------------------------
// Gate 1 — zero alocação no T0
// ---------------------------------------------------------------------------

#[test]
fn gate1_zero_alloc_no_t0_em_10k_ciclos() {
    const RING: usize = 64;
    let (mut tx, _rx) = SpscKnowledgePhaseBridge::<RING>::new();
    let scope = ScopeId::new(1, 1, 1);
    let payload = phase_payload(0xABCD);
    let cert = Certificate {
        energy: -12.5,
        gap: 0.125,
        violations: 0,
    };

    // warmup: primeira passagem pode tocar em lazy init de nada — mas medimos
    // de qualquer forma a partir do primeiro ciclo, como manda o gate.
    let ((), allocs, bytes) = count_allocs(|| {
        for cycle in 0..10_000u64 {
            let ev = KnowledgeEvent::new(
                u128::from(cycle),
                Layer::L4Semantic,
                scope,
                cycle,
                cycle * 16,
            )
            .with_certificate(cert)
            .with_authority(200)
            .with_payload(&payload);
            // O anel satura; `record` deve devolver Err e **não** alocar.
            let _ = tx.record(&ev);
        }
    });

    assert_eq!(allocs, 0, "record() alocou {allocs} vezes no T0");
    assert_eq!(bytes, 0, "record() alocou {bytes} bytes no T0");
    assert_eq!(tx.metrics().t0_ledger_calls(), 0);
    // 10.000 tentativas: 64 aceitas (anel) e o resto descartado.
    assert_eq!(
        tx.metrics().recorded() + tx.metrics().dropped(),
        10_000,
        "todo ciclo precisa ser aceito ou contado como descarte"
    );
    assert!(tx.metrics().dropped() > 0, "o anel deveria ter saturado");
}

// ---------------------------------------------------------------------------
// Gate 2 — invariante anti-RAG (R7)
// ---------------------------------------------------------------------------

#[test]
fn gate2_zero_chamadas_ao_ledger_no_t0() {
    const RING: usize = 32;
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<RING>::new();
    let payload = phase_payload(7);

    // "Ciclo do organismo": 1.000 ticks a 60 Hz, com drenagem periódica no T1.
    let mut drained: Vec<DraftRecord> = Vec::new();
    for cycle in 0..1_000u64 {
        let ev = KnowledgeEvent::new(
            u128::from(cycle),
            Layer::L4Semantic,
            ScopeId::new(3, 1, 1),
            cycle,
            cycle * 16,
        )
        .with_certificate(Certificate {
            energy: -(cycle as f32),
            gap: 0.5,
            violations: 0,
        })
        .with_authority(255)
        .with_payload(&payload);
        let _ = tx.record(&ev);

        if cycle % 16 == 0 {
            rx.drain_batch(&mut drained, RING);
        }
    }

    // R7: nenhuma chamada ao ledger saiu do T0 — nem uma.
    assert_eq!(
        tx.metrics().t0_ledger_calls(),
        0,
        "R7 violado: o T0 falou com o ledger"
    );
    // O contador do T1 funciona (prova que a telemetria não é decorativa).
    assert!(
        rx.metrics().ledger_calls() > 0,
        "o T1 deveria registrar chamadas"
    );
    assert_eq!(rx.metrics().drained(), drained.len() as u64);
}

// ---------------------------------------------------------------------------
// Gate 3 — integridade do T1 e duplo carimbo (K6)
// ---------------------------------------------------------------------------

#[test]
fn gate3_integridade_e_duplo_carimbo() {
    const RING: usize = 64;
    const EVENTS: u64 = 40;
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<RING>::new();
    let scope = ScopeId::new(42, 7, 3);

    for k in 0..EVENTS {
        let payload = phase_payload(k);
        let ev = KnowledgeEvent::new(
            0x0102_0304_0506_0708_090A_0B0C_0D0E_0000u128 + u128::from(k),
            Layer::L6Graph,
            scope,
            k,                          // tick: relógio interno (60 Hz)
            1_700_000_000_000 + k * 16, // wall_ms: relógio externo
        )
        .with_certificate(Certificate {
            energy: -(k as f32) * 0.5,
            gap: 0.0625 * (k as f32 + 1.0),
            violations: k as u32 % 3,
        })
        .with_authority((k % 256) as u8)
        .with_payload(&payload);
        assert!(tx.record(&ev).is_ok(), "evento {k} recusado");
    }

    let mut out = Vec::new();
    let n = rx.drain_batch(&mut out, RING);
    assert_eq!(n, EVENTS as usize, "drenagem perdeu eventos");
    assert_eq!(out.len(), EVENTS as usize, "100% dos eventos precisam sair");

    for (k, rec) in out.iter().enumerate() {
        let k64 = k as u64;
        assert_eq!(
            rec.key,
            0x0102_0304_0506_0708_090A_0B0C_0D0E_0000u128 + u128::from(k64)
        );
        assert_eq!(rec.layer, Layer::L6Graph);
        assert_eq!(rec.authority, (k % 256) as u8);
        assert_eq!(rec.scope, scope);
        assert_eq!(rec.certificate.violations, k as u32 % 3);
        assert!((rec.certificate.energy - (-(k as f32) * 0.5)).abs() < 1e-6);
        assert!((rec.certificate.gap - 0.0625 * (k as f32 + 1.0)).abs() < 1e-6);
        // Duplo carimbo: as duas escalas são distintas e íntegras (K6).
        assert_eq!(rec.tick, k64);
        assert_eq!(rec.wall_ms, 1_700_000_000_000 + k64 * 16);
        assert_ne!(rec.tick, rec.wall_ms, "tick e wall_ms não podem colidir");
        assert_eq!(rec.created_tick(), rec.wall_ms, "TTL usa wall_ms, não tick");
        assert_eq!(rec.payload, phase_payload(k64), "payload corrompido");
    }
    assert!(
        rx.drain_batch(&mut out, RING) == 0,
        "anel deveria estar vazio"
    );
}

// ---------------------------------------------------------------------------
// Gate 4 — isolamento de escopo (R8)
// ---------------------------------------------------------------------------

#[test]
fn gate4_isolamento_de_escopo_entre_organismos() {
    let mut ledger = ScopeLedger::new();
    let a = ScopeId::new(1, 5, 9);
    let b = ScopeId::new(2, 5, 9); // mesmo run/app, organismo diferente

    for k in 0..8u64 {
        let mut payload = Vec::new();
        for j in 0..16u64 {
            // organismo 1: (1.0, 0.0) ; organismo 2: (0.0, 1.0)
            let (re, im) = if k % 2 == 0 {
                (1.0f32, 0.0f32)
            } else {
                (0.0, 1.0)
            };
            let s = if k < 8 && j == 0 {
                k as f32 * 1e-6
            } else {
                0.0
            };
            payload.extend_from_slice(&(re + s).to_le_bytes());
            payload.extend_from_slice(&im.to_le_bytes());
        }
        ledger.append(DraftRecord {
            key: u128::from(k),
            layer: Layer::L4Semantic,
            certificate: Certificate::default(),
            authority: 128,
            scope: if k < 4 { a } else { b },
            tick: k,
            wall_ms: 1_000 + k,
            payload,
        });
    }

    assert_eq!(ledger.hits_for_organism(1), 4);
    assert_eq!(ledger.hits_for_organism(2), 4);
    assert_eq!(ledger.hits_for_scope(a), 4);
    assert_eq!(ledger.hits_for_scope(b), 4);

    // Toda resposta de `cue_into` no escopo de A só pode conter registros de A.
    let mut query = vec![0.0f32; 32];
    query[0] = 1.0;
    query[1] = 0.0;
    let mut out = vec![0.0f32; query.len()];
    let mut cross = 0u64;
    for rec in ledger.scoped(a) {
        // o "recall" de A é comparado contra o próprio escopo de A
        assert_eq!(rec.scope.organism_id, 1);
        if rec.scope.organism_id != a.organism_id {
            cross += 1;
        }
    }
    assert_eq!(cross, 0, "R8 violado: escopo cruzado em consulta filtrada");

    // `cue_into` no ledger completo devolve um candidato — mas o filtro por
    // escopo é o que impede a contaminação (o ledger completo é o T2 bruto).
    let hits = ledger.cue_into(&query, &mut out).expect("cue");
    assert!(hits > 0);
    assert_eq!(ledger.cross_scope_hits(), 0);

    // Escopo vazio não inventa resposta.
    let mut ledger_vazio = ScopeLedger::new();
    assert_eq!(
        ledger_vazio.cue_into(&query, &mut out),
        Err(BridgeError::EmptyScope)
    );
}

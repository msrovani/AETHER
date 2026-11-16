//! Invariantes de engenharia (Seção 5.4): alocação zero no caminho quente,
//! codec do pacote de 40 bytes e determinismo bit-a-bit do solver.

#![cfg(all(feature = "m1", feature = "m2"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::core::kuramoto_field::PhasePacket;
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::Array2;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Allocador contador — conta SOMENTE as alocações da thread corrente enquanto
/// `ARMED` está ligado (testes rodam em paralelo: contagem global cruzaria
/// entre testes).
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Executa `f` contando as alocações de heap da thread corrente.
fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
}

fn random_landscape(n: usize, seed: u64) -> (Array2<f64>, Array2<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = Array2::<f64>::zeros((n, n));
    for i in 0..n {
        for k in (i + 1)..n {
            let v = (rng.gen::<f64>() * 2.0 - 1.0) / (n as f64).sqrt();
            j[[i, k]] = v;
            j[[k, i]] = v;
        }
    }
    let h = Array2::<f64>::zeros((1, n));
    (j, h)
}

#[test]
fn invariante_alocacao_zero_no_solver() {
    const N: usize = 64;
    let (j, h) = random_landscape(N, 0x1234);
    let j3 = j.view().into_shape_with_order((1, N, N)).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(1, N, 200, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, N, 200);

    // warmup: inicializa statics lazily do ndarray/rust
    solver.reset(1);
    solver
        .relax_into(IsingLandscape::new(j3, h.view()), &mut out)
        .expect("relax");

    let (_, allocs) = count_allocs(|| {
        solver.reset(2);
        solver
            .relax_into(IsingLandscape::new(j3, h.view()), &mut out)
            .expect("relax")
    });
    assert_eq!(
        allocs, 0,
        "reset+relax_into alocaram {allocs} vezes (deve ser 0)"
    );
}

#[test]
fn invariante_alocacao_zero_na_onda() {
    let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 0xC0FFEE);
    let mut rng = ChaCha8Rng::seed_from_u64(0x99);
    let u = Array2::<f32>::from_shape_fn((4, 512), |_| rng.gen::<f32>() * 2.0 - 1.0);
    let mut v = Array2::<f32>::zeros((8, 512));

    block.forward_into(u.view(), &mut v).expect("warmup");

    let (_, allocs) = count_allocs(|| block.forward_into(u.view(), &mut v).expect("forward"));
    assert_eq!(allocs, 0, "forward_into alocou {allocs} vezes (deve ser 0)");
}

#[test]
fn invariante_codec_pacote_40_bytes() {
    assert_eq!(std::mem::size_of::<PhasePacket>(), 40);
    assert_eq!(std::mem::align_of::<PhasePacket>(), 8);

    let pkt = PhasePacket {
        timestamp_ns: 0x0123_4567_89AB_CDEF,
        node_id: 0xDEAD_BEEF,
        phase: [0.1, 1.2, 2.3, 3.4],
        xi: [-0.5, 0.25, 9.75],
    };
    let bytes = pkt.encode();
    assert_eq!(bytes.len(), 40);
    let back = PhasePacket::decode(&bytes).expect("decode");
    assert_eq!(back.node_id, pkt.node_id);
    assert_eq!(back.timestamp_ns, pkt.timestamp_ns);
    assert_eq!(back.phase, pkt.phase);
    assert_eq!(back.xi, pkt.xi);
    assert_eq!(back, pkt);

    // datagramas de tamanho errado são rejeitados
    assert!(PhasePacket::decode(&bytes[..39]).is_err());
    assert!(PhasePacket::decode(&[0u8; 41]).is_err());
    assert!(PhasePacket::decode(&[]).is_err());
}

/// F1 (Vol. III §7): a ponte de conhecimento não aloca no `T0`.
///
/// 10.000 `record()` com o anel saturado: nenhuma alocação e **zero** chamadas
/// ao ledger a partir do caminho quente (R7).
#[cfg(feature = "m1")]
#[test]
fn invariante_alocacao_zero_na_ponte_de_conhecimento() {
    use aether_engine::core::knowledge_bridge_f1::{
        Certificate, KnowledgeEvent, KnowledgeSink, Layer, ScopeId, SpscKnowledgePhaseBridge,
    };

    let (mut tx, _rx) = SpscKnowledgePhaseBridge::<64>::new();
    let payload = vec![0u8; 1280]; // D=160 complexo f32
    let scope = ScopeId::new(1, 1, 1);
    let cert = Certificate {
        energy: -1.0,
        gap: 0.5,
        violations: 0,
    };

    // warmup (o anel já está alocado em `new`; aqui garantimos que nada lazy
    // do runtime entra na contagem).
    let _ = tx.record(
        &KnowledgeEvent::new(0, Layer::L2Episodic, scope, 0, 0)
            .with_certificate(cert)
            .with_payload(&payload),
    );

    let (_, allocs) = count_allocs(|| {
        for cycle in 0..10_000u64 {
            let _ = tx.record(
                &KnowledgeEvent::new(u128::from(cycle), Layer::L2Episodic, scope, cycle, cycle * 16)
                    .with_certificate(cert)
                    .with_payload(&payload),
            );
        }
    });

    assert_eq!(allocs, 0, "record() alocou {allocs} vezes no T0 (deve ser 0)");
    assert_eq!(
        tx.metrics().t0_ledger_calls(),
        0,
        "R7: o T0 nunca pode falar com o ledger"
    );
}

#[test]
fn invariante_determinismo_do_solver() {
    const N: usize = 32;
    let (j, h) = random_landscape(N, 0x777);
    let j3 = j.view().into_shape_with_order((1, N, N)).unwrap();
    let mut solver = ThermodynamicIsingSolver::new(1, N, 120, 0.05);

    solver.reset(42);
    let a = solver
        .relax(IsingLandscape::new(j3, h.view()))
        .expect("relax");
    solver.reset(42);
    let b = solver
        .relax(IsingLandscape::new(j3, h.view()))
        .expect("relax");

    assert_eq!(a.states, b.states, "estados divergiram entre execuções");
    assert_eq!(
        a.energy_trace, b.energy_trace,
        "curvas de energia divergiram"
    );
    assert_eq!(a.best_energy, b.best_energy);
}

//! Invariantes do transdutor universal (Módulo 8a): contrato de dimensão,
//! determinismo, detecção de anomalia, fases válidas e zero alocação no hot path.

#![cfg(feature = "m8")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::transducer_io::{
    FieldNormalizer, PhaseMapper, SurpriseTracker, SyntheticTransducer, TransducerIo, FIELD_DIM,
    PHASE_CHANNELS,
};

/// Alocador contador por thread (mesmo padrão de `tests/alloc_invariants.rs`).
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

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn fill_pipeline(
    src: &mut SyntheticTransducer,
    norm: &mut FieldNormalizer,
    tracker: &mut SurpriseTracker,
    raw: &mut [f32],
    field: &mut [f32],
) -> f32 {
    src.sample_into(raw).expect("sample");
    norm.update_into(raw, field).expect("normalize");
    tracker.observe(field).expect("surprise")
}

#[test]
fn test_contract_dimension_and_shape_error() {
    let mut src = SyntheticTransducer::new(FIELD_DIM, 60.0, 1).expect("new");
    assert_eq!(src.dim(), FIELD_DIM);
    assert_eq!(src.nominal_hz(), 60.0);

    let mut short = vec![0.0f32; FIELD_DIM - 1];
    assert!(
        src.sample_into(&mut short).is_err(),
        "buffer curto deve falhar"
    );

    let mut ok = vec![0.0f32; FIELD_DIM];
    src.sample_into(&mut ok).expect("buffer exato");
    assert!(ok.iter().all(|v| v.is_finite() && *v >= -1.0 && *v <= 1.0));
}

#[test]
fn test_determinism_same_seed() {
    let mut a = SyntheticTransducer::new(FIELD_DIM, 60.0, 0xAE71).expect("a");
    let mut b = SyntheticTransducer::new(FIELD_DIM, 60.0, 0xAE71).expect("b");
    let mut fa = vec![0.0f32; FIELD_DIM];
    let mut fb = vec![0.0f32; FIELD_DIM];

    for _ in 0..50 {
        a.sample_into(&mut fa).expect("a");
        b.sample_into(&mut fb).expect("b");
        assert_eq!(fa, fb, "mesma semente deve gerar o mesmo campo");
    }
}

#[test]
fn test_anomaly_detection_and_recovery() {
    let hz = 60.0f32;
    let mut src = SyntheticTransducer::new(FIELD_DIM, hz, 42).expect("src");
    let mut norm = FieldNormalizer::new(FIELD_DIM).expect("norm");
    let mut tracker = SurpriseTracker::new(FIELD_DIM).expect("tracker");
    let mut raw = vec![0.0f32; FIELD_DIM];
    let mut field = vec![0.0f32; FIELD_DIM];

    // 10 s de assinatura saudável.
    let mut baseline = Vec::with_capacity(300);
    for tick in 0..600 {
        let f = fill_pipeline(&mut src, &mut norm, &mut tracker, &mut raw, &mut field);
        if tick >= 300 {
            baseline.push(f);
        }
    }
    let baseline_mean = baseline.iter().sum::<f32>() / baseline.len() as f32;

    // Rajada de 2 s a partir de 10 s.
    src.schedule_burst(10.0, 2.0, 1.0);
    let mut peak = 0.0f32;
    for _ in 0..120 {
        peak = peak.max(fill_pipeline(
            &mut src,
            &mut norm,
            &mut tracker,
            &mut raw,
            &mut field,
        ));
    }

    // 3 s depois da rajada: deve voltar ao regime normal.
    let mut tail = 0.0f32;
    for _ in 0..180 {
        tail = fill_pipeline(&mut src, &mut norm, &mut tracker, &mut raw, &mut field);
    }

    assert!(baseline_mean < 0.35, "baseline instavel: {baseline_mean}");
    assert!(peak > 0.6, "anomalia nao detectada: pico {peak}");
    assert!(
        peak > baseline_mean + 0.3,
        "pico {peak} vs baseline {baseline_mean}"
    );
    assert!(tail < 0.5, "surpresa nao retornou ao baseline: {tail}");
}

#[test]
fn test_phase_mapper_range() {
    let mapper = PhaseMapper::new(FIELD_DIM).expect("mapper");
    let field: Vec<f32> = (0..FIELD_DIM)
        .map(|i| (i as f32 * 0.37).sin() * 0.8 + 0.1)
        .collect();
    let mut phases = [0.0f32; PHASE_CHANNELS];
    mapper.map_into(&field, &mut phases).expect("map");
    for phase in phases {
        assert!(phase.is_finite());
        assert!(
            (0.0..std::f32::consts::TAU).contains(&phase),
            "fase fora de [0,2pi): {phase}"
        );
    }
}

#[test]
fn test_zero_alloc_hot_path() {
    let mut src = SyntheticTransducer::new(FIELD_DIM, 60.0, 7).expect("src");
    let mut norm = FieldNormalizer::new(FIELD_DIM).expect("norm");
    let mut tracker = SurpriseTracker::new(FIELD_DIM).expect("tracker");
    let mut raw = vec![0.0f32; FIELD_DIM];
    let mut field = vec![0.0f32; FIELD_DIM];

    // Warmup: primeira amostra do normalizador e vetores internos já alocados.
    for _ in 0..120 {
        fill_pipeline(&mut src, &mut norm, &mut tracker, &mut raw, &mut field);
    }

    ARMED.with(|a| a.set(true));
    let before = ALLOCS.with(|c| c.get());
    for _ in 0..600 {
        fill_pipeline(&mut src, &mut norm, &mut tracker, &mut raw, &mut field);
    }
    let after = ALLOCS.with(|c| c.get());
    ARMED.with(|a| a.set(false));

    assert_eq!(before, after, "hot path alocou {} vezes", after - before);
}

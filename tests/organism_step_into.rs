//! M4 — contrato de [`ClosedLoopOrganism::step_into`]:
//!
//! * `INV-O1`: `step_into` preenche um `OrganismFrame` pré-dimensionado
//!   (`with_capacity`) **sem alocar** após o warmup (contador thread-local).
//! * `INV-O2`: formas erradas de `out` retornam [`AetherError::Shape`] sem
//!   redimensionar buffers.
//! * `INV-O3`: `step()` e `step_into()` produzem o **mesmo quadro** (mesma
//!   semente, mesma física).
//!
//! Para `INV-O3` a física dos nós é congelada (`ω = 0`, `K = 0`): como a fase
//! inicial dos nós é determinística por `node_id`, o campo sensorial, a
//! entropia, `h(t)` e o colapso termodinâmico ficam reprodutíveis entre duas
//! instâncias — sem depender do instante em que cada quadro foi lido.

#![cfg(feature = "m4")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Duration;

use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};
use aether_engine::AetherError;

// ---------------------------------------------------------------------------
// Alocador contador
//
// A contagem é *thread-local* (padrão de `tests/alloc_invariants.rs`): o
// harness e os outros testes deste binário rodam em threads paralelas e um
// contador global cruzaria as contagens. `Cell` const-inicializado não aloca
// dentro de `alloc`, logo não há recursão.
// ---------------------------------------------------------------------------

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

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

// ---------------------------------------------------------------------------
// Utilitários
// ---------------------------------------------------------------------------

/// Organismo pequeno e reprodutível.
///
/// `ω = 0` e `K = 0` congelam a fase dos nós (a fase inicial é determinística
/// por `node_id`), tornando o quadro inteiro reprodutível entre instâncias.
fn test_config() -> OrganismConfig {
    OrganismConfig {
        decision_dim: 16,
        sensor_channels: 4,
        window: 64,
        steps: 20,
        hz: 60.0,
        perturb_after: Duration::from_secs(3_600),
        seed: 0xA37E_0002,
        k_coupling: 0.0,
        omega: [0.0; 4],
        ..Default::default()
    }
}

fn new_organism(cfg: &OrganismConfig) -> ClosedLoopOrganism {
    ClosedLoopOrganism::new(cfg.clone()).expect("organismo")
}

fn assert_close(a: f64, b: f64, what: &str) {
    let tol = 1e-9 * a.abs().max(b.abs()).max(1.0);
    assert!(
        (a - b).abs() <= tol,
        "{what}: {a} != {b} (diferença {})",
        (a - b).abs()
    );
}

// ---------------------------------------------------------------------------
// INV-O1 — zero alocação após o warmup
// ---------------------------------------------------------------------------

#[test]
fn step_into_alocacao_zero_apos_warmup() {
    let cfg = test_config();
    let mut org = new_organism(&cfg);

    // deixa os nós publicarem quadros e ouvirem os peers (caminho quente real)
    std::thread::sleep(Duration::from_millis(300));

    let mut out = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);
    org.step_into(&mut out).expect("warmup");

    let (res, allocs) = count_allocs(|| org.step_into(&mut out));
    res.expect("step_into");
    assert_eq!(
        allocs, 0,
        "step_into alocou {allocs} vezes após o warmup (deve ser 0)"
    );

    // o quadro foi realmente preenchido, não ficou zerado
    assert_eq!(out.energy_trace.len(), cfg.steps + 1);
    assert_eq!(out.states.len(), cfg.decision_dim);
    assert!(out.energy.is_finite());
    assert!(out.energy_trace.iter().all(|v| v.is_finite()));
}

// ---------------------------------------------------------------------------
// INV-O2 — formas erradas são rejeitadas sem redimensionar
// ---------------------------------------------------------------------------

#[test]
fn step_into_rejeita_formas_erradas_sem_redimensionar() {
    let cfg = test_config();
    let mut org = new_organism(&cfg);

    let mut trace_curta = OrganismFrame::with_capacity(cfg.steps - 1, cfg.decision_dim);
    let trace_len = trace_curta.energy_trace.len();
    let err = org.step_into(&mut trace_curta).unwrap_err();
    assert!(matches!(err, AetherError::Shape { .. }), "veio {err:?}");
    assert_eq!(
        trace_curta.energy_trace.len(),
        trace_len,
        "energia não pode ser redimensionada no erro"
    );

    let mut states_longo = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim + 1);
    let states_len = states_longo.states.len();
    let err = org.step_into(&mut states_longo).unwrap_err();
    assert!(matches!(err, AetherError::Shape { .. }), "veio {err:?}");
    assert_eq!(
        states_longo.states.len(),
        states_len,
        "estados não podem ser redimensionados no erro"
    );
}

// ---------------------------------------------------------------------------
// INV-O3 — `step()` == `step_into()` com a mesma semente/física
// ---------------------------------------------------------------------------

#[test]
fn step_e_step_into_equivalentes() {
    let cfg = test_config();
    let mut a = new_organism(&cfg);
    let mut b = new_organism(&cfg);

    // nós aquecidos em ambas as instâncias (peers ouvidos, R estável)
    std::thread::sleep(Duration::from_millis(300));

    // warmup simétrico: A via `step`, B via `step_into`
    let _ = a.step().expect("warmup step");
    let mut warm = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);
    b.step_into(&mut warm).expect("warmup step_into");

    let fa = a.step().expect("step");
    let mut fb = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);
    b.step_into(&mut fb).expect("step_into");

    // formas
    assert_eq!(fa.energy_trace.len(), cfg.steps + 1);
    assert_eq!(fb.energy_trace.len(), cfg.steps + 1);
    assert_eq!(fa.states.len(), cfg.decision_dim);
    assert_eq!(fb.states.len(), cfg.decision_dim);

    // colapso termodinâmico
    assert_eq!(fa.energy, fb.energy, "energy divergiu");
    assert_eq!(
        fa.energy_initial, fb.energy_initial,
        "energy_initial divergiu"
    );
    assert_eq!(fa.states, fb.states, "states divergiram");
    for (i, (x, y)) in fa
        .energy_trace
        .iter()
        .zip(fb.energy_trace.iter())
        .enumerate()
    {
        assert_eq!(x, y, "energy_trace[{i}] divergiu");
    }

    // sensores / onda / leitura
    assert_close(fa.order_parameter, fb.order_parameter, "order_parameter");
    for i in 0..3 {
        assert_close(fa.r_per_node[i], fb.r_per_node[i], "r_per_node");
    }
    assert_close(fa.entropy_raw, fb.entropy_raw, "entropy_raw");
    assert_close(fa.entropy_wave, fb.entropy_wave, "entropy_wave");
    assert_close(fa.phase_bancada[0], fb.phase_bancada[0], "phase_bancada[0]");
    assert_eq!(fa.phase_bancada, fb.phase_bancada, "phase_bancada divergiu");

    // metadados
    assert!(!fa.perturbed && !fb.perturbed);
    assert_eq!(fa.dropped_frames, fb.dropped_frames);
    // os estados caem em {-1, +1}
    assert!(fa.states.iter().all(|s| *s == -1 || *s == 1));
}

// ---------------------------------------------------------------------------
// Inércia do atuador: comando contínuo suavizado, estado discreto intacto.
// ---------------------------------------------------------------------------

#[test]
fn inercia_do_atuador_reduz_chattering_sem_tocar_no_estado() {
    let cfg = test_config();
    let mut org = new_organism(&cfg);
    let mut frame = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);

    org.step_into(&mut frame).expect("warmup");
    let mut prev_states = frame.states.clone();
    let mut state_chatter = 0.0f32;
    let mut act_chatter = 0.0f32;
    let mut fractionario = false;

    for step in 0..8 {
        org.step_into(&mut frame).expect("step");

        // O estado emergente continua EXATAMENTE discreto.
        assert!(
            frame.states.iter().all(|s| *s == 1 || *s == -1),
            "estado saiu de {{-1,+1}} no passo {step}"
        );
        // O comando contínuo fica no intervalo físico e é finito.
        assert!(
            frame
                .actuation
                .iter()
                .all(|u| u.is_finite() && u.abs() <= 1.0 + 1e-6),
            "atuação fora de [-1,1] no passo {step}"
        );
        if frame
            .actuation
            .iter()
            .any(|u| u.abs() > 0.05 && u.abs() < 0.95)
        {
            fractionario = true;
        }

        let mut ds = 0.0f32;
        for i in 0..cfg.decision_dim {
            let d = frame.states[i] as f32 - prev_states[i] as f32;
            ds += d * d;
        }
        state_chatter += ds.sqrt();
        act_chatter += frame.actuation_delta;
        prev_states.clone_from_slice(&frame.states);
    }

    assert!(
        state_chatter > 0.0,
        "o estado não oscilou: o teste não exercita a inércia"
    );
    assert!(
        fractionario,
        "a atuação ficou idêntica ao estado discreto (filtro não agiu)"
    );
    println!(
        "INERCIA: chattering atuacao={act_chatter:.3} estado={state_chatter:.3} razao={:.3} (alpha={:.4})",
        act_chatter / state_chatter,
        (1.0 / 60.0) / (0.1 + 1.0 / 60.0)
    );
    assert!(
        act_chatter < 0.5 * state_chatter,
        "inércia insuficiente: atuação {act_chatter:.3} vs estado {state_chatter:.3}"
    );
}

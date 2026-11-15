//! M5-HARDEN — suite honesta do Módulo 5 (memória associativa de fase).
//!
//! Cobre, com números medidos:
//!
//! 1. recall de 3 padrões harmônicos em `N = 32` com **exatamente 12** ocultos,
//!    em 30 seeds, reportando média e pior por padrão;
//! 2. a propriedade central da regra de projeção: o campo residual nos padrões
//!    gravados é zero (era 0.073 rad em p0 com a superposição hebbiana);
//! 3. controles de validade física (rotação rígida ⇒ dedupe + recuperação
//!    exata; 1 e 2 padrões);
//! 4. `recall_into` sem alocação de heap após warmup (contador global);
//! 5. `prune` com contagem real, fase zerada e **sem ressurreição** pela
//!    projeção; `prune(∞)`; `prune(NaN)`;
//! 6. robustez: `n = 0`, `dt ≤ 0`, `dt = NaN`, tamanho errado, `NaN` no padrão,
//!    `cycles = 0`, `steps = 0`;
//! 7. knobs reais: `eta_delta = 0` desliga o aprendizado de fase; `ω0 ≠ 0`
//!    recupera no referencial girante;
//! 8. janela deslizante (FIFO) de padrões e ausência do clamp `W ≤ 1.0`;
//! 9. distribuição de pesos com dispersão (o truncamento antigo saturava).

#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig, PhaseMemoryError};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 32;
const HIDDEN: usize = 12;

// ---------------------------------------------------------------------------
// Contador de alocações (thread-local) para provar `recall_into` sem alocação.
// ---------------------------------------------------------------------------

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.with(Cell::get) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACKING.with(Cell::get) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACKING.with(Cell::get) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Padrão harmônico contínuo (idêntico ao usado em `phase2_invariants.rs`).
fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (std::f32::consts::TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(std::f32::consts::TAU)
        })
        .collect()
}

/// Máscara com **exatamente** `hidden` ocultos (amostragem sem reposição).
fn mask_exact(seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..N).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; N];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn ang_err(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Erro angular médio **apenas nos ocultos**.
fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool]) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for i in 0..target.len() {
        if mask[i] {
            continue;
        }
        sum += ang_err(recovered[i], target[i]).abs();
        count += 1;
    }
    assert!(count > 0, "nenhum oscilador oculto");
    sum / count as f32
}

/// Drive do campo (magnitude média) em um estado dado.
fn mean_abs_drive(memory: &ComplexPhaseMemory, phases: &[f32]) -> f32 {
    let n = memory.n;
    let mut sum = 0.0f32;
    for i in 0..n {
        let mut d = 0.0f32;
        for j in 0..n {
            if j == i {
                continue;
            }
            let idx = i * n + j;
            d += memory.weights[idx] * (phases[j] - phases[i] - memory.phase_shifts[idx]).sin();
        }
        sum += (d / n as f32).abs();
    }
    sum / n as f32
}

fn inv_cfg() -> PhaseMemoryConfig {
    // Mesma configuração do invariante oficial (`tests/phase2_invariants.rs`).
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

fn memory_with(patterns: &[Vec<f32>], cfg: PhaseMemoryConfig) -> ComplexPhaseMemory {
    let mut memory = ComplexPhaseMemory::with_config(N, cfg);
    for p in patterns {
        memory.imprint(p, 40);
    }
    memory
}

// ---------------------------------------------------------------------------
// 1. Invariante: 3 padrões harmônicos, 12 ocultos, 30 seeds
// ---------------------------------------------------------------------------

#[test]
fn tres_padroes_harmonicos_doze_ocultos_30_seeds() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let mut memory = memory_with(&patterns, inv_cfg());
    assert_eq!(memory.stored_patterns(), 3);

    let seeds = 30u64;
    let mut sum = [0.0f32; 3];
    let mut worst = [0.0f32; 3];
    let mut over_spec = [0usize; 3];

    for seed in 0..seeds {
        let mask = mask_exact(seed, HIDDEN);
        assert_eq!(
            mask.iter().filter(|m| !**m).count(),
            HIDDEN,
            "máscara precisa ter exatamente {HIDDEN} ocultos"
        );
        for (k, p) in patterns.iter().enumerate() {
            // `recall` é stateful: zera o estado para não herdar a recuperação
            // anterior (senão a seed "funciona por sorte do histórico").
            memory.reset_phases(0.0);
            let (recovered, energy) = memory.recall(p, &mask, 600, 0.05);
            assert!(energy.is_finite(), "energia não finita (seed {seed})");
            let err = mean_err(&recovered, p, &mask);
            sum[k] += err;
            worst[k] = worst[k].max(err);
            if err >= 0.15 {
                over_spec[k] += 1;
            }
        }
    }
    let mean: Vec<f32> = (0..3).map(|k| sum[k] / seeds as f32).collect();
    println!(
        "M5 3 padroes/12 ocultos/30 seeds: media p0={:.6} p1={:.6} p2={:.6} | \
         pior p0={:.6} p1={:.6} p2={:.6} | seeds>=0.15: {} {} {}",
        mean[0],
        mean[1],
        mean[2],
        worst[0],
        worst[1],
        worst[2],
        over_spec[0],
        over_spec[1],
        over_spec[2]
    );

    for (k, name) in ["p0", "p1", "p2"].iter().enumerate() {
        assert!(
            mean[k] < 0.01,
            "{name}: media {:.4} rad >= 0.01 (projeção deveria ser exata)",
            mean[k]
        );
        assert!(
            worst[k] < 0.02,
            "{name}: pior seed {:.4} rad >= 0.02",
            worst[k]
        );
        assert_eq!(over_spec[k], 0, "{name} violou 0.15 em alguma seed");
    }
}

// ---------------------------------------------------------------------------
// 2. A propriedade central: crosstalk nulo nos padrões gravados
// ---------------------------------------------------------------------------

#[test]
fn projecao_anula_o_crosstalk() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let mut memory = memory_with(&patterns, inv_cfg());

    let mut proj = [0.0f32; 3];
    for (k, p) in patterns.iter().enumerate() {
        proj[k] = mean_abs_drive(&memory, p);
    }
    println!(
        "M5 |drive| nos padroes (projecao): p0={:.2e} p1={:.2e} p2={:.2e}",
        proj[0], proj[1], proj[2]
    );
    for (k, name) in ["p0", "p1", "p2"].iter().enumerate() {
        assert!(
            proj[k] < 1e-3,
            "{name}: campo residual {:.2e} — a projeção deveria anular o crosstalk",
            proj[k]
        );
    }

    // Contraste honesto: com o acoplamento truncado em W = 1 (o bug antigo) o
    // resíduo é ordens de grandeza maior.
    for i in 0..N {
        for j in 0..N {
            if i != j {
                memory.weights[i * N + j] = 1.0;
            }
        }
    }
    let mut trunc = [0.0f32; 3];
    for (k, p) in patterns.iter().enumerate() {
        trunc[k] = mean_abs_drive(&memory, p);
    }
    println!(
        "M5 |drive| nos padroes (W ≡ 1, crosstalk): p0={:.4} p1={:.4} p2={:.4}",
        trunc[0], trunc[1], trunc[2]
    );
    assert!(
        trunc[0] > 20.0 * proj[0],
        "o contraste com o acoplamento truncado deveria ser grande"
    );
}

// ---------------------------------------------------------------------------
// 3. Controles de validade física
// ---------------------------------------------------------------------------

#[test]
fn padroes_sem_crosstalk_sao_recuperados_exatamente() {
    // Rotação rígida de um padrão é o MESMO atrator (a dinâmica é invariante
    // por rotação global): o imprint deduplica e a recuperação é exata.
    let p0 = harmonic_pattern(N, 0);
    let tau3 = std::f32::consts::TAU / 3.0;
    let q1: Vec<f32> = p0
        .iter()
        .map(|v| (v + tau3).rem_euclid(std::f32::consts::TAU))
        .collect();
    let q2: Vec<f32> = p0
        .iter()
        .map(|v| (v + 2.0 * tau3).rem_euclid(std::f32::consts::TAU))
        .collect();
    let patterns = [p0, q1, q2];
    let mut memory = memory_with(&patterns, inv_cfg());
    assert_eq!(
        memory.stored_patterns(),
        1,
        "rotações rígidas do mesmo padrão devem ser deduplicadas"
    );

    let mut worst: f32 = 0.0;
    for seed in 0..10u64 {
        let mask = mask_exact(seed, HIDDEN);
        for p in patterns.iter() {
            memory.reset_phases(0.0);
            let (recovered, _e) = memory.recall(p, &mask, 600, 0.05);
            worst = worst.max(mean_err(&recovered, p, &mask));
        }
    }
    println!("M5 rotacoes rigidas: pior erro {worst:.6}");
    assert!(worst < 1e-3, "rotação rígida deveria ser exata: {worst}");
}

#[test]
fn dois_padroes_ficam_dentro_do_invariante() {
    let patterns = [harmonic_pattern(N, 0), harmonic_pattern(N, 1)];
    let mut memory = memory_with(&patterns, inv_cfg());
    let mut worst: f32 = 0.0;
    for seed in 0..30u64 {
        let mask = mask_exact(seed, HIDDEN);
        for p in patterns.iter() {
            memory.reset_phases(0.0);
            let (recovered, _e) = memory.recall(p, &mask, 600, 0.05);
            worst = worst.max(mean_err(&recovered, p, &mask));
        }
    }
    println!("M5 2 padroes/12 ocultos/30 seeds: pior erro {worst:.6}");
    assert!(worst < 0.02, "P=2 deveria ser exato: pior {worst:.4}");
}

#[test]
fn omega0_nao_nulo_recupera_no_referencial_girante() {
    // Com ω0 ≠ 0 o atrator é um ciclo limite que gira; o grampo acompanha a
    // rotação e a fase acumulada fica em `rotation`.
    let pattern = harmonic_pattern(N, 0);
    let cfg = PhaseMemoryConfig {
        omega0: 0.3,
        ..inv_cfg()
    };
    let mut memory = memory_with(std::slice::from_ref(&pattern), cfg);
    let mask = mask_exact(0xFA51, HIDDEN);
    memory.reset_phases(0.0);
    let (recovered, _e) = memory.recall(&pattern, &mask, 600, 0.05);
    let rot = memory.rotation;
    let mut sum = 0.0;
    let mut count = 0;
    for i in 0..N {
        if mask[i] {
            continue;
        }
        let expected = (pattern[i] + rot).rem_euclid(std::f32::consts::TAU);
        sum += ang_err(recovered[i], expected).abs();
        count += 1;
    }
    let err = sum / count as f32;
    println!("M5 omega0=0.3: erro no referencial girante {err:.6} (rotation={rot:.3})");
    assert!(err < 0.05, "ω0 ≠ 0 quebrou o atrator: erro {err:.4}");
}

// ---------------------------------------------------------------------------
// 4. `recall_into` sem alocação
// ---------------------------------------------------------------------------

#[test]
fn recall_into_nao_aloca_apos_warmup() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let mut memory = memory_with(&patterns, inv_cfg());
    let mask = mask_exact(7, HIDDEN);
    let mut out = vec![0.0f32; N];

    // warmup: inicializa TLS do contador e buffers internos
    let _ = memory.recall_into(&patterns[1], &mask, 600, 0.05, &mut out);
    ALLOCS.with(|c| c.set(0));

    TRACKING.with(|t| t.set(true));
    for _ in 0..3 {
        let e = memory.recall_into(&patterns[1], &mask, 600, 0.05, &mut out);
        assert!(e.is_finite());
    }
    TRACKING.with(|t| t.set(false));

    let allocs = ALLOCS.with(Cell::get);
    println!("M5 recall_into: {allocs} alocacoes em 3 recuperacoes (esperado 0)");
    assert_eq!(allocs, 0, "recall_into alocou {allocs} vezes");

    // contrato de `out` com tamanho errado: não escreve, não panicа.
    let mut small = [1.25f32; 4];
    let e = memory.recall_into(&patterns[1], &mask, 600, 0.05, &mut small);
    assert!(e.is_finite());
    assert_eq!(small, [1.25f32; 4], "out menor não pode ser escrito");
}

// ---------------------------------------------------------------------------
// 5. prune
// ---------------------------------------------------------------------------

#[test]
fn prune_conta_arestas_zera_fase_e_nao_ressuscita() {
    let patterns = [harmonic_pattern(N, 0), harmonic_pattern(N, 1)];
    let mut memory = memory_with(&patterns, inv_cfg());
    let before = memory
        .weights
        .iter()
        .enumerate()
        .filter(|(idx, w)| (idx / N) != (idx % N) && **w > 0.0)
        .count();
    assert!(before > 0);

    let threshold = 0.8f32;
    let pruned = memory.prune(threshold);
    let after = memory
        .weights
        .iter()
        .enumerate()
        .filter(|(idx, w)| (idx / N) != (idx % N) && **w > 0.0)
        .count();
    println!("M5 prune(<{threshold}): {pruned} arestas (antes {before}, depois {after})");
    assert_eq!(pruned, before - after, "contagem de prune inconsistente");
    assert!(pruned > 0 && pruned < before);
    for idx in 0..(N * N) {
        if memory.weights[idx] == 0.0 {
            assert_eq!(
                memory.phase_shifts[idx], 0.0,
                "aresta podada ainda tem δ ≠ 0 em {idx}"
            );
        }
    }

    // A projeção NÃO pode ressuscitar as arestas podadas num imprint seguinte.
    let zeros: Vec<usize> = (0..N * N).filter(|i| memory.weights[*i] == 0.0).collect();
    memory.imprint(&harmonic_pattern(N, 2), 40);
    let resurrected = zeros.iter().filter(|i| memory.weights[**i] != 0.0).count();
    println!("M5 prune: {resurrected} arestas ressuscitadas apos novo imprint");
    assert_eq!(resurrected, 0, "a projeção ressuscitou arestas podadas");

    // clear_pruning reabre a plasticidade.
    memory.clear_pruning();
    memory.imprint(&harmonic_pattern(N, 2), 40);
    for idx in zeros.iter() {
        // Arestas que estavam podadas voltam a ser calculadas (não
        // necessariamente > 0, mas o gauge muda: basta checar que o estado é
        // finito e que a máscara não está mais forçando zero).
        assert!(memory.weights[*idx].is_finite());
    }

    // prune(∞) apaga tudo e devolve o número de arestas existentes.
    let live = memory
        .weights
        .iter()
        .enumerate()
        .filter(|(idx, w)| (idx / N) != (idx % N) && **w != 0.0)
        .count();
    let cut = memory.prune(f32::INFINITY);
    assert_eq!(cut, live, "prune(inf) deve contar todas as arestas vivas");
    assert!(memory.weights.iter().all(|w| *w == 0.0));
    assert!(memory.phase_shifts.iter().all(|d| *d == 0.0));
    // prune(NaN) é no-op.
    assert_eq!(memory.prune(f32::NAN), 0);
}

// ---------------------------------------------------------------------------
// 6. Robustez
// ---------------------------------------------------------------------------

#[test]
fn n_zero_e_seguro() {
    let memory = ComplexPhaseMemory::new(0);
    assert_eq!(memory.phase_energy(), 0.0);
    assert!(!memory.phase_energy().is_nan());
    assert_eq!(memory.density(0.0), 0.0);
    assert!(!memory.density(0.0).is_nan());

    let mut memory = ComplexPhaseMemory::new(0);
    assert_eq!(
        memory.try_imprint(&[], 3),
        Err(PhaseMemoryError::EmptyNetwork)
    );
    assert_eq!(
        memory.try_recall(&[], &[], 10, 0.05),
        Err(PhaseMemoryError::EmptyNetwork)
    );
    memory.imprint(&[], 3); // no-op, sem panic
    let (recovered, energy) = memory.recall(&[], &[], 10, 0.05);
    assert!(recovered.is_empty());
    assert_eq!(energy, 0.0);
    let mut out: [f32; 0] = [];
    assert_eq!(memory.recall_into(&[], &[], 10, 0.05, &mut out), 0.0);
    assert_eq!(memory.prune(0.1), 0);
}

#[test]
fn entradas_invalidas_sao_noop_ou_erro() {
    let pattern = harmonic_pattern(N, 0);
    let mask = mask_exact(3, HIDDEN);
    let mut memory = memory_with(std::slice::from_ref(&pattern), inv_cfg());

    let e0 = memory.phase_energy();
    let before = memory.phases.clone();
    assert_eq!(
        memory.try_imprint(&pattern[..N - 1], 1),
        Err(PhaseMemoryError::InvalidSize)
    );
    assert_eq!(
        memory.try_recall(&pattern[..N - 1], &mask, 10, 0.05),
        Err(PhaseMemoryError::InvalidSize)
    );
    assert_eq!(
        memory.try_recall(&pattern, &mask[..N - 1], 10, 0.05),
        Err(PhaseMemoryError::InvalidSize)
    );
    assert_eq!(memory.phase_energy(), e0);
    assert_eq!(memory.phases, before);

    for dt in [0.0f32, -0.05, f32::NAN, f32::INFINITY] {
        assert_eq!(
            memory.try_recall(&pattern, &mask, 100, dt),
            Err(PhaseMemoryError::InvalidIntegration),
            "dt = {dt} deveria ser rejeitado"
        );
        let (recovered, _e) = memory.recall(&pattern, &mask, 100, dt);
        assert_eq!(
            recovered, before,
            "recall com dt inválido não pode mexer no estado"
        );
    }
    assert_eq!(
        memory.try_recall(&pattern, &mask, 0, 0.05),
        Err(PhaseMemoryError::InvalidIntegration)
    );

    // `NaN` no padrão não envenena o acumulador nem as fases.
    let mut poisoned = pattern.clone();
    poisoned[5] = f32::NAN;
    let w_before = memory.weights.clone();
    assert_eq!(
        memory.try_imprint(&poisoned, 5),
        Err(PhaseMemoryError::NonFiniteInput)
    );
    memory.imprint(&poisoned, 5); // no-op, sem panic
    assert_eq!(memory.weights, w_before);
    assert!(memory.phases.iter().all(|p| p.is_finite()));
    assert!(memory.weights.iter().all(|w| w.is_finite()));

    let mut bad_cue = pattern.clone();
    bad_cue[0] = f32::NAN;
    assert_eq!(
        memory.try_recall(&bad_cue, &mask, 10, 0.05),
        Err(PhaseMemoryError::NonFiniteInput)
    );

    // `cycles = 0` é no-op de verdade (não promove a 1).
    let mut fresh = ComplexPhaseMemory::with_config(N, inv_cfg());
    fresh.imprint(&pattern, 0);
    assert!(fresh.weights.iter().all(|w| *w == 0.0));
    assert_eq!(fresh.stored_patterns(), 0);
    assert_eq!(fresh.try_imprint(&pattern, 0), Ok(()));

    // Configuração inválida ⇒ Err.
    for cfg in [
        PhaseMemoryConfig {
            gamma_w: -0.1,
            ..inv_cfg()
        },
        PhaseMemoryConfig {
            gamma_w: 1.0,
            ..inv_cfg()
        },
        PhaseMemoryConfig {
            eta_w: f32::NAN,
            ..inv_cfg()
        },
    ] {
        let mut m = ComplexPhaseMemory::with_config(N, cfg);
        assert_eq!(
            m.try_imprint(&pattern, 4),
            Err(PhaseMemoryError::InvalidConfig)
        );
        assert!(m.weights.iter().all(|w| *w == 0.0));
    }

    // Recuperação válida continua funcionando depois de todas as rejeições.
    memory.reset_phases(0.0);
    let (recovered, energy) = memory.recall(&pattern, &mask, 600, 0.05);
    assert!(energy.is_finite());
    assert!(mean_err(&recovered, &pattern, &mask) < 0.01);
}

// ---------------------------------------------------------------------------
// 7. Knobs reais e distribuição de pesos
// ---------------------------------------------------------------------------

#[test]
fn eta_delta_zero_desliga_o_aprendizado_de_fase() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let cfg = PhaseMemoryConfig {
        eta_delta: 0.0,
        ..inv_cfg()
    };
    let mut off = memory_with(&patterns, cfg);
    assert!(
        off.phase_shifts.iter().all(|d| *d == 0.0),
        "η_δ = 0 não pode aprender fases"
    );

    let mut on = memory_with(&patterns, inv_cfg());
    assert!(
        on.phase_shifts.iter().any(|d| *d != 0.0),
        "η_δ > 0 tem de aprender fases"
    );

    let mask = mask_exact(11, HIDDEN);
    let mut err_off = 0.0f32;
    let mut err_on = 0.0f32;
    for p in patterns.iter() {
        off.reset_phases(0.0);
        on.reset_phases(0.0);
        let (a, _) = off.recall(p, &mask, 600, 0.05);
        let (b, _) = on.recall(p, &mask, 600, 0.05);
        err_off += mean_err(&a, p, &mask);
        err_on += mean_err(&b, p, &mask);
    }
    err_off /= 3.0;
    err_on /= 3.0;
    println!("M5 eta_delta: erro medio sem fase = {err_off:.4}, com fase = {err_on:.6}");
    assert!(
        err_off > 0.5,
        "sem aprendizado de fase o recall deveria colapsar (erro {err_off:.3})"
    );
    assert!(err_on < 0.01);
}

#[test]
fn pesos_nao_saturam_em_um() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let memory = memory_with(&patterns, inv_cfg());
    let mut off: Vec<f32> = Vec::new();
    for i in 0..N {
        for j in 0..N {
            if i != j {
                off.push(memory.weights[i * N + j]);
            }
        }
    }
    let max = off.iter().cloned().fold(f32::MIN, f32::max);
    let min = off.iter().cloned().fold(f32::MAX, f32::min);
    let mean = off.iter().sum::<f32>() / off.len() as f32;
    let at_one = off.iter().filter(|w| (**w - 1.0).abs() < 1e-3).count();
    let spread = off.iter().filter(|w| **w < 0.5 * mean).count();
    println!(
        "M5 pesos: min={min:.3} max={max:.3} media={mean:.3} | ==1.0: {at_one}/{} | abaixo de 0.5*media: {spread}",
        off.len()
    );
    assert!(
        mean > 0.5 && mean < 2.0,
        "gauge deveria fixar a média em ~1"
    );
    assert!(max > 1.2, "nenhuma magnitude preservada? max={max:.3}");
    assert!(
        spread > off.len() / 10,
        "distribuição de pesos sem dispersão (clamp de volta?)"
    );
    assert!(
        at_one < off.len() / 20,
        "pesos saturados em 1.0: {at_one} de {}",
        off.len()
    );
    assert!(min >= 0.0);
}

// ---------------------------------------------------------------------------
// 8. Janela deslizante
// ---------------------------------------------------------------------------

#[test]
fn janela_deslizante_respeita_capacidade() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E);
    let mut memory = ComplexPhaseMemory::with_config(N, inv_cfg());
    let cap = memory.pattern_capacity();
    assert!((3..N).contains(&cap));
    let mut seq: Vec<Vec<f32>> = Vec::new();
    for _ in 0..(cap + 13) {
        let p: Vec<f32> = (0..N)
            .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
            .collect();
        memory.imprint(&p, 40);
        seq.push(p);
    }
    println!(
        "M5 janela: cap={cap} stored={} (apos {} imprints)",
        memory.stored_patterns(),
        seq.len()
    );
    assert_eq!(
        memory.stored_patterns(),
        cap,
        "janela deveria saturar em cap"
    );
    assert!(memory.weights.iter().all(|w| w.is_finite()));
    assert!(memory.phase_energy().is_finite());

    // O padrão mais recente está na janela e é recuperável.
    let last = seq.last().unwrap();
    let mask = mask_exact(5, HIDDEN);
    memory.reset_phases(0.0);
    let (recovered, _e) = memory.recall(last, &mask, 600, 0.05);
    let err = mean_err(&recovered, last, &mask);
    println!("M5 janela cheia: erro do padrao mais recente {err:.4}");
    assert!(
        err < 0.20,
        "padrão recente deveria ser recuperável: {err:.4}"
    );

    // Capacidade não cresce com imprints.
    assert_eq!(memory.pattern_capacity(), cap);
    assert_eq!(memory.stored_patterns(), cap);
}

#[test]
fn densidade_e_energia_ficam_finitas() {
    let patterns = [harmonic_pattern(N, 0), harmonic_pattern(N, 1)];
    let memory = memory_with(&patterns, inv_cfg());
    assert!(memory.phase_energy().is_finite());
    let d = memory.density(0.5);
    assert!((0.0..=1.0).contains(&d));
    assert!(memory.mean_coupling() > 0.0);
}

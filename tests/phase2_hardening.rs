//! Hardening do Módulo 6 (sonhador termodinâmico): contrato de zero alocação,
//! bordas sem `panic`/`NaN`, determinismo por sequência e verificação da
//! referência aleatória (média, não mínimo).
//!
//! O allocador contador mede DUAS coisas durante a janela armada:
//! * thread-local (mesmo padrão de `tests/alloc_invariants.rs`) — alocações da
//!   thread que chama;
//! * global (atômico) — alocações de qualquer thread, incluindo o pool rayon,
//!   que é onde as trajetórias paralelas alocariam buffers por amostra.
//!
//! Os testes deste arquivo são serializados por um mutex de processo para que
//! a contagem global não capture alocações de outro teste rodando em paralelo.

#![cfg(feature = "m6")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use aether_engine::thermodynamic_dreamer::{
    DreamEnsemble, DreamerConfig, ThermodynamicDreamer,
};
use aether_engine::AetherError;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Allocador contador (thread-local + global).
struct Counting;

static ARMED_GLOBAL: AtomicBool = AtomicBool::new(false);
static ALLOCS_GLOBAL: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ARMED_TLS: Cell<bool> = const { Cell::new(false) };
    static ALLOCS_TLS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[inline]
fn record() {
    if ARMED_GLOBAL.load(Ordering::Relaxed) {
        ALLOCS_GLOBAL.fetch_add(1, Ordering::Relaxed);
    }
    if ARMED_TLS.with(|a| a.get()) {
        ALLOCS_TLS.with(|c| c.set(c.get() + 1));
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Serializa os testes deste binário (necessário para a contagem global).
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Executa `f` e devolve `(resultado, allocs_thread_local, allocs_globais)`.
fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    ARMED_TLS.with(|a| a.set(true));
    ALLOCS_TLS.with(|c| c.set(0));
    ARMED_GLOBAL.store(true, Ordering::SeqCst);
    ALLOCS_GLOBAL.store(0, Ordering::SeqCst);

    let out = f();

    ARMED_GLOBAL.store(false, Ordering::SeqCst);
    ARMED_TLS.with(|a| a.set(false));
    (
        out,
        ALLOCS_TLS.with(|c| c.get()),
        ALLOCS_GLOBAL.load(Ordering::SeqCst),
    )
}

/// `J` simétrica, diagonal nula, escala `1/sqrt(N)` (como o Módulo 1) e `h`
/// alternado — paisagem não trivial para a dinâmica.
fn landscape(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let scale = 1.0 / (n as f64).sqrt();
    let mut j = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let v = ((rng.gen::<f64>() * 2.0 - 1.0) * scale) as f32;
            j[i * n + k] = v;
            j[k * n + i] = v;
        }
    }
    let h: Vec<f32> = (0..n)
        .map(|i| if i % 2 == 0 { 0.05 } else { -0.05 })
        .collect();
    (j, h)
}

/// Energia de Ising (cópia local, para checar a referência aleatória).
fn ising_energy_local(j: &[f32], h: &[f32], s: &[i8], n: usize) -> f64 {
    let mut e = 0.0f64;
    for i in 0..n {
        let off = i * n;
        let mut acc = 0.0f64;
        for k in 0..n {
            acc += j[off + k] as f64 * s[k] as f64;
        }
        e -= 0.5 * acc * s[i] as f64;
        e -= h[i] as f64 * s[i] as f64;
    }
    e
}

fn assert_ensemble_well_formed(ens: &DreamEnsemble, n: usize, num_samples: usize) {
    assert_eq!(ens.samples.len(), num_samples, "linhas de amostras");
    assert_eq!(ens.energies.len(), num_samples, "energias");
    for (k, s) in ens.samples.iter().enumerate() {
        assert_eq!(s.len(), n, "amostra {k} com {n} spins");
        assert!(
            s.iter().all(|v| *v == 1 || *v == -1),
            "amostra {k} fora de {{-1,+1}}"
        );
    }
    for (k, e) in ens.energies.iter().enumerate() {
        assert!(e.is_finite(), "energia {k} não finita: {e}");
    }
    assert!(ens.mean_energy.is_finite(), "⟨H⟩ não finita");
    assert!(
        ens.epistemic_uncertainty.is_finite()
            && (0.0..=1.0).contains(&ens.epistemic_uncertainty),
        "U fora de [0,1]: {}",
        ens.epistemic_uncertainty
    );
    assert!(ens.temperature.is_finite(), "T não finita");
}

#[test]
fn probe_rayon_alloc() {
    let _guard = lock();
    use rayon::prelude::*;
    let mut samples: Vec<Vec<i8>> = vec![vec![0i8; 8]; 32];
    let mut energies = vec![0.0f64; 32];
    let mut scratch = vec![0.0f64; 32 * 24];

    for _ in 0..6 {
        let (_, tls, g) = count_allocs(|| ());
        println!("PROBE noop: tls={tls} global={g}");
    }
    samples.par_iter_mut().for_each(|s| s[0] = 1);
    for _ in 0..6 {
        let (_, tls, g) = count_allocs(|| scratch.par_chunks_mut(24).for_each(|w| w[0] = 1.0));
        println!("PROBE chunks: tls={tls} global={g}");
    }

    const N: usize = 32;
    let mut j = vec![0.0f32; N * N];
    for i in 0..N {
        for k in 0..N {
            if i != k {
                j[i * N + k] = 0.01 * ((i * 31 + k * 17) % 7) as f32;
            }
        }
    }
    let h = vec![0.01f32; N];
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());
    let mut out = DreamEnsemble::with_capacity(N, 32);
    dreamer
        .imagine_futures_into(&j, &h, 0.5, 32, 40, 0.05, &mut out)
        .unwrap();
    for _ in 0..8 {
        let (_, tls, g) =
            count_allocs(|| dreamer.imagine_futures_into(&j, &h, 0.5, 32, 40, 0.05, &mut out));
        println!("PROBE dream32: tls={tls} global={g}");
    }
    let mut out4 = DreamEnsemble::with_capacity(N, 4);
    dreamer
        .imagine_futures_into(&j, &h, 0.5, 4, 40, 0.05, &mut out4)
        .unwrap();
    for _ in 0..8 {
        let (_, tls, g) =
            count_allocs(|| dreamer.imagine_futures_into(&j, &h, 0.5, 4, 40, 0.05, &mut out4));
        println!("PROBE dream4: tls={tls} global={g}");
    }
}

// ---------------------------------------------------------------------------
// 1. Contrato de zero alocação (§1.3)
// ---------------------------------------------------------------------------

#[test]
fn invariante_zero_alocacao_imagine_into() {
    let _guard = lock();
    const N: usize = 32;
    const SAMPLES: usize = 32;
    const STEPS: usize = 50;

    let (j, h) = landscape(N, 0xD12E_0006);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());
    let mut out = DreamEnsemble::with_capacity(N, SAMPLES);

    // warmup: inicializa pool rayon + workspace interno + shapes de `out`.
    dreamer
        .imagine_futures_into(&j, &h, 0.5, SAMPLES, STEPS, 0.05, &mut out)
        .expect("warmup");
    assert_ensemble_well_formed(&out, N, SAMPLES);

    // mesma shape, mais quente: 0 alocação.
    for round in 0..4 {
        let (res, tls, global) = count_allocs(|| {
            dreamer.imagine_futures_into(&j, &h, 0.8, SAMPLES, STEPS, 0.05, &mut out)
        });
        res.expect("aquecido");
        println!("ROUND {round}: tls={tls} global={global}");
    }
    let (res, tls, global) = count_allocs(|| {
        dreamer.imagine_futures_into(&j, &h, 0.8, SAMPLES, STEPS, 0.05, &mut out)
    });
    res.expect("aquecido");
    assert_eq!(tls, 0, "imagine_futures_into alocou {tls}x na thread chamadora");
    assert_eq!(global, 0, "imagine_futures_into alocou {global}x (todas as threads)");

    // outro número de passos, mesma shape: continua 0 (steps não realoca).
    let (res, tls, global) = count_allocs(|| {
        dreamer.imagine_futures_into(&j, &h, 0.2, SAMPLES, STEPS + 17, 0.05, &mut out)
    });
    res.expect("passos diferentes");
    assert_eq!(tls, 0, "steps diferente alocou {tls}x");
    assert_eq!(global, 0, "steps diferente alocou {global}x (todas as threads)");

    assert_ensemble_well_formed(&out, N, SAMPLES);
}

#[test]
fn contrato_encolher_ensemble_continua_correto() {
    let _guard = lock();
    const N: usize = 16;
    let (j, h) = landscape(N, 0x51);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());
    let mut out = DreamEnsemble::with_capacity(N, 8);

    dreamer
        .imagine_futures_into(&j, &h, 0.4, 8, 30, 0.05, &mut out)
        .expect("8 amostras");
    assert_ensemble_well_formed(&out, N, 8);

    // encolhe: linhas sobrando são descartadas (dealloc, não alloc).
    dreamer
        .imagine_futures_into(&j, &h, 0.4, 3, 30, 0.05, &mut out)
        .expect("3 amostras");
    assert_ensemble_well_formed(&out, N, 3);
    assert_eq!(out.mean_energy, out.energies.iter().sum::<f64>() / 3.0);
}

// ---------------------------------------------------------------------------
// 2. Bordas: sem panic, sem NaN, erro tipado
// ---------------------------------------------------------------------------

#[test]
fn bordas_retornam_erro_tipado_sem_panic() {
    let _guard = lock();
    const N: usize = 8;
    let (j, h) = landscape(N, 0xE0E0);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    let config_err = |r: Result<DreamEnsemble, AetherError>, what: &str| match r {
        Err(AetherError::Config(msg)) => {
            assert!(!msg.is_empty(), "{what}: mensagem de erro vazia");
        }
        Err(other) => panic!("{what}: esperava Config, veio {other:?}"),
        Ok(e) => panic!("{what}: esperava Err, veio ensemble com {} amostras", e.samples.len()),
    };

    // dt inválido
    config_err(dreamer.try_imagine_futures(&j, &h, 0.5, 4, 10, -0.05), "dt<0");
    config_err(dreamer.try_imagine_futures(&j, &h, 0.5, 4, 10, 0.0), "dt=0");
    config_err(
        dreamer.try_imagine_futures(&j, &h, 0.5, 4, 10, f64::NAN),
        "dt=NaN",
    );
    config_err(
        dreamer.try_imagine_futures(&j, &h, 0.5, 4, 10, f64::INFINITY),
        "dt=inf",
    );

    // temperatura inválida
    config_err(
        dreamer.try_imagine_futures(&j, &h, f64::NAN, 4, 10, 0.05),
        "T=NaN",
    );
    config_err(
        dreamer.try_imagine_futures(&j, &h, -1.0, 4, 10, 0.05),
        "T<0",
    );
    config_err(
        dreamer.try_imagine_futures(&j, &h, f64::INFINITY, 4, 10, 0.05),
        "T=inf",
    );

    // contagens degeneradas
    config_err(
        dreamer.try_imagine_futures(&j, &h, 0.5, 0, 10, 0.05),
        "num_samples=0",
    );
    config_err(
        dreamer.try_imagine_futures(&j, &h, 0.5, 4, 0, 0.05),
        "steps=0",
    );

    // shapes errados
    let j_short = vec![0.0f32; N * N - 1];
    let h_short = vec![0.0f32; N - 1];
    match dreamer.try_imagine_futures(&j_short, &h, 0.5, 4, 10, 0.05) {
        Err(AetherError::Shape { expected, got }) => {
            assert_eq!(expected, vec![N, N]);
            assert_eq!(got, vec![N * N - 1]);
        }
        other => panic!("J curto: esperava Shape, veio {other:?}"),
    }
    match dreamer.try_imagine_futures(&j, &h_short, 0.5, 4, 10, 0.05) {
        Err(AetherError::Shape { expected, got }) => {
            assert_eq!(expected, vec![N]);
            assert_eq!(got, vec![N - 1]);
        }
        other => panic!("h curto: esperava Shape, veio {other:?}"),
    }

    // n == 0
    let mut zero = ThermodynamicDreamer::new(0, DreamerConfig::default());
    config_err(zero.try_imagine_futures(&[], &[], 0.5, 4, 10, 0.05), "n=0");

    // nenhuma chamada inválida avançou o contador de ciclos
    assert_eq!(dreamer.call_index(), 0);
    assert_eq!(zero.call_index(), 0);
}

#[test]
fn bordas_no_wrapper_legado_nao_geram_nan() {
    let _guard = lock();
    const N: usize = 8;
    let (j, h) = landscape(N, 0xE0E1);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    let cases: [(f64, usize, usize, f64); 6] = [
        (0.5, 4, 10, -0.05),         // dt < 0  → antes NaN
        (0.5, 4, 10, 0.0),           // dt = 0  → antes retorno degenerado silencioso
        (f64::NAN, 4, 10, 0.05),     // T NaN
        (-2.0, 4, 10, 0.05),         // T < 0
        (0.5, 4, 0, 0.05),           // steps = 0
        (0.5, 0, 10, 0.05),          // num_samples = 0
    ];
    for (t, samples, steps, dt) in cases {
        let ens = dreamer.imagine_futures(&j, &h, t, samples, steps, dt);
        assert_eq!(ens.samples.len(), samples);
        assert_eq!(ens.energies.len(), samples);
        for s in &ens.samples {
            assert_eq!(s.len(), N);
            assert!(s.iter().all(|v| *v == -1), "fallback deve ser tudo -1");
        }
        assert!(ens.energies.iter().all(|e| *e == 0.0));
        assert!(ens.mean_energy.is_finite() && ens.mean_energy == 0.0);
        assert_eq!(ens.epistemic_uncertainty, 0.0);
        assert!(ens.temperature.is_finite());
    }

    // n == 0 não entra em panic nem produz NaN.
    let mut zero = ThermodynamicDreamer::new(0, DreamerConfig::default());
    let ens = zero.imagine_futures(&[], &[], 0.5, 3, 10, 0.05);
    assert_eq!(ens.samples.len(), 3);
    assert!(ens.samples.iter().all(|s| s.is_empty()));
    assert_eq!(ens.epistemic_uncertainty, 0.0);

    // J/h com tamanho errado também caem no fallback.
    let ens = dreamer.imagine_futures(&[1.0f32; 3], &[0.0f32; 2], 0.5, 2, 10, 0.05);
    assert_eq!(ens.samples.len(), 2);
    assert!(ens.energies.iter().all(|e| e.is_finite()));
}

// ---------------------------------------------------------------------------
// 3. Determinismo por sequência
// ---------------------------------------------------------------------------

#[test]
fn determinismo_bit_a_bit_por_sequencia() {
    let _guard = lock();
    const N: usize = 16;
    const SAMPLES: usize = 16;
    const STEPS: usize = 60;

    let (j, h) = landscape(N, 0xDE7E);
    let cfg = DreamerConfig {
        a0: 1.0,
        gamma_damp: 0.5,
        beta_chaos: 0.55,
        seed: 0xA37E_0002,
    };
    let mut a = ThermodynamicDreamer::new(N, cfg);
    let mut b = ThermodynamicDreamer::new(N, cfg);

    let a1 = a.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    let b1 = b.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    for k in 0..SAMPLES {
        assert_eq!(a1.samples[k], b1.samples[k], "samples[{k}] divergiu (ciclo 0)");
        assert_eq!(
            a1.energies[k].to_bits(),
            b1.energies[k].to_bits(),
            "energies[{k}] divergiu (ciclo 0)"
        );
    }
    assert_eq!(a1.mean_energy.to_bits(), b1.mean_energy.to_bits());
    assert_eq!(
        a1.epistemic_uncertainty.to_bits(),
        b1.epistemic_uncertainty.to_bits()
    );

    // segunda chamada: mesma sequência ⇒ mesmos bits.
    let a2 = a.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    let b2 = b.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    for k in 0..SAMPLES {
        assert_eq!(a2.samples[k], b2.samples[k], "samples[{k}] divergiu (ciclo 1)");
        assert_eq!(a2.energies[k].to_bits(), b2.energies[k].to_bits());
    }

    // o contador de ciclos varia a semente: o mesmo J/h não repete o sonho.
    assert_eq!(a.call_index(), 2);
    assert!(
        a1.samples != a2.samples || a1.energies != a2.energies,
        "ciclos consecutivos produziram o mesmo sonho (call_index não variou a semente)"
    );
}

// ---------------------------------------------------------------------------
// 4. Amostras / energias / referência aleatória
// ---------------------------------------------------------------------------

#[test]
fn amostras_em_mais_menos_um_e_energias_finitas() {
    let _guard = lock();
    for (n, samples, steps, t) in [(16usize, 12usize, 40usize, 1.2f64), (8, 4, 25, 0.0)] {
        let (j, h) = landscape(n, 0xA11);
        let mut dreamer = ThermodynamicDreamer::new(n, DreamerConfig::default());
        let ens = dreamer
            .try_imagine_futures(&j, &h, t, samples, steps, 0.05)
            .expect("amostragem válida");
        assert_ensemble_well_formed(&ens, n, samples);
        assert_eq!(ens.temperature, t);
        let expected_mean: f64 = ens.energies.iter().sum::<f64>() / samples as f64;
        assert_eq!(ens.mean_energy, expected_mean);
    }
}

#[test]
fn referencia_aleatoria_e_media_nao_minimo() {
    let _guard = lock();
    const N: usize = 16;
    const TRIALS: usize = 64;

    let (j, h) = landscape(N, 0x5EED);
    let cfg = DreamerConfig {
        seed: 0xA37E_0002,
        ..DreamerConfig::default()
    };

    // trials == 0 ⇒ 0.0 (antes devolvia +inf).
    let mut dreamer = ThermodynamicDreamer::new(N, cfg);
    assert_eq!(dreamer.random_reference_energy(&j, &h, 0), 0.0);

    // reproduz a sequência do RNG interno: a referência deve ser a MÉDIA
    // (e não o mínimo) das TRIALS amostras uniformes.
    let mut replay = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut mean = 0.0f64;
    let mut min = f64::INFINITY;
    for _ in 0..TRIALS {
        let state: Vec<i8> = (0..N)
            .map(|_| {
                if replay.gen::<f64>() < 0.5 {
                    1
                } else {
                    -1
                }
            })
            .collect();
        let e = ising_energy_local(&j, &h, &state, N);
        mean += e;
        min = min.min(e);
    }
    mean /= TRIALS as f64;

    let got = dreamer.random_reference_energy(&j, &h, TRIALS);
    assert_eq!(
        got.to_bits(),
        mean.to_bits(),
        "referência {got} != média reproduzida {mean}"
    );
    assert!(
        mean > min + 1e-9,
        "média deveria ser maior que o mínimo ({mean} vs {min})"
    );

    // sequência stateful: a chamada seguinte não repete o mesmo bloco.
    let again = dreamer.random_reference_energy(&j, &h, TRIALS);
    assert!(again.is_finite());
    assert_ne!(again.to_bits(), got.to_bits());
}

#[test]
fn ensemble_degenerado_e_consistente() {
    let _guard = lock();
    let ens = DreamEnsemble::degenerate(5, 3, f64::NAN);
    assert_eq!(ens.samples.len(), 3);
    assert_eq!(ens.energies.len(), 3);
    assert!(ens.samples.iter().all(|s| s.len() == 5 && s.iter().all(|v| *v == -1)));
    assert!(ens.energies.iter().all(|e| *e == 0.0));
    assert_eq!(ens.temperature, 0.0);
    assert_eq!(ens.mean_energy, 0.0);
    assert_eq!(ens.epistemic_uncertainty, 0.0);
}

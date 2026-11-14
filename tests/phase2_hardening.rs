//! Hardening do Módulo 6 (sonhador termodinâmico): contrato de zero alocação,
//! bordas sem `panic`/`NaN`, determinismo por sequência e verificação da
//! referência aleatória (média, não mínimo).
//!
//! O allocador contador mede **apenas a thread que chama** (thread-local, o
//! mesmo padrão de `tests/alloc_invariants.rs`). Alocações de threads do pool
//! rayon não entram na contagem: elas são internas do escalonador (blocos do
//! injector), variam de execução para execução e contaminariam a medição sob
//! `cargo test` paralelo. O invariante medido aqui — o sonhador não alocar no
//! caminho quente — é uma propriedade da thread chamadora, determinística.

#![cfg(feature = "m6")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aether_engine::thermodynamic_dreamer::{DreamEnsemble, DreamerConfig, ThermodynamicDreamer};
use aether_engine::AetherError;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Allocador contador thread-local.
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
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
    if ARMED.with(|a| a.get()) {
        ALLOCS.with(|c| c.set(c.get() + 1));
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Executa `f` e devolve `(resultado, allocs da thread chamadora)`.
///
/// Só conta a thread corrente: testes de outros arquivos/threads rodando em
/// paralelo não afetam o número.
fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));

    let out = f();

    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
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
        assert!(e.is_finite(), "energia {k} nao finita: {e}");
    }
    assert!(ens.mean_energy.is_finite(), "<H> nao finita");
    assert!(
        ens.epistemic_uncertainty.is_finite() && (0.0..=1.0).contains(&ens.epistemic_uncertainty),
        "U fora de [0,1]: {}",
        ens.epistemic_uncertainty
    );
    assert!(ens.temperature.is_finite(), "T nao finita");
}

// ---------------------------------------------------------------------------
// 1. Contrato de zero alocação (§1.3)
// ---------------------------------------------------------------------------

#[test]
fn invariante_zero_alocacao_imagine_into() {
    const N: usize = 32;
    const SAMPLES: usize = 32;
    const STEPS: usize = 50;

    let (j, h) = landscape(N, 0xD12E_0006);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());
    let mut out = DreamEnsemble::with_capacity(N, SAMPLES);

    // warmup: dimensiona `out` e o workspace interno (alocação de shape).
    for _ in 0..3 {
        dreamer
            .imagine_futures_into(&j, &h, 0.5, SAMPLES, STEPS, 0.05, &mut out)
            .expect("warmup");
    }
    assert_ensemble_well_formed(&out, N, SAMPLES);

    // Mesma shape: a thread chamadora não aloca nada.
    for round in 0..5 {
        let (res, allocs) = count_allocs(|| {
            dreamer.imagine_futures_into(&j, &h, 0.8, SAMPLES, STEPS, 0.05, &mut out)
        });
        res.expect("aquecido");
        assert_eq!(
            allocs, 0,
            "rodada {round}: imagine_futures_into alocou {allocs}x na thread chamadora"
        );
    }

    // outro número de passos, mesma shape: continua 0 (steps não realoca).
    let (res, allocs) = count_allocs(|| {
        dreamer.imagine_futures_into(&j, &h, 0.2, SAMPLES, STEPS + 17, 0.05, &mut out)
    });
    res.expect("passos diferentes");
    assert_eq!(allocs, 0, "steps diferente alocou {allocs}x");

    assert_ensemble_well_formed(&out, N, SAMPLES);
}

#[test]
fn alocacao_nao_escala_com_num_samples() {
    const N: usize = 16;
    const STEPS: usize = 30;

    let (j, h) = landscape(N, 0x0A11);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());
    let mut small = DreamEnsemble::with_capacity(N, 4);
    let mut large = DreamEnsemble::with_capacity(N, 64);

    // warmup de shapes (uma alocação de dimensionamento por tamanho).
    for _ in 0..3 {
        dreamer
            .imagine_futures_into(&j, &h, 0.6, 4, STEPS, 0.05, &mut small)
            .expect("warmup 4");
        dreamer
            .imagine_futures_into(&j, &h, 0.6, 64, STEPS, 0.05, &mut large)
            .expect("warmup 64");
    }

    // Contagem thread-local: se houvesse um `Vec` por amostra, 64 amostras
    // custariam mais que 4. A tolerância existe apenas para ruído de
    // manutenção de buffers na thread chamadora (esperado: 0 == 0).
    let mut worst_small = 0usize;
    let mut worst_large = 0usize;
    for _ in 0..4 {
        let (res, allocs) =
            count_allocs(|| dreamer.imagine_futures_into(&j, &h, 0.6, 4, STEPS, 0.05, &mut small));
        res.expect("4 amostras");
        worst_small = worst_small.max(allocs);

        let (res, allocs) =
            count_allocs(|| dreamer.imagine_futures_into(&j, &h, 0.6, 64, STEPS, 0.05, &mut large));
        res.expect("64 amostras");
        worst_large = worst_large.max(allocs);
    }

    assert_eq!(worst_small, 0, "4 amostras alocaram {worst_small}x (TLS)");
    assert_eq!(worst_large, 0, "64 amostras alocaram {worst_large}x (TLS)");
    assert!(
        worst_small <= worst_large + 1,
        "alocação cresceu com num_samples: 4 -> {worst_small}, 64 -> {worst_large}"
    );
}

#[test]
fn contrato_encolher_ensemble_continua_correto() {
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
    const N: usize = 8;
    let (j, h) = landscape(N, 0xE0E0);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    let config_err = |r: Result<DreamEnsemble, AetherError>, what: &str| match r {
        Err(AetherError::Config(msg)) => {
            assert!(!msg.is_empty(), "{what}: mensagem de erro vazia");
        }
        Err(other) => panic!("{what}: esperava Config, veio {other:?}"),
        Ok(e) => panic!(
            "{what}: esperava Err, veio ensemble com {} amostras",
            e.samples.len()
        ),
    };

    // dt inválido
    config_err(
        dreamer.try_imagine_futures(&j, &h, 0.5, 4, 10, -0.05),
        "dt<0",
    );
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
    const N: usize = 8;
    let (j, h) = landscape(N, 0xE0E1);
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    let cases: [(f64, usize, usize, f64); 6] = [
        (0.5, 4, 10, -0.05),     // dt < 0  -> antes NaN
        (0.5, 4, 10, 0.0),       // dt = 0  -> antes degenerado silencioso
        (f64::NAN, 4, 10, 0.05), // T NaN
        (-2.0, 4, 10, 0.05),     // T < 0
        (0.5, 4, 0, 0.05),       // steps = 0
        (0.5, 0, 10, 0.05),      // num_samples = 0
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
        assert_eq!(
            a1.samples[k], b1.samples[k],
            "samples[{k}] divergiu (ciclo 0)"
        );
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

    // segunda chamada: mesma sequência => mesmos bits.
    let a2 = a.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    let b2 = b.imagine_futures(&j, &h, 0.6, SAMPLES, STEPS, 0.05);
    for k in 0..SAMPLES {
        assert_eq!(
            a2.samples[k], b2.samples[k],
            "samples[{k}] divergiu (ciclo 1)"
        );
        assert_eq!(a2.energies[k].to_bits(), b2.energies[k].to_bits());
    }

    // o contador de ciclos varia a semente: o mesmo J/h não repete o sonho.
    assert_eq!(a.call_index(), 2);
    assert!(
        a1.samples != a2.samples || a1.energies != a2.energies,
        "ciclos consecutivos produziram o mesmo sonho (call_index nao variou a semente)"
    );
}

// ---------------------------------------------------------------------------
// 4. Amostras / energias / referência aleatória
// ---------------------------------------------------------------------------

#[test]
fn amostras_em_mais_menos_um_e_energias_finitas() {
    for (n, samples, steps, t) in [(16usize, 12usize, 40usize, 1.2f64), (8, 4, 25, 0.0)] {
        let (j, h) = landscape(n, 0xA11);
        let mut dreamer = ThermodynamicDreamer::new(n, DreamerConfig::default());
        let ens = dreamer
            .try_imagine_futures(&j, &h, t, samples, steps, 0.05)
            .expect("amostragem valida");
        assert_ensemble_well_formed(&ens, n, samples);
        assert_eq!(ens.temperature, t);
        let expected_mean: f64 = ens.energies.iter().sum::<f64>() / samples as f64;
        assert_eq!(ens.mean_energy, expected_mean);
    }
}

#[test]
fn referencia_aleatoria_e_media_nao_minimo() {
    const N: usize = 16;
    const TRIALS: usize = 64;

    let (j, h) = landscape(N, 0x5EED);
    let cfg = DreamerConfig {
        seed: 0xA37E_0002,
        ..DreamerConfig::default()
    };

    // trials == 0 => 0.0 (antes devolvia +inf).
    let mut dreamer = ThermodynamicDreamer::new(N, cfg);
    assert_eq!(dreamer.random_reference_energy(&j, &h, 0), 0.0);

    // reproduz a sequência do RNG interno: a referência deve ser a MÉDIA
    // (e não o mínimo) das TRIALS amostras uniformes.
    let mut replay = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut mean = 0.0f64;
    let mut min = f64::INFINITY;
    for _ in 0..TRIALS {
        let state: Vec<i8> = (0..N)
            .map(|_| if replay.gen::<f64>() < 0.5 { 1 } else { -1 })
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
        "referencia {got} != media reproduzida {mean}"
    );
    assert!(
        mean > min + 1e-9,
        "media deveria ser maior que o minimo ({mean} vs {min})"
    );

    // sequência stateful: a chamada seguinte não repete o mesmo bloco.
    let again = dreamer.random_reference_energy(&j, &h, TRIALS);
    assert!(again.is_finite());
    assert_ne!(again.to_bits(), got.to_bits());
}

#[test]
fn ensemble_degenerado_e_consistente() {
    let ens = DreamEnsemble::degenerate(5, 3, f64::NAN);
    assert_eq!(ens.samples.len(), 3);
    assert_eq!(ens.energies.len(), 3);
    assert!(ens
        .samples
        .iter()
        .all(|s| s.len() == 5 && s.iter().all(|v| *v == -1)));
    assert!(ens.energies.iter().all(|e| *e == 0.0));
    assert_eq!(ens.temperature, 0.0);
    assert_eq!(ens.mean_energy, 0.0);
    assert_eq!(ens.epistemic_uncertainty, 0.0);
}

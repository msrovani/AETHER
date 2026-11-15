//! M7-HARDEN — invariantes honestos da morfogênese (física de poda/crescimento,
//! homeostase por ρ real, robustez de bordas e zero alocação no loop crítico).
//!
//! Features `m5`, `m6`, `m7` (o crate de teste é binário separado; o alocador
//! global abaixo conta apenas a thread do teste de alocação via flag TLS).

#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogenesisError, MorphogeneticField};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

// ---------------------------------------------------------------------------
// Contador de alocação por thread (o harness roda testes em paralelo).
// ---------------------------------------------------------------------------

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn note_alloc() {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            let _ = ALLOCS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

struct TrackingAllocator;

// SAFETY: delega toda a alocação ao `System`; apenas incrementa um contador
// thread-local antes de cada alocação.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_alloc();
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note_alloc();
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_alloc();
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn alternating_spins(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect()
}

/// J0 simétrico, diagonal zero, com `percent`% das sinapses ativas.
fn sparse_j0(n: usize, percent: usize, seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            if (i * 31 + k * 17) % 100 < percent {
                let mag = 0.05 + rng.gen::<f32>() * 0.5;
                let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
                let v = mag * sign;
                j[i * n + k] = v;
                j[k * n + i] = v;
            }
        }
    }
    j
}

/// J0 denso com ruído simétrico de escala `scale`.
fn dense_j0(n: usize, scale: f32, seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let v = (rng.gen::<f32>() * 2.0 - 1.0) * scale;
            j[i * n + k] = v;
            j[k * n + i] = v;
        }
    }
    j
}

/// Raio espectral **independente** por rotações de Jacobi (f64, 30 varreduras).
///
/// Usado para auditar a homeostase sem reutilizar a iteração de potência do
/// módulo (evita validar o estimador com ele mesmo). `O(n³)` — só em checkpoints.
fn raio_espectral_jacobi(a: &[f32], n: usize) -> f32 {
    let mut m = vec![0.0f64; n * n];
    for (dst, src) in m.iter_mut().zip(a.iter()) {
        *dst = f64::from(*src);
    }
    for _ in 0..30 {
        let mut off = 0.0f64;
        for p in 0..n {
            for q in (p + 1)..n {
                off += m[p * n + q] * m[p * n + q];
            }
        }
        if off < 1e-20 {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = m[p * n + q];
                if apq.abs() < 1e-14 {
                    continue;
                }
                let app = m[p * n + p];
                let aqq = m[q * n + q];
                let theta = 0.5 * (aqq - app) / apq;
                let t = if theta < 0.0 { -1.0 } else { 1.0 }
                    / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let mkp = m[k * n + p];
                    let mkq = m[k * n + q];
                    m[k * n + p] = c * mkp - s * mkq;
                    m[k * n + q] = s * mkp + c * mkq;
                }
                for k in 0..n {
                    let mpk = m[p * n + k];
                    let mqk = m[q * n + k];
                    m[p * n + k] = c * mpk - s * mqk;
                    m[q * n + k] = s * mpk + c * mqk;
                }
            }
        }
    }
    let mut max = 0.0f64;
    for i in 0..n {
        let v = m[i * n + i].abs();
        if v > max {
            max = v;
        }
    }
    max as f32
}

/// Invariantes estruturais: simetria estrita, diagonal nula, tudo finito.
fn assert_structural(field: &MorphogeneticField, n: usize, cycle: usize) {
    for (idx, v) in field.j.iter().enumerate() {
        assert!(
            v.is_finite(),
            "J[{idx}] não finito ({} = {v}) no ciclo {cycle}",
            idx / n
        );
    }
    for i in 0..n {
        assert_eq!(
            field.j[i * n + i],
            0.0,
            "diagonal não nula no ciclo {cycle}"
        );
        for k in (i + 1)..n {
            let d = (field.j[i * n + k] - field.j[k * n + i]).abs();
            assert!(d < 1e-6, "assimetria {d} em ({i},{k}) no ciclo {cycle}");
        }
    }
}

// ---------------------------------------------------------------------------
// 1 + 2. Silêncio poda de verdade; surpresa alta reconstrói.
// ---------------------------------------------------------------------------

#[test]
fn test_m7_silence_prunes_and_surprise_grows() {
    const N: usize = 32;
    const SILENCE_CYCLES: usize = 300;
    const SURPRISE_CYCLES: usize = 200;
    const DT: f32 = 0.05;

    let spins = alternating_spins(N);
    let j0 = sparse_j0(N, 10, 0x7A11);
    let mut field = MorphogeneticField::new(N, &j0, MorphogenesisConfig::default());

    let pairs = N * (N - 1);
    let zeros_initial = field.zero_synapses();
    let sparsity_initial = field.sparsity_ratio();
    let mut rho = field.spectral_radius();
    let rho_jacobi_initial = raio_espectral_jacobi(&field.j, N);
    assert!(
        (rho - rho_jacobi_initial).abs() <= 0.05,
        "ρ inicial: potência {rho} vs Jacobi {rho_jacobi_initial}"
    );
    let f_silence = field.free_energy(0.0);

    // Auditoria #2: F > 0 no silêncio (custo metabólico) NÃO pode gerar crescimento.
    assert!(
        f_silence > 0.0,
        "F metabólica deveria ser > 0 com J≠0; obtido {f_silence}"
    );
    assert!(
        rho.is_finite() && rho <= 1.5,
        "ρ inicial {rho} fora do regime"
    );
    assert!(
        zeros_initial < pairs,
        "J0 deveria começar com sinapses ativas ({zeros_initial}/{pairs} zeros)"
    );

    let mut zeros_prev = zeros_initial;
    let mut zeros_monotone = true;
    let mut max_rho_silence = rho;
    let mut cycle_full_prune = None;
    for cycle in 0..SILENCE_CYCLES {
        let f = field
            .try_evolve_topology(0.0, &spins, DT)
            .expect("silêncio deveria ser entrada válida");
        assert!(f.is_finite(), "F não finita no silêncio, ciclo {cycle}");
        assert_structural(&field, N, cycle);

        let zeros = field.zero_synapses();
        if zeros < zeros_prev {
            zeros_monotone = false;
        }
        zeros_prev = zeros;
        if zeros == pairs && cycle_full_prune.is_none() {
            cycle_full_prune = Some(cycle + 1);
        }

        rho = field.spectral_radius();
        assert!(
            rho.is_finite() && rho <= 1.5,
            "ρ {rho} > 1.5 no silêncio, ciclo {cycle}"
        );
        if rho > max_rho_silence {
            max_rho_silence = rho;
        }
    }

    let zeros_silence = field.zero_synapses();
    let sparsity_silence = field.sparsity_ratio();
    let density_silence = field.density();
    let rho_silence = field.spectral_radius();

    assert!(
        zeros_monotone,
        "silêncio ressuscitou sinapse (zeros não monotônico)"
    );
    assert!(
        zeros_silence > zeros_initial,
        "silêncio não podou: zeros {zeros_initial} → {zeros_silence}"
    );
    assert!(
        sparsity_silence > sparsity_initial,
        "esparsidade não subiu: {sparsity_initial:.4} → {sparsity_silence:.4}"
    );
    assert_eq!(
        zeros_silence,
        pairs,
        "silêncio deveria drenar TODAS as sinapses; sobraram {}",
        pairs - zeros_silence
    );
    assert!(
        density_silence <= 1e-6,
        "densidade deveria ir a zero no silêncio; obtido {density_silence}"
    );
    assert!(
        rho_silence == 0.0,
        "ρ deveria zerar com J=0; obtido {rho_silence}"
    );

    // ---- Surpresa alta: crescimento real (e ρ continua sob controle) ----
    let zeros_before_growth = field.zero_synapses();
    let density_before_growth = field.density();
    let mut max_rho_growth = 0.0f32;
    let mut cycle_first_synapse = None;
    for cycle in 0..SURPRISE_CYCLES {
        let f = field
            .try_evolve_topology(2.0, &spins, DT)
            .expect("surpresa deveria ser entrada válida");
        assert!(f.is_finite(), "F não finita na surpresa, ciclo {cycle}");
        assert_structural(&field, N, cycle);
        if cycle_first_synapse.is_none() && field.zero_synapses() < pairs {
            cycle_first_synapse = Some(cycle + 1);
        }
        rho = field.spectral_radius();
        assert!(
            rho.is_finite() && rho <= 1.5,
            "ρ {rho} > 1.5 na surpresa, ciclo {cycle}"
        );
        if rho > max_rho_growth {
            max_rho_growth = rho;
        }
    }

    let zeros_growth = field.zero_synapses();
    let density_growth = field.density();
    let f_growth = field.last_free_energy();
    let rho_growth_power = field.spectral_radius();
    let rho_growth_jacobi = raio_espectral_jacobi(&field.j, N);

    let cycle_full_prune = cycle_full_prune.expect("silêncio não drenou todas as sinapses");
    let cycle_first_synapse = cycle_first_synapse.expect("surpresa não criou nenhuma sinapse");

    assert!(
        rho_growth_jacobi <= 1.5 + 1e-3,
        "ρ (Jacobi independente) {rho_growth_jacobi} > 1.5 após surpresa"
    );
    assert!(
        (rho_growth_power - rho_growth_jacobi).abs() <= 0.05,
        "iteração de potência ({rho_growth_power}) divergiu de Jacobi ({rho_growth_jacobi})"
    );

    assert!(
        zeros_growth < zeros_before_growth,
        "surpresa não criou sinapses: zeros {zeros_before_growth} → {zeros_growth}"
    );
    assert!(
        density_growth > density_before_growth + 0.9,
        "densidade não subiu o suficiente: {density_before_growth:.4} → {density_growth:.4}"
    );
    assert!(
        zeros_growth == 0,
        "surpresa deveria reativar todas as sinapses; restam {zeros_growth}"
    );
    assert!(
        f_growth.is_finite() && f_growth > 2.0,
        "F final {f_growth} deveria ser finita e >= surpresa 2.0"
    );

    println!(
        "M7 silence/surprise: zeros {zeros_initial}→{zeros_silence}/{pairs} | sparsity {sparsity_initial:.4}→{sparsity_silence:.4} | density {density_before_growth:.4}→{density_growth:.4} | rho_silence_max {max_rho_silence:.4} → {rho_silence:.4} | rho_growth_max {max_rho_growth:.4} (jacobi {rho_growth_jacobi:.4}) | prune {} em {cycle_full_prune} ciclos | regrow em {cycle_first_synapse} ciclos | F_silence {f_silence:.4} | F_growth {f_growth:.4}",
        zeros_silence - zeros_initial
    );
}

// ---------------------------------------------------------------------------
// 3. 2000 ciclos alternando surpresa/silêncio: sem NaN, simetria, diagonal.
// ---------------------------------------------------------------------------

#[test]
fn test_m7_two_thousand_alternating_cycles() {
    const N: usize = 32;
    const CYCLES: usize = 2000;
    const DT: f32 = 0.05;

    let spins = alternating_spins(N);
    let mut field =
        MorphogeneticField::new(N, &dense_j0(N, 0.06, 0x404), MorphogenesisConfig::default());

    let mut max_asym = 0.0f32;
    let mut max_rho = 0.0f32;
    for cycle in 0..CYCLES {
        let pe = if (cycle / 50) % 2 == 0 { 2.0 } else { 0.0 };
        let f = field
            .try_evolve_topology(pe, &spins, DT)
            .expect("entrada válida");
        assert!(f.is_finite(), "F não finita no ciclo {cycle}");
        assert_structural(&field, N, cycle);

        for i in 0..N {
            for k in (i + 1)..N {
                let d = (field.j[i * N + k] - field.j[k * N + i]).abs();
                if d > max_asym {
                    max_asym = d;
                }
            }
        }
        let rho = field.spectral_radius();
        assert!(
            rho.is_finite() && rho <= 1.5,
            "ρ {rho} > 1.5 no ciclo {cycle}"
        );
        if rho > max_rho {
            max_rho = rho;
        }
    }

    // O último bloco (1950..2000) é silêncio: deve terminar podado.
    assert_eq!(max_asym, 0.0, "assimetria acumulada {max_asym}");
    assert_eq!(
        field.zero_synapses(),
        N * (N - 1),
        "último bloco de silêncio não podou tudo"
    );
    println!(
        "M7 2000 ciclos: max_asym={max_asym} max_rho={max_rho:.4} zeros_finais={}",
        field.zero_synapses()
    );
}

// ---------------------------------------------------------------------------
// 4. Homeostase: ρ real ≈ 1 após normalizar e ≤ 1.5 entre normalizações.
// ---------------------------------------------------------------------------

#[test]
fn test_m7_homeostasis_real_spectral_radius() {
    const N: usize = 32;
    const CYCLES: usize = 500;
    const DT: f32 = 0.05;
    let cfg = MorphogenesisConfig::default();

    let spins = alternating_spins(N);
    let mut field = MorphogeneticField::new(N, &dense_j0(N, 0.05, 0xBEEF), cfg);

    let mut max_rho = 0.0f32;
    let mut max_rho_jacobi = 0.0f32;
    let mut normalized_samples = 0usize;
    let mut worst_after_norm = 0.0f32;
    let mut rho_prev = field.spectral_radius();

    for cycle in 0..CYCLES {
        let f = field
            .try_evolve_topology(2.0, &spins, DT)
            .expect("entrada válida");
        assert!(f.is_finite());
        assert_structural(&field, N, cycle);

        let rho = field.spectral_radius();
        assert!(rho.is_finite(), "ρ não finito no ciclo {cycle}");
        assert!(rho <= 1.5, "ρ {rho} > 1.5 no ciclo {cycle}");
        if rho > max_rho {
            max_rho = rho;
        }

        let normalized_now = field.cycles() % cfg.homeostasis_period == 0;
        if normalized_now && rho_prev > 1.0 {
            normalized_samples += 1;
            let after = (rho - 1.0f32).abs();
            if after > worst_after_norm {
                worst_after_norm = after;
            }
            assert!(
                (0.9..=1.15).contains(&rho),
                "após normalizar, ρ deveria ser ≈1; obtido {rho} (ciclo {cycle})"
            );
        }
        rho_prev = rho;

        // Auditoria independente (sem reutilizar a iteração de potência).
        if cycle % 50 == 0 || cycle + 1 == CYCLES {
            let rho_jacobi = raio_espectral_jacobi(&field.j, N);
            assert!(
                rho_jacobi <= 1.5 + 1e-3,
                "ρ (Jacobi) {rho_jacobi} > 1.5 no ciclo {cycle}"
            );
            assert!(
                (rho - rho_jacobi).abs() <= 0.05,
                "potência {rho} vs Jacobi {rho_jacobi} no ciclo {cycle}"
            );
            if rho_jacobi > max_rho_jacobi {
                max_rho_jacobi = rho_jacobi;
            }
        }
    }

    assert!(
        normalized_samples > 50,
        "normalizações efetivas insuficientes: {normalized_samples}"
    );
    assert!(
        worst_after_norm < 0.15,
        "desvio pós-normalização {worst_after_norm} > 0.15"
    );
    println!(
        "M7 homeostase: max_rho={max_rho:.4} max_rho_jacobi={max_rho_jacobi:.4} normalizacoes={normalized_samples} desvio_max={worst_after_norm:.4} gershgorin_final={:.4}",
        field.spectral_radius_bound()
    );
}

// ---------------------------------------------------------------------------
// 5. Zero alocação em 1000 passos (inclui normalização espectral).
// ---------------------------------------------------------------------------

#[test]
fn test_m7_evolve_topology_zero_alloc() {
    const N: usize = 16;
    let spins = alternating_spins(N);
    let mut field = MorphogeneticField::new(
        N,
        &dense_j0(N, 0.05, 0x0A11),
        MorphogenesisConfig::default(),
    );

    // Warmup (inclui a primeira iteração de potência e qualquer lazy init).
    for _ in 0..20 {
        field
            .try_evolve_topology(2.0, &spins, 0.05)
            .expect("entrada válida");
    }

    TRACKING.with(|t| t.set(true));
    ALLOCS.with(|c| c.set(0));
    for _ in 0..1000 {
        let f = field.evolve_topology(2.0, &spins, 0.05);
        if !f.is_finite() {
            TRACKING.with(|t| t.set(false));
            panic!("F não finita durante o teste de alocação");
        }
    }
    TRACKING.with(|t| t.set(false));

    let allocs = ALLOCS.with(|c| c.get());
    assert_eq!(
        allocs, 0,
        "evolve_topology alocou {allocs} vezes em 1000 chamadas"
    );
    println!("M7 allocs em 1000 chamadas: {allocs}");
}

// ---------------------------------------------------------------------------
// 6. Bordas: entradas inválidas, n=0, J0 inválido, NaN em J.
// ---------------------------------------------------------------------------

#[test]
fn test_m7_edge_cases_never_panic_or_nan() {
    const N: usize = 8;
    let cfg = MorphogenesisConfig::default();
    let spins = alternating_spins(N);
    let mut field = MorphogeneticField::new(N, &dense_j0(N, 0.05, 0xED6E), cfg);

    let snapshot = field.j.clone();

    // Surpresa não finita.
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            field.try_evolve_topology(bad, &spins, 0.05),
            Err(MorphogenesisError::InvalidPredictionError { .. })
        ));
        // Wrapper legado: no-op + 0.0, sem panic.
        assert_eq!(field.evolve_topology(bad, &spins, 0.05), 0.0);
    }

    // dt inválido.
    for bad in [0.0f32, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            field.try_evolve_topology(2.0, &spins, bad),
            Err(MorphogenesisError::InvalidDt { .. })
        ));
        assert_eq!(field.evolve_topology(2.0, &spins, bad), 0.0);
    }

    // Spins com tamanho errado / não finito / magnitude absurda.
    assert!(matches!(
        field.try_evolve_topology(2.0, &spins[..N - 1], 0.05),
        Err(MorphogenesisError::SpinCountMismatch { .. })
    ));
    let mut nan_spins = spins.clone();
    nan_spins[0] = f32::NAN;
    assert!(matches!(
        field.try_evolve_topology(2.0, &nan_spins, 0.05),
        Err(MorphogenesisError::InvalidSpin { .. })
    ));
    let huge_spins = vec![1.0e30f32; N];
    assert!(matches!(
        field.try_evolve_topology(2.0, &huge_spins, 0.05),
        Err(MorphogenesisError::SpinMagnitudeTooLarge { .. })
    ));

    // Nenhuma chamada rejeitada mutou o estado.
    assert_eq!(field.j, snapshot, "entrada inválida mutou J");
    assert_structural(&field, N, 0);

    // dt gigante é saturado em max_dt: sem NaN/Inf.
    let f = field
        .try_evolve_topology(2.0, &spins, 1.0e9)
        .expect("dt finito positivo é válido (saturado)");
    assert!(f.is_finite());
    assert_structural(&field, N, 0);

    // Dimensão zero.
    let mut empty = MorphogeneticField::new(0, &[], cfg);
    assert_eq!(empty.dim(), 0);
    assert!(matches!(
        empty.try_evolve_topology(0.0, &[], 0.05),
        Err(MorphogenesisError::EmptyField)
    ));
    assert_eq!(empty.evolve_topology(0.0, &[], 0.05), 0.0);
    assert_eq!(empty.spectral_radius_bound(), 0.0);
    assert_eq!(empty.spectral_radius(), 0.0);
    assert_eq!(empty.sparsity_ratio(), 0.0);

    // new() com J0 de tamanho errado: nasce zerado, sem panic.
    let wrong = MorphogeneticField::new(4, &[1.0, 2.0], cfg);
    assert_eq!(wrong.j.len(), 16);
    assert!(wrong.j.iter().all(|v| *v == 0.0));
    assert_eq!(wrong.zero_synapses(), 4 * 3);

    // new() com J0 contendo NaN/Inf: saneado.
    let mut bad0 = vec![0.25f32; 16];
    bad0[1] = f32::NAN;
    bad0[2] = f32::INFINITY;
    let sane = MorphogeneticField::new(4, &bad0, cfg);
    assert!(sane.j.iter().all(|v| v.is_finite()));
    assert_structural(&sane, 4, 0);

    // new() aplica dissolve: |J| < θ vira zero.
    let mut tiny = vec![0.0f32; 16];
    tiny[1] = 0.001;
    tiny[4] = 0.001;
    tiny[2] = 0.2;
    tiny[8] = 0.2;
    let dissolved = MorphogeneticField::new(4, &tiny, cfg);
    assert_eq!(dissolved.j[1], 0.0);
    assert_eq!(dissolved.j[4], 0.0);
    assert!(dissolved.j[2].abs() > 0.0);
    assert_eq!(dissolved.j[8], 0.2);

    // Mutação externa de j (campo público): um passo repara simetria/NaN.
    let mut mutated = MorphogeneticField::new(N, &dense_j0(N, 0.05, 0x77), cfg);
    mutated.j[0] = f32::NAN;
    mutated.j[1] = f32::INFINITY;
    mutated.j[2] = 5.0;
    mutated.j[N] = -5.0; // J[1,0] dessincronizado
    let r = mutated
        .try_evolve_topology(2.0, &spins, 0.05)
        .expect("mutação externa deve ser saneada");
    assert!(r.is_finite());
    assert_structural(&mutated, N, 0);

    // Diagnóstico: Gershgorin propaga NaN (nunca mascara como 0.0).
    let mut nan_field = MorphogeneticField::new(4, &[0.0; 16], cfg);
    nan_field.j[0] = f32::NAN;
    assert!(nan_field.spectral_radius_bound().is_nan());
    assert!(nan_field.spectral_radius().is_nan());
    // Homeostase recusa normalizar e não muta J.
    assert!(matches!(
        nan_field.try_homeostasis(),
        Err(MorphogenesisError::NonFiniteSpectralRadius)
    ));
    assert!(nan_field.j[0].is_nan(), "homeostase mutou J com ρ inválido");
    nan_field.homeostasis(); // wrapper tolerante: não entra em panic

    // try_new estrito.
    assert!(matches!(
        MorphogeneticField::try_new(4, &[0.0; 15], cfg),
        Err(MorphogenesisError::InvalidInitialMatrix { .. })
    ));
    let mut with_nan = vec![0.0f32; 16];
    with_nan[7] = f32::NAN;
    assert!(matches!(
        MorphogeneticField::try_new(4, &with_nan, cfg),
        Err(MorphogenesisError::NonFiniteInitialMatrix { .. })
    ));

    // Configuração patológica é saneada para os defaults físicos.
    let bad_cfg = MorphogenesisConfig {
        k_grow: f32::NAN,
        k_decay: -1.0,
        k_density: f32::INFINITY,
        dissolve: -0.1,
        lambda_metabolic: f32::NAN,
        target_spectral_radius: 0.0,
        homeostasis_period: 0,
        max_dt: f32::NAN,
        gershgorin_cap: f32::NAN,
        homeostatic_density: f32::NAN,
    };
    let mut resilient = MorphogeneticField::new(N, &dense_j0(N, 0.05, 0x5A5A), bad_cfg);
    let f = resilient
        .try_evolve_topology(2.0, &spins, 0.05)
        .expect("config saneada deveria evoluir");
    assert!(f.is_finite());
    assert_structural(&resilient, N, 0);

    println!("M7 bordas: todas as validações Ok (sem panic/NaN)");
}

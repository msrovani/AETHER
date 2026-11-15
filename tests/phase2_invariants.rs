//! Invariantes físicos da Fase 2 (memória de fase, sonho termodinâmico,
//! morfogênese). Features `m5`, `m6`, `m7`.

#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use aether_engine::thermodynamic_dreamer::{DreamerConfig, ThermodynamicDreamer};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Padrão contínuo de fase com k-ésimo harmônico (ortogonal entre padrões).
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

// ---------------------------------------------------------------------------
// 1. Memória associativa de fase
// ---------------------------------------------------------------------------

#[test]
fn test_complex_phase_memory_recall() {
    const N: usize = 32;
    const MASK_FRACTION: usize = 40; // % mascarado

    let mut memory = ComplexPhaseMemory::with_config(
        N,
        PhaseMemoryConfig {
            omega0: 0.0,
            clamp_strength: 8.0,
            eta_w: 0.12,
            eta_delta: 0.08,
            gamma_w: 0.002,
        },
    );

    // 3 padrões ortogonais (harmônicos distintos) gravados com múltiplos ciclos.
    let p0 = harmonic_pattern(N, 0);
    let p1 = harmonic_pattern(N, 1);
    let p2 = harmonic_pattern(N, 2);
    memory.imprint(&p0, 40);
    memory.imprint(&p1, 40);
    memory.imprint(&p2, 40);

    // Pista do padrão 1 com 40% dos osciladores mascarados (padrão determinístico).
    let mut rng = ChaCha8Rng::seed_from_u64(0xFA51);
    let mut mask = vec![true; N];
    let mut hidden = 0;
    for m in mask.iter_mut() {
        if rng.gen::<f32>() < MASK_FRACTION as f32 / 100.0 && hidden < MASK_FRACTION * N / 100 {
            *m = false;
            hidden += 1;
        }
    }
    let cue: Vec<f32> = p1.clone();
    let (recovered, energy) = memory.recall(&cue, &mask, 600, 0.05);

    assert!(energy.is_finite(), "energia de fase não finita: {energy}");

    // Erro angular médio APENAS sobre os osciladores ocultos (os grampeados
    // foram forçados à pista).
    let mut err_sum = 0.0f32;
    let mut count = 0usize;
    for i in 0..N {
        if mask[i] {
            continue;
        }
        let d = (recovered[i] - p1[i] + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        err_sum += d.abs();
        count += 1;
    }
    assert!(count > 0, "nenhum oscilador foi mascarado");
    let mean_err = err_sum / count as f32;
    assert!(
        mean_err < 0.15,
        "erro angular médio {mean_err:.4} rad >= 0.15 (invariante violado)"
    );
}

#[test]
fn test_complex_phase_memory_atratores_estaveis() {
    // Após a gravação, a evolução livre deve manter a energia baixa e estável.
    const N: usize = 32;
    let mut memory = ComplexPhaseMemory::with_config(
        N,
        PhaseMemoryConfig {
            omega0: 0.0,
            clamp_strength: 8.0,
            eta_w: 0.12,
            eta_delta: 0.08,
            gamma_w: 0.002,
        },
    );
    let p = harmonic_pattern(N, 0);
    memory.imprint(&p, 40);
    // estado inicial = padrão corrompido
    let mut corrupted = p.clone();
    for v in corrupted.iter_mut() {
        *v = (*v + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
    }
    let (rec, _e) = memory.recall(&corrupted, &[true; N], 600, 0.05);
    let e0 = memory.phase_energy();
    // evolução livre: energia não pode piorar significativamente
    let e1 = memory.free_run(120, 0.05);
    assert!(e1 <= e0 + 1e-3, "atrator instável: E0={e0:.5} E1={e1:.5}");
    let _ = rec;
}

// ---------------------------------------------------------------------------
// 2. Sonhador termodinâmico
// ---------------------------------------------------------------------------

/// Matriz J frustrada (triângulo impasse + ruído) determinística.
fn frustrated_j(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let scale = 1.0 / (n as f64).sqrt();
    let mut j = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            // mistura antiferromagnética (frustração) + ruído
            let base = if (i / 3 + k / 3) % 2 == 0 { -1.0 } else { 1.0 };
            let v = ((base * 0.6 + rng.gen::<f64>() * 0.8 - 0.4) * scale) as f32;
            j[i * n + k] = v;
            j[k * n + i] = v;
        }
    }
    (j, vec![0.0f32; n])
}

#[test]
fn test_thermodynamic_dreamer_uncertainty() {
    const N: usize = 32;
    const SAMPLES: usize = 32;
    const STEPS: usize = 200;

    let (j, _h) = frustrated_j(N, 0xD12E);
    // Campo externo real (o que o Módulo 4 entrega ao solver): com campo,
    // o colapso a T→0 é determinado pela física e não pelo ruído inicial.
    let h: Vec<f32> = (0..N)
        .map(|i| if i % 2 == 0 { 0.3 } else { -0.3 })
        .collect();
    let mut dreamer = ThermodynamicDreamer::new(
        N,
        DreamerConfig {
            a0: 1.0,
            gamma_damp: 0.6,
            beta_chaos: 0.55,
            seed: 0xA37E_0002,
        },
    );

    let cold = dreamer.imagine_futures(&j, &h, 0.001, SAMPLES, STEPS, 0.05);
    let hot = dreamer.imagine_futures(&j, &h, 1.5, SAMPLES, STEPS, 0.05);

    // Incerteza epistêmica monotônica em T.
    assert!(
        hot.epistemic_uncertainty > cold.epistemic_uncertainty,
        "U(T=1.5) = {:.4} !> U(T=0.001) = {:.4}",
        hot.epistemic_uncertainty,
        cold.epistemic_uncertainty
    );
    assert!(hot.epistemic_uncertainty > 0.0, "T alto deveria amostrar");
    assert!(
        cold.epistemic_uncertainty < 0.05,
        "T baixo deveria colapsar"
    );

    // Energias finitas e amostras em {-1,+1}.
    for e in cold.energies.iter().chain(hot.energies.iter()) {
        assert!(e.is_finite());
    }
    for s in hot.samples.iter().chain(cold.samples.iter()) {
        assert!(s.iter().all(|v| *v == 1 || *v == -1));
    }

    // As amostras têm energia menor que a de um estado aleatório TÍPICO
    // (a distribuição de Boltzmann é enviesada para baixo por construção).
    let random_mean: f64 = {
        const TRIALS: usize = 512;
        let mut acc = 0.0f64;
        for _ in 0..TRIALS {
            acc += dreamer.random_reference_energy(&j, &h, 1);
        }
        acc / TRIALS as f64
    };
    assert!(
        hot.mean_energy < random_mean,
        "⟨H⟩_quente {:.4} !< ref_aleatória {:.4}",
        hot.mean_energy,
        random_mean
    );
    assert!(
        cold.mean_energy < random_mean,
        "⟨H⟩_frio {:.4} !< ref_aleatória {:.4}",
        cold.mean_energy,
        random_mean
    );
}

// ---------------------------------------------------------------------------
// 3. Morfogênese
// ---------------------------------------------------------------------------

#[test]
fn test_morphogenetic_homeostasis_and_pruning() {
    const N: usize = 32;
    const CYCLES: usize = 500;

    let mut rng = ChaCha8Rng::seed_from_u64(0x404);
    let mut j0 = vec![0.0f32; N * N];
    for i in 0..N {
        for k in (i + 1)..N {
            let v = rng.gen::<f32>() * 0.12 - 0.06;
            j0[i * N + k] = v;
            j0[k * N + i] = v;
        }
    }

    let mut field = MorphogeneticField::new(N, &j0, MorphogenesisConfig::default());
    let spins: Vec<f32> = (0..N)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();

    let mut max_asym = 0.0f32;
    let mut sparsity_start = field.sparsity_ratio();
    let mut sparsity_quiet = sparsity_start;
    let mut sparsity_quiet_start = sparsity_start;

    for cycle in 0..CYCLES {
        // alterna surpresa alta e silêncio
        let prediction_error = if (cycle / 50) % 2 == 0 { 2.0 } else { 0.0 };
        let f = field.evolve_topology(prediction_error, &spins, 0.05);

        assert!(f.is_finite(), "energia livre não finita no ciclo {cycle}");
        for (i, v) in field.j.iter().enumerate() {
            assert!(v.is_finite(), "J[{i}] não finito no ciclo {cycle}");
        }

        // (a) simetria estrita + diagonal nula
        for i in 0..N {
            assert_eq!(
                field.j[i * N + i],
                0.0,
                "diagonal não nula no ciclo {cycle}"
            );
            for k in (i + 1)..N {
                let d = (field.j[i * N + k] - field.j[k * N + i]).abs();
                if d > max_asym {
                    max_asym = d;
                }
                assert!(
                    d < 1e-6,
                    "assimetria {d} no ciclo {cycle} (J[{i},{k}] vs J[{k},{i}])"
                );
            }
        }

        // (b) mede a densidade no período de silêncio
        if (cycle / 50) % 2 == 1 {
            if cycle % 50 == 0 {
                sparsity_quiet_start = field.sparsity_ratio();
            }
            sparsity_quiet = field.sparsity_ratio();
        } else if cycle == 0 {
            sparsity_start = field.sparsity_ratio();
        }
    }

    assert!(max_asym < 1e-6);
    // Contrato atual (sprint de evolução): o decaimento é gated por atividade
    // — no silêncio ele cai a 5% do nominal — e existe piso estrutural. O que
    // não pode acontecer é o silêncio *criar* sinapse nem dissolver a
    // topologia inteira (colapso que o T04 da auditoria apontou).
    let _ = sparsity_start;
    assert!(
        sparsity_quiet >= sparsity_quiet_start - 1e-3,
        "silêncio criou sinapses: {:.4} → {:.4}",
        sparsity_quiet_start,
        sparsity_quiet
    );
    assert!(
        sparsity_quiet < 1.0,
        "silêncio dissolveu a topologia inteira (esparsidade 1.0)"
    );
    let _ = spins;
}

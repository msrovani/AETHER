//! Gate de abstenção do M5 (T10b da auditoria cognitiva).
//!
//! A pergunta do teste é a do achado FL-1 da MSG 0058: quando a pista não
//! corresponde a nenhum atrator guardado, a memória ainda converge para algum
//! estado. O sinal que separa "lembrou" de "inventou" é a **margem** entre o
//! melhor e o segundo melhor casamento — não a similaridade absoluta (que
//! depende de N), nem o resíduo nos nós grampeados, nem a coerência global
//! (os três foram medidos e descartados).

#![cfg(feature = "m5")]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig, RecallConfidence};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const TAU: f32 = std::f32::consts::TAU;

fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(TAU)
        })
        .collect()
}

fn mask_exact(seed: u64, n: usize, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; n];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn cfg() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

fn memory(n: usize, patterns: &[Vec<f32>]) -> ComplexPhaseMemory {
    let mut m = ComplexPhaseMemory::with_config(n, cfg());
    for p in patterns {
        m.imprint(p, 40);
    }
    m
}

#[test]
fn pista_real_tem_margem_alta_e_pista_inexistente_abstem() {
    for n in [32usize, 64] {
        let patterns: Vec<Vec<f32>> = (0..5).map(|o| harmonic_pattern(n, o)).collect();
        let mut mem = memory(n, &patterns);
        let mask = mask_exact(0x99, n, n * 40 / 100);

        // (a) pista real: o padrão 3, com 40% ocluído.
        mem.reset_phases(0.0);
        let (_rec, real) = mem
            .recall_with_confidence(&patterns[3], &mask, 200, 0.05)
            .expect("recall real");
        assert_eq!(real.best, 3, "N={n}: melhor casamento errado");
        assert!(
            real.margin >= RecallConfidence::ABSTAIN_MARGIN,
            "N={n}: margem {:.4} baixa demais para pista real",
            real.margin
        );
        assert!(!real.should_abstain(), "N={n}: absteve de uma pista real");

        // (b) pista inexistente: tudo zero.
        let empty = vec![0.0f32; n];
        mem.reset_phases(0.0);
        let (_rec, fake) = mem
            .recall_with_confidence(&empty, &mask, 200, 0.05)
            .expect("recall vazio");
        assert!(
            fake.should_abstain(),
            "N={n}: aceitou pista inexistente (margem {:.4}, sim {:.4})",
            fake.margin,
            fake.best_similarity
        );

        // A separação precisa ser grande, não coisa de arredondamento.
        assert!(
            real.margin > 4.0 * fake.margin,
            "N={n}: separação fraca (real {:.4} vs vazio {:.4})",
            real.margin,
            fake.margin
        );
    }
}

#[test]
fn memoria_vazia_abstem() {
    let mut mem = ComplexPhaseMemory::with_config(32, cfg());
    let mask = mask_exact(1, 32, 8);
    let cue = vec![0.3f32; 32];
    let (_rec, conf) = mem
        .recall_with_confidence(&cue, &mask, 20, 0.05)
        .expect("recall");
    assert_eq!(conf.margin, 0.0);
    assert!(conf.should_abstain());
}

#[test]
fn confianca_nao_altera_a_dinamica_do_recall() {
    // A confiança é leitura: as fases têm de ser idênticas às de `recall`.
    let n = 32usize;
    let patterns: Vec<Vec<f32>> = (0..3).map(|o| harmonic_pattern(n, o)).collect();
    let mut mem = memory(n, &patterns);
    let mask = mask_exact(7, n, n * 30 / 100);

    mem.reset_phases(0.0);
    let (plain, _e) = mem.recall(&patterns[1], &mask, 120, 0.05);
    mem.reset_phases(0.0);
    let (with_conf, _c) = mem
        .recall_with_confidence(&patterns[1], &mask, 120, 0.05)
        .expect("recall");

    for i in 0..n {
        assert!(
            (plain[i] - with_conf[i]).abs() < 1e-6,
            "fase {i} divergiu: {:.6} vs {:.6}",
            plain[i],
            with_conf[i]
        );
    }
}

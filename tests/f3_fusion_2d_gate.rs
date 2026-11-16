//! F3 — fusão 2D (`autoridade × factibilidade`) e `cue_into` escopado.
//!
//! Gates (Vol. III §13, pergunta #7):
//! 1. o quadrante “plausível sem fonte e fisicamente ambíguo” é detectado em
//!    ≥ 90% dos casos e mapeado para `CITA NÃO ATUA`;
//! 2. o round-trip `ledger → cue_into → atrator` perde < 5% de `Recall@1`
//!    frente ao recall direto;
//! 3. R8: `0` hits cruzados entre `ScopeId` distintos (com controle positivo
//!    de que o candidato estrangeiro existe).

use std::f32::consts::TAU;

use aether_engine::core::knowledge_bridge_f1::{BridgeError, Certificate, DraftRecord, Layer, ScopeId};
use aether_engine::core::knowledge_fusion::{FusionAction, FusionLedger, Quadrant};
use aether_engine::phase_memory::ComplexPhaseMemory;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 16;
const STEPS: usize = 200;
const DT: f32 = 0.05;
const HIDDEN: usize = 6;

fn patterns(seed: u64, count: usize) -> Vec<Vec<f32>> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..count)
        .map(|_| (0..N).map(|_| rng.gen::<f32>() * TAU).collect())
        .collect()
}

fn payload(phases: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(phases.len() * 8);
    for p in phases {
        out.extend_from_slice(&p.cos().to_le_bytes());
        out.extend_from_slice(&p.sin().to_le_bytes());
    }
    out
}

fn interleaved(phases: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(phases.len() * 2);
    for p in phases {
        out.push(p.cos());
        out.push(p.sin());
    }
    out
}

fn mask(seed: u64) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut m = vec![true; N];
    let mut idx: Vec<usize> = (0..N).collect();
    for i in (1..N).rev() {
        let j = rng.gen_range(0..=i);
        idx.swap(i, j);
    }
    for &i in idx.iter().take(HIDDEN) {
        m[i] = false;
    }
    m
}

fn record(key: u128, scope: ScopeId, phases: &[f32], authority: u8, gap: f32) -> DraftRecord {
    DraftRecord {
        key,
        layer: Layer::L4Semantic,
        certificate: Certificate {
            energy: -1.0,
            gap,
            violations: 0,
        },
        authority,
        scope,
        tick: 1,
        wall_ms: 1_000,
        payload: payload(phases),
    }
}

#[test]
fn f3_matriz_2d_e_round_trip_escopado() {
    let a = ScopeId::new(1, 1, 1);
    let b = ScopeId::new(2, 1, 1);
    let c = ScopeId::new(3, 1, 1);
    let pa = patterns(0xA37E_0F30, 3);
    let pb = patterns(0xA37E_0F31, 3);

    let mut ledger = FusionLedger::new();
    for (i, p) in pa.iter().enumerate() {
        ledger.stage(record(100 + i as u128, a, p, 200, 0.1));
    }
    for (i, p) in pb.iter().enumerate() {
        ledger.stage(record(200 + i as u128, b, p, 200, 0.1));
    }
    assert_eq!(ledger.commit_run(), 6);

    // ------------------------------------------------------------------
    // Gate 1: matriz 2D — 4 quadrantes, alvo ≥ 90%.
    // ------------------------------------------------------------------
    let mut target_hits = 0usize;
    let mut target_total = 0usize;
    let mut noise_hits = 0usize;
    let mut noise_total = 0usize;
    for authority in [200u8, 10u8] {
        for gap in [0.10f32, 0.90f32] {
            for violations in [0u32, 2u32] {
                for _ in 0..100 {
                    let d = ledger.decide(
                        authority,
                        Certificate {
                            energy: 0.0,
                            gap,
                            violations,
                        },
                    );
                    // Alvo “plausível sem fonte e fisicamente ambíguo”:
                    // autoridade alta, gap alto, certificado sem violação.
                    if authority >= 128 && gap > 0.25 && violations == 0 {
                        target_total += 1;
                        if d.action == FusionAction::CiteNoAct
                            && d.quadrant == Quadrant::TrustedAmbiguous
                        {
                            target_hits += 1;
                        }
                    }
                    // “Ruído sem fonte”: autoridade baixa + ambiguidade/violação.
                    if authority < 128 && (gap > 0.25 || violations > 0) {
                        noise_total += 1;
                        if d.action == FusionAction::Abstain {
                            noise_hits += 1;
                        }
                    }
                }
            }
        }
    }
    let detection = target_hits as f32 / target_total.max(1) as f32;
    let noise_detection = noise_hits as f32 / noise_total.max(1) as f32;
    println!(
        "[F3] alvo_plausivel_ambiguo={detection:.3} ({target_hits}/{target_total}) ruido_sem_fonte={noise_detection:.3} ({noise_hits}/{noise_total})"
    );
    assert!(detection >= 0.90, "deteccao {detection:.3} < 0.90");
    assert!(noise_detection >= 0.90, "ruido {noise_detection:.3} < 0.90");

    // ------------------------------------------------------------------
    // Gate 2: round-trip ledger → cue_into → atrator vs recall direto.
    // ------------------------------------------------------------------
    let mut mem = ComplexPhaseMemory::new(N);
    for p in &pa {
        mem.imprint(p, 40);
    }
    let m = mask(0xA37E_0F32);
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E_0F33);

    let mut direct_hits = 0usize;
    let mut round_hits = 0usize;
    let mut total = 0usize;
    let mut seed = [0.0f32; N];
    for (label, p) in pa.iter().enumerate() {
        for _ in 0..10 {
            let cue: Vec<f32> = p
                .iter()
                .map(|x| x + (rng.gen::<f32>() - 0.5) * 0.30)
                .collect();
            // Direto: estado inicial zero.
            mem.phases.fill(0.0);
            mem.rotation = 0.0;
            let (_v, conf_d) = mem
                .recall_with_confidence(&cue, &m, STEPS, DT)
                .expect("recall direto");
            if conf_d.best == label {
                direct_hits += 1;
            }

            // Round-trip: o ledger devolve a semente, o atrator a reacende.
            let q = interleaved(&cue);
            let (_consulted, key) = ledger
                .cue_into_scoped(a, &q, &mut seed)
                .expect("cue_into A");
            assert!(key >= 100 && key < 110, "key fora do escopo A: {key}");
            mem.phases.copy_from_slice(&seed);
            mem.rotation = 0.0;
            let (_v2, conf_r) = mem
                .recall_with_confidence(&cue, &m, STEPS, DT)
                .expect("recall round-trip");
            if conf_r.best == label {
                round_hits += 1;
            }
            total += 1;
        }
    }
    let direct_recall = direct_hits as f32 / total.max(1) as f32;
    let round_recall = round_hits as f32 / total.max(1) as f32;
    let loss = if direct_recall > 0.0 {
        ((direct_recall - round_recall) / direct_recall).max(0.0)
    } else {
        1.0
    };
    println!(
        "[F3] recall@1 direto={direct_recall:.3} round-trip={round_recall:.3} perda={loss:.3}"
    );
    assert!(direct_recall >= 0.95, "recall direto baixo: {direct_recall:.3}");
    assert!(loss < 0.05, "perda no round-trip {loss:.3} >= 0.05");

    // ------------------------------------------------------------------
    // Gate 3: R8 — isolamento cross-organism.
    // ------------------------------------------------------------------
    let qb = interleaved(&pb[0]);
    let (_hits, key_foreign_query) = ledger
        .cue_into_scoped(a, &qb, &mut seed)
        .expect("consulta cruzada nao pode vazar");
    assert!(
        ledger.key_in_scope(a, key_foreign_query),
        "hit cruzado: {key_foreign_query} nao pertence ao escopo A"
    );
    assert_eq!(ledger.cross_scope_hits(), 0, "R8 violado");
    assert!(
        ledger.foreign_candidates() > 0,
        "controle positivo ausente: nao houve candidato estrangeiro"
    );
    assert_eq!(
        ledger.cue_into_scoped(c, &qb, &mut seed),
        Err(BridgeError::EmptyScope)
    );

    // A mesma consulta no escopo B devolve chave de B.
    let (_hits_b, key_b) = ledger.cue_into_scoped(b, &qb, &mut seed).expect("B");
    assert!(key_b >= 200 && key_b < 210, "key fora do escopo B: {key_b}");
    assert!(ledger.key_in_scope(b, key_b));

    // ------------------------------------------------------------------
    // Poda para gist continua reconhecendo o padrão.
    // ------------------------------------------------------------------
    let pruned = ledger.prune_to_gist(a, 102).expect("prune A/102");
    assert_eq!(pruned.key, 102);
    assert_eq!(ledger.gist_count(), 1);
    assert_eq!(
        ledger.gist_cue(a, &interleaved(&pa[2]), 0.80),
        Some(102),
        "gist nao reconheceu o padrao podado"
    );

    // Nenhum hit cruzado foi contabilizado em toda a execução.
    assert_eq!(ledger.cross_scope_hits(), 0);
}

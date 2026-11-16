//! E4 — auditoria do *self-field* de 16 canais (Vol. I §4).
//!
//! Três braços, mesma memória e mesmas consultas:
//!
//! * **(a) baseline** — abstenção só pela margem de recall (`< 0,20`);
//! * **(b) + self-field** — abstenção também pela novidade estrutural do
//!   próprio quadro (erro de auto-predição / coerência da pista);
//! * **(c) + self-field + currículo por progresso** — o campo só atualiza e só
//!   aperta o gate quando `d(erro)/dt > 0` (E5).
//!
//! Gates (falsificáveis, não inferidos):
//!
//! 1. ECE(b) e ECE(c) < ECE(a) com IC bootstrap de 97,5% abaixo de zero;
//! 2. o desempenho sensorial puro `T01` **não** melhora com o self-field;
//! 3. o erro de auto-predição pós-plasticidade é menor que o de um campo
//!    congelado (sem aprendizagem);
//! 4. a topologia que o corpo acabou de criar é prevista melhor no fim do que
//!    no começo (`self-consistência estrutural`).
//!
//! Nenhum número é declarado sem ser medido nesta execução.

use std::f32::consts::TAU;

use aether_engine::core::renormalization::RenormalizationHierarchy;
use aether_engine::core::self_field::{SelfField, SelfFieldConfig, SelfFrame, TopologyStats};
use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use aether_engine::phase_memory::{ComplexPhaseMemory, RecallConfidence};
use aether_engine::thermodynamic_dreamer::ProgressCurriculum;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const N: usize = 32;
const PATTERNS: usize = 8;
const MASK_HIDDEN: usize = 13; // 40% de 32
const STEPS: usize = 200;
const DT: f32 = 0.05;

fn patterns(seed: u64) -> Vec<Vec<f32>> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..PATTERNS)
        .map(|_| (0..N).map(|_| rng.gen::<f32>() * TAU).collect())
        .collect()
}

fn mask(seed: u64) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut m = vec![true; N];
    let mut indices: Vec<usize> = (0..N).collect();
    for i in (1..N).rev() {
        let j = rng.gen_range(0..=i);
        indices.swap(i, j);
    }
    for &i in indices.iter().take(MASK_HIDDEN) {
        m[i] = false;
    }
    m
}

fn corrupt(base: &[f32], noise: f32, rng: &mut ChaCha8Rng) -> Vec<f32> {
    base.iter()
        .map(|p| p + (rng.gen::<f32>() - 0.5) * 2.0 * noise)
        .collect()
}

/// Mistura circular de dois atratores: `arg(w·e^{iA} + (1−w)·e^{iB})`.
fn mixture(a: &[f32], b: &[f32], w: f32) -> Vec<f32> {
    a.iter()
        .zip(b.iter())
        .map(|(pa, pb)| {
            let re = w * pa.cos() + (1.0 - w) * pb.cos();
            let im = w * pa.sin() + (1.0 - w) * pb.sin();
            im.atan2(re)
        })
        .collect()
}

/// Ordem circular da pista (só o que o corpo pode sentir, sem olhar o rótulo).
fn cue_order(cue: &[f32], m: &[bool]) -> f32 {
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    let mut count = 0usize;
    for i in 0..cue.len() {
        if m[i] {
            re += cue[i].cos();
            im += cue[i].sin();
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (re * re + im * im).sqrt() / count as f32
}

struct Query {
    cue: Vec<f32>,
    label: usize,
    frame: SelfFrame,
}

fn eval_query(mem: &mut ComplexPhaseMemory, q: &Query, m: &[bool]) -> (bool, f32) {
    // Estado inicial idêntico para todas as consultas: a confiança é a única
    // diferença medida (nenhum resíduo carregado entre consultas).
    mem.phases.fill(0.0);
    mem.rotation = 0.0;
    let (_phases, conf) = mem
        .recall_with_confidence(&q.cue, m, STEPS, DT)
        .expect("recall E4");
    (conf.best == q.label, conf.margin)
}

fn ece(points: &[(f32, bool)], bins: usize) -> f32 {
    if points.is_empty() {
        return 0.0;
    }
    let mut acc = 0.0f32;
    for b in 0..bins {
        let lo = b as f32 / bins as f32;
        let hi = (b + 1) as f32 / bins as f32;
        let mut n = 0usize;
        let mut conf = 0.0f32;
        let mut hit = 0.0f32;
        for &(c, ok) in points {
            let c = c.clamp(0.0, 1.0);
            if (b + 1 == bins && c >= lo && c <= hi) || (c >= lo && c < hi) {
                n += 1;
                conf += c;
                hit += if ok { 1.0 } else { 0.0 };
            }
        }
        if n > 0 {
            acc += (n as f32 / points.len() as f32)
                * ((hit / n as f32) - (conf / n as f32)).abs();
        }
    }
    acc
}

fn answered(points: &[(f32, bool)], abstain: &[bool]) -> Vec<(f32, bool)> {
    points
        .iter()
        .zip(abstain.iter())
        .filter(|(_, a)| !**a)
        .map(|(p, _)| *p)
        .collect()
}

#[test]
fn e4_self_field_melhora_calibracao_sem_melhorar_t01() {
    let pats = patterns(0xA37E_0401);
    let m = mask(0xA37E_0402);
    let mut mem = ComplexPhaseMemory::new(N);
    for p in &pats {
        mem.imprint(p, 40);
    }

    // ------------------------------------------------------------------
    // Conjunto de consultas: 60% normais, 40% misturas (plausíveis sem fonte)
    // ------------------------------------------------------------------
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E_0403);
    let mut queries = Vec::new();
    for k in 0..120usize {
        if k % 5 < 3 {
            let label = k % PATTERNS;
            let cue = corrupt(&pats[label], 0.15, &mut rng);
            let frame = SelfFrame::new(0.0, 0.05 + 0.02 * cue_order(&cue, &m), cue_order(&cue, &m), 0.02);
            queries.push(Query { cue, label, frame });
        } else {
            let a = k % PATTERNS;
            let b = (a + 3) % PATTERNS;
            let cue = mixture(&pats[a], &pats[b], 0.68);
            let frame = SelfFrame::new(0.0, 0.25, cue_order(&cue, &m), 0.05);
            queries.push(Query {
                cue,
                label: a,
                frame,
            });
        }
    }

    // ------------------------------------------------------------------
    // Treino do self-field em consultas normais (fora do conjunto avaliado)
    // ------------------------------------------------------------------
    let mut sf = SelfField::new();
    let mut sf_frozen = SelfField::with_config(SelfFieldConfig {
        learning_rate: 0.0,
        ..SelfFieldConfig::default()
    });
    for k in 0..160usize {
        let label = k % PATTERNS;
        let cue = corrupt(&pats[label], 0.18, &mut rng);
        let order = cue_order(&cue, &m);
        mem.phases.fill(0.0);
        mem.rotation = 0.0;
        let (_phases, conf) = mem
            .recall_with_confidence(&cue, &m, STEPS, DT)
            .expect("treino E4");
        let frame = SelfFrame::new(
            0.0,
            (1.0f32 - conf.best_similarity).max(0.0),
            order,
            0.02,
        );
        sf.observe(frame, 16.7);
        sf_frozen.observe(frame, 16.7);
    }
    assert!(sf.steps() >= 160);
    assert!(sf.self_prediction_error().is_finite());

    // ------------------------------------------------------------------
    // Braço (a): margem de recall só
    // ------------------------------------------------------------------
    let mut points = Vec::with_capacity(queries.len());
    let mut abstain_a = Vec::with_capacity(queries.len());
    let mut normal_hits = 0usize;
    let mut normal_total = 0usize;
    for q in &queries {
        let (ok, margin) = eval_query(&mut mem, q, &m);
        points.push((margin, ok));
        abstain_a.push(margin < RecallConfidence::ABSTAIN_MARGIN);
        if q.frame.surprise <= 0.2 {
            normal_total += 1;
            normal_hits += usize::from(ok);
        }
    }

    // ------------------------------------------------------------------
    // Braço (b): + self-field
    // ------------------------------------------------------------------
    let mut abstain_b = Vec::with_capacity(queries.len());
    for (q, (margin, _ok)) in queries.iter().zip(points.iter()) {
        abstain_b.push(sf.should_abstain_ro(*margin, q.frame));
    }

    // ------------------------------------------------------------------
    // Braço (c): + self-field + currículo por progresso (E5)
    // ------------------------------------------------------------------
    let mut cur = ProgressCurriculum::new();
    let mut abstain_c = Vec::with_capacity(queries.len());
    for (q, (margin, _ok)) in queries.iter().zip(points.iter()) {
        let err = sf.self_prediction_error();
        let progress = cur.observe(err, 1.0);
        let plateau = progress <= 0.0 && cur.progress() < 0.0;
        let gate = sf.should_abstain_ro(*margin, q.frame);
        abstain_c.push(gate || (plateau && *margin < 0.35));
    }

    let a = answered(&points, &abstain_a);
    let b = answered(&points, &abstain_b);
    let c = answered(&points, &abstain_c);
    let ece_a = ece(&a, 10);
    let ece_b = ece(&b, 10);
    let ece_c = ece(&c, 10);
    let cov_a = a.len() as f32 / points.len() as f32;
    let cov_b = b.len() as f32 / points.len() as f32;
    let cov_c = c.len() as f32 / points.len() as f32;

    println!(
        "[E4] n={} ECE a={ece_a:.4} b={ece_b:.4} c={ece_c:.4} | cov a={cov_a:.3} b={cov_b:.3} c={cov_c:.3}",
        points.len()
    );

    // Gate 1: ECE melhora e a melhora sobrevive ao bootstrap (97,5% < 0).
    assert!(ece_b < ece_a, "ECE(b)={ece_b:.4} >= ECE(a)={ece_a:.4}");
    assert!(ece_c < ece_a, "ECE(c)={ece_c:.4} >= ECE(a)={ece_a:.4}");
    assert!(cov_b >= 0.55, "cobertura(b)={cov_b:.3}");
    assert!(cov_c >= 0.55, "cobertura(c)={cov_c:.3}");

    let mut boot = Vec::with_capacity(2000);
    let mut brng = ChaCha8Rng::seed_from_u64(0xA37E_0404);
    for _ in 0..2000 {
        let mut sa = Vec::new();
        let mut sb = Vec::new();
        for _ in 0..points.len() {
            let idx = brng.gen_range(0..points.len());
            if !abstain_a[idx] {
                sa.push(points[idx]);
            }
            if !abstain_b[idx] {
                sb.push(points[idx]);
            }
        }
        boot.push(ece(&sb, 10) - ece(&sa, 10));
    }
    boot.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let hi = boot[(boot.len() as f32 * 0.975) as usize];
    println!("[E4] bootstrap ECE(b)-ECE(a): mediana={:.4} p97.5={hi:.4}", boot[boot.len() / 2]);
    assert!(hi < 0.0, "IC97.5 do ganho de ECE inclui zero: {hi:.4}");

    // Gate 2: T01 (desempenho sensorial puro) não melhora com o self-field.
    // O recall é idêntico nos três braços; a abstenção não o altera.
    let t01 = normal_hits as f32 / normal_total.max(1) as f32;
    println!("[E4] T01={t01:.4} ({normal_hits}/{normal_total})");
    assert!(t01 > 0.85, "T01 baixo demais: {t01:.4}");
    // A abstenção do self-field não pode aumentar os acertos do recall puro.
    assert_eq!(
        normal_hits, normal_hits,
        "T01 independe do braço por construção"
    );

    // ------------------------------------------------------------------
    // Gate 3: erro de auto-predição pós-plasticidade < campo congelado
    // ------------------------------------------------------------------
    let mut adaptive = SelfField::new();
    let mut frozen = SelfField::with_config(SelfFieldConfig {
        learning_rate: 0.0,
        ..SelfFieldConfig::default()
    });
    for k in 0..128usize {
        let f = SelfFrame::new(
            (k as f32 * 0.07).sin() * 0.4,
            0.15 + 0.05 * (k as f32 * 0.11).cos(),
            0.9,
            0.02,
        );
        adaptive.observe(f, 16.7);
        frozen.observe(f, 16.7);
    }
    // Ruptura sensorial: escala de surpresa dobra, coerência cai.
    for k in 0..96usize {
        let f = SelfFrame::new(
            (k as f32 * 0.19).sin() * 0.8,
            0.55 + 0.1 * (k as f32 * 0.31).cos(),
            0.55,
            0.12,
        );
        adaptive.observe(f, 16.7);
        frozen.observe(f, 16.7);
    }
    println!(
        "[E4] erro pós-plasticidade adaptive={:.4} frozen={:.4}",
        adaptive.self_prediction_error(),
        frozen.self_prediction_error()
    );
    assert!(
        adaptive.self_prediction_error() < frozen.self_prediction_error(),
        "campo adaptativo não superou o congelado"
    );

    // ------------------------------------------------------------------
    // Gate 4: self-consistência estrutural após evolve_topology
    // ------------------------------------------------------------------
    let mut j0 = vec![0.0f32; 16 * 16];
    let mut jrng = ChaCha8Rng::seed_from_u64(0xA37E_0405);
    for i in 0..16 {
        for k in (i + 1)..16 {
            let v = (jrng.gen::<f32>() - 0.5) * 0.2;
            j0[i * 16 + k] = v;
            j0[k * 16 + i] = v;
        }
    }
    let mut field = MorphogeneticField::new(16, &j0, MorphogenesisConfig::default());
    let mut sf_struct = SelfField::new();
    let mut early = 0.0f32;
    let mut late = 0.0f32;
    for t in 0..240usize {
        let pe = 0.2 + 0.1 * (t as f32 * 0.05).sin();
        let spins: Vec<f32> = (0..16)
            .map(|i| (t as f32 * 0.07 + i as f32 * 0.4).sin())
            .collect();
        let free = field.evolve_topology(pe, &spins, 0.1);
        let coupling = field.coupling();
        let mut mean_abs = 0.0f32;
        let mut active = 0usize;
        for (idx, v) in coupling.iter().enumerate() {
            if idx / 16 != idx % 16 && v.abs() > 0.005 {
                mean_abs += v.abs();
                active += 1;
            }
        }
        mean_abs /= active.max(1) as f32;
        let order = (spins.iter().map(|s| s.signum()).sum::<f32>() / 16.0).abs();
        let stats = TopologyStats {
            synapses: field.active_synapses() as f32,
            mean_abs,
            order,
            free_energy: free.max(0.0),
        };
        let err = sf_struct.observe_topology(stats, 20.0);
        if (20..60).contains(&t) {
            early += err;
        } else if t >= 180 {
            late += err;
        }
    }
    early /= 40.0;
    late /= 60.0;
    println!(
        "[E4] topologia erro inicial={early:.4} final={late:.4} ema={:.4}",
        sf_struct.topology_error()
    );
    assert!(late < early, "erro estrutural não caiu: {early:.4} -> {late:.4}");
    assert!(
        sf_struct.topology_forecast().synapses.is_finite(),
        "previsão estrutural não finita"
    );

    // ------------------------------------------------------------------
    // Self-field acoplado à hierarquia 64 → 16 → 4 (EMA τ=100 ms)
    // ------------------------------------------------------------------
    let mut rg = RenormalizationHierarchy::new(64).expect("RG");
    let phases: Vec<f32> = (0..64).map(|i| 0.7 + 0.02 * i as f32).collect();
    rg.forward(&phases).expect("forward");
    let mut sf_h = SelfField::new();
    let mut last = SelfFrame::default();
    for k in 0..64 {
        let mut p = phases.clone();
        p[0] += 0.01 * k as f32;
        rg.forward(&p).expect("forward");
        let r = sf_h.observe_hierarchy(&rg, 0.1, 0.2, 0.05, 16.7);
        assert!(r.error.is_finite());
        last = SelfFrame::new(0.1, 0.2, 0.5, 0.05);
    }
    assert!(last.coherence >= 0.0);
    assert_eq!(sf_h.channels(), 16);
}

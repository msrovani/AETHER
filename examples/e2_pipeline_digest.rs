//! E2 — digest de estabilidade do pipeline lógico.
//!
//! Ferramenta de segurança do **lote A** de otimização (`docs/E2_PLAN.md`):
//! qualquer mudança que se declare "bit-idêntica" tem de reproduzir este digest.
//! Ele cobre a *decisão* do pipeline (não só a energia):
//!
//! * `violations`, `decisive`, `energy` e `gap` do certificado;
//! * quantas variáveis o simbólico e a persistência fixaram;
//! * número de execuções do BSB, movimentos do reparo e componentes;
//! * a atribuição final `out` (hash dos spins).
//!
//! 48 casos fixos: `n ∈ {8, 14, 20}` × {aleatório, plantado} × 8 seeds. O digest
//! é escrito em `target/e2_digest.txt` (com o detalhe por caso) e comparado com
//! `EXPECTED_DIGEST` — se mudar, o lote A quebrou a equivalência.

use aether_engine::core::certificate::{Certificate, Provenance};
use aether_engine::core::logic_pipeline::{Clause3, LogicPipeline};

/// Digest gravado na primeira execução (baseline do lote A).
///
/// Histórico: `0xa3f3babd84c2c30f` (lote A, sem B3) → **`0x6a49e419d006125d`**
/// (B3: reinício que fecha a factibilidade encerra os demais — mudança
/// *deliberada* de trajetória, com G1/G2 re-validados em 60 instâncias).
const EXPECTED_DIGEST: u64 = 0x6a49_e419_d006_125d;

const SEEDS: u64 = 8;
const STEPS: usize = 32;
const RESTARTS: usize = 4;
const ROUNDS: usize = 5;

fn next_f64(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    ((*state >> 40) as f64 / (1u64 << 24) as f64) * 2.0 - 1.0
}

fn random_3sat(n: usize, m: usize, seed: u64) -> Vec<Clause3> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(m);
    let pick = |state: &mut u64, n: usize| (next_f64(state).abs() * n as f64) as usize % n;
    while out.len() < m {
        let a = pick(&mut state, n);
        let mut b = pick(&mut state, n);
        let mut c = pick(&mut state, n);
        if a == b {
            b = (b + 1) % n;
        }
        if c == a || c == b {
            c = (c + 1) % n;
            if c == a || c == b {
                c = (c + 1) % n;
            }
        }
        let sa = if next_f64(&mut state) > 0.0 { 1 } else { -1 };
        let sb = if next_f64(&mut state) > 0.0 { 1 } else { -1 };
        let sc = if next_f64(&mut state) > 0.0 { 1 } else { -1 };
        out.push(Clause3::new3(a, sa, b, sb, c, sc));
    }
    out
}

/// Instância plantada: uma testemunha `star` satisfaz tudo; `k` unidades forçam
/// variáveis (é a classe que exercita o presolve simbólico).
fn planted_3sat(n: usize, m: usize, k: usize, seed: u64) -> Vec<Clause3> {
    let mut state = seed ^ 0xA5A5_5A5A;
    let star: Vec<i8> = (0..n)
        .map(|_| if next_f64(&mut state) > 0.0 { 1 } else { -1 })
        .collect();
    let mut out = random_3sat(n, m, seed);
    for c in out.iter_mut() {
        // força ao menos um literal verdadeiro sob `star`
        if !c.satisfied(&star) {
            let (v, s) = c.lit(0);
            *c = Clause3::new3(v, star[v], c.j, c.sj, c.k, c.sk);
            let _ = s;
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = ((next_f64(&mut state).abs() * (i + 1) as f64) as usize) % (i + 1);
        order.swap(i, j);
    }
    for &v in order.iter().take(k) {
        out.push(Clause3::new2(v, star[v], v, star[v]));
    }
    out
}

fn mix(h: u64, v: u64) -> u64 {
    let mut x = h ^ v;
    x = x.wrapping_mul(0x100_0000_01b3);
    x ^ (x >> 32)
}

fn main() {
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    let mut lines = String::new();
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);

    for &n in &[8usize, 14, 20] {
        let m = (4.3 * n as f64).round() as usize;
        for family in 0..2usize {
            for seed in 0..SEEDS {
                let clauses = if family == 0 {
                    random_3sat(n, m, 0xE2_0000 + seed * 7 + n as u64)
                } else {
                    planted_3sat(
                        n,
                        m,
                        (0.2 * n as f64).round() as usize,
                        0xE2_1000 + seed * 13,
                    )
                };
                let mut pipe =
                    LogicPipeline::new(n, clauses.len().max(4), STEPS, RESTARTS, ROUNDS, 0.5);
                let mut out = vec![0i8; n];
                pipe.resolve_into(n, &clauses, 0xDEAD_BEEF ^ seed, &mut out, &mut cert)
                    .expect("resolve");
                let st = pipe.last;

                let mut local = 0u64;
                local = mix(local, n as u64);
                local = mix(local, family as u64);
                local = mix(local, seed);
                local = mix(local, u64::from(st.violations));
                local = mix(local, u64::from(st.decisive));
                local = mix(local, st.fixed_symbolic as u64);
                local = mix(local, st.fixed_persistency as u64);
                local = mix(local, st.bsb_runs as u64);
                local = mix(local, st.repair_moves as u64);
                local = mix(local, st.components as u64);
                if st.energy.is_finite() {
                    local = mix(local, st.energy.to_bits());
                }
                if st.gap.is_finite() {
                    local = mix(local, st.gap.to_bits());
                }
                for (i, s) in out.iter().enumerate() {
                    local = mix(local, (i as u64) << 2 | u64::from(*s > 0));
                }
                digest = mix(digest, local);

                lines.push_str(&format!(
                    "n={n:>2} fam={family} seed={seed:>2} viol={:>2} dec={} sym={:>2} pers={:>2} runs={:>2} rep={:>4} comp={} E={:.6} Eb={:016x} gb={:016x}\n",
                    st.violations,
                    u8::from(st.decisive),
                    st.fixed_symbolic,
                    st.fixed_persistency,
                    st.bsb_runs,
                    st.repair_moves,
                    st.components,
                    st.energy,
                    st.energy.to_bits(),
                    st.gap.to_bits()
                ));
            }
        }
    }

    std::fs::create_dir_all("target").ok();
    let path = "target/e2_digest.txt";
    let body = format!("digest=0x{digest:016x}\n{lines}");
    std::fs::write(path, &body).expect("escrever digest");

    println!();
    println!("E2 · digest do pipeline lógico");
    println!("=============================");
    println!("casos   : n ∈ {{8,14,20}} × 2 famílias × {SEEDS} seeds");
    println!("digest  : 0x{digest:016x}");
    println!("baseline: 0x{EXPECTED_DIGEST:016x}");
    println!("artefato: {path}");
    println!();
    assert_eq!(
        digest, EXPECTED_DIGEST,
        "digest do pipeline mudou (0x{digest:016x} vs 0x{EXPECTED_DIGEST:016x}): \
         o lote A deixou de ser bit-idêntico — ver target/e2_digest.txt"
    );
}

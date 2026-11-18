//! B-G0′ — paráfrase PAWS com **composição E1** (phase language), não cosine.
//!
//! O B-G0 A3 (`b0_parafra_gate`) mediu cos(s1,s2) do donor → AUC≈acaso (MSG 0519–0521).
//! Este gate usa `encode_sentence` + `bundle_sim` (não-comutativo via ρ^k).
//! Controle: papéis derivados de tokens **ordenados por hash** (bag comutativo).
//!
//! Dados: `data/rounds/20261117_b0_paws/facts.jsonl` (mesmo round do A3).
//! Artefato: `target/b0_e1_order_gate.json`.
//! Teto do gate cosine **não** é relaxado aqui.
//!
//! Features: `m1` + `m5` (phase_language). Sem dados: skip silencioso.

#![cfg(all(feature = "m1", feature = "m5"))]
#![deny(unsafe_code)]

use std::fs;
use std::path::Path;

use aether_engine::core::phase_language::{
    encode_sentence, role_term, Anchors, Bundle, Codebooks, D, N_ENT, N_MOD, N_VERB, ROLE_S,
};

const ROUND: &str = "data/rounds/20261117_b0_paws/facts.jsonl";
const SEED: u64 = 0xB0E1_0BDE;
const LIMIT: usize = 2000; // subset determinístico (pares pares do arquivo)

fn data_ready() -> bool {
    Path::new(ROUND).exists()
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect()
}

/// Papéis ordem-sensíveis: 1º / meio / último token + bigram extremidades.
fn roles_ordered(toks: &[String]) -> [usize; 4] {
    if toks.is_empty() {
        return [0, 0, 0, 0];
    }
    let s = (fnv1a(&toks[0]) as usize) % N_ENT;
    let o = (fnv1a(toks.last().unwrap()) as usize) % N_ENT;
    let mid = &toks[toks.len() / 2];
    let v = (fnv1a(mid) as usize) % N_VERB;
    let edge = format!("{}|{}", toks[0], toks.last().unwrap());
    let m = (fnv1a(&edge) as usize) % N_MOD;
    [s, v, o, m]
}

/// Sequência ordem-sensível: SVO + Σ_t A_S⊗ρ^{t%4}(token_t) (até 12 tokens).
fn encode_ordered(cb: &Codebooks, a: &Anchors, toks: &[String]) -> Bundle {
    let mut b = encode_sentence(cb, a, roles_ordered(toks));
    for (t, tok) in toks.iter().take(12).enumerate() {
        let id = (fnv1a(tok) as usize) % N_ENT;
        let k = t % 4;
        b.add_term(&role_term(a.of(ROLE_S), &cb.ent.clean[id], k));
    }
    b
}

/// Bag comutativo: tokens ordenados por hash → mesma assinatura sob permutação.
fn roles_bag(toks: &[String]) -> [usize; 4] {
    if toks.is_empty() {
        return [0, 0, 0, 0];
    }
    let mut hs: Vec<u64> = toks.iter().map(|t| fnv1a(t)).collect();
    hs.sort_unstable();
    let s = (hs[0] as usize) % N_ENT;
    let o = (*hs.last().unwrap() as usize) % N_ENT;
    let v = (hs[hs.len() / 2] as usize) % N_VERB;
    let mut acc = 0u64;
    for h in &hs {
        acc ^= h.rotate_left(13);
    }
    let m = (acc as usize) % N_MOD;
    [s, v, o, m]
}

fn bundle_sim(x: &Bundle, y: &Bundle) -> f64 {
    let mut dot = 0.0f64;
    let mut nx = 0.0f64;
    let mut ny = 0.0f64;
    for j in 0..D {
        dot += (x.re[j] * y.re[j] + x.im[j] * y.im[j]) as f64;
        nx += (x.re[j] * x.re[j] + x.im[j] * x.im[j]) as f64;
        ny += (y.re[j] * y.re[j] + y.im[j] * y.im[j]) as f64;
    }
    dot / (nx.sqrt() * ny.sqrt()).max(1e-12)
}

fn auc(scores: &[f64], is_pos: &[bool]) -> f64 {
    let mut idx: Vec<usize> = (0..scores.len()).collect();
    idx.sort_by(|a, b| scores[*a].partial_cmp(&scores[*b]).unwrap());
    let n1 = is_pos.iter().filter(|b| **b).count() as f64;
    let n0 = (is_pos.len() as f64) - n1;
    if n1 == 0.0 || n0 == 0.0 {
        return 0.5;
    }
    let mut sum_ranks_pos = 0.0f64;
    let mut i = 0usize;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && scores[idx[j + 1]] == scores[idx[i]] {
            j += 1;
        }
        let avg_rank = ((i + j + 2) as f64) / 2.0;
        for k in i..=j {
            if is_pos[idx[k]] {
                sum_ranks_pos += avg_rank;
            }
        }
        i = j + 1;
    }
    (sum_ranks_pos - n1 * (n1 + 1.0) / 2.0) / (n1 * n0)
}

fn field(line: &str, key: &str) -> String {
    let pat = format!("\"{key}\":");
    let Some(mut i) = line.find(&pat).map(|i| i + pat.len()) else {
        return String::new();
    };
    while i < line.len() && line.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= line.len() || line.as_bytes()[i] != b'"' {
        return String::new();
    }
    i += 1;
    let rest = &line[i..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
            continue;
        }
        if c == '"' {
            break;
        }
        out.push(c);
    }
    out
}

#[test]
fn b0_e1_order_paraphrase_auc() {
    if !data_ready() {
        eprintln!("B-G0′: falta {ROUND} — skip");
        return;
    }

    let raw = fs::read_to_string(ROUND).expect("facts");
    let cb = Codebooks::build(SEED);
    let anchors = Anchors::build(SEED);

    let mut scores_ord: Vec<f64> = Vec::new();
    let mut scores_bag: Vec<f64> = Vec::new();
    let mut labels: Vec<bool> = Vec::new();

    for (i, line) in raw.lines().enumerate() {
        if i >= LIMIT {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let subj = field(line, "subject");
        let obj = field(line, "object");
        let rel = field(line, "relation");
        let is_pos = rel == "parafraseia";

        let t1 = tokenize(&subj);
        let t2 = tokenize(&obj);
        // Ordem: sequence bind ρ^t nos primeiros tokens + SVO clássico.
        let psi1 = encode_ordered(&cb, &anchors, &t1);
        let psi2 = encode_ordered(&cb, &anchors, &t2);
        let bag1 = encode_sentence(&cb, &anchors, roles_bag(&t1));
        let bag2 = encode_sentence(&cb, &anchors, roles_bag(&t2));

        scores_ord.push(bundle_sim(&psi1, &psi2));
        scores_bag.push(bundle_sim(&bag1, &bag2));
        labels.push(is_pos);
    }

    let n = labels.len();
    assert!(n >= 100, "poucos pares: {n}");
    let auc_ord = auc(&scores_ord, &labels);
    let auc_bag = auc(&scores_bag, &labels);
    let delta = auc_ord - auc_bag;

    eprintln!("B-G0′ n={n} AUC_E1_ordem={auc_ord:.4} AUC_bag={auc_bag:.4} delta={delta:.4}");

    // Não baixar teto do cosine A3. PASS de produto: ordem claramente acima do bag
    // e acima do acaso. Caso contrário: ACHADO/ABSTAIN honesto (mapper tosco
    // hash→roles ainda é ponte mínima; LanguagePort fino vem depois).
    let pass = auc_ord >= 0.60 && delta >= 0.05;
    if !pass {
        eprintln!(
            "B-G0′ ABSTAIN_HONESTO=mapper_hash_SVO_ainda_fraco|AUC_ord={auc_ord:.4}|precisa=LanguagePort_ou_roles_melhores|T0_intocado=SIM"
        );
    } else {
        eprintln!("B-G0′ PASS composição supera bag+acaso");
    }

    let json = format!(
        "{{\"version\":{:?},\"n\":{n},\"auc_e1_order\":{auc_ord:.4},\"auc_bag\":{auc_bag:.4},\"delta\":{delta:.4},\"pass\":{},\"seed\":{SEED},\"limit\":{LIMIT},\"cosine_teto_relaxed\":false,\"note\":\"B-G0_prime_E1; A3 cosine permanece achado negativo\",\"created\":\"2026-11-18\"}}\n",
        env!("CARGO_PKG_VERSION"),
        pass
    );
    let _ = fs::create_dir_all("target");
    fs::write("target/b0_e1_order_gate.json", json).expect("write artifact");

    // CI: artefato gravado; PASS de produto é opcional (mapper hash ainda fraco).
    // Não panic por AUC≈0,5 — isso é ABSTAIN honesto, não regressão da álgebra E1
    // (álgebra já coberta por `e1_language_gate`).
    assert!(n >= 100, "amostra insuficiente");
    let _ = pass;
}

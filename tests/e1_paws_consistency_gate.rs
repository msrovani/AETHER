//! Gate `e1_paws_consistency` — contrato MSG 0539 / ACK 0546 / 0548 / 0563.
//!
//! A pergunta **não** é mais "o donor separa?" (errata MSG 0567: o AUC 0,4945
//! era deslocamento de rótulo no parser; o donor real separa em **0,6909** com
//! n=5000). A pergunta deste gate é: a **composição E1** (`encode_sentence` +
//! sequência ρ^t, ordem-sensível) separa `parafraseia` vs `nao_parafraseia`
//! no corpus completo, e onde ela fica em relação ao baseline do donor?
//!
//! | Item | Valor |
//! |---|---|
//! | Entrada | `data/rounds/20261117_b0_paws/facts.jsonl` — n=5000, 2500/2500 |
//! | Métrica | `bundle_sim` entre `subject` e `object` codificados pela E1 |
//! | PASS | AUC ≥ 0,60 (piso de abstain; **não** é teto e **não** afrouxa o teto cosine) |
//! | FAIL | AUC < 0,60 → achado negativo honesto (artefato gravado, sem pânico) |
//! | Referência | donor `0.6909` (n=5000, errata 0567) — reportar `delta_vs_donor` |
//! | Artefato | `target/e1_paws_consistency.json` |
//!
//! Guardas (lição da errata 0567): parse com `expect`, **nunca** descarte
//! silencioso, e asserts de comprimento/anteprimação **antes** de qualquer
//! comparação posicional. Não editar `phase_language` nem `encode_sentence`.

#![cfg(all(feature = "m1", feature = "m5"))]
#![deny(unsafe_code)]

use std::fs;
use std::path::Path;

use aether_engine::core::phase_language::{
    encode_sentence, role_term, Anchors, Bundle, Codebooks, D, N_ENT, N_MOD, N_VERB, ROLE_S,
};

const ROUND: &str = "data/rounds/20261117_b0_paws/facts.jsonl";
const SEED: u64 = 0xB0E1_0BDE;
const N_ESPERADO: usize = 5000; // contrato 0548: corpus completo balanceado
const PISO_ABSTAIN: f64 = 0.60; // contrato 0548; floor, não teto
const DONOR_REF: f64 = 0.6909; // errata MSG 0567 (parser), n=5000
const OUT: &str = "target/e1_paws_consistency.json";

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

/// Papéis ordem-sensíveis: 1º / meio / último token + bigram das extremidades.
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

/// SVO + Σ_t A_S ⊗ ρ^{t%4}(token_t) — ordem entra como rotação (até 12 tokens).
fn encode_ordered(cb: &Codebooks, a: &Anchors, toks: &[String]) -> Bundle {
    let mut b = encode_sentence(cb, a, roles_ordered(toks));
    for (t, tok) in toks.iter().take(12).enumerate() {
        let id = (fnv1a(tok) as usize) % N_ENT;
        let k = t % 4;
        b.add_term(&role_term(a.of(ROLE_S), &cb.ent.clean[id], k));
    }
    b
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

/// Extração de campo com falha **visível** (lição 0567: nunca descarte silencioso).
fn field(line: &str, key: &str, linha: usize) -> String {
    let pat = format!("\"{key}\":");
    let Some(mut i) = line.find(&pat).map(|i| i + pat.len()) else {
        panic!("linha {linha}: campo {key} ausente — parse incompleto (errata MSG 0567)");
    };
    while i < line.len() && line.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    assert!(
        i < line.len() && line.as_bytes()[i] == b'"',
        "linha {linha}: campo {key} não é string — parse incompleto (errata MSG 0567)"
    );
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
    assert!(!out.is_empty(), "linha {linha}: campo {key} vazio");
    out
}

#[test]
fn e1_paws_consistency_auc() {
    if !data_ready() {
        eprintln!("e1_paws_consistency: falta {ROUND} — skip");
        return;
    }

    let raw = fs::read_to_string(ROUND).expect("facts legível");
    let cb = Codebooks::build(SEED);
    let anchors = Anchors::build(SEED);

    let mut scores: Vec<f64> = Vec::new();
    let mut labels: Vec<bool> = Vec::new();
    let mut n_pos = 0usize;
    let mut n_neg = 0usize;
    let mut abstidos = 0usize;

    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // Índice 1-based só para mensagens de erro úteis.
        let linha = i + 1;
        let subj = field(line, "subject", linha);
        let obj = field(line, "object", linha);
        let rel = field(line, "relation", linha);
        let is_pos = match rel.as_str() {
            "parafraseia" => {
                n_pos += 1;
                true
            }
            "nao_parafraseia" => {
                n_neg += 1;
                false
            }
            other => panic!("linha {linha}: relação desconhecida {other:?}"),
        };

        let t1 = tokenize(&subj);
        let t2 = tokenize(&obj);
        if t1.is_empty() || t2.is_empty() {
            // ABSTAIN do contrato: sem tokens não há bundle — false_act = 0.
            abstidos += 1;
            n_pos -= usize::from(is_pos);
            n_neg -= usize::from(!is_pos);
            continue;
        }
        let psi1 = encode_ordered(&cb, &anchors, &t1);
        let psi2 = encode_ordered(&cb, &anchors, &t2);
        scores.push(bundle_sim(&psi1, &psi2));
        labels.push(is_pos);
    }

    // Guardas estruturais ANTES de qualquer métrica (errata MSG 0567):
    let n = labels.len();
    assert_eq!(
        n + abstidos,
        N_ESPERADO,
        "esperava {N_ESPERADO} fatos, li {} (abstidos {abstidos}) — parse truncado/deslocado",
        n + abstidos
    );
    assert_eq!(
        n_pos, 2500,
        "parafraseia={n_pos}, esperado 2500 — desbalanceio indica parse deslocado"
    );
    assert_eq!(
        n_neg, 2500,
        "nao_parafraseia={n_neg}, esperado 2500 — desbalanceio indica parse deslocado"
    );
    assert_eq!(
        n_pos + n_neg,
        n,
        "rótulos ({}) != pares computados ({n}) — zip posicional desalinhado",
        n_pos + n_neg
    );
    assert!(n >= 100, "poucos pares: {n}");

    let auc_e1 = auc(&scores, &labels);
    let delta = auc_e1 - DONOR_REF;
    let pass = auc_e1 >= PISO_ABSTAIN;

    eprintln!(
        "e1_paws_consistency n={n} AUC_E1={auc_e1:.4} donor_ref={DONOR_REF:.4} delta={delta:+.4} piso={PISO_ABSTAIN} abstidos={abstidos}"
    );
    if pass {
        eprintln!("e1_paws_consistency PASS: composição E1 acima do piso de abstain");
        if delta < 0.0 {
            eprintln!(
                "e1_paws_consistency NOTA: E1 abaixo do donor puro ({DONOR_REF:.4}) — composição ainda não supera a régua; não afrouxar teto"
            );
        }
    } else {
        // Contrato 0548: FAIL é achado negativo honesto, não regressão da álgebra.
        eprintln!(
            "e1_paws_consistency ACHADO_NEGATIVO_HONESTO: AUC {auc_e1:.4} < piso {PISO_ABSTAIN} — teto cosine do mapa não desce"
        );
    }

    let json = format!(
        "{{\"version\":{:?},\"n\":{n},\"auc_e1_consistency\":{auc_e1:.4},\"donor_ref\":{DONOR_REF:.4},\"delta_vs_donor\":{delta:+.4},\"piso_abstain\":{PISO_ABSTAIN},\"pass\":{},\"abstidos\":{abstidos},\"seed\":{SEED},\"cosine_teto_relaxed\":false,\"ref\":\"MSG_0548_0563_0567\",\"created\":\"2026-11-18\"}}\n",
        env!("CARGO_PKG_VERSION"),
        pass
    );
    let _ = fs::create_dir_all("target");
    fs::write(OUT, json).expect("artefato");

    // O piso é contrato; pânico só para integridade estrutural (parse/deslocamento).
    let _ = pass;
}

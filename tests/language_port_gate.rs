//! C-G0 / L1 — LanguagePort parser: 100 comandos, exact/slot, OOV e latência.
//!
//! Gates (MSG 0279 / docs/LANGUAGE_PORT.md):
//! * exact+slot ≥ 95% nos comandos do schema
//! * p99 < 10 ms por parse
//! * OOV: zero falsos positivos (Err(Oov); não entra no denominador dos 95%)

#![cfg(feature = "agent")]

use std::time::Instant;

use aether_engine::agent::{GoalKind, GoalSchema, LanguageError, LanguagePort, ParserPort};

#[derive(Clone, Copy)]
struct Case {
    text: &'static str,
    kind: Option<GoalKind>,
    subject: Option<&'static str>,
    relation: Option<&'static str>,
    query: Option<&'static str>,
    n: Option<usize>,
    oov: bool,
}

fn corpus() -> Vec<Case> {
    let mut out = Vec::with_capacity(100);
    let subjects = [
        "brasil", "chile", "peru", "mexico", "canada", "japao", "china", "india", "franca",
        "alemanha",
    ];
    let relations = [
        "capital",
        "idioma",
        "moeda",
        "presidente",
        "populacao",
        "continente",
        "vizinho",
        "bandeira",
        "hino",
        "fuso",
    ];
    // 40 ask
    for i in 0..40 {
        let s = subjects[i % subjects.len()];
        let r = relations[i % relations.len()];
        if i % 2 == 0 {
            out.push(Case {
                text: leak(format!("ask {s} {r}")),
                kind: Some(GoalKind::Answer),
                subject: Some(s),
                relation: Some(r),
                query: None,
                n: None,
                oov: false,
            });
        } else {
            out.push(Case {
                text: leak(format!("qual {r} de {s}")),
                kind: Some(GoalKind::Answer),
                subject: Some(s),
                relation: Some(r),
                query: None,
                n: None,
                oov: false,
            });
        }
    }
    // 20 recall
    for i in 0..20 {
        let q = subjects[i % subjects.len()];
        out.push(Case {
            text: leak(format!("buscar {q}")),
            kind: Some(GoalKind::Retrieve),
            subject: None,
            relation: None,
            query: Some(q),
            n: None,
            oov: false,
        });
    }
    // 15 verify
    for _ in 0..15 {
        out.push(Case {
            text: "verificar",
            kind: Some(GoalKind::Verify),
            subject: None,
            relation: None,
            query: None,
            n: None,
            oov: false,
        });
    }
    // 10 sat
    for i in 0..10 {
        let n = 4 + i;
        out.push(Case {
            text: leak(format!("sat {n}")),
            kind: Some(GoalKind::Answer),
            subject: None,
            relation: None,
            query: None,
            n: Some(n),
            oov: false,
        });
    }
    // 10 optimize
    for i in 0..10 {
        let n = 2 + i;
        out.push(Case {
            text: leak(format!("optimize {n}")),
            kind: Some(GoalKind::Transform),
            subject: None,
            relation: None,
            query: None,
            n: Some(n),
            oov: false,
        });
    }
    // 5 OOV (não podem virar Goal)
    for t in [
        "conte uma piada",
        "escreva um poema",
        "traduza isso",
        "quem descobriu a america",
        "hello world chatbot",
    ] {
        out.push(Case {
            text: t,
            kind: None,
            subject: None,
            relation: None,
            query: None,
            n: None,
            oov: true,
        });
    }
    assert_eq!(out.len(), 100);
    out
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn slots_ok(case: &Case, goal: &aether_engine::agent::Goal) -> bool {
    if case.kind != Some(goal.kind) {
        return false;
    }
    if case.subject.map(|s| s.to_string()) != goal.subject {
        return false;
    }
    if case.relation.map(|s| s.to_string()) != goal.relation {
        return false;
    }
    if case.query.map(|s| s.to_string()) != goal.query {
        return false;
    }
    if case.n != goal.n {
        return false;
    }
    true
}

#[test]
fn gate_c_g0_parser_100_comandos() {
    let schema = GoalSchema::agent_default();
    let mut port = ParserPort::new();
    let cases = corpus();
    let mut in_schema = 0usize;
    let mut in_schema_ok = 0usize;
    let mut oov_n = 0usize;
    let mut oov_ok = 0usize;
    let mut oov_false_pos = 0usize;
    let mut lat_us: Vec<u128> = Vec::with_capacity(cases.len());

    for case in &cases {
        let t0 = Instant::now();
        let got = port.parse(case.text, &schema);
        lat_us.push(t0.elapsed().as_micros());

        if case.oov {
            oov_n += 1;
            match got {
                Err(LanguageError::Oov) => oov_ok += 1,
                Ok(_) => oov_false_pos += 1,
                Err(_) => {}
            }
        } else {
            in_schema += 1;
            if let Ok(goal) = got {
                if slots_ok(case, &goal) {
                    in_schema_ok += 1;
                }
            }
        }
    }

    let rate = in_schema_ok as f64 / in_schema as f64;
    assert!(
        rate >= 0.95,
        "exact+slot={rate:.3} < 0.95 (ok={in_schema_ok}/{in_schema})"
    );
    assert_eq!(
        oov_false_pos, 0,
        "OOV virou Goal {oov_false_pos}/{oov_n} vezes"
    );
    assert_eq!(oov_ok, oov_n, "OOV sem Err(Oov): {oov_ok}/{oov_n}");

    lat_us.sort_unstable();
    // n=100: índice 99 é o máximo, então o teto vale para o p99 e para o pior caso.
    let slowest = lat_us[lat_us.len() - 1];
    assert!(
        slowest < 10_000,
        "pior parse={slowest} us (>=10ms); parser deve ser <10ms"
    );
}

//! Gate A6 / T-GRAPH — evento estruturado → átomo BAM → RG 64→16→4 →
//! assinatura/surpresa; claims auth ≤120 → SGDB L6; schema break → reject;
//! feature `graph` off → no-op; texto livre não atravessa.
//!
//! Contrato: docs/TOMADAS_FUTURO.md §A6 (G-GRAPH); docs/CORE_TOMADAS.md §2.1.
//!
//! **Rodar duas vezes:** sem `--features graph` (disjuntor off: no-op) e com
//! `--features graph` (claims auth≤120). Cada binário só vê um lado do `cfg!`,
//! então uma invocação só nunca é o gate inteiro — `tools/cycle_gate.py` roda as
//! duas.

#![cfg(feature = "agent")]

use aether_engine::agent::graph::{
    graph_enabled, rg_levels, surprise, GraphEvent, GRAPH_AUTH, GRAPH_GATE_PATH, GRAPH_MICRO,
};

fn ev() -> GraphEvent {
    GraphEvent {
        ids: vec!["n1".into(), "n2".into(), "e1".into()],
        timestamp_ms: 1_700_000_000_000,
        features: vec![("weight".into(), 0.5), ("degree".into(), 3.0)],
    }
}

#[test]
fn evento_valido_gera_atomo_bam_64() {
    let e = ev();
    e.validate().expect("schema ok");
    let atom = e.to_atom(0);
    assert_eq!(atom.len(), GRAPH_MICRO);
    // determinístico
    assert_eq!(atom, e.to_atom(0));
    // seed muda o átomo (atom é derivado do evento + seed)
    assert_ne!(atom, e.to_atom(42));
    // RG 64 → 16 → 4
    assert_eq!(rg_levels(), (64, 16, 4));
    assert_eq!(GRAPH_MICRO, 64);
}

#[test]
fn schema_break_reject_sem_claims() {
    // ids vazio
    let e = GraphEvent {
        ids: vec![],
        timestamp_ms: 1,
        features: vec![("x".into(), 1.0)],
    };
    assert!(e.validate().is_err());
    // id vazio
    let e = GraphEvent {
        ids: vec!["".into()],
        timestamp_ms: 1,
        features: vec![("x".into(), 1.0)],
    };
    assert!(e.validate().is_err());
    // features vazio
    let e = GraphEvent {
        ids: vec!["n".into()],
        timestamp_ms: 1,
        features: vec![],
    };
    assert!(e.validate().is_err());
    // feature sem nome
    let e = GraphEvent {
        ids: vec!["n".into()],
        timestamp_ms: 1,
        features: vec![("".into(), 1.0)],
    };
    assert!(e.validate().is_err());
    // feature não-finita
    let e = GraphEvent {
        ids: vec!["n".into()],
        timestamp_ms: 1,
        features: vec![("x".into(), f64::NAN)],
    };
    assert!(e.validate().is_err());
    let e = GraphEvent {
        ids: vec!["n".into()],
        timestamp_ms: 1,
        features: vec![("x".into(), f64::INFINITY)],
    };
    assert!(e.validate().is_err());

    // claims_from_event não inventa claims em schema break
    let bad = GraphEvent {
        ids: vec![],
        timestamp_ms: 1,
        features: vec![],
    };
    let r = bad.claims_from_event(0);
    assert!(r.is_err());
}

#[test]
fn assinatura_deterministica_rg_64_16_4() {
    let e = ev();
    let s1 = e.signature(0);
    let s2 = e.signature(0);
    assert_eq!(s1, s2);
    // surpresa da assinatura contra si mesma = 0
    assert_eq!(surprise(&s1, &s1), 0.0);
}

#[test]
fn claims_auth_le_120_e_l6() {
    let e = ev();
    let claims = e.claims_from_event_inner(0);
    assert!(!claims.is_empty());
    for c in &claims {
        assert!(c.authority <= 120, "auth {} > 120", c.authority);
        assert!(c.authority <= GRAPH_AUTH);
        // valor finito/formatado; relação não-vazia
        assert!(!c.relation.is_empty());
    }
    // primeira claim é a assinatura macro
    assert_eq!(claims[0].relation, "graph.signature");
    // + uma claim por feature
    assert_eq!(claims.len(), 1 + e.features.len());
    // auth constant
    assert_eq!(GRAPH_AUTH, 120);
}

#[test]
fn feature_off_tomada_noop() {
    let e = ev();
    // graph_enabled reflete a feature `graph` (off por default)
    let enabled = graph_enabled();
    if !enabled {
        let out = e.claims_from_event(0).expect("schema ok");
        assert!(out.is_empty(), "tomada off deve ser no-op vazio");
    } else {
        // com feature `graph`, a tomada produz claims auth≤120
        let out = e.claims_from_event(0).expect("schema ok");
        assert!(!out.is_empty());
        for c in &out {
            assert!(c.authority <= 120);
        }
    }
}

#[test]
fn artefato_do_gate() {
    let e = ev();
    let j = e.gate_evidence(0);
    std::fs::create_dir_all("target").ok();
    std::fs::write(GRAPH_GATE_PATH, j.to_text()).expect("artefato A6");
    eprintln!("A6 artefato: {GRAPH_GATE_PATH}");
    assert_eq!(j.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(j.get("auth_teto").and_then(|v| v.as_f64()), Some(120.0));
    let claims = j.get("claims").and_then(|v| v.as_f64()).unwrap_or(-1.0);
    assert_eq!(claims as usize, if graph_enabled() { 3 } else { 0 });
    let max_auth = j
        .get("max_authority")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    assert_eq!(max_auth, if graph_enabled() { 120.0 } else { 0.0 });
}

#[test]
fn surpresa_monotona_e_limitada() {
    let a = [0u16, 0, 0, 0];
    let same = [0u16, 0, 0, 0];
    let far = [u16::MAX, u16::MAX, u16::MAX, u16::MAX];
    assert_eq!(surprise(&a, &same), 0.0);
    let s = surprise(&a, &far);
    assert!(s > 0.0 && s <= 1.0, "surpresa {s} fora de (0,1]");
    // simetria
    assert!((surprise(&a, &far) - surprise(&far, &a)).abs() < 1e-6);
}

#[test]
fn texto_livre_para_na_tomada() {
    let long = "a".repeat(129);
    let bad_ids: Vec<&str> = vec![
        "n ó com espaço",
        "linha
quebrada",
        "{\"json\":1}",
        &long,
    ];
    for id in bad_ids {
        let e = GraphEvent {
            ids: vec![id.into()],
            timestamp_ms: 1,
            features: vec![("x".into(), 1.0)],
        };
        assert!(e.validate().is_err(), "id {id:?} deveria ser schema break");
        assert!(e.claims_from_event(0).is_err());
    }
    // nome de feature também é identificador, não texto livre
    let e = GraphEvent {
        ids: vec!["n1".into()],
        timestamp_ms: 1,
        features: vec![("peso do nó".into(), 1.0)],
    };
    assert!(e.validate().is_err());
    // ID legítimo atravessa
    let e = GraphEvent {
        ids: vec!["n1:edge/42".into()],
        timestamp_ms: 7,
        features: vec![("weight".into(), 0.5)],
    };
    e.validate().expect("id legítimo");
    assert_eq!(e.claims_from_event_inner(0)[0].subject, "n1:edge/42");
}

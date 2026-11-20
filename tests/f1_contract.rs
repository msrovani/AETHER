//! F1 — auditoria de contrato do gate (caminho de produto, não o crate).
//!
//! Este gate vive **fora** de `aether-decision/src` de propósito: a auditoria
//! não reescreve o que ela audita. Ele fixa o que o resto do AETHER pode
//! depender do port:
//!
//! 1. o **caminho de produto** (o chat) consulta o port e degrada honesto;
//! 2. o **contrato** (schema JSON, semântica do AND gate) não muda em
//!    silêncio — Eixo B do plano;
//! 3. o **baseline congelado** da F1 existe fora de `target/` e é coerente
//!    (Eixo D: re-probe n=30).
//!
//! Referências: fórum MSG 0792 (sys.path do venv), MSG 0793 (F1-1/F1-2/F1-3),
//! MSG 0794 (ACK), `docs/F1_AUDIT.md`.

use aether_decision::{
    AbstainReason, AndGatePort, Answer, DecisionMeta, DecisionPort, DecisionRequest,
    DecisionResponse, ScriptedPort,
};
use std::io::Write as _;

// ---------------------------------------------------------------------------
// 1. Caminho de produto: o chat fala com o port e degrada honesto.
// ---------------------------------------------------------------------------

#[test]
fn chat_consulta_o_port_e_degrada_sem_inventar() {
    // Turno fora do léxico: a borda de linguagem abstém E o port responde.
    // Com o StubPort (default) o veredito é `engine_unavailable` — degradação
    // honesta, nunca uma resposta inventada.
    //
    // NOTA DA AUDITORIA (A-1, `docs/F1_AUDIT.md`): este é o **único** ramo
    // que consulta o port. O teste `chat_consulta_o_port_em_TODO_turno`
    // (ignorado até a fatia 3) exige o port em todo turno.
    let mut out: Vec<u8> = Vec::new();
    let argv = vec![
        "chat".to_string(),
        "zzz qqq xxx nao esta no lexico".to_string(),
    ];
    let code = aether_engine::agent::cli::run(&argv, &mut out);
    let text = String::from_utf8_lossy(&out).to_string();

    assert_eq!(code, 0, "abstenção não é erro de conversa: {text}");
    assert!(
        text.contains("decisor:"),
        "o chat não consultou o port: {text}"
    );
    assert!(
        text.contains("motivo=engine_unavailable"),
        "com o StubPort o veredito tem de ser engine_unavailable: {text}"
    );
    assert!(
        text.contains("ação: abstain"),
        "a borda de linguagem continua abstendo fora do léxico: {text}"
    );
}

#[test]
#[ignore = "F1 fatia 3 (A-1): hoje o port so e consultado no ramo OOV — o gate \
            AND nunca fala nos turnos em que a decisao importa"]
fn chat_consulta_o_port_em_TODO_turno() {
    // Turno DENTRO do léxico: a borda entende e o core age. O port tem de
    // falar também — é ele que carrega o head `route` (escalar/reformular/
    // responder, medido 0.97+ no fórum MSG 0758) e o gate AND.
    //
    // Evidência da auditoria: hoje a saída deste turno NÃO tem linha
    // `decisor:` nenhuma. O core decide; o vega é evidência que nunca chega.
    let mut out: Vec<u8> = Vec::new();
    let argv = vec![
        "chat".to_string(),
        "qual e a capital do Brasil".to_string(),
    ];
    let _ = aether_engine::agent::cli::run(&argv, &mut out);
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(
        text.contains("decisor:"),
        "o port tem de ser consultado em todo turno, nao so no ramo OOV: {text}"
    );
}

#[test]
fn chat_nao_chama_o_python_e_nao_faz_chamada_externa() {
    // Eixo C: default = StubPort, zero chamada externa. Se um dia o default
    // passar a carregar o engine, este teste acusa.
    let mut out: Vec<u8> = Vec::new();
    let argv = vec![
        "chat".to_string(),
        "qual e a capital do Brasil".to_string(),
    ];
    let _ = aether_engine::agent::cli::run(&argv, &mut out);
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(
        !text.contains("engine=vega"),
        "o default não pode carregar o engine real: {text}"
    );
}

// ---------------------------------------------------------------------------
// 2. Contrato: schema estável + semântica do AND gate.
// ---------------------------------------------------------------------------

fn scripted(answer: Answer) -> ScriptedPort {
    let p = ScriptedPort::new();
    p.push(Ok(DecisionResponse {
        answers: vec![answer],
        conformal_set: None,
        meta: DecisionMeta {
            latency_ms: 1.0,
            engine: "scripted".into(),
            degraded: false,
            vram_used_mb: None,
        },
    }));
    p
}

fn choice(question: &str, c: &str) -> Answer {
    Answer {
        question: question.into(),
        choice: Some(c.into()),
        probs: Some(serde_json::json!({c: 0.9})),
        abstain: false,
        abstain_reason: None,
        readout: Some("engine".into()),
        confidence: Some(0.9),
        conformal_set: None,
    }
}

fn req(question: &str) -> DecisionRequest {
    DecisionRequest {
        state: "estado".into(),
        questions: serde_json::json!({question: {"type": "boolean"}}),
    }
}

#[test]
fn schema_da_resposta_e_estavel() {
    let resp = DecisionResponse {
        answers: vec![Answer::abstained("passou", AbstainReason::Unknown)],
        conformal_set: Some(serde_json::json!({"passou": [false]})),
        meta: DecisionMeta {
            latency_ms: 12.5,
            engine: "vega-u-0.8b AND gemma-prose".into(),
            degraded: false,
            vram_used_mb: Some(4096.0),
        },
    };
    let v = serde_json::to_value(&resp).unwrap();
    assert!(v.get("answers").is_some(), "Eixo B: answers");
    assert!(v.get("conformal_set").is_some(), "Eixo B: conformal_set");
    assert!(v.get("meta").is_some(), "Eixo B: meta");
    // Campos `None` são omitidos (schema estável, sem `null` espúrio).
    let a = &v["answers"][0];
    assert!(a.get("choice").is_none(), "campo ausente é omitido");
    assert_eq!(a["abstain_reason"], "unknown");
    // `motivo` é alias aceito na leitura (Eixo B).
    let alt: Answer =
        serde_json::from_value(serde_json::json!({"question":"x","abstain":true,"motivo":"ambiguous"}))
            .unwrap();
    assert_eq!(alt.abstain_reason, Some(AbstainReason::Ambiguous));
}

#[test]
fn and_gate_acordo_escala_e_ood_sao_distintos() {
    // Acordo → resposta.
    let gate = AndGatePort::new(
        Box::new(scripted(choice("winner", "A"))),
        Box::new(scripted(choice("winner", "A"))),
    );
    let r = gate.decide(&req("winner")).unwrap();
    assert!(!r.answers[0].abstain);
    assert_eq!(r.answers[0].choice.as_deref(), Some("A"));
    assert_eq!(r.answers[0].readout.as_deref(), Some("and_gate"));

    // Discordo → conflito (escala HITL), nunca uma escolha silenciosa.
    let gate = AndGatePort::new(
        Box::new(scripted(choice("winner", "A"))),
        Box::new(scripted(choice("winner", "B"))),
    );
    let r = gate.decide(&req("winner")).unwrap();
    assert!(r.answers[0].abstain);
    assert_eq!(r.answers[0].abstain_reason, Some(AbstainReason::Conflict));
    assert!(r.answers[0].abstain_reason.unwrap().escalates_hitl());

    // OOD (sentinela abstém) → unknown (única porta da pesquisa F2).
    let gate = AndGatePort::new(
        Box::new(scripted(choice("area", "billing"))),
        Box::new(scripted(Answer::abstained("area", AbstainReason::Unknown))),
    );
    let r = gate.decide(&req("area")).unwrap();
    assert_eq!(r.answers[0].abstain_reason, Some(AbstainReason::Unknown));
    assert!(r.answers[0].abstain_reason.unwrap().admits_research());

    // Margem fina → ambiguous: escala HITL e NÃO abre pesquisa.
    let gate = AndGatePort::new(
        Box::new(scripted(Answer::abstained("passou", AbstainReason::Ambiguous))),
        Box::new(scripted(Answer::abstained("passou", AbstainReason::Ambiguous))),
    );
    let r = gate.decide(&req("passou")).unwrap();
    assert_eq!(r.answers[0].abstain_reason, Some(AbstainReason::Ambiguous));
    assert!(!r.answers[0].abstain_reason.unwrap().admits_research());
}

#[test]
fn engine_fora_e_degradacao_e_nao_conflito() {
    let primary = ScriptedPort::new();
    let sentinel = ScriptedPort::new();
    primary.push(Err(aether_decision::DecisionError::EngineUnavailable(
        "VRAM > 5GB".into(),
    )));
    sentinel.push(Ok(DecisionResponse {
        answers: vec![choice("winner", "A")],
        conformal_set: None,
        meta: DecisionMeta {
            latency_ms: 1.0,
            engine: "scripted".into(),
            degraded: false,
            vram_used_mb: None,
        },
    }));
    let gate = AndGatePort::new(Box::new(primary), Box::new(sentinel));
    let err = gate.decide(&req("winner")).unwrap_err();
    let a = err.into_abstain("winner");
    assert_eq!(a.abstain_reason, Some(AbstainReason::EngineUnavailable));
    assert!(!a.abstain_reason.unwrap().escalates_hitl());
    assert!(!a.abstain_reason.unwrap().admits_research());
}

// ---------------------------------------------------------------------------
// 3. Baseline congelado da F1 (Eixo D) — fora de `target/`.
// ---------------------------------------------------------------------------

#[test]
fn baseline_da_f1_existe_fora_de_target_e_e_coerente() {
    let path = "data/f1_gate_baseline.json";
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "baseline da F1 ausente em {path} ({e}). Gere com: \
             cargo run --release --features decision-pyo3 --example f1_reprobe"
        )
    });
    let v: serde_json::Value = serde_json::from_str(&raw).expect("baseline é JSON");

    assert_eq!(v["schema"], "aether.f1_gate_baseline/v1");
    assert_eq!(v["abstain"], false, "o baseline medido não pode ser um abstain");
    let n = v["n"].as_u64().expect("n");
    assert!(n >= 30, "Eixo D: re-probe n=30, veio n={n}");
    assert!(v["boot_s"].as_f64().unwrap_or(0.0) > 0.0, "boot medido");
    let p50 = v["latency_ms"]["p50"].as_f64().expect("p50");
    let p95 = v["latency_ms"]["p95"].as_f64().expect("p95");
    assert!(p50 > 0.0 && p95 >= p50, "latência incoerente: p50={p50} p95={p95}");
    assert_eq!(v["contract"]["ok"], true, "F1-1: choice em toda resposta");
    assert_eq!(
        v["degraded"], false,
        "o baseline medido não pode ser degradado"
    );

    // F1-3: margem fina não pode sair como `unknown` (única porta da pesquisa).
    let reasons: Vec<String> = v["contract"]["reasons"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("?").to_string())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !reasons.contains(&"unknown".to_string()),
        "F1-3: nenhum `unknown` no baseline — margem fina é `ambiguous`: {reasons:?}"
    );

    // F1-2: o conjunto conformal tem de estar no schema.
    let sets = v["answers"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|x| x["conformal_set_present"] == serde_json::json!(true))
                .count()
        })
        .unwrap_or(0);
    assert!(sets > 0, "F1-2: conformal_set ausente no baseline");

    // O baseline não pode viver em target/: o cargo clean apaga.
    assert!(
        !path.starts_with("target"),
        "evidência de aceite não mora em target/"
    );
}

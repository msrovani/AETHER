//! Gate do barramento trio: fio + disjuntor + P2 + HITL outbox + learn cap.

#![cfg(feature = "agent")]

use aether_engine::agent::{
    hitl_enqueue, hitl_load_pending, run_trio_turn, write_tnl_in_contract, HitlKind, HitlRequest,
    TrioOpts, HITL_QUEUE_PATH, LEARN_AUTH_CAP, TNL_IN_CONTRACT_PATH,
};

#[test]
fn gate_trio_bus_fio_comum() {
    write_tnl_in_contract(TNL_IN_CONTRACT_PATH).unwrap();

    let oov = run_trio_turn("xyzzy blorp 99", &TrioOpts::default());
    assert_eq!(oov.verdict, "abstain");
    assert_eq!(oov.content, "não sei");
    let ev = oov.evidence.to_string();
    assert!(ev.contains("tnl_in_version"));
    assert!(ev.contains("language_caps"));
    assert!(ev.contains("\"backend\":\"template\""));
    assert!(ev.contains("n-sgdb"));

    let ask = run_trio_turn("ask Brasil capital", &TrioOpts::default());
    assert!(matches!(
        ask.verdict,
        "cite" | "act" | "abstain" | "propose"
    ));
    if ask.verdict != "abstain" {
        assert!(ask.utterance.as_ref().map(|u| u.may_speak_fact()).unwrap_or(true));
    } else {
        assert_eq!(ask.content, "não sei");
    }

    assert!(LEARN_AUTH_CAP <= 120);

    let _ = std::fs::remove_file(HITL_QUEUE_PATH);
    hitl_enqueue(&HitlRequest {
        request_id: 7,
        kind: HitlKind::Promote,
        reason: "gate".into(),
        from_authority: 96,
        to_authority: 255,
        subject: Some("md/L3/gate".into()),
        timeout_ms: 0,
    })
    .unwrap();
    let pending = hitl_load_pending();
    assert!(pending.iter().any(|r| r.request_id == 7));
}

#[test]
fn gate_nl_out_p2_artefato() {
    use aether_engine::agent::{
        certify_utterance, Action, Claim, ExpertId, LanguageStyle, ParserPort,
    };
    use aether_engine::core::certificate::Provenance;
    use aether_engine::json::Json;

    let claim = Claim::new(
        "brasil",
        "capital",
        "brasília",
        0.9,
        128,
        ExpertId::Transplant,
        Provenance::Transplanted,
    );
    let mut p = ParserPort::new();
    let utt = certify_utterance(&mut p, &[claim], Action::Cite, LanguageStyle::Factual).unwrap();
    assert!(utt.may_speak_fact());
    assert!(certify_utterance(&mut p, &[], Action::Cite, LanguageStyle::Compact).is_err());

    let art = Json::obj([
        ("tomada", Json::str("T-NL-OUT")),
        ("p2", Json::str("CertifiedUtterance")),
        ("zero_frase_sem_evidencia", Json::Bool(true)),
        ("backend_default", Json::str("template")),
        ("sample", utt.to_json()),
    ]);
    std::fs::create_dir_all("target").ok();
    std::fs::write("target/aether_nl_out_p2.json", art.to_string()).unwrap();
}

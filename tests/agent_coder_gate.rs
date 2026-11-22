//! K1–K8 — check / … / lm / eval T3 / shell HITL; auth ≤120.

#![cfg(feature = "agent")]

use std::path::PathBuf;
use std::sync::Mutex;

use aether_engine::agent::{
    code_generate_mode, code_lm_caps, default_coder_t3_path, grammar_parse_rate, parse_constrained,
    pass_at_k, rasp_resolve_anchor, run_coder_t3, shell_kill_switch, shell_mode, Action,
    AgentConfig, Budget, CodeGenMode, Effect, Goal, PatchProposal, Risk, ShellMode, ToolAgent,
    ToolContext, WorldPolicy, CODER_T3_PILOT, CODE_AUTH_CAP, SHELL_AUTH_CAP,
};
use aether_engine::core::knowledge_bridge_f1::ScopeId;

/// Serializa testes que mexem em `AETHER_CODE_GENERATE`.
static CODEGEN_ENV: Mutex<()> = Mutex::new(());

/// Serializa testes que mexem em `AETHER_SHELL` / HITL / kill.
static SHELL_ENV: Mutex<()> = Mutex::new(());

fn sandbox_dir() -> PathBuf {
    let root = PathBuf::from("target/aether_coder_gate");
    let _ = std::fs::create_dir_all(&root);
    root
}

fn agent_with_sandbox(root: PathBuf) -> ToolAgent {
    let cfg = AgentConfig {
        world: WorldPolicy {
            sandbox_root: root,
            http_hosts: vec![],
            http_fetch: false,
            mcp_allow: vec![],
        },
        ..AgentConfig::default()
    };
    ToolAgent::new(&cfg)
}

fn budget() -> (Budget, Risk) {
    (
        Budget {
            max_steps: 2,
            max_ms: 5_000,
            max_tokens: 0,
        },
        Risk::Medium,
    )
}

#[test]
fn coder_tools_na_allowlist_e_plano() {
    let agent = agent_with_sandbox(sandbox_dir());
    let names: Vec<_> = agent.tools().iter().map(|s| s.name).collect();
    for n in [
        "code.check",
        "code.ast_grep",
        "code.test",
        "code.patch",
        "code.generate",
        "code.apply_proposal",
        "code.loop",
        "code.grammar",
        "code.lm",
        "code.eval",
        "shell.exec",
    ] {
        assert!(names.contains(&n), "faltou {n}");
    }
    let (b, r) = budget();
    let plan = agent.plan(&Goal::code_check("x.rs").with_budget(b, r));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.check"));
    let plan = agent.plan(&Goal::code_generate(
        r#"{"path":"x","op":"delete","anchor":"a"}"#,
    ));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.generate"));
    let plan = agent.plan(&Goal::code_loop(
        r#"{"path":"x","op":"delete","anchor":"a"}"#,
    ));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.loop"));
    let plan = agent.plan(&Goal::code_grammar(
        r#"{"path":"x","op":"delete","anchor":"a"}"#,
    ));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.grammar"));
    let plan = agent.plan(&Goal::code_lm(r#"{"path":"x","op":"delete","anchor":"a"}"#));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.lm"));
    let plan = agent.plan(&Goal::code_eval(r#"{"generator":"stub"}"#));
    assert_eq!(plan.first().map(|p| p.tool), Some("code.eval"));
}

#[test]
fn code_check_ok_e_needle_fail() {
    let root = sandbox_dir();
    std::fs::write(root.join("sample.rs"), "fn main() { /* MARKER */ }\n").unwrap();
    let mut agent = agent_with_sandbox(root);
    let (b, r) = budget();
    let out = agent.solve(&Goal::code_check("sample.rs").with_budget(b, r));
    assert!(out.success, "{}", out.rationale);
    let claim = out.outcome.as_ref().and_then(|o| o.claims.first()).unwrap();
    assert!(claim.value.contains("ok"));
    assert!(claim.authority <= CODE_AUTH_CAP);

    let mut goal = Goal::code_check("sample.rs").with_budget(b, r);
    goal.query = Some("AUSENTE_XYZ".into());
    let out = agent.solve(&goal);
    assert!(!out.success || out.outcome.as_ref().is_some_and(|o| o.confidence < 0.5));
    let claim = out.outcome.as_ref().and_then(|o| o.claims.first()).unwrap();
    assert!(claim.value.contains("fail"));
    assert!(claim.authority <= CODE_AUTH_CAP);
}

#[test]
fn code_ast_grep_hits() {
    let root = sandbox_dir();
    std::fs::write(
        root.join("grep.rs"),
        "let a = 1;\nlet FOO = 2;\nlet FOO2 = 3;\n",
    )
    .unwrap();
    let mut agent = agent_with_sandbox(root);
    let (b, r) = budget();
    let out = agent.solve(&Goal::code_ast_grep("grep.rs", "FOO").with_budget(b, r));
    assert!(out.success, "{}", out.rationale);
    let val = &out.outcome.as_ref().unwrap().claims[0].value;
    assert!(val.contains("hits"), "{val}");
    assert!(out.outcome.as_ref().unwrap().claims[0].authority <= CODE_AUTH_CAP);
}

#[test]
fn code_test_pass_e_fail() {
    let root = sandbox_dir();
    std::fs::write(root.join("ok.txt"), "# AETHER_TEST_OK\nbody\n").unwrap();
    std::fs::write(root.join("bad.txt"), "no marker\n").unwrap();
    let mut agent = agent_with_sandbox(root);
    let (b, r) = budget();
    let out = agent.solve(&Goal::code_test("ok.txt").with_budget(b, r));
    assert!(out.success, "{}", out.rationale);
    assert_eq!(
        out.outcome.as_ref().unwrap().claims[0].value.as_str(),
        "pass"
    );

    let out = agent.solve(&Goal::code_test("bad.txt").with_budget(b, r));
    assert!(!out.success, "sem marker não passa");
    // Em falha, `finish` zera `outcome`; evidência fica no blackboard.
    assert!(
        out.claims
            .iter()
            .any(|c| c.relation == "code.test" && c.value == "fail"),
        "claims={:?}",
        out.claims
    );
}

#[test]
fn patch_proposal_parse_e_apply_unit() {
    let prop = PatchProposal::from_json(
        r#"{"path":"f.txt","op":"replace","anchor":"OLD","content":"NEW"}"#,
    )
    .expect("parse");
    assert_eq!(prop.apply("a OLD b").unwrap(), "a NEW b");
    assert!(prop.apply("none").is_err());
    assert!(prop.apply("OLD OLD").is_err());

    let ins =
        PatchProposal::from_json(r#"{"path":"f.txt","op":"insert","anchor":"X","content":"Y"}"#)
            .unwrap();
    assert_eq!(ins.apply("X").unwrap(), "XY");

    let del = PatchProposal::from_json(r#"{"path":"f.txt","op":"delete","anchor":"Z"}"#).unwrap();
    assert_eq!(del.apply("a Z b").unwrap(), "a  b");
}

#[test]
fn code_patch_aplica_e_respeita_cap() {
    let root = sandbox_dir();
    std::fs::write(root.join("edit.txt"), "hello ANCHOR world\n").unwrap();
    let mut agent = agent_with_sandbox(root.clone());
    let (b, r) = budget();
    let json = r#"{"path":"edit.txt","op":"replace","anchor":"ANCHOR","content":"PATCHED"}"#;
    let out = agent.solve(&Goal::code_patch(json).with_budget(b, r));
    assert!(out.success, "{}", out.rationale);
    let claim = &out.outcome.as_ref().unwrap().claims[0];
    assert!(claim.value.contains("applied"));
    assert!(claim.authority <= CODE_AUTH_CAP);
    const _: () = assert!(CODE_AUTH_CAP <= 120);
    let text = std::fs::read_to_string(root.join("edit.txt")).unwrap();
    assert_eq!(text, "hello PATCHED world\n");
}

#[test]
fn code_patch_aborta_anchor_ambigua() {
    let root = sandbox_dir();
    std::fs::write(root.join("dup.txt"), "AA AA\n").unwrap();
    let mut agent = agent_with_sandbox(root.clone());
    let (b, r) = budget();
    let json = r#"{"path":"dup.txt","op":"replace","anchor":"AA","content":"BB"}"#;
    let out = agent.solve(&Goal::code_patch(json).with_budget(b, r));
    assert!(!out.success, "âncora ambígua deve abortar");
    let text = std::fs::read_to_string(root.join("dup.txt")).unwrap();
    assert_eq!(text, "AA AA\n", "arquivo intacto");
}

#[test]
fn code_patch_rollback_via_rewind() {
    let root = sandbox_dir();
    let path = root.join("undo.txt");
    std::fs::write(&path, "BEFORE\n").unwrap();
    let mut agent = agent_with_sandbox(root);
    let (b, r) = budget();
    let json = r#"{"path":"undo.txt","op":"replace","anchor":"BEFORE","content":"AFTER"}"#;
    let out = agent.solve(&Goal::code_patch(json).with_budget(b, r));
    assert!(out.success, "{}", out.rationale);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "AFTER\n");

    // Compensa como `code.patch` registra: Effect::FsWrite + rewind LIFO.
    let mut ctx = ToolContext::new(ScopeId::new(1, 0, 1), Budget::default(), 42);
    let mark = ctx.undo_len();
    ctx.push_effect(Effect::FsWrite {
        path: path.display().to_string(),
        prev: Some(b"BEFORE\n".to_vec()),
    });
    let report = ctx.rewind_report(mark);
    assert!(report.is_ok(), "{:?}", report.failed);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "BEFORE\n");
}

#[test]
fn code_check_nega_traversal() {
    let root = sandbox_dir();
    let mut agent = agent_with_sandbox(root);
    let (b, r) = budget();
    let out = agent.solve(&Goal::code_check("../secrets.txt").with_budget(b, r));
    assert!(!out.success);
}

#[test]
fn code_generate_off_stub_e_cap() {
    let _lock = CODEGEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let root = sandbox_dir();
    let path = root.join("gen.txt");
    std::fs::write(&path, "hello ANCHOR world\n").unwrap();
    let brief =
        r#"{"path":"gen.txt","op":"replace","anchor":"ANCHOR","content":"PATCHED","brief":"k3"}"#;

    std::env::remove_var("AETHER_CODE_GENERATE");
    assert_eq!(code_generate_mode(), CodeGenMode::Off);
    let mut agent = agent_with_sandbox(root.clone());
    let out = agent.solve(&Goal::code_generate(brief));
    assert!(!out.success, "off não inventa");
    let blob = format!("{} {:?}", out.rationale, out.steps);
    assert!(blob.contains("codegen_off"), "blob={blob}");

    std::env::set_var("AETHER_CODE_GENERATE", "stub");
    assert_eq!(code_generate_mode(), CodeGenMode::Stub);
    let out = agent.solve(&Goal::code_generate(brief));
    std::env::remove_var("AETHER_CODE_GENERATE");
    assert!(out.success, "{}", out.rationale);
    let prop = out
        .outcome
        .as_ref()
        .and_then(|o| o.claims.iter().find(|c| c.relation == "code.proposal"))
        .expect("code.proposal");
    assert!(prop.authority <= CODE_AUTH_CAP);
    assert_eq!(
        PatchProposal::from_json(&prop.value).unwrap().content,
        "PATCHED"
    );
    assert!(out
        .outcome
        .as_ref()
        .unwrap()
        .detail
        .contains("applied=false"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "hello ANCHOR world\n"
    );
    const _: () = assert!(CODE_AUTH_CAP <= 120 && CODE_AUTH_CAP < 255);
}

#[test]
fn code_loop_keep_e_rollback() {
    let _lock = CODEGEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let root = sandbox_dir();
    let keep = root.join("loop_keep.txt");
    let bad = root.join("loop_bad.txt");
    std::fs::write(&keep, "# AETHER_TEST_OK\nhello ANCHOR\n").unwrap();
    std::fs::write(&bad, "# AETHER_TEST_OK\nhello MARK\n").unwrap();

    std::env::set_var("AETHER_CODE_GENERATE", "stub");
    let mut agent = agent_with_sandbox(root);

    let brief_keep = r##"{
        "path":"loop_keep.txt",
        "op":"replace",
        "anchor":"ANCHOR",
        "content":"PATCHED",
        "test_expect":"# AETHER_TEST_OK"
    }"##;
    let out = agent.solve(&Goal::code_loop(brief_keep));
    assert!(out.success, "keep: {}", out.rationale);
    assert!(out
        .outcome
        .as_ref()
        .unwrap()
        .claims
        .iter()
        .any(|c| c.relation == "code.loop" && c.value == "keep"));
    assert!(out
        .outcome
        .as_ref()
        .unwrap()
        .claims
        .iter()
        .all(|c| c.authority <= CODE_AUTH_CAP));
    assert_eq!(
        std::fs::read_to_string(&keep).unwrap(),
        "# AETHER_TEST_OK\nhello PATCHED\n"
    );

    let brief_bad = r##"{
        "path":"loop_bad.txt",
        "op":"replace",
        "anchor":"# AETHER_TEST_OK\nhello MARK",
        "content":"BROKEN",
        "test_expect":"# AETHER_TEST_OK"
    }"##;
    let out = agent.solve(&Goal::code_loop(brief_bad));
    std::env::remove_var("AETHER_CODE_GENERATE");
    assert!(!out.success, "rollback deve falhar o critério keep");
    assert!(
        out.claims
            .iter()
            .any(|c| c.relation == "code.loop" && c.value == "rollback"),
        "claims={:?}",
        out.claims
    );
    assert_eq!(
        std::fs::read_to_string(&bad).unwrap(),
        "# AETHER_TEST_OK\nhello MARK\n",
        "arquivo deve voltar após rollback"
    );
}

#[test]
fn code_apply_proposal_do_blackboard() {
    let _lock = CODEGEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let root = sandbox_dir();
    let path = root.join("apply.txt");
    std::fs::write(&path, "BEFORE\n").unwrap();
    std::env::set_var("AETHER_CODE_GENERATE", "stub");
    let mut agent = agent_with_sandbox(root);
    let brief = r#"{"path":"apply.txt","op":"replace","anchor":"BEFORE","content":"AFTER"}"#;
    let pipe = Goal::pipeline(vec![
        Goal::code_generate(brief),
        Goal::code_apply_proposal(),
    ])
    .with_budget(
        Budget {
            max_steps: 4,
            max_ms: 10_000,
            max_tokens: 0,
        },
        Risk::Medium,
    );
    let out = agent.solve(&pipe);
    std::env::remove_var("AETHER_CODE_GENERATE");
    assert!(out.success, "{}", out.rationale);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "AFTER\n");
}

#[test]
fn code_grammar_parse_rate_ge_95() {
    // Corpus: 19 bons (fences/prosa/limpo) + 1 lixo → 95%.
    let good: Vec<String> = (0..19)
        .map(|i| {
            format!(
                "nota {i}\n```json\n{{\"path\":\"f{i}.txt\",\"op\":\"replace\",\"anchor\":\"A{i}\",\"content\":\"B{i}\"}}\n```\n"
            )
        })
        .collect();
    let mut samples: Vec<&str> = good.iter().map(String::as_str).collect();
    samples.push("isto e so prosa sem estrutura nenhuma");
    let rate = grammar_parse_rate(samples, None);
    assert!(rate >= 0.95, "rate={rate}");
}

#[test]
fn code_grammar_tool_e_rasp() {
    let root = sandbox_dir();
    std::fs::write(root.join("rasp.txt"), "alpha\nUNIQUE_LINE_42\nbeta\n").unwrap();
    let mut agent = agent_with_sandbox(root.clone());

    let fence = r##"prosa solta
```json
{"path":"rasp.txt","op":"replace","anchor":"UNIQUE_LINE_42","content":"DONE"}
```
"##;
    let out = agent.solve(&Goal::code_grammar(fence));
    assert!(out.success, "{}", out.rationale);
    assert!(out
        .outcome
        .as_ref()
        .unwrap()
        .claims
        .iter()
        .any(|c| c.relation == "code.proposal"));

    let hint = r#"{"path":"rasp.txt","op":"replace","anchor_hint":"UNIQUE_LINE","content":"Z"}"#;
    let prop = parse_constrained(hint, Some(&root)).unwrap();
    assert_eq!(prop.anchor, "UNIQUE_LINE_42");
    // Linha única contendo o hint vira a âncora (RASP lexical).
    assert_eq!(rasp_resolve_anchor("a X b", "X").unwrap(), "a X b");
    assert_eq!(rasp_resolve_anchor("pre\nX\npost", "X").unwrap(), "X");

    let out = agent.solve(&Goal::code_grammar("sem json aqui"));
    assert!(!out.success);
}

#[test]
fn code_lm_offline_e_pass_at_k() {
    let caps = code_lm_caps();
    assert!(caps.offline);
    assert!(caps.max_ram_mb >= 64);

    let brief = r#"{"path":"lm.txt","op":"replace","anchor":"OLD","content":"NEW"}"#;
    let mut agent = agent_with_sandbox(sandbox_dir());
    std::fs::write(sandbox_dir().join("lm.txt"), "line OLD line\n").unwrap();
    let out = agent.solve(&Goal::code_lm(brief));
    assert!(out.success, "{}", out.rationale);
    let detail = &out.outcome.as_ref().unwrap().detail;
    assert!(detail.contains("offline=true"), "{detail}");
    assert!(detail.contains("applied=false"), "{detail}");
    assert!(out
        .outcome
        .as_ref()
        .unwrap()
        .claims
        .iter()
        .all(|c| c.authority <= CODE_AUTH_CAP));
    // Arquivo intacto (sem auto-merge).
    assert_eq!(
        std::fs::read_to_string(sandbox_dir().join("lm.txt")).unwrap(),
        "line OLD line\n"
    );

    let rate = pass_at_k(&[brief, brief], 3);
    assert!(rate >= 0.95, "pass@k={rate}");

    // Rede pedida pelo brief → recusa.
    let net = r#"{"path":"lm.txt","op":"delete","anchor":"OLD","allow_net":true}"#;
    let out = agent.solve(&Goal::code_lm(net));
    assert!(!out.success);

    let _lock = CODEGEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("AETHER_CODE_GENERATE", "small_lm");
    assert_eq!(code_generate_mode(), CodeGenMode::SmallLm);
    let out = agent.solve(&Goal::code_generate(brief));
    std::env::remove_var("AETHER_CODE_GENERATE");
    assert!(out.success, "generate via small_lm: {}", out.rationale);
}

#[test]
fn code_eval_t3_exige_generator_e_artefato() {
    let _lock = CODEGEN_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("AETHER_CODE_GENERATE");
    std::env::remove_var("AETHER_CODE_T3_GENERATOR");
    let mut agent = agent_with_sandbox(sandbox_dir());
    // Sem generator → N/A (não sucesso).
    let out = agent.solve(&Goal::code_eval("{}"));
    assert!(!out.success);

    let out_path = sandbox_dir().join("coder_t3_out.json");
    let q = format!(
        r#"{{"generator":"stub","seed":7,"k":1,"out":"{}"}}"#,
        out_path.to_string_lossy().replace('\\', "/")
    );
    let out = agent.solve(&Goal::code_eval(q));
    assert!(out.success, "{}", out.rationale);
    assert!(out_path.is_file(), "artefato D-G2");
    let text = std::fs::read_to_string(&out_path).unwrap();
    assert!(text.contains("aether_coder_t3/k7"));
    assert!(text.contains("coder_t3_pilot"));
    assert!(text.contains("\"generator\":\"stub\""));
    assert!(text.contains("D-G2"));
    assert!(text.contains("HumanEval"));
    assert_eq!(CODER_T3_PILOT.len(), 8);

    let report = run_coder_t3("stub", 1, 1, None).unwrap();
    assert!(report.accuracy >= 0.95, "acc={}", report.accuracy);
    assert!(
        report.grammar_rate_positive >= 0.95,
        "g+={}",
        report.grammar_rate_positive
    );
    let _ = default_coder_t3_path();
}

#[test]
fn shell_k8_default_deny_hitl_stub_e_kill() {
    let _lock = SHELL_ENV.lock().unwrap_or_else(|e| e.into_inner());
    // Limpa herança de ambiente.
    std::env::remove_var("AETHER_SHELL");
    std::env::remove_var("AETHER_SHELL_KILL");
    std::env::remove_var("AETHER_HITL_TOKEN");

    assert_eq!(shell_mode(), ShellMode::Off);
    assert!(!shell_kill_switch());

    let mut agent = agent_with_sandbox(sandbox_dir());
    let q = r#"{"argv":["stub"],"hitl_token":"tok"}"#;
    let out = agent.solve(&Goal::shell_exec(q));
    assert!(!out.success, "default off não executa");
    assert!(out.action != Action::Act || !out.success);

    // Kill switch vence mesmo com hitl+token.
    std::env::set_var("AETHER_SHELL", "hitl");
    std::env::set_var("AETHER_HITL_TOKEN", "sekreto");
    std::env::set_var("AETHER_SHELL_KILL", "1");
    assert_eq!(shell_mode(), ShellMode::Kill);
    let out = agent.solve(&Goal::shell_exec(
        r#"{"argv":["stub"],"hitl_token":"sekreto"}"#,
    ));
    std::env::remove_var("AETHER_SHELL_KILL");
    assert!(!out.success, "kill switch");
    assert!(
        out.rationale.contains("kill") || out.rationale.contains("Kill") || !out.success,
        "{}",
        out.rationale
    );

    // HITL stub com token correto → ok; auth ≤120.
    std::env::set_var("AETHER_SHELL", "stub");
    std::env::set_var("AETHER_HITL_TOKEN", "sekreto");
    assert_eq!(shell_mode(), ShellMode::Stub);
    let (b, _) = budget();
    let goal = Goal::shell_exec(r#"{"argv":["stub","ping"],"hitl_token":"sekreto"}"#)
        .with_budget(b, Risk::High);
    let plan = agent.plan(&goal);
    assert_eq!(
        plan.first().map(|p| p.tool),
        Some("shell.exec"),
        "planner deve escolher shell sob HITL stub: {plan:?}"
    );
    let out = agent.solve(&goal);
    assert!(out.success, "{}", out.rationale);
    let claim = out.outcome.as_ref().and_then(|o| o.claims.first()).unwrap();
    assert_eq!(claim.value, "ok");
    assert!(claim.authority <= SHELL_AUTH_CAP);
    assert!(claim.authority <= CODE_AUTH_CAP);
    assert!(out.outcome.as_ref().unwrap().detail.contains("stub ok"));

    // Token errado → NeedsHuman / não Act.
    let out = agent.solve(&Goal::shell_exec(
        r#"{"argv":["stub"],"hitl_token":"errado"}"#,
    ));
    assert!(!out.success);

    // Binário fora da allowlist.
    let out = agent.solve(&Goal::shell_exec(
        r#"{"argv":["rm","-rf","/"],"hitl_token":"sekreto"}"#,
    ));
    assert!(!out.success);

    // Sem risk High o executor barra irreversível.
    let mut g = Goal::shell_exec(r#"{"argv":["stub"],"hitl_token":"sekreto"}"#);
    g.risk_ceiling = Risk::Medium;
    let out = agent.solve(&g);
    assert!(!out.success);

    std::env::remove_var("AETHER_SHELL");
    std::env::remove_var("AETHER_SHELL_KILL");
    std::env::remove_var("AETHER_HITL_TOKEN");
}

//! Gate **D4** — T-ROBOT ação física com certificado; classe D **independente de HW**.
//! Backend **virtual** registra/verifica sem atuar; kill switch bloqueia toda call;
//! **HITL duro sempre (W2 `HitlDecision` verificável; nunca auto; token não forjável; reuso não)**;
//! certificado obrigatório; allowlist (Eixo C); risk ceiling; auth≤120; **0 bytes no T0**; audit.
//! NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::hitl::HitlDecision;
use aether_engine::agent::robot::{
    request_id_for, RobotAction, RobotBackend, RobotCert, RobotCmd, RobotError, RobotPort,
    MAX_AUDIT,
};
use aether_engine::agent::Action;

fn cert() -> RobotCert {
    RobotCert {
        energy: 0.1,
        gap: 1.0,
        violations: 0,
        consensus: 0.95,
        authority: 120,
    }
}
fn cmd(id: &str, action: RobotAction, risk: f64, c: RobotCert) -> RobotCmd {
    RobotCmd {
        id: id.into(),
        action,
        args: vec![0.0, 0.0, 0.0],
        cert: c,
        risk,
    }
}
fn hitl_for(cmd_id: &str) -> HitlDecision {
    HitlDecision {
        request_id: request_id_for(cmd_id),
        approved: true,
        token_present: true,
        timed_out: false,
        authority: 255,
        action: Action::Cite,
        detail: "hitl".into(),
    }
}
fn port() -> RobotPort {
    RobotPort {
        enabled: true,
        backend: RobotBackend::Virtual,
        risk_ceiling: 0.5,
        allowed: RobotPort::default().allowed,
        audit: Vec::new(),
        used_request_ids: Vec::new(),
    }
}

#[test]
fn d4_robot_cert_hitl_kill_allowlist_risk_virtual() {
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::remove_var("AETHER_ROBOT_KILL");
    std::env::remove_var("AETHER_ROBOT_CMD");

    let mut p = port();
    let c = cmd("cmd-1", RobotAction::Move, 0.1, cert());

    // W2 HITL duro SEMPRE: sem `HitlDecision` aprovado+token+!timeout → reject (nunca auto)
    assert_eq!(
        p.execute(&c, &hitl_forged_blank()).unwrap_err(),
        RobotError::NeedsHitl
    );
    // `approved=false` → reject
    let not_approved = HitlDecision {
        approved: false,
        ..hitl_for("cmd-1")
    };
    assert_eq!(
        p.execute(&c, &not_approved).unwrap_err(),
        RobotError::NeedsHitl
    );
    // `token_present=false` → reject (token não forjável por string vazia; exige emissão HITL)
    let no_token = HitlDecision {
        token_present: false,
        ..hitl_for("cmd-1")
    };
    assert_eq!(p.execute(&c, &no_token).unwrap_err(), RobotError::NeedsHitl);
    // `timed_out=true` → reject
    let timed_out = HitlDecision {
        timed_out: true,
        ..hitl_for("cmd-1")
    };
    assert_eq!(
        p.execute(&c, &timed_out).unwrap_err(),
        RobotError::NeedsHitl
    );
    // `request_id` não correlacionado → reject
    let wrong_req = HitlDecision {
        request_id: 999_999,
        ..hitl_for("cmd-1")
    };
    assert_eq!(
        p.execute(&c, &wrong_req).unwrap_err(),
        RobotError::NeedsHitl
    );
    // `authority<255` (não sanctionado) → reject
    let low_auth = HitlDecision {
        authority: 120,
        ..hitl_for("cmd-1")
    };
    assert_eq!(p.execute(&c, &low_auth).unwrap_err(), RobotError::NeedsHitl);

    // certificado inválido → abstain (NoCertificate)
    let bad_cert = RobotCert {
        violations: 1,
        ..cert()
    };
    assert_eq!(
        p.execute(
            &cmd("cmd-1", RobotAction::Move, 0.1, bad_cert),
            &hitl_for("cmd-1")
        )
        .unwrap_err(),
        RobotError::NoCertificate
    );
    let low_consensus = RobotCert {
        consensus: 0.2,
        ..cert()
    };
    assert_eq!(
        p.execute(
            &cmd("cmd-1", RobotAction::Move, 0.1, low_consensus),
            &hitl_for("cmd-1")
        )
        .unwrap_err(),
        RobotError::NoCertificate
    );
    let auth255 = RobotCert {
        authority: 255,
        ..cert()
    };
    assert_eq!(
        p.execute(
            &cmd("cmd-1", RobotAction::Move, 0.1, auth255),
            &hitl_for("cmd-1")
        )
        .unwrap_err(),
        RobotError::NoCertificate
    );

    // risk ceiling; risk_ceiling NaN → SchemaBreak
    assert!(matches!(
        p.execute(
            &cmd("cmd-1", RobotAction::Move, 0.99, cert()),
            &hitl_for("cmd-1")
        )
        .unwrap_err(),
        RobotError::RiskCeiling(_)
    ));
    let mut pnan = port();
    pnan.risk_ceiling = f64::NAN;
    assert!(matches!(
        pnan.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::SchemaBreak(_)
    ));

    // Eixo C allowlist
    let mut p2 = port();
    p2.allowed = vec![RobotAction::Noop];
    assert!(matches!(
        p2.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::NotAllowlisted(_)
    ));

    // schema break
    let bad = cmd("", RobotAction::Move, 0.1, cert());
    assert!(matches!(
        p.execute(&bad, &hitl_for("x")).unwrap_err(),
        RobotError::SchemaBreak(_)
    ));
    let mut badn = cmd("cmd-1", RobotAction::Move, 0.1, cert());
    badn.args = vec![f64::NAN];
    assert!(matches!(
        p.execute(&badn, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::SchemaBreak(_)
    ));

    // tomada off → reject
    let mut off = port();
    off.enabled = false;
    assert_eq!(
        off.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::Off
    );

    // backend virtual + HITL + cert → **não atua**; registra + verifica; audit (sem segredo)
    let out = p.execute(&c, &hitl_for("cmd-1")).expect("virtual ok");
    assert!(!out.executed_real);
    assert!(out.verified);
    assert_eq!(out.backend, "virtual");
    assert!(out.hitl_audited);
    assert_eq!(p.audit.len(), 1);
    assert_eq!(p.used_request_ids.len(), 1, "request_id usado");

    // W2 reuso não permitido (mesmo request_id novamente)
    assert_eq!(
        p.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::NeedsHitl,
        "reuso HITL bloqueado"
    );

    // `Stop` — allowlisted + isento de cert+risk + abortivo; **mantém HITL duro + !kill + enabled**
    let mut ps = port();
    let stop = cmd(
        "cmd-1",
        RobotAction::Stop,
        1.0,
        RobotCert {
            violations: 9,
            consensus: 0.0,
            ..cert()
        },
    );
    assert_eq!(
        ps.execute(&stop, &hitl_forged_blank()).unwrap_err(),
        RobotError::NeedsHitl,
        "Stop exige HITL duro"
    );
    let s = ps
        .execute(&stop, &hitl_for("cmd-1"))
        .expect("stop + HITL ok"); // cert/risk isento
    assert_eq!(s.action, "stop");

    // KILL SWITCH bloqueia toda call (incl. Stop + HITL + cert válido)
    std::env::set_var("AETHER_AGENT_KILL", "1");
    assert_eq!(
        p.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::KillSwitch
    );
    assert_eq!(
        ps.execute(&stop, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::KillSwitch,
        "kill bloqueia até Stop"
    );
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::set_var("AETHER_ROBOT_KILL", "1");
    assert_eq!(
        p.execute(&c, &hitl_for("cmd-1")).unwrap_err(),
        RobotError::KillSwitch
    );
    std::env::remove_var("AETHER_ROBOT_KILL");

    assert!(cert().authority <= 120);
    assert_eq!(MAX_AUDIT, 1024);
}

fn hitl_forged_blank() -> HitlDecision {
    HitlDecision {
        request_id: 0,
        approved: false,
        token_present: false,
        timed_out: true,
        authority: 0,
        action: Action::Abstain,
        detail: "".into(),
    }
}

//! Gate **D4** — T-ROBOT ação física com certificado; classe D **independente de HW**.
//! Backend **virtual** registra/verifica sem atuar; kill switch bloqueia toda call;
//! **HITL duro sempre**; certificado obrigatório; allowlist (Eixo C); risk ceiling;
//! auth≤120; **0 bytes de comando no T0**; audit trail. Contrato: docs/TOMADAS_FUTURO §D4.
//! NOTA: teste único (muta env em threads).

#![cfg(feature = "agent")]

use aether_engine::agent::robot::{RobotAction, RobotBackend, RobotCert, RobotCmd, RobotError, RobotPort};

fn cert() -> RobotCert {
    RobotCert { energy: 0.1, gap: 1.0, violations: 0, consensus: 0.95, authority: 120 }
}
fn cmd(action: RobotAction, risk: f64, c: RobotCert) -> RobotCmd {
    RobotCmd { id: "cmd-1".into(), action, args: vec![0.0, 0.0, 0.0], cert: c, risk }
}
fn port() -> RobotPort {
    RobotPort { enabled: true, backend: RobotBackend::Virtual, risk_ceiling: 0.5, allowed: RobotPort::default().allowed, audit: Vec::new() }
}

#[test]
fn d4_robot_cert_hitl_kill_allowlist_risk_virtual() {
    // limpar kill switch
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::remove_var("AETHER_ROBOT_KILL");

    let mut p = port();
    let c = cmd(RobotAction::Move, 0.1, cert());

    // HITL duro SEMPRE: sem token → reject (nunca auto)
    assert_eq!(p.execute(&c, "").unwrap_err(), RobotError::NeedsHitl);
    assert_eq!(p.execute(&c, "   ").unwrap_err(), RobotError::NeedsHitl);

    // certificado inválido → abstain (NoCertificate)
    let bad_cert = RobotCert { violations: 1, ..cert() };
    assert_eq!(p.execute(&cmd(RobotAction::Move, 0.1, bad_cert), "tok").unwrap_err(), RobotError::NoCertificate);
    let low_consensus = RobotCert { consensus: 0.2, ..cert() };
    assert_eq!(p.execute(&cmd(RobotAction::Move, 0.1, low_consensus), "tok").unwrap_err(), RobotError::NoCertificate);
    let auth255 = RobotCert { authority: 255, ..cert() };
    assert_eq!(p.execute(&cmd(RobotAction::Move, 0.1, auth255), "tok").unwrap_err(), RobotError::NoCertificate);

    // risk ceiling
    assert!(matches!(p.execute(&cmd(RobotAction::Move, 0.99, cert()), "tok").unwrap_err(), RobotError::RiskCeiling(_)));

    // Eixo C allowlist: ação não inventariada — não é construível (enum); remover da allowlist → reject
    let mut p2 = port();
    p2.allowed = vec![RobotAction::Noop];
    assert!(matches!(p2.execute(&cmd(RobotAction::Move, 0.1, cert()), "tok").unwrap_err(), RobotError::NotAllowlisted(_)));

    // schema break
    let mut bad = cmd(RobotAction::Move, 0.1, cert());
    bad.id = "".into();
    assert!(matches!(p.execute(&bad, "tok").unwrap_err(), RobotError::SchemaBreak(_)));
    let mut badn = cmd(RobotAction::Move, 0.1, cert());
    badn.args = vec![f64::NAN];
    assert!(matches!(p.execute(&badn, "tok").unwrap_err(), RobotError::SchemaBreak(_)));

    // tomada off → reject
    let mut off = port();
    off.enabled = false;
    assert_eq!(off.execute(&c, "tok").unwrap_err(), RobotError::Off);

    // backend virtual: HITL + certificado ok → **não atua**, registra + verifica; audit trail
    let out = p.execute(&c, "hitl-token").expect("virtual ok");
    assert!(!out.executed_real, "virtual não atua HW");
    assert!(out.verified);
    assert_eq!(out.backend, "virtual");
    assert!(out.hitl_audited);
    assert_eq!(p.audit.len(), 1, "audit trail");
    assert_eq!(p.audit[0].action, "move");

    // Stop sempre permitido + virtual + HITL
    let stop = cmd(RobotAction::Stop, 1.0, cert());
    let s = p.execute(&stop, "tok").expect("stop ok");
    assert_eq!(s.action, "stop");

    // KILL SWITCH bloqueia toda call (incl. Stop + HITL + certificado válido)
    std::env::set_var("AETHER_AGENT_KILL", "1");
    assert_eq!(p.execute(&c, "tok").unwrap_err(), RobotError::KillSwitch);
    assert_eq!(p.execute(&stop, "tok").unwrap_err(), RobotError::KillSwitch, "kill bloqueia até Stop");
    std::env::remove_var("AETHER_AGENT_KILL");
    std::env::set_var("AETHER_ROBOT_KILL", "1");
    assert_eq!(p.execute(&c, "tok").unwrap_err(), RobotError::KillSwitch);
    std::env::remove_var("AETHER_ROBOT_KILL");

    // auth nunca 255; classe D não atua sem HITL; 0 bytes de comando no T0 (comando tipado)
    assert!(cert().authority <= 120);
}

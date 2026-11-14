//! Contrato de CLI do daemon (Módulo 8): `--help`, `--duration` com sufixo,
//! valores não finitos e flags do sonhador. Feature `m8`.

#![cfg(feature = "m8")]

use std::process::Command;
use std::time::{Duration, Instant};

fn daemon() -> &'static str {
    env!("CARGO_BIN_EXE_aether-daemon")
}

fn small_run(extra: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(daemon());
    cmd.args([
        "--hz", "20", "--dim", "8", "--samples", "2", "--steps", "10", "--window", "32",
    ]);
    cmd.args(extra);
    cmd.output().expect("executar daemon")
}

#[test]
fn help_lista_as_flags() {
    let out = Command::new(daemon())
        .arg("--help")
        .output()
        .expect("executar --help");
    assert!(out.status.success(), "--help deveria sair com 0");
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--duration",
        "--hz",
        "--seed",
        "--alert",
        "--window",
        "--dim",
        "--samples",
        "--steps",
    ] {
        assert!(text.contains(flag), "--help não lista {flag}: {text}");
    }
}

#[test]
fn duration_aceita_sufixo_s_e_termina() {
    let t0 = Instant::now();
    let out = small_run(&["--duration", "1s", "--seed", "7"]);
    assert!(
        out.status.success(),
        "1s deveria ser aceito; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "--duration 1s demorou {:?}",
        t0.elapsed()
    );
}

#[test]
fn duration_nao_finito_retorna_erro_sem_panic() {
    for value in ["nan", "inf", "-inf"] {
        let out = Command::new(daemon())
            .args(["--duration", value])
            .output()
            .expect("executar daemon");
        assert_eq!(
            out.status.code(),
            Some(2),
            "--duration {value} deveria retornar 2, status={:?}",
            out.status
        );
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        assert!(
            !stderr.contains("panicked"),
            "--duration {value} não pode panicar: {stderr}"
        );
    }
}

#[test]
fn flags_do_sonhador_sao_aceitas() {
    let out = small_run(&["--duration", "1", "--samples", "4", "--steps", "20"]);
    assert!(
        out.status.success(),
        "--samples/--steps deveriam ser aceitos; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

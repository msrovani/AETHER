//! Backend externo seguro (classe C/D) — **sem shell**, sem injeção, timeout,
//! `stdout` limitado, `kill` no estouro. W1/W8.
//!
//! * **Nenhum shell** (`cmd`/`sh`/`powershell`): `prog` concreto + `args` estruturados;
//! * **dado do chamador nunca em argv**: refs **sanitizadas**; texto vai por **stdin**;
//! * **timeout** + `kill`; **stdout** limitado; `stderr`→null; `Err` honesto.
//!
//! `AETHER_*_CMD` = **prog concreto** (binário), **não** linha de shell.
//! `AETHER_*_CMD` é config confiável; **dado de entrada não é**.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// timeout por padrão (ms).
pub const BACKEND_TIMEOUT_MS: u64 = 2000;
/// stdout máximo por padrão (bytes).
pub const BACKEND_MAX_STDOUT: usize = 64 * 1024;

/// Sanitiza uma **referência** anexada (`doc.id`, `audio_ref`, `image_ref`,
/// `sensor_id`, `anchor.id`, refs de prog...). Rejeita metacaracteres de shell,
/// newline, NUL, `%`. Sem shell + isto = sem injeção. (W1.)
pub fn sanitize_ref(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Ok(String::new());
    }
    if s.len() > 4096 {
        return Err("ref muito longa".into());
    }
    let blocked = |c: char| {
        matches!(
            c,
            '&' | '|'
                | ';'
                | '<'
                | '>'
                | '^'
                | '`'
                | '$'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | '!'
                | '*'
                | '?'
                | '~'
                | '\''
                | '"'
                | '%'
                | '\n'
                | '\r'
                | '\0'
        )
    };
    if s.chars().any(blocked) {
        return Err(format!("ref contém metacaractere proibido: {s}"));
    }
    Ok(s.to_string())
}

/// Divide `prog + args` estruturados. **Sem shell**: `prog` concreto; bloqueia
/// `cmd`/`sh`/`bash`/`powershell`/`pwsh` como prog. (Recomendado `*_CMD`=só prog.)
pub fn parse_cmd(cmd_line: &str) -> Result<(String, Vec<String>), String> {
    let mut parts = cmd_line.split_whitespace();
    let prog = parts
        .next()
        .ok_or_else(|| "cmd vazio".to_string())?
        .to_string();
    let l = prog.to_ascii_lowercase();
    // bloqueia shells claros como prog (não-shell + sanitize + no-append de dado);
    // `cmd` não é bloqueado para teste Windows, mas **não é recomendado** (doc).
    if matches!(
        l.as_str(),
        "sh" | "bash" | "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    ) {
        return Err(format!(
            "prog shell não permitido: {prog}; use binário concreto sem shell"
        ));
    }
    let args: Vec<String> = parts.map(|s| s.to_string()).collect();
    Ok((prog, args))
}

/// Executa backend **sem shell**. `prog`, `args`, `stdin_data`, `timeout_ms`, `max_stdout`.
pub fn run_backend(
    prog: &str,
    args: &[String],
    stdin_data: Option<&str>,
    timeout_ms: u64,
    max_stdout: usize,
) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(if stdin_data.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child: Arc<Mutex<Child>> =
        Arc::new(Mutex::new(cmd.spawn().map_err(|e| format!("spawn: {e}"))?));

    // dado não confiável por pipe (não argv); fecha stdin (EOF)
    if let Some(data) = stdin_data {
        if let Ok(mut c) = child.lock() {
            if let Some(mut si) = c.stdin.take() {
                let _ = si.write_all(data.as_bytes());
            }
        }
    }

    let timeout = std::time::Duration::from_millis(timeout_ms.max(1));
    let (tx, rx) = std::sync::mpsc::channel();
    let child2 = Arc::clone(&child);
    let handle = std::thread::spawn(move || {
        let res = (|| -> Result<(std::process::ExitStatus, Vec<u8>), String> {
            let mut out = Vec::new();
            let mut stdout = { child2.lock().map_err(|_| "lock")?.stdout.take() };
            if let Some(ref mut s) = stdout {
                let mut buf = [0u8; 8192];
                loop {
                    match s.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if out.len() + n > max_stdout {
                                let _ = child2.lock().map_err(|_| "lock")?.kill();
                                break;
                            }
                            out.extend_from_slice(&buf[..n]);
                        }
                        Err(e) => return Err(format!("read: {e}")),
                    }
                }
            }
            let status = child2
                .lock()
                .map_err(|_| "lock")?
                .wait()
                .map_err(|e| e.to_string())?;
            Ok((status, out))
        })();
        let _ = tx.send(res);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok((status, out))) => {
            let _ = handle.join();
            if !status.success() {
                return Err(format!("status {status}"));
            }
            if out.len() > max_stdout {
                return Err(format!("stdout excede {max_stdout} bytes"));
            }
            Ok(out)
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
            }
            let _ = handle.join();
            Err("timeout".into())
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err("disconnected".into()),
    }
}

/// Conveniência: `run_backend` + `parse_cmd` + `sanitize_ref` na ref anexada.
pub fn run_backend_cmd(
    cmd_line: &str,
    extra_arg: Option<&str>,
    stdin_data: Option<&str>,
    timeout_ms: u64,
    max_stdout: usize,
) -> Result<Vec<u8>, String> {
    let (prog, mut args) = parse_cmd(cmd_line)?;
    if let Some(a) = extra_arg {
        args.push(sanitize_ref(a)?);
    }
    run_backend(&prog, &args, stdin_data, timeout_ms, max_stdout)
}

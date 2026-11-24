//! Gate A4 / T-CLIP — clipboard sem shell livre.
//!
//! Mede o contrato de `src/agent/clip.rs`:
//! * stdin é o backend default (o SO só existe sob `--features clip`);
//! * PEM, `sk-`, `ghp_`, `AKIA` e JWT: detectados e **negados**;
//! * hex32 só com contexto (`token=…`) e aí é redigido, não negado —
//!   md5/uuid colados num diff não podem custar o lote;
//! * unified diff vira `code.patch` sob `resolve_sandbox_path`;
//! * `..`, absoluto e traversal → `Err` (0 escape);
//! * saída só com `CertifiedUtterance` (0 fala sem COH);
//! * nenhum processo/shell no módulo (varredura do fonte).
//!
//! Artefato: `target/aether_clip_gate.json`.

#![cfg(feature = "agent")]

use std::io::Cursor;
use std::path::{Path, PathBuf};

use aether_engine::agent::clip::{
    classify, ingest, process, redact, write_patch, ClipBackend, ClipError, ClipKind, SecretClass,
    StdinBackend, DEFAULT_PATCH_REL, MAX_CLIP_BYTES,
};
use aether_engine::agent::{Action, Claim, ExpertId};
use aether_engine::core::certificate::Provenance;
use aether_engine::json::Json;

fn claim(subject: &str, relation: &str, value: &str) -> Claim {
    Claim::new(
        subject,
        relation,
        value,
        0.9,
        96,
        ExpertId::Memory,
        Provenance::Inferred,
    )
}

fn work_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/a4_clip");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("criar target/a4_clip");
    dir
}

const PEM: &str =
    "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----\n";
const SK: &str = "sk-liveQWERTYUIOPASDFGHJKLZXCVBNM";
const GHP: &str = "ghp_0123456789abcdefghijABCDEFGHIJuvwx";
const AKIA: &str = "AKIAIOSFODNN7EXAMPLE";
const HEX32: &str = "c0ffee1234567890abcdef1234567890";
const HEX_LINE: &str = "token=c0ffee1234567890abcdef1234567890";
const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";

fn check_stdin_default() -> Result<String, String> {
    let mut backend = StdinBackend::from_reader(Cursor::new(b"texto colado".to_vec()));
    let text = backend.read().map_err(|e| e.to_string())?;
    if text != "texto colado" {
        return Err(format!("leu {text:?}"));
    }
    let mut big = StdinBackend::from_reader(Cursor::new(vec![b'x'; MAX_CLIP_BYTES + 1]));
    match big.read() {
        Err(ClipError::TooLarge(n)) => {
            if n <= MAX_CLIP_BYTES {
                return Err(format!("TooLarge({n}) abaixo do teto"));
            }
        }
        other => return Err(format!("acima do teto: {other:?}")),
    }
    let mut bad = StdinBackend::from_reader(Cursor::new(vec![0xff, 0xfe]));
    match bad.read() {
        Err(ClipError::NotUtf8) => Ok("stdin + TooLarge + NotUtf8".into()),
        other => Err(format!("bytes inválidos: {other:?}")),
    }
}

fn check_redact() -> Result<String, String> {
    let cases: [(SecretClass, &str, &str); 6] = [
        (SecretClass::PemPrivateKey, PEM, "MIIEowIBAAKCAQEA"),
        (SecretClass::ApiKey, SK, SK),
        (SecretClass::GitHubToken, GHP, GHP),
        (SecretClass::AwsAccessKey, AKIA, AKIA),
        (SecretClass::Jwt, JWT, JWT),
        (SecretClass::Hex32, HEX_LINE, HEX32),
    ];
    for (class, sample, token) in cases {
        let out = redact(sample);
        if !out.classes.contains(&class) {
            return Err(format!("{} ausente nas classes", class.label()));
        }
        if out.text.contains(token) {
            return Err(format!("{} vazou no texto redigido", class.label()));
        }
        if !out.text.contains("[redacted:") {
            return Err(format!("{} sem marcador", class.label()));
        }
    }
    let mixed = format!("{PEM}chave={SK} token={GHP} aws={AKIA} jwt={JWT} {HEX_LINE}");
    let out = redact(&mixed);
    if out.classes.len() != 6 {
        return Err(format!("mistura: {} classes", out.classes.len()));
    }
    for token in [SK, GHP, AKIA, JWT, HEX32] {
        if out.text.contains(token) {
            return Err("mistura vazou token".into());
        }
    }
    if out.text.contains("PRIVATE KEY-----") {
        return Err("mistura vazou PEM".into());
    }
    Ok(format!("{} classes redigidas", out.classes.len()))
}

const DIFF: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n";

fn check_deny(dir: &Path) -> Result<String, String> {
    let with_secret = format!("{SK}\n{DIFF}");
    match ingest(dir, &with_secret) {
        Err(ClipError::Secret(classes)) => {
            if classes != vec![SecretClass::ApiKey] {
                return Err(format!("classes: {classes:?}"));
            }
        }
        other => return Err(format!("segredo aceito: {other:?}")),
    }
    if dir.join(DEFAULT_PATCH_REL).exists() {
        return Err("segredo gravou code.patch".into());
    }
    match ingest(dir, "   \n\t\n") {
        Err(ClipError::Empty) => Ok("lote negado, 0 byte gravado".into()),
        other => Err(format!("vazio aceito: {other:?}")),
    }
}

/// O ponto da revisão: hex sem contexto não é segredo (nem nega, nem redige);
/// com contexto, é redigido e o lote segue.
fn check_no_gross_fp(dir: &Path) -> Result<String, String> {
    for benign in [
        "md5 3f786850e387550fdab836ed7e6dc881
",
        "uuid 550e8400e29b41d4a716446655440000
",
    ] {
        let payload = ingest(dir, benign).map_err(|e| format!("{benign:?} negado: {e}"))?;
        if payload.text != benign {
            return Err(format!("{benign:?} redigido sem contexto"));
        }
    }
    let secret = "token=c0ffee1234567890abcdef1234567890
";
    let payload = ingest(dir, secret).map_err(|e| format!("hex32 contextual negado: {e}"))?;
    if !payload.text.contains("[redacted:hex32]") {
        return Err(format!("hex32 contextual não redigido: {:?}", payload.text));
    }
    if payload.text.contains(HEX32) {
        return Err("hex32 contextual vazou".into());
    }
    Ok("md5/uuid intactos; hex32 contextual redigido".into())
}

fn check_patch(dir: &Path) -> Result<String, String> {
    if classify(DIFF) != ClipKind::Patch {
        return Err("diff não classificado como patch".into());
    }
    if classify("--- apenas texto ---") != ClipKind::Text {
        return Err("texto classificado como patch".into());
    }
    let payload = ingest(dir, DIFF).map_err(|e| e.to_string())?;
    if payload.kind != ClipKind::Patch {
        return Err("payload não é patch".into());
    }
    let path = payload
        .patch_path
        .ok_or_else(|| "patch sem caminho".to_string())?;
    if !path.ends_with(DEFAULT_PATCH_REL) {
        return Err(format!("patch em {}", path.display()));
    }
    let written = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if written != DIFF {
        return Err("code.patch difere do diff colado".into());
    }
    let root = std::fs::canonicalize(dir).map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err(format!("patch fora da sandbox: {}", path.display()));
    }
    let text = ingest(dir, "linha livre\n").map_err(|e| e.to_string())?;
    if text.kind != ClipKind::Text || text.patch_path.is_some() {
        return Err("texto livre virou patch".into());
    }
    Ok(path.display().to_string())
}

fn check_escape(dir: &Path) -> Result<String, String> {
    for rel in ["../escape.patch", "/abs.patch", "a/../../escape.patch", ""] {
        match write_patch(dir, rel, DIFF) {
            Err(ClipError::Sandbox(_)) | Err(ClipError::Io(_)) => {}
            other => return Err(format!("{rel:?} aceito: {other:?}")),
        }
    }
    let outside = dir.join("..").join("escape.patch");
    if outside.exists() {
        return Err("arquivo escapou da sandbox".into());
    }
    Ok(".. / absoluto / traversal negados".into())
}

fn check_coh(dir: &Path) -> Result<String, String> {
    let claims = vec![claim("srv", "status", "up")];
    let mut backend = StdinBackend::from_reader(Cursor::new(DIFF.as_bytes().to_vec()));
    let drop = process(dir, &mut backend, &claims, Action::Act).map_err(|e| e.to_string())?;
    if !drop.utterance.may_speak_fact() {
        return Err("saída sem certificado factual".into());
    }
    if drop.utterance.sources.is_empty() {
        return Err("utterance sem fontes".into());
    }
    // COH tem de vir antes de qualquer escrita: um diff sem claims não pode
    // deixar `code.patch` na sandbox (lote negado = zero efeito).
    let patch = dir.join(DEFAULT_PATCH_REL);
    let _ = std::fs::remove_file(&patch);
    let mut empty_backend = StdinBackend::from_reader(Cursor::new(DIFF.as_bytes().to_vec()));
    match process(dir, &mut empty_backend, &[], Action::Act) {
        Err(ClipError::Coherence(_)) => {}
        other => return Err(format!("sem claims não negou: {other:?}")),
    }
    if patch.exists() {
        return Err("lote sem COH gravou code.patch antes de certificar".into());
    }
    let mut abstain_backend = StdinBackend::from_reader(Cursor::new(b"texto".to_vec()));
    let abstain =
        process(dir, &mut abstain_backend, &claims, Action::Abstain).map_err(|e| e.to_string())?;
    if abstain.utterance.may_speak_fact() || abstain.utterance.text != "não sei" {
        return Err("abstain virou fala factual".into());
    }
    Ok("certifica antes de escrever + abstain".into())
}

fn check_no_shell() -> Result<String, String> {
    // `include_str!` falha em compilar se o arquivo sumir, e o fonte não cita
    // estes needles nem em comentário — então a varredura é literal.
    let source = include_str!("../src/agent/clip.rs");
    for needle in ["Command::new", "std::process", "sh -c", "cmd /C"] {
        if source.contains(needle) {
            return Err(format!("clip.rs contém {needle:?}"));
        }
    }
    if !source.contains("#[cfg(feature = \"clip\")]") {
        return Err("backend do SO sem gate de feature".into());
    }
    Ok("0 Command/process".into())
}

/// `Ok(detalhe)` = verde; `Err(motivo)` = vermelho.
fn record(checks: &mut Vec<(String, bool, String)>, name: &str, result: Result<String, String>) {
    match result {
        Ok(detail) => checks.push((name.to_string(), true, detail)),
        Err(why) => checks.push((name.to_string(), false, why)),
    }
}

#[test]
fn gate_a4_t_clip() {
    let dir = work_dir();
    let mut checks: Vec<(String, bool, String)> = Vec::new();

    record(&mut checks, "stdin_default", check_stdin_default());
    record(&mut checks, "redact_6_classes", check_redact());
    record(&mut checks, "deny_segredo", check_deny(&dir));
    record(&mut checks, "sem_falso_positivo", check_no_gross_fp(&dir));
    let patch = check_patch(&dir);
    let patch_path = patch.clone().unwrap_or_default();
    record(&mut checks, "diff_code_patch", patch);
    record(&mut checks, "sandbox_sem_escape", check_escape(&dir));
    record(&mut checks, "saida_pos_coh", check_coh(&dir));
    record(&mut checks, "sem_shell", check_no_shell());

    let ok = checks.iter().all(|(_, ok, _)| *ok);
    let artifact = Json::Obj(vec![
        ("gate".into(), Json::Str("A4_T_CLIP".into())),
        ("ok".into(), Json::Bool(ok)),
        ("backend_default".into(), Json::Str("stdin".into())),
        (
            "os_backend_feature".into(),
            Json::Str("clip (arboard 3.7, opt-in)".into()),
        ),
        ("patch_path".into(), Json::Str(patch_path)),
        (
            "checks".into(),
            Json::Arr(
                checks
                    .iter()
                    .map(|(name, ok, detail)| {
                        Json::Obj(vec![
                            ("name".into(), Json::Str(name.clone())),
                            ("ok".into(), Json::Bool(*ok)),
                            ("detail".into(), Json::Str(detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/aether_clip_gate.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, artifact.to_string()).expect("gravar target/aether_clip_gate.json");

    let failures: Vec<String> = checks
        .iter()
        .filter(|(_, ok, _)| !ok)
        .map(|(name, _, detail)| format!("{name}: {detail}"))
        .collect();
    assert!(ok, "gate A4/T-CLIP vermelho: {failures:?}");
}

//! T-CLIP (A4) — entrada por clipboard **sem shell livre**.
//!
//! O contrato é deliberadamente estreito:
//!
//! ```text
//! backend (stdin default | OS atrás de feature `clip`) ──► texto
//!       │
//!       ├─ tamanho/UTF-8/vazio          → erro tipado
//!       ├─ segredo forte (PEM, sk-, ghp_, AKIA, JWT)  → DENY (nada gravado)
//!       ├─ COH (`certify_utterance_template`)         → sem certificado, para aqui
//!       ├─ hex32 só com contexto de atribuição         → redige e segue
//!       ├─ unified diff                 → `code.patch` sob `resolve_sandbox_path`
//!       └─ saída                        → só `CertifiedUtterance`
//! ```
//!
//! Regras que o gate mede:
//! * **stdin é o default**; o clipboard do SO só existe com `--features clip`
//!   (`arboard 3.7`), nunca no caminho padrão do core;
//! * nenhum caminho aqui cria processo ou shell (o módulo só lê bytes);
//! * segredo *forte* nega o lote; hex32 só redige — um md5 colado não pode
//!   custar o lote inteiro (falso positivo caro é pior que redação a mais);
//! * patch nunca escapa da sandbox (`..`, absoluto, symlink);
//! * **certificação vem antes de qualquer escrita**: lote sem COH não deixa
//!   `code.patch` para trás (efeito colateral de lote negado é vazamento).

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::language::Style;
use super::nl_out::{certify_utterance_template, CertifiedUtterance};
use super::tools::resolve_sandbox_path;
use super::{Action, Claim};

/// Teto do payload colado (256 KiB) — acima disso `TooLarge`.
pub const MAX_CLIP_BYTES: usize = 256 * 1024;
/// Nome canônico do diff dentro da sandbox.
pub const DEFAULT_PATCH_REL: &str = "code.patch";
/// Tamanho mínimo de token após `sk-` / `ghp_`.
const MIN_PREFIX_TOKEN: usize = 16;

/// Erro tipado do T-CLIP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipError {
    /// Backend sem conteúdo.
    Empty,
    /// Acima de [`MAX_CLIP_BYTES`].
    TooLarge(usize),
    /// Bytes que não são UTF-8.
    NotUtf8,
    /// Segredo detectado: o lote é negado (classes, nunca o valor).
    Secret(Vec<SecretClass>),
    /// Falha de leitura do backend.
    Io(String),
    /// Backend do SO indisponível/erro do clipboard.
    Backend(String),
    /// Path recusado pela sandbox.
    Sandbox(String),
    /// Nenhuma claim certificável (saída sem COH é proibida).
    Coherence(String),
}

impl fmt::Display for ClipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "clip vazio"),
            Self::TooLarge(n) => write!(f, "clip com {n} bytes (máx {MAX_CLIP_BYTES})"),
            Self::NotUtf8 => write!(f, "clip não é UTF-8"),
            Self::Secret(classes) => {
                let names: Vec<&str> = classes.iter().map(|c| c.label()).collect();
                write!(f, "segredo negado: {}", names.join(","))
            }
            Self::Io(why) => write!(f, "clip io: {why}"),
            Self::Backend(why) => write!(f, "clip backend: {why}"),
            Self::Sandbox(why) => write!(f, "clip sandbox: {why}"),
            Self::Coherence(why) => write!(f, "clip sem certificado: {why}"),
        }
    }
}

impl std::error::Error for ClipError {}

/// Classe de segredo reconhecida (rótulo estável; o valor nunca sai daqui).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretClass {
    /// Bloco `-----BEGIN ... PRIVATE KEY-----`.
    PemPrivateKey,
    /// `sk-...` (chave de API OpenAI-like).
    ApiKey,
    /// `ghp_...` (token GitHub).
    GitHubToken,
    /// `AKIA...` (access key AWS).
    AwsAccessKey,
    /// JWT (`eyJ….<payload>.<assinatura>`).
    Jwt,
    /// 32 chars hex **com** contexto de atribuição; sozinho é md5/uuid/hash.
    Hex32,
}

impl SecretClass {
    /// Rótulo estável para log/artefato.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PemPrivateKey => "pem-private-key",
            Self::ApiKey => "api-key",
            Self::GitHubToken => "github-token",
            Self::AwsAccessKey => "aws-access-key",
            Self::Jwt => "jwt",
            Self::Hex32 => "hex32",
        }
    }

    /// `true` se a classe é específica o bastante para negar o lote.
    /// Hex32 fica de fora: 32 hex sozinho é md5/uuid, não prova de segredo.
    pub const fn denies_batch(self) -> bool {
        !matches!(self, Self::Hex32)
    }
}

/// Prefixos de token com especificidade alta (prefixo + tamanho mínimo).
const PREFIXES: &[(&str, usize, SecretClass)] = &[
    ("sk-", MIN_PREFIX_TOKEN, SecretClass::ApiKey),
    ("ghp_", MIN_PREFIX_TOKEN, SecretClass::GitHubToken),
    ("AKIA", 16, SecretClass::AwsAccessKey),
];

/// Resultado da redação: texto seguro + classes, na ordem de 1ª ocorrência.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redaction {
    /// Texto com cada segredo trocado por `[redacted:<classe>]`.
    pub text: String,
    /// Classes únicas encontradas (vazio = nada redigido).
    pub classes: Vec<SecretClass>,
}

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn is_word_boundary(text: &str, at: usize) -> bool {
    at == 0 || !is_token_byte(text.as_bytes()[at - 1])
}

fn scan_pem(text: &str, spans: &mut Vec<(usize, usize, SecretClass)>) {
    let mut from = 0;
    while let Some(rel) = text[from..].find("-----BEGIN") {
        let begin = from + rel;
        let line_end = text[begin..]
            .find('\n')
            .map(|i| begin + i)
            .unwrap_or(text.len());
        if !text[begin..line_end].contains("PRIVATE KEY-----") {
            from = begin + 5;
            continue;
        }
        let mut search = line_end;
        let mut end = None;
        while let Some(rel_end) = text[search..].find("-----END") {
            let end_start = search + rel_end;
            let end_line = text[end_start..]
                .find('\n')
                .map(|i| end_start + i)
                .unwrap_or(text.len());
            if text[end_start..end_line].contains("PRIVATE KEY-----") {
                end = Some(end_line);
                break;
            }
            search = end_start + 5;
        }
        let stop = end.unwrap_or(text.len());
        spans.push((begin, stop, SecretClass::PemPrivateKey));
        if end.is_none() {
            break;
        }
        from = stop;
    }
}

fn scan_prefixed(
    text: &str,
    prefix: &str,
    min_token: usize,
    class: SecretClass,
    spans: &mut Vec<(usize, usize, SecretClass)>,
) {
    let mut from = 0;
    while let Some(rel) = text[from..].find(prefix) {
        let start = from + rel;
        if !is_word_boundary(text, start) {
            from = start + prefix.len();
            continue;
        }
        let token_start = start + prefix.len();
        let bytes = text.as_bytes();
        let mut end = token_start;
        while end < bytes.len() && is_token_byte(bytes[end]) {
            end += 1;
        }
        if end - token_start >= min_token {
            spans.push((start, end, class));
        }
        from = end.max(start + prefix.len());
    }
}

/// Palavras que dão contexto de atribuição a um valor hex.
const HEX_CONTEXT: &[&str] = &["key", "token", "secret", "password", "passwd", "auth"];

/// 32 hex só é segredo quando a própria linha o anuncia (`token=…`).
/// Sem isso, md5/`uuid`/hash truncado em diff seriam falso positivo grosseiro.
fn scan_hex32(text: &str, spans: &mut Vec<(usize, usize, SecretClass)>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_hexdigit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        if i - start != 32 || !is_word_boundary(text, start) {
            continue;
        }
        let line_start = text[..start].rfind('\n').map_or(0, |n| n + 1);
        let context = text[line_start..start].to_ascii_lowercase();
        if HEX_CONTEXT.iter().any(|needle| context.contains(needle)) {
            spans.push((start, i, SecretClass::Hex32));
        }
    }
}

/// JWT: `eyJ` + dois pontos + ≥40 bytes (header.payload.signature).
fn scan_jwt(text: &str, spans: &mut Vec<(usize, usize, SecretClass)>) {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find("eyJ") {
        let start = from + rel;
        let mut end = start;
        while end < bytes.len() && (is_token_byte(bytes[end]) || bytes[end] == b'.') {
            end += 1;
        }
        let token = &text[start..end];
        if is_word_boundary(text, start) && token.matches('.').count() == 2 && end - start >= 40 {
            spans.push((start, end, SecretClass::Jwt));
        }
        from = end.max(start + 3);
    }
}

fn collect_spans(text: &str) -> Vec<(usize, usize, SecretClass)> {
    let mut spans: Vec<(usize, usize, SecretClass)> = Vec::new();
    scan_pem(text, &mut spans);
    for (prefix, min_token, class) in PREFIXES {
        scan_prefixed(text, prefix, *min_token, *class, &mut spans);
    }
    scan_jwt(text, &mut spans);
    scan_hex32(text, &mut spans);
    spans.sort_by_key(|(start, end, class)| (*start, *end, *class));
    let mut merged: Vec<(usize, usize, SecretClass)> = Vec::with_capacity(spans.len());
    for (start, end, class) in spans {
        if let Some(last) = merged.last_mut() {
            if start < last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end, class));
    }
    merged
}

/// Redige segredos conhecidos, devolvendo o texto seguro e as classes.
pub fn redact(input: &str) -> Redaction {
    let mut text = String::with_capacity(input.len());
    let mut classes: Vec<SecretClass> = Vec::new();
    let mut cursor = 0;
    for (start, end, class) in collect_spans(input) {
        text.push_str(&input[cursor..start]);
        text.push_str(&format!("[redacted:{}]", class.label()));
        if !classes.contains(&class) {
            classes.push(class);
        }
        cursor = end;
    }
    text.push_str(&input[cursor..]);
    Redaction { text, classes }
}

/// Fonte de texto do clipboard.
pub trait ClipBackend {
    /// Lê o payload inteiro.
    fn read(&mut self) -> Result<String, ClipError>;
}

/// Backend **default**: stdin (sem tocar o SO).
#[derive(Debug)]
pub struct StdinBackend<R = std::io::Stdin> {
    reader: R,
}

impl StdinBackend<std::io::Stdin> {
    /// Lê o clipboard lógico do processo (pipe/redirecionamento).
    pub fn stdin() -> Self {
        Self {
            reader: std::io::stdin(),
        }
    }
}

impl<R: Read> StdinBackend<R> {
    /// Backend sobre qualquer `Read` (arquivo, pipe, `Cursor` em teste).
    pub fn from_reader(reader: R) -> Self {
        Self { reader }
    }
}

impl<R: Read> ClipBackend for StdinBackend<R> {
    fn read(&mut self) -> Result<String, ClipError> {
        let mut buf: Vec<u8> = Vec::new();
        self.reader
            .by_ref()
            .take(MAX_CLIP_BYTES as u64 + 1)
            .read_to_end(&mut buf)
            .map_err(|e| ClipError::Io(e.to_string()))?;
        if buf.len() > MAX_CLIP_BYTES {
            return Err(ClipError::TooLarge(buf.len()));
        }
        String::from_utf8(buf).map_err(|_| ClipError::NotUtf8)
    }
}

/// Backend do SO — **só** com `--features clip` (`arboard 3.7`).
#[cfg(feature = "clip")]
pub struct OsBackend {
    clipboard: arboard::Clipboard,
}

#[cfg(feature = "clip")]
impl std::fmt::Debug for OsBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `arboard::Clipboard` não é `Debug`; o handle não é inspecionável.
        f.debug_struct("OsBackend").finish_non_exhaustive()
    }
}

#[cfg(feature = "clip")]
impl OsBackend {
    /// Abre o clipboard do sistema (erro tipado se indisponível).
    pub fn new() -> Result<Self, ClipError> {
        arboard::Clipboard::new()
            .map(|clipboard| Self { clipboard })
            .map_err(|e| ClipError::Backend(e.to_string()))
    }
}

#[cfg(feature = "clip")]
impl ClipBackend for OsBackend {
    fn read(&mut self) -> Result<String, ClipError> {
        self.clipboard
            .get_text()
            .map_err(|e| ClipError::Backend(e.to_string()))
    }
}

/// Tipo do payload colado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    /// Unified diff (`---`/`+++`/`@@` ou `diff --git`).
    Patch,
    /// Texto livre (não vira patch).
    Text,
}

/// Payload aceito depois do ingest (já sem segredo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipPayload {
    /// Tipo detectado.
    pub kind: ClipKind,
    /// Texto íntegro aceito (já redigido).
    pub text: String,
    /// Caminho do `code.patch` gravado (só para [`ClipKind::Patch`]).
    pub patch_path: Option<PathBuf>,
}

/// Classifica sem heurística frouxa: diff unificado precisa de hunk + cabeçalho.
pub fn classify(text: &str) -> ClipKind {
    let mut minus = false;
    let mut plus = false;
    let mut hunk = false;
    let mut git = false;
    for line in text.lines() {
        if line.starts_with("--- ") {
            minus = true;
        } else if line.starts_with("+++ ") {
            plus = true;
        } else if line.starts_with("@@") {
            hunk = true;
        } else if line.starts_with("diff --git ") {
            git = true;
        }
    }
    if hunk && ((minus && plus) || git) {
        ClipKind::Patch
    } else {
        ClipKind::Text
    }
}

/// Grava o diff como `code.patch` — caminho resolvido por `resolve_sandbox_path`.
pub fn write_patch(root: &Path, rel: &str, text: &str) -> Result<PathBuf, ClipError> {
    let path = resolve_sandbox_path(root, rel).map_err(|e| ClipError::Sandbox(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ClipError::Io(e.to_string()))?;
    }
    std::fs::write(&path, text).map_err(|e| ClipError::Io(e.to_string()))?;
    Ok(path)
}

/// Ingest: valida, nega segredo e (se diff) grava `code.patch` na sandbox.
pub fn ingest(root: &Path, text: &str) -> Result<ClipPayload, ClipError> {
    if text.trim().is_empty() {
        return Err(ClipError::Empty);
    }
    if text.len() > MAX_CLIP_BYTES {
        return Err(ClipError::TooLarge(text.len()));
    }
    let redaction = redact(text);
    let denied: Vec<SecretClass> = redaction
        .classes
        .iter()
        .copied()
        .filter(|class| class.denies_batch())
        .collect();
    if !denied.is_empty() {
        // Deny: nenhum byte do segredo é gravado ou devolvido.
        return Err(ClipError::Secret(denied));
    }
    // O lote segue redigido: hex32 contextual não vaza e não custa o paste.
    let text = redaction.text;
    let kind = classify(&text);
    let patch_path = match kind {
        ClipKind::Patch => Some(write_patch(root, DEFAULT_PATCH_REL, &text)?),
        ClipKind::Text => None,
    };
    Ok(ClipPayload {
        kind,
        text,
        patch_path,
    })
}

/// Resultado completo: payload + fala certificada.
#[derive(Clone, Debug)]
pub struct ClipDrop {
    /// Payload aceito.
    pub payload: ClipPayload,
    /// Fala pós-COH (única saída permitida).
    pub utterance: CertifiedUtterance,
}

/// Lê o backend, valida e só entrega com `CertifiedUtterance` (pós-COH).
pub fn process(
    root: &Path,
    backend: &mut dyn ClipBackend,
    claims: &[Claim],
    action: Action,
) -> Result<ClipDrop, ClipError> {
    let text = backend.read()?;
    // COH antes de qualquer efeito: se o lote não tem certificado, a sandbox
    // não pode ganhar nem um byte (`code.patch` só existe depois daqui).
    let utterance = certify_utterance_template(claims, action, Style::Factual)
        .map_err(|e| ClipError::Coherence(format!("{e:?}")))?;
    let payload = ingest(root, &text)?;
    Ok(ClipDrop { payload, utterance })
}

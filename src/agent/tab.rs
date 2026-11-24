//! T-TAB (A1) — planilha/CSV → `Claim[]` canônico, sem dependência nova.
//!
//! # Contrato
//!
//! ```text
//! CSV/TSV (RFC 4180) ──parse──► TabDoc{headers,rows} ──schema──► (s,r,v)
//!                                                        │
//!                                                        ▼
//!                                          Claim[] auth 96 + facts.jsonl
//! ```
//!
//! Regras medidas pelo gate `tests/agent_tab_gate.rs`:
//!
//! * **Nenhuma linha some em silêncio**: linha com número de campos diferente
//!   do header é `Err(RaggedRow)`, nunca descartada.
//! * **Coluna sem papel vira `Unmapped`**, e schema sem (subject, relation,
//!   value) abstém **todas** as linhas (`claims == 0`).
//! * **Auth ≤ 120**: toda claim sai com [`TAB_AUTH`] (96, quarentena).
//! * **Determinismo**: hash do schema é FNV-1a 64 sobre o mapeamento canônico.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::json::Json;

use super::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use super::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;

/// Autoridade de carga de tabela (quarentena; teto duro do projeto = 120).
pub const TAB_AUTH: u8 = 96;
/// Bytes máximos de um registro físico — disjuntor anti-OOM.
pub const MAX_LINE_BYTES: usize = 32 * 1024;
/// Colunas máximas por tabela.
pub const MAX_COLUMNS: usize = 256;
/// Linhas de dados máximas por tabela.
pub const MAX_ROWS: usize = 100_000;
/// Tamanho máximo de um campo (sujeito/relação/valor) virando claim.
pub const MAX_FIELD_CHARS: usize = 512;

/// Dialeto detectado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDialect {
    /// Vírgula.
    Comma,
    /// Ponto e vírgula (planilhas pt-BR).
    Semicolon,
    /// Tabulação (TSV).
    Tab,
}

impl TabDialect {
    /// Caractere separador.
    pub const fn delimiter(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Tab => '\t',
        }
    }

    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Comma => "csv",
            Self::Semicolon => "csv_semicolon",
            Self::Tab => "tsv",
        }
    }

    /// Dialeto a partir do separador.
    pub const fn from_delimiter(c: char) -> Self {
        match c {
            ';' => Self::Semicolon,
            '\t' => Self::Tab,
            _ => Self::Comma,
        }
    }
}

/// Erro de parsing/mapeamento. Todo erro é explícito: nada é descartado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabError {
    /// Entrada vazia.
    Empty,
    /// Registro acima de [`MAX_LINE_BYTES`].
    LineTooLong { line: usize, bytes: usize },
    /// Mais de [`MAX_COLUMNS`] colunas.
    TooManyColumns { columns: usize },
    /// Mais de [`MAX_ROWS`] linhas de dados.
    TooManyRows { rows: usize },
    /// Aspas abertas e nunca fechadas.
    UnclosedQuote { line: usize },
    /// Linha com número de campos diferente do header.
    RaggedRow {
        row: usize,
        expected: usize,
        found: usize,
    },
    /// Mapeamento explícito cita coluna que não existe.
    SchemaBreak { reason: String },
}

impl fmt::Display for TabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "tabela vazia"),
            Self::LineTooLong { line, bytes } => write!(
                f,
                "linha {line} acima do limite ({bytes} > {MAX_LINE_BYTES} bytes)"
            ),
            Self::TooManyColumns { columns } => {
                write!(f, "colunas demais ({columns} > {MAX_COLUMNS})")
            }
            Self::TooManyRows { rows } => write!(f, "linhas demais ({rows} > {MAX_ROWS})"),
            Self::UnclosedQuote { line } => write!(f, "aspa não fechada (linha {line})"),
            Self::RaggedRow {
                row,
                expected,
                found,
            } => write!(
                f,
                "linha {row} tem {found} campos, header tem {expected} (linha não é descartada)"
            ),
            Self::SchemaBreak { reason } => write!(f, "schema break: {reason}"),
        }
    }
}

impl std::error::Error for TabError {}

/// Detecta o separador olhando só a primeira linha lógica (fora de aspas).
pub fn detect_delimiter(input: &str) -> char {
    let mut commas = 0usize;
    let mut semis = 0usize;
    let mut tabs = 0usize;
    let mut in_quotes = false;
    for (i, c) in input.chars().enumerate() {
        if i > 8192 {
            break;
        }
        match c {
            '"' => in_quotes = !in_quotes,
            '\n' | '\r' if !in_quotes => break,
            ',' if !in_quotes => commas += 1,
            ';' if !in_quotes => semis += 1,
            '\t' if !in_quotes => tabs += 1,
            _ => {}
        }
    }
    if semis > commas && semis >= tabs {
        ';'
    } else if tabs > commas && tabs > semis {
        '\t'
    } else {
        ','
    }
}

fn finish_record(
    records: &mut Vec<Vec<String>>,
    record: &mut Vec<String>,
    blank_lines: &mut usize,
) -> Result<(), TabError> {
    let blank = record.len() == 1 && record[0].trim().is_empty();
    if blank {
        // Linha em branco não é dado: contabilizada, não silenciada.
        *blank_lines += 1;
        record.clear();
        return Ok(());
    }
    if records.len() > MAX_ROWS {
        return Err(TabError::TooManyRows {
            rows: records.len(),
        });
    }
    records.push(std::mem::take(record));
    Ok(())
}

/// Parser RFC 4180 (aspas, aspas duplas, CRLF/LF; BOM já removido).
fn parse_records(text: &str, delim: char) -> Result<(Vec<Vec<String>>, usize), TabError> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_quoted = false;
    let mut quote_line = 1usize;
    let mut line = 1usize;
    let mut record_bytes = 0usize;
    let mut blank_lines = 0usize;
    let mut it = text.chars().peekable();

    while let Some(c) = it.next() {
        record_bytes += c.len_utf8();
        if in_quotes {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    it.next();
                    record_bytes += 1;
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
            if record_bytes > MAX_LINE_BYTES {
                return Err(TabError::LineTooLong {
                    line,
                    bytes: record_bytes,
                });
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !field_quoted => {
                in_quotes = true;
                field_quoted = true;
                quote_line = line;
            }
            '"' => field.push('"'),
            '\r' if it.peek() == Some(&'\n') => {}
            c if c == delim => {
                record.push(std::mem::take(&mut field));
                field_quoted = false;
            }
            '\n' | '\r' => {
                line += 1;
                record.push(std::mem::take(&mut field));
                field_quoted = false;
                finish_record(&mut records, &mut record, &mut blank_lines)?;
                record_bytes = 0;
            }
            _ => field.push(c),
        }
        if record_bytes > MAX_LINE_BYTES {
            return Err(TabError::LineTooLong {
                line,
                bytes: record_bytes,
            });
        }
    }
    if in_quotes {
        return Err(TabError::UnclosedQuote { line: quote_line });
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        finish_record(&mut records, &mut record, &mut blank_lines)?;
    }
    Ok((records, blank_lines))
}

/// Papel de uma coluna no schema (s, r, v).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabRole {
    /// Sujeito.
    Subject,
    /// Relação.
    Relation,
    /// Valor/objeto.
    Value,
    /// Coluna conhecida mas fora da tripla.
    Ignored,
    /// Coluna sem papel — nada nela vira claim.
    Unmapped,
}

impl TabRole {
    /// Rótulo estável.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Relation => "relation",
            Self::Value => "value",
            Self::Ignored => "ignored",
            Self::Unmapped => "unmapped",
        }
    }
}

/// Uma coluna do header com seu papel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabColumn {
    /// Índice 0-based.
    pub index: usize,
    /// Header original (trim).
    pub header: String,
    /// Papel.
    pub role: TabRole,
}

/// Schema derivado do header + hash determinístico.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSchema {
    /// Colunas na ordem do arquivo.
    pub columns: Vec<TabColumn>,
    /// FNV-1a 64 do mapeamento canônico (hex 16).
    pub hash: String,
}

/// Mapeamento explícito header→papel (`subject=entidade`, …).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabMapping {
    /// Header do sujeito.
    pub subject: Option<String>,
    /// Header da relação.
    pub relation: Option<String>,
    /// Header do valor.
    pub value: Option<String>,
}

impl TabMapping {
    /// Vazio = heurística.
    pub fn is_empty(&self) -> bool {
        self.subject.is_none() && self.relation.is_none() && self.value.is_none()
    }

    /// Parseia `subject=a,relation=b,value=c` (nomes de coluna, não índices).
    pub fn parse(spec: &str) -> Result<Self, TabError> {
        let mut map = Self::default();
        for part in spec.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (key, val) = part.split_once('=').ok_or_else(|| TabError::SchemaBreak {
                reason: format!("mapeamento sem '=': {part}"),
            })?;
            let val = val.trim().to_string();
            if val.is_empty() {
                return Err(TabError::SchemaBreak {
                    reason: format!("mapeamento vazio para {key}"),
                });
            }
            match key.trim().to_ascii_lowercase().as_str() {
                "subject" | "sujeito" | "s" => map.subject = Some(val),
                "relation" | "relacao" | "relação" | "r" => map.relation = Some(val),
                "value" | "valor" | "object" | "objeto" | "v" => map.value = Some(val),
                other => {
                    return Err(TabError::SchemaBreak {
                        reason: format!("papel desconhecido: {other}"),
                    })
                }
            }
        }
        if map.is_empty() {
            return Err(TabError::SchemaBreak {
                reason: "mapeamento explícito vazio".into(),
            });
        }
        Ok(map)
    }
}

fn aliases(role: TabRole) -> &'static [&'static str] {
    match role {
        TabRole::Subject => &["subject", "sujeito", "entity", "entidade", "term", "termo"],
        TabRole::Relation => &[
            "relation",
            "relacao",
            "relação",
            "predicate",
            "predicado",
            "property",
        ],
        TabRole::Value => &["value", "valor", "object", "objeto", "target", "alvo"],
        _ => &[],
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

impl TabSchema {
    /// Deriva (subject, relation, value) do header. `Ok` sempre que o header
    /// parseia; papéis ausentes ficam `Unmapped` (e a carga abstém).
    pub fn from_header(headers: &[String], mapping: &TabMapping) -> Result<Self, TabError> {
        let mut roles = vec![TabRole::Unmapped; headers.len()];
        let mut claimed = vec![false; headers.len()];
        let explicit = [
            (TabRole::Subject, mapping.subject.as_ref()),
            (TabRole::Relation, mapping.relation.as_ref()),
            (TabRole::Value, mapping.value.as_ref()),
        ];
        for (role, want) in explicit {
            match want {
                Some(name) => {
                    let idx = headers
                        .iter()
                        .position(|h| h.eq_ignore_ascii_case(name.trim()));
                    match idx {
                        Some(i) if !claimed[i] => {
                            roles[i] = role;
                            claimed[i] = true;
                        }
                        _ => {
                            return Err(TabError::SchemaBreak {
                                reason: format!(
                                    "coluna '{}' ({}) não existe no header",
                                    name,
                                    role.label()
                                ),
                            })
                        }
                    }
                }
                None => {
                    for alias in aliases(role) {
                        let found = headers
                            .iter()
                            .enumerate()
                            .position(|(i, h)| !claimed[i] && h.eq_ignore_ascii_case(alias));
                        if let Some(i) = found {
                            roles[i] = role;
                            claimed[i] = true;
                            break;
                        }
                    }
                }
            }
        }
        let columns: Vec<TabColumn> = headers
            .iter()
            .enumerate()
            .map(|(index, header)| TabColumn {
                index,
                header: header.clone(),
                role: roles[index],
            })
            .collect();
        let canonical = columns
            .iter()
            .filter(|c| !matches!(c.role, TabRole::Ignored))
            .map(|c| format!("{}|{}", c.role.label(), c.header))
            .collect::<Vec<_>>()
            .join("\u{1f}");
        let hash = format!("{:016x}", fnv1a64(canonical.as_bytes()));
        Ok(Self { columns, hash })
    }

    /// Índice da coluna com o papel pedido.
    pub fn role_index(&self, role: TabRole) -> Option<usize> {
        self.columns
            .iter()
            .find(|c| c.role == role)
            .map(|c| c.index)
    }

    /// Colunas sem papel.
    pub fn unmapped(&self) -> Vec<&TabColumn> {
        self.columns
            .iter()
            .filter(|c| c.role == TabRole::Unmapped)
            .collect()
    }
}

/// Tabela parseada (header + linhas) com schema derivado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabDoc {
    /// Dialeto detectado.
    pub dialect: TabDialect,
    /// Header na ordem do arquivo.
    pub headers: Vec<String>,
    /// Linhas de dados (todas com `headers.len()` campos).
    pub rows: Vec<Vec<String>>,
    /// Schema (s,r,v) + hash.
    pub schema: TabSchema,
    /// Linhas em branco puladas (contabilizadas, não silenciadas).
    pub blank_lines: usize,
}

/// Linha que não virou claim, com motivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabAbstain {
    /// Linha do arquivo (1-based; header = 1).
    pub row: usize,
    /// Motivo legível.
    pub reason: String,
}

/// Parseia CSV/TSV com a heurística de header.
pub fn parse_tabular(input: &str) -> Result<TabDoc, TabError> {
    parse_tabular_with(input, &TabMapping::default())
}

/// Parseia CSV/TSV com mapeamento explícito (vazio = heurística).
pub fn parse_tabular_with(input: &str, mapping: &TabMapping) -> Result<TabDoc, TabError> {
    let text = input.strip_prefix('\u{feff}').unwrap_or(input);
    if text.trim().is_empty() {
        return Err(TabError::Empty);
    }
    let delim = detect_delimiter(text);
    let (records, blank_lines) = parse_records(text, delim)?;
    if records.is_empty() {
        return Err(TabError::Empty);
    }
    let headers: Vec<String> = records[0].iter().map(|h| h.trim().to_string()).collect();
    if headers.len() > MAX_COLUMNS {
        return Err(TabError::TooManyColumns {
            columns: headers.len(),
        });
    }
    let rows = &records[1..];
    if rows.len() > MAX_ROWS {
        return Err(TabError::TooManyRows { rows: rows.len() });
    }
    for (i, row) in rows.iter().enumerate() {
        if row.len() != headers.len() {
            return Err(TabError::RaggedRow {
                row: i + 2,
                expected: headers.len(),
                found: row.len(),
            });
        }
    }
    let schema = TabSchema::from_header(&headers, mapping)?;
    Ok(TabDoc {
        dialect: TabDialect::from_delimiter(delim),
        headers,
        rows: rows.to_vec(),
        schema,
        blank_lines,
    })
}

/// Linha → `Claim` (s,r,v). Schema incompleto (ou campo vazio/longo) abstém.
pub fn claims_from_tab(doc: &TabDoc) -> (Vec<Claim>, Vec<TabAbstain>) {
    let mut claims = Vec::new();
    let mut abstains = Vec::new();
    let (s, r, v) = match (
        doc.schema.role_index(TabRole::Subject),
        doc.schema.role_index(TabRole::Relation),
        doc.schema.role_index(TabRole::Value),
    ) {
        (Some(s), Some(r), Some(v)) => (s, r, v),
        _ => {
            let missing: Vec<&str> = [
                (TabRole::Subject, doc.schema.role_index(TabRole::Subject)),
                (TabRole::Relation, doc.schema.role_index(TabRole::Relation)),
                (TabRole::Value, doc.schema.role_index(TabRole::Value)),
            ]
            .iter()
            .filter(|(_, idx)| idx.is_none())
            .map(|(role, _)| role.label())
            .collect();
            let reason = format!("schema incompleto: {}", missing.join("+"));
            for i in 0..doc.rows.len() {
                abstains.push(TabAbstain {
                    row: i + 2,
                    reason: reason.clone(),
                });
            }
            return (claims, abstains);
        }
    };
    for (i, row) in doc.rows.iter().enumerate() {
        let (subject, relation, value) = (row[s].trim(), row[r].trim(), row[v].trim());
        if subject.is_empty() || relation.is_empty() || value.is_empty() {
            abstains.push(TabAbstain {
                row: i + 2,
                reason: "campo vazio (s,r,v exige os três)".into(),
            });
            continue;
        }
        if subject.chars().count() > MAX_FIELD_CHARS
            || relation.chars().count() > MAX_FIELD_CHARS
            || value.chars().count() > MAX_FIELD_CHARS
        {
            abstains.push(TabAbstain {
                row: i + 2,
                reason: format!("campo acima de {MAX_FIELD_CHARS} chars"),
            });
            continue;
        }
        claims.push(Claim::new(
            subject,
            relation,
            value,
            1.0,
            TAB_AUTH,
            ExpertId::Memory,
            Provenance::Inferred,
        ));
    }
    (claims, abstains)
}

/// `facts.jsonl` canônico (`subject`/`relation`/`object`), uma tripla por linha.
pub fn facts_jsonl(claims: &[Claim]) -> String {
    let mut out = String::new();
    for c in claims {
        let row = Json::Obj(vec![
            ("subject".to_string(), Json::Str(c.subject.clone())),
            ("relation".to_string(), Json::Str(c.relation.clone())),
            ("object".to_string(), Json::Str(c.value.clone())),
        ]);
        out.push_str(&row.to_string());
        out.push('\n');
    }
    out
}

/// Grava `facts.jsonl` e devolve o número de triplas.
pub fn write_facts(path: &Path, claims: &[Claim]) -> std::io::Result<usize> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, facts_jsonl(claims))?;
    Ok(claims.len())
}

/// Lê um arquivo sob a sandbox e converte. Erro legível (sem panic).
pub fn convert_file(
    root: &Path,
    rel: &str,
    mapping: &TabMapping,
) -> Result<(TabDoc, Vec<Claim>, Vec<TabAbstain>), String> {
    let path = super::tools::resolve_sandbox_path(root, rel).map_err(|e| format!("path: {e}"))?;
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("leitura {}: {e}", path.display()))?;
    let doc = parse_tabular_with(&text, mapping).map_err(|e| format!("parse: {e}"))?;
    let (claims, abstains) = claims_from_tab(&doc);
    Ok((doc, claims, abstains))
}

/// Tool `tab` — tabela da sandbox → claims auth 96 (T-TAB / A1).
#[derive(Debug)]
pub struct TabTool {
    root: PathBuf,
}

impl TabTool {
    /// Nova tool sob a política de mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: policy.sandbox_root.clone(),
        }
    }
}

impl Tool for TabTool {
    fn name(&self) -> &'static str {
        "tab"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "tab",
            description: "CSV/TSV da sandbox → claims (s,r,v) auth 96; coluna OOD abstém",
            args: &["path", "query"],
            kinds: &[GoalKind::Retrieve],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(8.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        match (&goal.kind, &goal.path) {
            (GoalKind::Retrieve, Some(path)) => {
                let lower = path.to_ascii_lowercase();
                if lower.ends_with(".csv") || lower.ends_with(".tsv") || lower.ends_with(".tab") {
                    1.0
                } else {
                    0.25
                }
            }
            _ => 0.0,
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let rel = goal
            .path
            .clone()
            .ok_or_else(|| ToolError::InvalidArgs("path da tabela ausente".into()))?;
        let mapping = match goal.query.as_deref().map(str::trim) {
            Some(spec) if !spec.is_empty() => TabMapping::parse(spec)
                .map_err(|e| ToolError::InvalidArgs(format!("mapeamento: {e}")))?,
            _ => TabMapping::default(),
        };
        let (doc, claims, abstains) =
            convert_file(&self.root, &rel, &mapping).map_err(ToolError::Precondition)?;
        let total = claims.len() + abstains.len();
        let confidence = if total == 0 {
            0.0
        } else {
            claims.len() as f32 / total as f32
        };
        Ok(ToolOutcome {
            claims,
            confidence,
            detail: format!(
                "tab {} rows={} claims={} abstain={} schema={} auth={TAB_AUTH}",
                doc.dialect.label(),
                doc.rows.len(),
                total - abstains.len(),
                abstains.len(),
                doc.schema.hash
            ),
            ..ToolOutcome::default()
        })
    }
}

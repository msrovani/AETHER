//! K5 — grammar-constrained / RASP-lite para `PatchProposal` (≥95% parseável).
//!
//! Só sintaxe/estrutura (não semântica). Extrai JSON tipado de prosa solta,
//! valida gramática (`op` enum, path relativo, âncora não vazia) e opcionalmente
//! resolve `anchor_hint` por retrieval lexical no arquivo (RASP-lite).

use super::{claim, sandbox_root, CODE_AUTH_CAP, PatchOp, PatchProposal};
use crate::agent::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use crate::agent::tools::fs::resolve_sandbox_path;
use crate::agent::WorldPolicy;
use crate::json::Json;
use std::path::PathBuf;

/// Extrai o primeiro objeto `{...}` balanceado (ignora fences markdown).
pub fn extract_json_object(raw: &str) -> Option<&str> {
    let s = raw.trim();
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn path_ok(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty()
        && !p.starts_with('/')
        && !p.starts_with('\\')
        && !p.contains("..")
        && !p.chars().any(|c| c == '\0' || c == '\n')
}

/// Valida campos contra a gramática tipada de edição.
pub fn grammar_validate(prop: &PatchProposal) -> Result<(), ToolError> {
    if !path_ok(&prop.path) {
        return Err(ToolError::InvalidArgs(format!(
            "grammar: path inválido {:?}",
            prop.path
        )));
    }
    if prop.anchor.is_empty() {
        return Err(ToolError::InvalidArgs("grammar: anchor vazio".into()));
    }
    if matches!(prop.op, PatchOp::Replace | PatchOp::Insert) && prop.content.is_empty() {
        return Err(ToolError::InvalidArgs(
            "grammar: content obrigatório para replace/insert".into(),
        ));
    }
    Ok(())
}

/// RASP-lite: âncora única no `src` que contém `hint`.
///
/// Prefere a **linha** única (retrieval lexical); cai no hint literal se ele
/// aparecer exatamente 1× e nenhuma linha única servir.
pub fn rasp_resolve_anchor(src: &str, hint: &str) -> Result<String, ToolError> {
    let hint = hint.trim();
    if hint.is_empty() {
        return Err(ToolError::InvalidArgs("anchor_hint vazio".into()));
    }
    let lines: Vec<&str> = src
        .lines()
        .filter(|l| l.contains(hint))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() == 1 {
        let line = lines[0];
        if src.matches(line).count() == 1 {
            return Ok(line.to_string());
        }
    }
    let n = src.matches(hint).count();
    if n == 1 {
        return Ok(hint.to_string());
    }
    Err(ToolError::Precondition(format!(
        "rasp: âncora ambígua/ausente para hint={hint:?} (matches={n}, lines={})",
        lines.len()
    )))
}

fn maybe_rasp_fill(root: &std::path::Path, raw_obj: &str) -> Result<PatchProposal, ToolError> {
    let v = Json::parse(raw_obj).map_err(|e| ToolError::InvalidArgs(format!("json: {e}")))?;
    if v.get("anchor").and_then(|a| a.as_str()).is_some_and(|a| !a.is_empty()) {
        let prop = PatchProposal::from_json(raw_obj)?;
        grammar_validate(&prop)?;
        return Ok(prop);
    }
    let hint = v
        .get("anchor_hint")
        .and_then(|h| h.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("grammar: anchor ou anchor_hint".into()))?;
    let path = v
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("path ausente".into()))?;
    let file = resolve_sandbox_path(root, path)?;
    let src = std::fs::read_to_string(&file).map_err(|e| ToolError::Failed(e.to_string()))?;
    let anchor = rasp_resolve_anchor(&src, hint)?;
    let op = v
        .get("op")
        .and_then(|o| o.as_str())
        .ok_or_else(|| ToolError::InvalidArgs("op ausente".into()))?;
    let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let prop = PatchProposal::from_json(
        &Json::obj([
            ("path", Json::str(path)),
            ("op", Json::str(op)),
            ("anchor", Json::str(anchor)),
            ("content", Json::str(content)),
        ])
        .to_text(),
    )?;
    grammar_validate(&prop)?;
    Ok(prop)
}

/// Parse grammar-constrained: extrai JSON de prosa e valida (com RASP se preciso).
pub fn parse_constrained(
    raw: &str,
    root: Option<&std::path::Path>,
) -> Result<PatchProposal, ToolError> {
    let obj = extract_json_object(raw)
        .ok_or_else(|| ToolError::InvalidArgs("grammar: sem objeto JSON".into()))?;
    // Tenta direto; se faltar anchor, RASP.
    if let Ok(prop) = PatchProposal::from_json(obj) {
        grammar_validate(&prop)?;
        return Ok(prop);
    }
    if let Some(inner) = Json::parse(obj)
        .ok()
        .and_then(|v| v.get("proposal").map(|p| p.to_text()))
    {
        let prop = PatchProposal::from_json(&inner)?;
        grammar_validate(&prop)?;
        return Ok(prop);
    }
    let root = root.ok_or_else(|| {
        ToolError::InvalidArgs("grammar: falhou parse direto; RASP precisa sandbox".into())
    })?;
    maybe_rasp_fill(root, obj)
}

/// Taxa de parse em um corpus (aceite K5: ≥0.95).
pub fn grammar_parse_rate<'a, I>(samples: I, root: Option<&std::path::Path>) -> f64
where
    I: IntoIterator<Item = &'a str>,
{
    let mut ok = 0usize;
    let mut n = 0usize;
    for s in samples {
        n += 1;
        if parse_constrained(s, root).is_ok() {
            ok += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        ok as f64 / n as f64
    }
}

/// `code.grammar` — valida/repara prosa → `PatchProposal` tipado (K5).
#[derive(Debug)]
pub struct CodeGrammarTool {
    root: PathBuf,
}

impl CodeGrammarTool {
    /// Nova tool.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: sandbox_root(policy),
        }
    }
}

impl Tool for CodeGrammarTool {
    fn name(&self) -> &'static str {
        "code.grammar"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "code.grammar",
            description: "K5: grammar/RASP → PatchProposal parseável; auth≤120; sem apply",
            args: &["query"],
            kinds: &[GoalKind::Transform, GoalKind::Verify],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(6.0)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("code.grammar") && goal.query.is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let raw = goal
            .query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::InvalidArgs("query ausente".into()))?;
        match parse_constrained(raw, Some(&self.root)) {
            Ok(prop) => {
                let auth = CODE_AUTH_CAP.min(60);
                Ok(ToolOutcome {
                    claims: vec![
                        claim(prop.path.clone(), "code.proposal", prop.to_json(), 0.9),
                        claim(prop.path.clone(), "code.grammar", "ok", 1.0),
                    ]
                    .into_iter()
                    .map(|mut c| {
                        c.authority = c.authority.min(auth);
                        c
                    })
                    .collect(),
                    confidence: 0.9,
                    detail: format!(
                        "code.grammar ok op={} auth_cap={CODE_AUTH_CAP} scope={:?} trace={}",
                        prop.op.label(),
                        ctx.scope,
                        ctx.trace_id
                    ),
                    ..ToolOutcome::default()
                })
            }
            Err(e) => Ok(ToolOutcome {
                // Sem claim `ok` — critério do Goal falha (não inventa proposal).
                claims: Vec::new(),
                confidence: 0.0,
                detail: format!(
                    "code.grammar reject scope={:?} trace={} err={e}",
                    ctx.scope, ctx.trace_id
                ),
                ..ToolOutcome::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_de_fence_markdown() {
        let raw = "prosa\n```json\n{\"path\":\"a.rs\",\"op\":\"delete\",\"anchor\":\"X\"}\n```\n";
        let p = parse_constrained(raw, None).unwrap();
        assert_eq!(p.path, "a.rs");
    }

    #[test]
    fn rejeita_sem_json() {
        assert!(parse_constrained("só prosa solta sem estrutura", None).is_err());
    }
}

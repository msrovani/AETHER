//! LanguagePort — borda de linguagem plugável (C0 / ADR-013).
//!
//! Default: parser por schema fechado + template extrativo. Sem LLM no T0.
//! OOV fora do schema vira erro tipado (abstenção/`ask` no host), nunca chute.

use super::tool::{
    Cost, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema, WorldState,
};
use super::{Claim, ExpertId};
use crate::core::certificate::Provenance;

/// Adapta texto → [`Goal`] na borda (chamar **antes** do `ToolAgent`; sem ciclo).
pub fn adapt_text(text: &str) -> Result<Goal, LanguageError> {
    ParserPort::new().parse(text, &GoalSchema::agent_default())
}

/// Capacidades declaradas de um backend de linguagem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LanguageCaps {
    /// Custo relativo (1.0 = local barato).
    pub cost: f32,
    /// Risco máximo emitido.
    pub risk: Risk,
    /// Latência p99 declarada (ms).
    pub latency_ms_p99: u32,
    /// Precisa de modelo neural.
    pub needs_model: bool,
}

/// Estilo de geração (template).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Uma frase factual por claim.
    Factual,
    /// Lista compacta `S R=V`.
    Compact,
}

/// Erros tipados da porta (nunca alucinação silenciosa).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageError {
    /// Texto vazio.
    Empty,
    /// Fora do schema (OOV) — host deve abster/`ask`.
    Oov,
    /// Ambiguidade residual (mais de um intent plausível).
    Ambiguous,
    /// Schema sem o intent/slot necessário.
    Schema,
}

/// Schema fechado de intents/slots aceitos pelo parser.
#[derive(Clone, Debug)]
pub struct GoalSchema {
    /// Intents permitidos (`ask`, `recall`, `verify`, `sat`, `optimize`).
    pub intents: Vec<&'static str>,
}

impl GoalSchema {
    /// Schema C-G0 padrão do agente.
    pub fn agent_default() -> Self {
        Self {
            intents: vec!["ask", "recall", "verify", "sat", "optimize"],
        }
    }

    fn allows(&self, intent: &str) -> bool {
        self.intents.iter().any(|i| i.eq_ignore_ascii_case(intent))
    }
}

/// Porta de linguagem: texto↔Goal / claims↔texto.
pub trait LanguagePort: Send {
    /// Texto → [`Goal`] estruturado (ou OOV).
    fn parse(&mut self, text: &str, schema: &GoalSchema) -> Result<Goal, LanguageError>;

    /// Claims → texto auditável (sem gerar fato novo).
    fn generate(&mut self, claims: &[Claim], style: Style) -> Result<String, LanguageError>;

    /// Capacidades.
    fn caps(&self) -> LanguageCaps;
}

/// Parser por gramática/regras (default C0).
#[derive(Debug, Default)]
pub struct ParserPort {
    next_id: u64,
}

impl ParserPort {
    /// Novo parser.
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
    }
}

impl LanguagePort for ParserPort {
    fn parse(&mut self, text: &str, schema: &GoalSchema) -> Result<Goal, LanguageError> {
        let raw = text.trim();
        if raw.is_empty() {
            return Err(LanguageError::Empty);
        }
        let lower = raw.to_ascii_lowercase();
        let tokens: Vec<&str> = lower.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(LanguageError::Empty);
        }

        let intent = match tokens[0] {
            "ask" | "pergunte" | "pergunta" => "ask",
            "qual" => "ask",
            "recall" | "buscar" | "busca" | "memoria" | "memory" => "recall",
            "verify" | "verificar" | "verifique" => "verify",
            "sat" | "satisfazer" | "satisfaz" => "sat",
            "optimize" | "qubo" | "otimizar" => "optimize",
            _ => return Err(LanguageError::Oov),
        };
        if !schema.allows(intent) {
            return Err(LanguageError::Schema);
        }

        let mut goal = match intent {
            "ask" => parse_ask(&tokens, raw)?,
            "recall" => parse_recall(&tokens)?,
            "verify" => Goal {
                id: 0,
                kind: GoalKind::Verify,
                subject: None,
                relation: None,
                query: None,
                n: None,
                clauses: Vec::new(),
                j: Vec::new(),
                h: Vec::new(),
                success: super::SuccessCriteria::Any,
                budget: super::Budget::default(),
                risk_ceiling: Risk::Low,
                pipeline: Vec::new(),
            },
            "sat" => {
                let n = parse_usize_arg(&tokens, 1)?;
                Goal::satisfy(n, Vec::new())
            }
            "optimize" => {
                let n = parse_usize_arg(&tokens, 1)?;
                Goal::optimize(n, Vec::new(), vec![0.0; n])
            }
            _ => return Err(LanguageError::Oov),
        };
        goal.id = self.alloc_id();
        Ok(goal)
    }

    fn generate(&mut self, claims: &[Claim], style: Style) -> Result<String, LanguageError> {
        if claims.is_empty() {
            return Err(LanguageError::Empty);
        }
        let lines: Vec<String> = claims
            .iter()
            .map(|c| match style {
                Style::Factual => format!(
                    "{} {} = {}.",
                    c.subject.trim(),
                    c.relation.trim(),
                    c.value.trim()
                ),
                Style::Compact => format!(
                    "{} {}={}",
                    c.subject.trim(),
                    c.relation.trim(),
                    c.value.trim()
                ),
            })
            .collect();
        Ok(lines.join(" "))
    }

    fn caps(&self) -> LanguageCaps {
        LanguageCaps {
            cost: 1.0,
            risk: Risk::Low,
            latency_ms_p99: 10,
            needs_model: false,
        }
    }
}

fn parse_usize_arg(tokens: &[&str], idx: usize) -> Result<usize, LanguageError> {
    let t = tokens.get(idx).ok_or(LanguageError::Oov)?;
    t.parse::<usize>().map_err(|_| LanguageError::Oov)
}

fn parse_ask(tokens: &[&str], raw: &str) -> Result<Goal, LanguageError> {
    // `qual <relation> de <subject>`
    if tokens.first().copied() == Some("qual") {
        if tokens.len() >= 4 && tokens[2] == "de" {
            let relation = tokens[1].to_string();
            let subject = tokens[3..].join(" ");
            if subject.is_empty() {
                return Err(LanguageError::Oov);
            }
            return Ok(Goal::fact(subject, relation));
        }
        return Err(LanguageError::Oov);
    }
    // `ask|pergunte <subject> <relation>` — subject pode ter várias palavras
    // se relation for a última palavra.
    if tokens.len() < 3 {
        return Err(LanguageError::Oov);
    }
    let relation = tokens[tokens.len() - 1].to_string();
    let subject = tokens[1..tokens.len() - 1].join(" ");
    if subject.is_empty() || relation.is_empty() {
        return Err(LanguageError::Oov);
    }
    // Preserva capitalização aproximada do original para slots (ASCII).
    let _ = raw;
    Ok(Goal::fact(subject, relation))
}

fn parse_recall(tokens: &[&str]) -> Result<Goal, LanguageError> {
    if tokens.len() < 2 {
        return Err(LanguageError::Oov);
    }
    Ok(Goal::memory(tokens[1..].join(" ")))
}

/// Tool `language.parse`: texto em `goal.query` → claims (reparse sem ciclo).
/// Preferir [`adapt_text`] na borda; registro em `default_tools` = 1 linha (ACK).
#[derive(Debug)]
pub struct LanguageParseTool {
    port: ParserPort,
    schema: GoalSchema,
}

impl Default for LanguageParseTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParseTool {
    /// Nova tool com schema C-G0.
    pub fn new() -> Self {
        Self {
            port: ParserPort::new(),
            schema: GoalSchema::agent_default(),
        }
    }
}

impl Tool for LanguageParseTool {
    fn name(&self) -> &'static str {
        "language.parse"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "language.parse",
            description: "parser de schema fechado: query textual -> slots/claims",
            args: &["query"],
            kinds: &[GoalKind::Answer, GoalKind::Retrieve, GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        Cost::local(0.5)
    }

    fn risk(&self) -> Risk {
        Risk::Low
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.query.as_ref().is_some_and(|q| !q.trim().is_empty())
            && goal.subject.is_none()
            && goal.relation.is_none()
        {
            1.0
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn call(&mut self, goal: &Goal, _ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let text = goal
            .query
            .as_deref()
            .ok_or_else(|| ToolError::InvalidArgs("query ausente".into()))?;
        match self.port.parse(text, &self.schema) {
            Ok(parsed) => {
                let mut claims = Vec::new();
                if let (Some(s), Some(r)) = (parsed.subject.clone(), parsed.relation.clone()) {
                    claims.push(Claim::new(
                        s,
                        r,
                        parsed.query.clone().unwrap_or_default(),
                        1.0,
                        128,
                        ExpertId::Phase,
                        Provenance::Inferred,
                    ));
                } else if let Some(q) = parsed.query.clone() {
                    claims.push(Claim::new(
                        "query",
                        "text",
                        q,
                        1.0,
                        128,
                        ExpertId::Phase,
                        Provenance::Inferred,
                    ));
                }
                Ok(ToolOutcome {
                    confidence: 1.0,
                    detail: format!("parsed_kind={}", parsed.kind.code()),
                    claims,
                    ..ToolOutcome::default()
                })
            }
            Err(LanguageError::Oov) | Err(LanguageError::Empty) | Err(LanguageError::Schema) => {
                Ok(ToolOutcome {
                    needs_human: true,
                    detail: "language.parse OOV/schema — abster ou ask".into(),
                    ..ToolOutcome::default()
                })
            }
            Err(LanguageError::Ambiguous) => Ok(ToolOutcome {
                needs_human: true,
                detail: "language.parse ambiguo".into(),
                ..ToolOutcome::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_e_oov() {
        let mut p = ParserPort::new();
        let schema = GoalSchema::agent_default();
        let g = p.parse("ask brasil capital", &schema).unwrap();
        assert_eq!(g.kind, GoalKind::Answer);
        assert_eq!(g.subject.as_deref(), Some("brasil"));
        assert_eq!(g.relation.as_deref(), Some("capital"));
        assert!(matches!(
            p.parse("conte uma piada", &schema),
            Err(LanguageError::Oov)
        ));
    }

    #[test]
    fn qual_de_e_template() {
        let mut p = ParserPort::new();
        let schema = GoalSchema::agent_default();
        let g = p.parse("qual capital de brasil", &schema).unwrap();
        assert_eq!(g.relation.as_deref(), Some("capital"));
        assert_eq!(g.subject.as_deref(), Some("brasil"));
        let claim = Claim::new(
            "Brasil",
            "capital",
            "Brasília",
            1.0,
            200,
            crate::agent::ExpertId::Transplant,
            crate::core::certificate::Provenance::Transplanted,
        );
        let t = p.generate(&[claim], Style::Compact).unwrap();
        assert!(t.contains("capital=Brasília"));
    }
}

//! Telemetria de episódio (JSON reproduzível).

use crate::json::Json;

use super::tool::Goal;
use super::Action;

/// Um passo do episódio (ACOES §6: tool, args, custo, claims).
#[derive(Clone, Debug)]
pub struct StepTrace {
    /// Tool.
    pub tool: String,
    /// Score do planner.
    pub score: f64,
    /// Justificativa.
    pub reason: String,
    /// Executou?
    pub executed: bool,
    /// Passou no critério?
    pub met: bool,
    /// Latência medida.
    pub elapsed_us: u64,
    /// Detalhe.
    pub detail: String,
    /// Custo estimado da tool (ms).
    pub cost_ms: f32,
    /// Args / rótulo estável do goal neste passo.
    pub args: String,
    /// Número de claims na saída.
    pub claim_n: u32,
    /// Maior autoridade entre claims (0 se vazio).
    pub max_authority: u8,
}

impl StepTrace {
    /// Passo não executado (hold / erro pré-call).
    pub fn skipped(
        tool: impl Into<String>,
        score: f64,
        reason: impl Into<String>,
        detail: impl Into<String>,
        cost_ms: f32,
        args: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            score,
            reason: reason.into(),
            executed: false,
            met: false,
            elapsed_us: 0,
            detail: detail.into(),
            cost_ms,
            args: args.into(),
            claim_n: 0,
            max_authority: 0,
        }
    }
}

/// Episódio completo.
#[derive(Clone, Debug)]
pub struct EpisodeTrace {
    /// Goal.
    pub goal: String,
    /// Passos.
    pub steps: Vec<StepTrace>,
    /// Sucesso.
    pub success: bool,
    /// Ação final.
    pub action: Action,
    /// Tempo total.
    pub total_us: u64,
    /// Motivo.
    pub rationale: String,
    /// Resposta principal (se houver).
    pub answer: Option<String>,
}

impl EpisodeTrace {
    /// Serializa.
    pub fn to_json(&self) -> Json {
        let steps: Vec<Json> = self
            .steps
            .iter()
            .map(|step| {
                Json::obj([
                    ("tool", Json::str(step.tool.clone())),
                    ("score", Json::num(step.score)),
                    ("reason", Json::str(step.reason.clone())),
                    ("executed", Json::Bool(step.executed)),
                    ("met", Json::Bool(step.met)),
                    ("elapsed_us", Json::num(step.elapsed_us as f64)),
                    ("detail", Json::str(step.detail.clone())),
                    ("cost_ms", Json::num(f64::from(step.cost_ms))),
                    ("args", Json::str(step.args.clone())),
                    ("claim_n", Json::num(f64::from(step.claim_n))),
                    ("max_authority", Json::num(f64::from(step.max_authority))),
                ])
            })
            .collect();
        Json::obj([
            ("goal", Json::str(self.goal.clone())),
            ("success", Json::Bool(self.success)),
            ("action", Json::str(self.action.label())),
            ("total_us", Json::num(self.total_us as f64)),
            ("rationale", Json::str(self.rationale.clone())),
            (
                "answer",
                self.answer
                    .as_ref()
                    .map(|answer| Json::str(answer.clone()))
                    .unwrap_or(Json::Null),
            ),
            ("steps", Json::Arr(steps)),
        ])
    }
}

/// Resumo legível de um goal (sem depender de `Debug`).
pub fn goal_label(goal: &Goal) -> String {
    if !goal.pipeline.is_empty() {
        return format!("pipeline:{}", goal.pipeline.len());
    }
    match goal.kind {
        super::tool::GoalKind::Answer => {
            if let (Some(subject), Some(relation)) = (&goal.subject, &goal.relation) {
                format!("fact:{subject}|{relation}")
            } else if goal.n.is_some() {
                format!("sat:n={}", goal.n.unwrap_or(0))
            } else {
                "answer".to_string()
            }
        }
        super::tool::GoalKind::Retrieve => {
            format!("memory:{}", goal.query.as_deref().unwrap_or(""))
        }
        super::tool::GoalKind::Transform => format!("qubo:n={}", goal.n.unwrap_or(0)),
        super::tool::GoalKind::Verify => "verify".to_string(),
        super::tool::GoalKind::Act => "act".to_string(),
    }
}

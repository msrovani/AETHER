//! Telemetria de episódio (JSON reproduzível).

use crate::json::Json;

use super::tool::Goal;
use super::Action;

/// Um passo do episódio.
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
                ])
            })
            .collect();
        Json::obj([
            ("goal", Json::str(self.goal.clone())),
            ("success", Json::Bool(self.success)),
            ("action", Json::str(self.action.label())),
            ("total_us", Json::num(self.total_us as f64)),
            ("rationale", Json::str(self.rationale.clone())),
            ("steps", Json::Arr(steps)),
        ])
    }
}

/// Resumo legível de um goal (sem depender de `Debug`).
pub fn goal_label(goal: &Goal) -> String {
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

//! Registro de tools com allowlist e validação de contrato.

use super::tool::{Cost, Goal, Risk, Tool, ToolSchema, WorldState};

/// Candidato do registry para um goal.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Índice interno.
    pub index: usize,
    /// Nome.
    pub name: &'static str,
    /// Competência declarada.
    pub affordance: f32,
    /// Custo.
    pub cost: Cost,
    /// Risco.
    pub risk: Risk,
}

/// Registro de ferramentas.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    allowlist: Vec<&'static str>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.names())
            .field("allowlist", &self.allowlist)
            .finish()
    }
}

impl ToolRegistry {
    /// Registro com allowlist (`vazio` = aceita todas as tools registradas no código).
    pub fn new(allowlist: Vec<&'static str>) -> Self {
        Self {
            tools: Vec::new(),
            allowlist,
        }
    }

    /// Registra uma tool; devolve `Err` se violar o contrato/allowlist.
    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), String> {
        let name = tool.name();
        if !self.allowlist.is_empty() && !self.allowlist.contains(&name) {
            return Err(format!("tool `{name}` fora da allowlist"));
        }
        if self.tools.iter().any(|t| t.name() == name) {
            return Err(format!("tool `{name}` duplicada"));
        }
        let schema = tool.schema();
        if schema.name != name {
            return Err(format!(
                "schema.name `{}` != tool.name `{name}`",
                schema.name
            ));
        }
        if schema.args.is_empty() {
            return Err(format!("tool `{name}` sem args declarados"));
        }
        if schema.kinds.is_empty() {
            return Err(format!("tool `{name}` sem GoalKind"));
        }
        if !tool.cost().ms.is_finite() || tool.cost().ms < 0.0 {
            return Err(format!("tool `{name}` com custo inválido"));
        }
        self.tools.push(tool);
        Ok(())
    }

    /// Nomes registrados.
    pub fn names(&self) -> Vec<&'static str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    /// Schemas registrados.
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    /// `true` se a tool existe.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.name() == name)
    }

    /// Tool mutável por nome.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Tool>> {
        self.tools.iter_mut().find(|t| t.name() == name)
    }

    /// Candidatos que atendem ao goal/estado.
    pub fn candidates(&self, goal: &Goal, state: &WorldState) -> Vec<Candidate> {
        self.tools
            .iter()
            .enumerate()
            .filter_map(|(index, tool)| {
                if !tool.preconditions(goal, state) {
                    return None;
                }
                let affordance = tool.affordance(goal, state).clamp(0.0, 1.0);
                if affordance <= 0.01 {
                    return None;
                }
                Some(Candidate {
                    index,
                    name: tool.name(),
                    affordance,
                    cost: tool.cost(),
                    risk: tool.risk(),
                })
            })
            .collect()
    }

    /// Número de tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Custo/risco de uma tool (sem executar).
    pub fn stats(&self, name: &str) -> Option<(Cost, Risk)> {
        self.tools
            .iter()
            .find(|tool| tool.name() == name)
            .map(|tool| (tool.cost(), tool.risk()))
    }
}

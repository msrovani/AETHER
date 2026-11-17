//! Especialistas: cada um é uma capacidade determinística do AETHER.
//!
//! Nenhum deles chama outro; o roteador decide quem fala e a coerência decide
//! se a resposta vale. O contrato é sempre o mesmo: [`Expert::probe`] declara
//! competência (0..1) e [`Expert::infer`] devolve claims + certificado.
//!
//! | Especialista | Base | Tarefa |
//! |---|---|---|
//! | `Transplant` | FHRR (`core::fhrr`) | fatos relacionais do transplante |
//! | `Phase` | `ComplexPhaseMemory` | completar padrão de fase |
//! | `Logic` | `LogicPipeline` | SAT/3-CNF |
//! | `Solver` | `ThermodynamicIsingSolver` | QUBO/Ising |
//! | `Memory` | `neural-sgdb` | fatos textuais duráveis |

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use ndarray::{Array2, Array3};

use crate::core::certificate::{self, Certificate as EngineCertificate, Provenance};
use crate::core::fhrr::FhrrStore;
use crate::core::knowledge_bridge_f1::ScopeId;
use crate::core::logic_pipeline::{violations_3, LogicPipeline};
use crate::core::thermodynamic_solver::{IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver};
use crate::json::Json;
use crate::knowledge::sgdb::{open, SgdbClient, SgdbConfig, Transport};
use crate::phase_memory::ComplexPhaseMemory;

use super::{AgentConfig, Claim, ExpertId, ExpertOutput, Feedback, Task, TaskFeatures};

/// Contrato de um especialista.
pub trait Expert: Send {
    /// Identidade.
    fn id(&self) -> ExpertId;

    /// Competência declarada para a tarefa (`0` = não atende).
    fn probe(&self, task: &Task, features: &TaskFeatures) -> f32;

    /// Executa a inferência.
    fn infer(&mut self, task: &Task) -> ExpertOutput;

    /// Aprende com feedback verificado.
    fn learn(&mut self, task: &Task, feedback: &Feedback);

    /// Fatos/atoms mantidos (diagnóstico).
    fn facts(&self) -> usize {
        0
    }
}

/// Monta os especialistas padrão a partir da configuração.
pub fn default_experts(cfg: &AgentConfig) -> Vec<Box<dyn Expert>> {
    let facts = load_facts(&cfg.facts_path);
    let mut experts: Vec<Box<dyn Expert>> = Vec::new();
    match TransplantExpert::new(cfg.dim, cfg.traces, &facts) {
        Ok(expert) => experts.push(Box::new(expert)),
        Err(e) => eprintln!("aviso: transplante indisponível ({e})"),
    }
    experts.push(Box::new(PhaseExpert::new(64)));
    experts.push(Box::new(LogicExpert::new(96, 512)));
    experts.push(Box::new(SolverExpert::new(256, 400)));
    if let Some(spec) = &cfg.sgdb {
        match MemoryExpert::new(spec) {
            Ok(expert) => experts.push(Box::new(expert)),
            Err(e) => eprintln!("aviso: memoria SGDB indisponível ({e})"),
        }
    }
    experts
}

/// Lê `subject|relation|object` de um JSONL (formato do transplante).
pub fn load_facts(path: &Path) -> Vec<(String, String, String)> {
    let Ok(text) = fs::read_to_string(path) else {
        return builtin_facts();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(json) = Json::parse(line) else {
            continue;
        };
        let Some(subject) = json.get("subject").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(relation) = json.get("relation").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(object) = json
            .get("object")
            .or_else(|| json.get("value"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        out.push((
            subject.to_string(),
            relation.to_string(),
            object.to_string(),
        ));
    }
    if out.is_empty() {
        builtin_facts()
    } else {
        out
    }
}

/// Fatos mínimos quando `data/facts.jsonl` não existe.
pub fn builtin_facts() -> Vec<(String, String, String)> {
    [
        ("Brasil", "capital", "Brasília"),
        ("França", "capital", "Paris"),
        ("Rust", "é_uma", "linguagem"),
        ("Python", "é_uma", "linguagem"),
        ("água", "é", "H2O"),
        ("sol", "é_uma", "estrela"),
        ("Lua", "orbita", "Terra"),
        ("AETHER", "usa", "fase"),
        ("AETHER", "tem", "certificado"),
        ("neural-sgdb", "guarda", "memória"),
    ]
    .into_iter()
    .map(|(s, r, o)| (s.to_string(), r.to_string(), o.to_string()))
    .collect()
}

fn hash64(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Fases determinísticas de um símbolo (o "transplante" sem embedding externo).
pub fn symbol_phases(label: &str, dim: usize) -> Vec<f32> {
    let mut x = hash64(label) | 1;
    (0..dim)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 11) as f32 / (1u64 << 53) as f32 * std::f32::consts::TAU
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Transplant (FHRR)
// ---------------------------------------------------------------------------

struct TransplantExpert {
    dim: usize,
    traces: usize,
    store: FhrrStore,
    symbols: HashMap<String, Vec<f32>>,
    values_by_index: Vec<String>,
    facts: usize,
}

impl TransplantExpert {
    fn new(dim: usize, traces: usize, facts: &[(String, String, String)]) -> Result<Self, String> {
        let dim = dim.max(64);
        let traces = traces.max(1);
        let mut store = FhrrStore::with_traces(dim, traces).map_err(|e| e.to_string())?;
        let mut symbols: HashMap<String, Vec<f32>> = HashMap::new();
        let mut values_by_index = Vec::new();
        for (subject, relation, value) in facts {
            let entity = symbol(&mut symbols, subject, dim);
            let rel = symbol(&mut symbols, relation, dim);
            let val = symbol(&mut symbols, value, dim);
            store
                .add_fact(&entity, &rel, &val)
                .map_err(|e| e.to_string())?;
            values_by_index.push(value.clone());
        }
        Ok(Self {
            dim,
            traces,
            store,
            symbols,
            values_by_index,
            facts: facts.len(),
        })
    }

    fn query(&self, subject: &str, relation: &str) -> Option<(String, f32, f32)> {
        let entity = self.symbols.get(subject)?;
        let rel = self.symbols.get(relation)?;
        let mask = vec![true; self.dim];
        let m = self
            .store
            .query_selective(entity, rel, &mask, 0.05)
            .ok()??;
        let value = self.values_by_index.get(m.value_index)?.clone();
        let confidence = m
            .value_similarities
            .get(m.value_index)
            .copied()
            .unwrap_or(m.similarity)
            .clamp(0.0, 1.0);
        Some((value, confidence, m.margin))
    }
}

fn symbol(cache: &mut HashMap<String, Vec<f32>>, label: &str, dim: usize) -> Vec<f32> {
    cache
        .entry(label.to_string())
        .or_insert_with(|| symbol_phases(label, dim))
        .clone()
}

impl Expert for TransplantExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Transplant
    }

    fn probe(&self, task: &Task, features: &TaskFeatures) -> f32 {
        if matches!(task, Task::Fact { .. }) {
            (1.0 - features.density).max(0.5)
        } else {
            0.0
        }
    }

    fn infer(&mut self, task: &Task) -> ExpertOutput {
        let Task::Fact { subject, relation } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não relacional");
        };
        match self.query(subject, relation) {
            Some((value, confidence, margin)) => {
                let claim = Claim::new(
                    subject.clone(),
                    relation.clone(),
                    value,
                    confidence,
                    200,
                    self.id(),
                    Provenance::Transplanted,
                );
                let cert = EngineCertificate::new(
                    0.0,
                    f64::from(confidence),
                    0,
                    f64::from(confidence),
                    Provenance::Transplanted,
                );
                ExpertOutput {
                    expert: self.id(),
                    claims: vec![claim],
                    confidence,
                    certificate: Some(cert),
                    detail: format!(
                        "FHRR dim={} traces={} fatos={} margem={margin:.3}",
                        self.dim, self.traces, self.facts
                    ),
                    cost_us: 0,
                }
            }
            None => ExpertOutput::empty(self.id(), "nenhum valor com evidência suficiente"),
        }
    }

    fn learn(&mut self, task: &Task, feedback: &Feedback) {
        let Some(true) = feedback.correct else {
            return;
        };
        let Some(expected) = &feedback.expected else {
            return;
        };
        let Task::Fact { subject, relation } = task else {
            return;
        };
        if !expected.value.trim().is_empty() && expected.same_value(expected) {
            let entity = symbol(&mut self.symbols, subject, self.dim);
            let rel = symbol(&mut self.symbols, relation, self.dim);
            let val = symbol(&mut self.symbols, &expected.value, self.dim);
            if self.store.add_fact(&entity, &rel, &val).is_ok() {
                self.values_by_index.push(expected.value.clone());
                self.facts += 1;
            }
        }
    }

    fn facts(&self) -> usize {
        self.facts
    }
}

// ---------------------------------------------------------------------------
// Phase (memória associativa)
// ---------------------------------------------------------------------------

struct PhaseExpert {
    memory: ComplexPhaseMemory,
    labels: Vec<String>,
    dim: usize,
}

impl PhaseExpert {
    fn new(dim: usize) -> Self {
        let dim = dim.max(8);
        Self {
            memory: ComplexPhaseMemory::new(dim),
            labels: Vec::new(),
            dim,
        }
    }

    /// Semeia padrões rotulados (para testes/demos).
    #[allow(dead_code)]
    pub fn with_patterns(dim: usize, patterns: &[Vec<f32>], labels: &[String]) -> Self {
        let mut expert = Self::new(dim);
        for (pattern, label) in patterns.iter().zip(labels.iter()) {
            if pattern.len() == expert.dim {
                expert.memory.imprint(pattern, 2);
                expert.labels.push(label.clone());
            }
        }
        expert
    }
}

impl Expert for PhaseExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Phase
    }

    fn probe(&self, task: &Task, _features: &TaskFeatures) -> f32 {
        if matches!(task, Task::Recall { .. }) {
            1.0
        } else {
            0.0
        }
    }

    fn infer(&mut self, task: &Task) -> ExpertOutput {
        let Task::Recall { cue } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não é recall");
        };
        if cue.len() != self.dim {
            return ExpertOutput::empty(
                self.id(),
                format!("cue de {} fases; esperado {}", cue.len(), self.dim),
            );
        }
        let mask = vec![true; self.dim];
        let result = self.memory.recall_with_confidence(cue, &mask, 12, 0.05);
        let (_, confidence) = match result {
            Ok(pair) => pair,
            Err(e) => return ExpertOutput::empty(self.id(), e.to_string()),
        };
        if confidence.should_abstain() || confidence.best >= self.labels.len() {
            return ExpertOutput::empty(
                self.id(),
                format!("margem {:.3} abaixo do piso", confidence.margin),
            );
        }
        let label = self.labels[confidence.best].clone();
        let support = confidence.best_similarity.clamp(0.0, 1.0);
        let cert = EngineCertificate::new(
            0.0,
            f64::from(support),
            0,
            f64::from(confidence.margin.max(0.0)),
            Provenance::Observed,
        );
        ExpertOutput {
            expert: self.id(),
            claims: vec![Claim::new(
                "phase",
                "pattern",
                label,
                support,
                200,
                self.id(),
                Provenance::Observed,
            )],
            confidence: support,
            certificate: Some(cert),
            detail: format!(
                "margem={:.3} sim={:.3} P={}",
                confidence.margin,
                confidence.best_similarity,
                self.labels.len()
            ),
            cost_us: 0,
        }
    }

    fn learn(&mut self, task: &Task, feedback: &Feedback) {
        let Some(true) = feedback.correct else {
            return;
        };
        let Task::Recall { cue } = task else {
            return;
        };
        if cue.len() == self.dim {
            self.memory.imprint(cue, 1);
            if let Some(expected) = &feedback.expected {
                self.labels.push(expected.value.clone());
            }
        }
    }

    fn facts(&self) -> usize {
        self.labels.len()
    }
}

// ---------------------------------------------------------------------------
// Logic (SAT)
// ---------------------------------------------------------------------------

struct LogicExpert {
    pipeline: LogicPipeline,
    n_max: usize,
    m_max: usize,
    seed: u64,
}

impl LogicExpert {
    fn new(n_max: usize, m_max: usize) -> Self {
        Self {
            pipeline: LogicPipeline::new(n_max, m_max, 400, 2, 3, 0.5),
            n_max,
            m_max,
            seed: 0xA37E_10C1,
        }
    }
}

impl Expert for LogicExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Logic
    }

    fn probe(&self, task: &Task, features: &TaskFeatures) -> f32 {
        if matches!(task, Task::Satisfy { .. }) {
            (0.6 + features.density).min(1.0)
        } else {
            0.0
        }
    }

    fn infer(&mut self, task: &Task) -> ExpertOutput {
        let Task::Satisfy { n, clauses } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não é SAT");
        };
        if *n == 0 || *n > self.n_max || clauses.len() > self.m_max {
            return ExpertOutput::empty(
                self.id(),
                format!("instância fora do limite (n={n}, m={})", clauses.len()),
            );
        }
        let mut state = vec![1i8; *n];
        let mut cert = EngineCertificate::new(f64::NAN, 0.0, 0, 0.0, Provenance::Inferred);
        if let Err(e) = self
            .pipeline
            .resolve_into(*n, clauses, self.seed, &mut state, &mut cert)
        {
            return ExpertOutput::empty(self.id(), e.to_string());
        }
        let violations = violations_3(clauses, &state);
        let value: String = state
            .iter()
            .map(|s| if *s > 0 { '1' } else { '0' })
            .collect();
        let support = if violations == 0 { 1.0 } else { 0.0 };
        ExpertOutput {
            expert: self.id(),
            claims: vec![Claim::new(
                "sat",
                "assignment",
                value,
                support,
                200,
                self.id(),
                Provenance::Inferred,
            )],
            confidence: support,
            certificate: Some(EngineCertificate::new(
                cert.energy,
                cert.gap,
                violations,
                cert.consensus,
                Provenance::Inferred,
            )),
            detail: format!(
                "n={} m={} violações={} stages={:?}",
                n,
                clauses.len(),
                violations,
                self.pipeline.last
            ),
            cost_us: 0,
        }
    }

    fn learn(&mut self, _task: &Task, _feedback: &Feedback) {}
}

// ---------------------------------------------------------------------------
// Solver (QUBO)
// ---------------------------------------------------------------------------

struct SolverExpert {
    n_max: usize,
    steps: usize,
}

impl SolverExpert {
    fn new(n_max: usize, steps: usize) -> Self {
        Self { n_max, steps }
    }
}
impl Expert for SolverExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Solver
    }

    fn probe(&self, task: &Task, _features: &TaskFeatures) -> f32 {
        if matches!(task, Task::Optimize { .. }) {
            1.0
        } else {
            0.0
        }
    }

    fn infer(&mut self, task: &Task) -> ExpertOutput {
        let Task::Optimize { n, j, h } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não é otimização");
        };
        if *n == 0 || *n > self.n_max || j.len() != n * n || h.len() != *n {
            return ExpertOutput::empty(
                self.id(),
                format!("instância inválida (n={n}, J={}, h={})", j.len(), h.len()),
            );
        }
        let j3 = Array3::from_shape_fn((1, *n, *n), |(_, i, k)| j[i * n + k]);
        let h2 = Array2::from_shape_fn((1, *n), |(_, i)| h[i]);
        let landscape = IsingLandscape::new(j3.view(), h2.view());
        let mut solver = ThermodynamicIsingSolver::new(1, *n, self.steps, 0.05);
        let mut out = RelaxOutcome::with_capacity(1, *n, self.steps);
        if let Err(e) = solver.relax_into(landscape, &mut out) {
            return ExpertOutput::empty(self.id(), e.to_string());
        }
        let state: Vec<i8> = out.states.row(0).to_vec();
        let energy = certificate::energy(j, h, *n, &state);
        let gap = certificate::local_gap(j, h, *n, &state);
        let value: String = state
            .iter()
            .map(|s| if *s > 0 { '+' } else { '-' })
            .collect();
        let support = if gap > 0.0 {
            (gap / (1.0 + gap)) as f32
        } else {
            0.0
        };
        ExpertOutput {
            expert: self.id(),
            claims: vec![Claim::new(
                "qubo",
                "argmin",
                value,
                support,
                200,
                self.id(),
                Provenance::Inferred,
            )],
            confidence: support,
            certificate: Some(EngineCertificate::new(
                energy,
                gap,
                0,
                1.0,
                Provenance::Inferred,
            )),
            detail: format!("E={energy:.4} ΔE={gap:.4} n={n}"),
            cost_us: 0,
        }
    }

    fn learn(&mut self, _task: &Task, _feedback: &Feedback) {}
}

// ---------------------------------------------------------------------------
// Memory (SGDB)
// ---------------------------------------------------------------------------

struct MemoryExpert {
    client: SgdbClient<Box<dyn Transport>>,
    scope: ScopeId,
}

impl MemoryExpert {
    fn new(spec: &str) -> Result<Self, String> {
        let cfg = SgdbConfig::from_env();
        let client = open(spec, cfg).map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            scope: ScopeId::new(1, 0, 1),
        })
    }
}

impl Expert for MemoryExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Memory
    }

    fn probe(&self, task: &Task, _features: &TaskFeatures) -> f32 {
        if matches!(task, Task::Memory { .. }) {
            0.9
        } else {
            0.0
        }
    }

    fn infer(&mut self, task: &Task) -> ExpertOutput {
        let Task::Memory { query } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não é memória");
        };
        let hits = match self.client.recall_text(self.scope, query, 8) {
            Ok(hits) => hits,
            Err(e) => return ExpertOutput::empty(self.id(), e.to_string()),
        };
        if hits.is_empty() {
            return ExpertOutput::empty(self.id(), "sem hit durável");
        }
        let claims = hits
            .iter()
            .map(|hit| {
                Claim::new(
                    "memory",
                    "text",
                    hit.text.clone(),
                    hit.score,
                    hit.authority,
                    self.id(),
                    Provenance::Transplanted,
                )
            })
            .collect();
        let confidence = hits
            .iter()
            .map(|hit| hit.score)
            .fold(0.0f32, f32::max)
            .clamp(0.0, 1.0);
        ExpertOutput {
            expert: self.id(),
            claims,
            confidence,
            certificate: None,
            detail: format!("{} hit(s) no SGDB", hits.len()),
            cost_us: 0,
        }
    }

    fn learn(&mut self, _task: &Task, _feedback: &Feedback) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::logic_pipeline::Clause3;

    fn fact_task(s: &str, r: &str) -> Task {
        Task::Fact {
            subject: s.to_string(),
            relation: r.to_string(),
        }
    }

    #[test]
    fn transplant_aprende_e_responde() {
        let mut expert = TransplantExpert::new(
            256,
            4,
            &[
                ("Brasil".into(), "capital".into(), "Brasília".into()),
                ("França".into(), "capital".into(), "Paris".into()),
            ],
        )
        .unwrap();
        let features = TaskFeatures::from_task(&fact_task("Brasil", "capital"));
        assert!(expert.probe(&fact_task("Brasil", "capital"), &features) > 0.5);
        let out = expert.infer(&fact_task("Brasil", "capital"));
        assert_eq!(out.claims.len(), 1);
        assert_eq!(out.claims[0].value, "Brasília");
        assert!(out.confidence > 0.5);
        // Aprende um fato novo com feedback verificado.
        let task = fact_task("Itália", "capital");
        expert.learn(
            &task,
            &Feedback {
                correct: Some(true),
                expected: Some(Claim::new(
                    "Itália",
                    "capital",
                    "Roma",
                    1.0,
                    255,
                    ExpertId::Transplant,
                    Provenance::Human,
                )),
            },
        );
        let out = expert.infer(&task);
        assert_eq!(out.claims[0].value, "Roma");
    }

    #[test]
    fn logic_resolve_sat_simples() {
        let mut expert = LogicExpert::new(16, 64);
        // (x1 ∨ x2) ∧ (¬x1 ∨ x3)
        let clauses = vec![Clause3::new2(0, 1, 1, 1), Clause3::new2(0, -1, 2, 1)];
        let task = Task::Satisfy { n: 3, clauses };
        let out = expert.infer(&task);
        assert_eq!(out.claims.len(), 1);
        assert!(out.confidence > 0.5);
        assert_eq!(out.certificate.unwrap().violations, 0);
    }

    #[test]
    fn solver_resolve_qubo_trivial() {
        let mut expert = SolverExpert::new(8, 200);
        let n = 4;
        // Campo h=+1 favorece todos +1 (J=0): mínimo único.
        let j = vec![0.0; n * n];
        let out = expert.infer(&Task::Optimize {
            n,
            j,
            h: vec![1.0; n],
        });
        assert_eq!(out.claims.len(), 1);
        assert_eq!(out.claims[0].value, "++++");
        assert!(out.certificate.unwrap().gap > 0.0);
    }
}

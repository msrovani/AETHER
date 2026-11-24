//! D0 — avaliação em tiers e artefato JSON reproduzível (ADR-011).
//!
//! Gate **D-G0**: `aether-agent eval` grava `target/aether_agent_eval.json`
//! com host, seed, versão, data e declaração explícita de que a suíte própria
//! (T0) **não** é benchmark externo (T3).

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::json::Json;

/// Tier de avaliação (não misturar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalTier {
    /// Suíte própria (ACQ / COGNITIVE_BENCHMARK_SUITE).
    T0SelfSuite,
    /// Coprocessador: recall, abstenção, latência, RAM.
    T1Coprocessor,
    /// Híbrido: LanguagePort na borda + AETHER.
    T2Hybrid,
    /// Benchmarks externos (MMLU/…): só com gerador de linguagem.
    T3External,
}

impl EvalTier {
    /// Rótulo estável.
    pub fn label(self) -> &'static str {
        match self {
            Self::T0SelfSuite => "T0_self_suite",
            Self::T1Coprocessor => "T1_coprocessor",
            Self::T2Hybrid => "T2_hybrid",
            Self::T3External => "T3_external",
        }
    }

    /// Pode comparar com LLM neste tier?
    pub fn comparable_to_llm(self) -> bool {
        matches!(self, Self::T1Coprocessor | Self::T2Hybrid)
    }

    /// Texto de aviso (mitiga overclaim).
    pub fn disclaimer(self) -> &'static str {
        match self {
            Self::T0SelfSuite => {
                "ACQ/self-suite mede cobertura do próprio escopo; nao e MMLU/GSM8K/Arena"
            }
            Self::T1Coprocessor => {
                "compara latencia/RAM/abstencao/recall com LLM; nao inteligencia geral"
            }
            Self::T2Hybrid => "exige LanguagePort; factualidade/custo vs LLM-only e AETHER-only",
            Self::T3External => "so quando houver gerador/adaptador de linguagem; ate la = N/A",
        }
    }
}

/// Uma linha por tier no relatório.
#[derive(Clone, Debug)]
pub struct TierStatus {
    /// Tier.
    pub tier: EvalTier,
    /// Pronto para medir neste host?
    pub ready: bool,
    /// Motivo curto.
    pub note: String,
}

/// Relatório D0.
#[derive(Clone, Debug)]
pub struct EvalReport {
    /// Schema do artefato.
    pub schema: &'static str,
    /// Versão do crate.
    pub version: String,
    /// Hostname (ou `unknown`).
    pub host: String,
    /// Seed declarada.
    pub seed: u64,
    /// Epoch UTC (s).
    pub generated_at_unix: u64,
    /// Perfil (debug/release) se conhecido.
    pub profile: String,
    /// Tiers.
    pub tiers: Vec<TierStatus>,
    /// Invariante ADR-011.
    pub self_suite_not_external: bool,
}

impl EvalReport {
    /// Monta relatório D0 a partir do ambiente.
    pub fn collect(seed: u64) -> Self {
        let host = hostname();
        let profile = if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        };
        let tiers = vec![
            TierStatus {
                tier: EvalTier::T0SelfSuite,
                ready: true,
                note: "suite propria disponivel (COGNITIVE_BENCHMARK_SUITE / ACQ)".into(),
            },
            TierStatus {
                tier: EvalTier::T1Coprocessor,
                ready: true,
                note: "gates transplant/e3/agent medem recall, abstencao, latencia".into(),
            },
            TierStatus {
                tier: EvalTier::T2Hybrid,
                ready: false,
                note: "LanguagePort C0 wired (language.parse + adapt_text); T2 parcial sem CLI/API text".into(),
            },
            TierStatus {
                tier: EvalTier::T3External,
                ready: false,
                note: "sem gerador de linguagem; MMLU/GSM8K/HumanEval = N/A".into(),
            },
        ];
        let mut report = Self {
            schema: "aether_agent_eval/d0",
            version: env!("CARGO_PKG_VERSION").to_string(),
            host,
            seed,
            generated_at_unix: now_unix(),
            profile,
            tiers,
            self_suite_not_external: true,
        };
        report.apply_coder_t3_generator();
        report
    }

    /// K7: se houver gerador coder declarado, T3 fica *parcialmente* ready
    /// (suíte `coder_t3_pilot` — ainda ≠ HumanEval).
    pub fn apply_coder_t3_generator(&mut self) {
        let gen = super::coder_t3_generator_declared();
        if let Some(g) = gen {
            if let Some(t3) = self
                .tiers
                .iter_mut()
                .find(|t| t.tier == EvalTier::T3External)
            {
                t3.ready = true;
                t3.note = format!(
                    "gerador declarado={g}; medir so coder_t3_pilot (code.eval); HumanEval/Arena ainda N/A sem escopo externo"
                );
            }
        }
    }

    /// Serializa.
    pub fn to_json(&self) -> Json {
        let tiers: Vec<Json> = self
            .tiers
            .iter()
            .map(|t| {
                Json::obj([
                    ("tier", Json::str(t.tier.label())),
                    ("ready", Json::Bool(t.ready)),
                    ("comparable_to_llm", Json::Bool(t.tier.comparable_to_llm())),
                    ("disclaimer", Json::str(t.tier.disclaimer())),
                    ("note", Json::str(t.note.clone())),
                ])
            })
            .collect();
        Json::obj([
            ("schema", Json::str(self.schema)),
            ("version", Json::str(self.version.clone())),
            ("host", Json::str(self.host.clone())),
            ("seed", Json::num(self.seed as f64)),
            (
                "generated_at_unix",
                Json::num(self.generated_at_unix as f64),
            ),
            ("profile", Json::str(self.profile.clone())),
            (
                "self_suite_not_external_benchmark",
                Json::Bool(self.self_suite_not_external),
            ),
            (
                "rule",
                Json::str("T0_self_suite != T3_external; nao misturar numeros"),
            ),
            ("tiers", Json::Arr(tiers)),
            ("gate", Json::str("D-G0")),
        ])
    }

    /// Grava JSON no path (cria dirs).
    pub fn write_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_json().to_string())?;
        Ok(())
    }
}

/// Path padrão do artefato D0.
pub fn default_eval_path() -> PathBuf {
    PathBuf::from("target/aether_agent_eval.json")
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d0_report_marks_self_suite_not_external() {
        let report = EvalReport::collect(7);
        assert!(report.self_suite_not_external);
        assert_eq!(report.tiers.len(), 4);
        assert_eq!(report.tiers[0].tier, EvalTier::T0SelfSuite);
        assert!(!report.tiers[3].ready);
        let json = report.to_json().to_string();
        assert!(json.contains("aether_agent_eval/d0"));
        assert!(json.contains("self_suite_not_external_benchmark"));
        assert!(json.contains("T3_external"));
    }
}

//! Pinos de versão e configuração do gate (Eixos B/C do plano F2→F3).
//!
//! Os pinos são contratos de build: mudou versão → rebuild + re-probe n=30
//! medido antes do próximo passo (Eixo D). O upgrade silencioso é proibido.

use serde::{Deserialize, Serialize};

/// Interpreter embutido via PyO3 (venv `target/vega`).
pub const PYTHON_PIN: &str = "3.14.10";
/// Torch CUDA no venv (2.15+cpu não sobe — versão "mais nova" sem CUDA).
pub const TORCH_PIN: &str = "2.14.1+cu126";
/// Binding FFI (0.27+ = suporte completo a Python 3.14).
pub const PYO3_PIN: &str = "0.30";
/// Modelo primário no hub (calibração persiste em cópia local, offline).
pub const PRIMARY_HUB: &str = "nandakishorm/vega-U";
/// Rótulo do gate completo (primário AND sentinela).
pub const ENGINE_LABEL: &str = "vega-u-0.8b AND gemma-prose";

/// Orçamento de VRAM do watchdog (MiB) — o plano fixa >5GB → abstain.
pub const VRAM_BUDGET_MB: u64 = 5 * 1024;

fn default_vram_threshold_mb() -> u64 {
    VRAM_BUDGET_MB
}

/// Configuração serializável do gate (arquivo JSON da cadeira).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// Checkpoint primário (None = hub default 0.8B calibrado).
    pub model_primary: Option<String>,
    /// Checkpoint sentinela (gemma-prose). None = AND degrada para primário
    /// só — modo transitório da F1, registrado na MSG do fórum.
    pub model_sentinel: Option<String>,
    /// Corpus de calibração (124 → 500 rótulos, F3); fit é rodado no start.
    pub calibration: Option<String>,
    /// Limite de VRAM do watchdog (MiB). Estouro → abstain `engine_unavailable`.
    pub vram_threshold_mb: u64,
    /// Rótulo de proveniência no meta da resposta.
    pub engine: String,
    /// Interpretador Python pinado do engine (`VEGA_PYTHON`).
    ///
    /// O embed do PyO3 (`auto-initialize`) resolve o **Python base** da
    /// máquina, não o venv: medido, `prefix=...Python314` e o
    /// `site-packages` do venv some do `sys.path`. Por isso o interpretador
    /// pinado é declarado aqui — é ele que diz onde está o motor.
    pub python: Option<String>,
    /// `site-packages` do venv (`VEGA_SITE_PACKAGES`).
    ///
    /// Precisa entrar no `sys.path` **antes** do `import vegaml`; sem isso o
    /// port degrada honesto para `engine_unavailable` (medido: `ModuleNotFound
    /// Error: No module named 'vegaml'`).
    pub python_site_packages: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            model_primary: None,
            model_sentinel: None,
            calibration: None,
            vram_threshold_mb: default_vram_threshold_mb(),
            engine: ENGINE_LABEL.to_string(),
            python: None,
            python_site_packages: None,
        }
    }
}

impl EngineConfig {
    /// Config da cadeira como ficou no fechamento da F1 de calibração
    /// (MSG 0756→0758): primário 0.8B + sentinela gemma-prose + corpus 124.
    pub fn gate_f1(corpus: Option<String>) -> Self {
        Self {
            model_sentinel: Some("target/vega/vega-u-gemma-1b-prose".to_string()),
            calibration: corpus,
            ..Self::default()
        }
    }

    /// Config a partir do ambiente — o daemon e o CLI apontam o motor aqui.
    ///
    /// `VEGA_PYTHON` / `VEGA_SITE_PACKAGES`; `PYTHONPATH` é o fallback
    /// porque é a variável que o próprio Python lê.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Some(v) = std::env::var("VEGA_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            cfg.model_primary = Some(v);
        }
        if let Some(v) = std::env::var("VEGA_PROSE_MODEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            cfg.model_sentinel = Some(v);
        }
        if let Some(v) = std::env::var("VEGA_CALIBRATION")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            cfg.calibration = Some(v);
        }
        if let Some(v) = std::env::var("VEGA_PYTHON")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            cfg.python = Some(v);
        }
        let site = std::env::var("VEGA_SITE_PACKAGES")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("PYTHONPATH")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            });
        cfg.python_site_packages = site;
        cfg
    }

    /// `site-packages` efetivo: config → `PYTHONPATH` → derivado do venv.
    ///
    /// Derivar do interpretador pinado é o fallback que faz o embed
    /// funcionar sem variável extra: o venv do engine tem layout fixo
    /// (`<venv>/Scripts/python.exe` → `<venv>/Lib/site-packages`).
    pub fn effective_site_packages(&self) -> Option<String> {
        if let Some(s) = &self.python_site_packages {
            if !s.trim().is_empty() {
                return Some(s.clone());
            }
        }
        self.python.as_ref().and_then(|py| {
            let p = std::path::Path::new(py);
            let dir = p.parent()?;
            let root = dir.parent()?;
            let cand = if cfg!(windows) {
                root.join("Lib").join("site-packages")
            } else {
                root.join("lib").join("python3.14").join("site-packages")
            };
            cand.exists().then(|| cand.to_string_lossy().to_string())
        })
    }

    /// Carrega de arquivo JSON (Eixo B: cano tipado, sem config solta).
    pub fn from_file(path: &str) -> Result<Self, crate::types::DecisionError> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| crate::types::DecisionError::Contract(format!("{path}: {e}")))?;
        serde_json::from_str(&json)
            .map_err(|e| crate::types::DecisionError::Contract(format!("config: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_approved_design() {
        let c = EngineConfig::default();
        assert_eq!(c.engine, "vega-u-0.8b AND gemma-prose");
        assert_eq!(c.vram_threshold_mb, VRAM_BUDGET_MB);
        assert!(c.model_primary.is_none());
    }

    #[test]
    fn serde_roundtrip_keeps_none_fields() {
        let c = EngineConfig::gate_f1(Some("data/vega_calibration_starter.json".into()));
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("gemma-1b-prose"));
        let back: EngineConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(back.calibration, c.calibration);
        assert_eq!(back.model_sentinel, c.model_sentinel);
    }

    #[test]
    fn pins_are_the_sanctioned_versions() {
        assert_eq!(PYTHON_PIN, "3.14.10");
        assert_eq!(TORCH_PIN, "2.14.1+cu126");
        assert!(PYO3_PIN.starts_with("0.30"));
    }
}

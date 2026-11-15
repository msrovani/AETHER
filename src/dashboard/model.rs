//! Carrega e normaliza o scorecard para o dashboard.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::error::{AetherError, Result};

/// Cartão de uma dimensão T0x.
#[derive(Clone, Debug)]
pub struct DimCard {
    pub id: String,
    pub name: String,
    pub score: f64,
    pub status: String,
    pub provenance: String,
    /// Pares (chave, valor) das métricas numéricas.
    pub metrics: Vec<(String, f64)>,
}

/// Regime de vazão.
#[derive(Clone, Debug, Default)]
pub struct ThroughputRegime {
    pub name: String,
    pub steps: u64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub p99_9_us: f64,
    pub cycles_per_s: f64,
    pub sop_per_s: f64,
    pub eq_t_per_s: f64,
    pub hot_path_allocations: u64,
}

impl ThroughputRegime {
    /// TOP/s aproximado: SOP/s / 1e12.
    pub fn tops(&self) -> f64 {
        self.sop_per_s / 1e12
    }
}

/// Bloco de throughput do scorecard.
#[derive(Clone, Debug, Default)]
pub struct ThroughputView {
    pub peak_heap_mb: f64,
    pub peak_rss_mb: f64,
    pub hot_path_allocations: u64,
    pub regimes: Vec<ThroughputRegime>,
}

/// Visão agregada para a UI.
#[derive(Clone, Debug, Default)]
pub struct ScorecardView {
    pub path: PathBuf,
    pub quotient: f64,
    pub real_coverage: f64,
    pub hot_path_bytes: f64,
    pub dimensions: Vec<DimCard>,
    pub throughput: Option<ThroughputView>,
    pub load_error: Option<String>,
}

#[derive(Deserialize)]
struct RawDim {
    id: String,
    name: String,
    score: f64,
    status: String,
    #[serde(default)]
    provenance: String,
    #[serde(default)]
    metrics: Value,
}

#[derive(Deserialize)]
struct RawScorecard {
    #[serde(default)]
    quotient: f64,
    #[serde(default)]
    real_coverage: f64,
    #[serde(default)]
    hot_path_bytes: f64,
    #[serde(default)]
    dimensions: Vec<RawDim>,
    #[serde(default)]
    throughput: Option<Value>,
}

fn parse_metrics(v: &Value) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            if let Some(n) = val.as_f64() {
                out.push((k.clone(), n));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn parse_throughput(v: &Value) -> ThroughputView {
    let peak_heap_mb = v.get("peak_heap_mb").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let peak_rss_mb = v.get("peak_rss_mb").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let hot_path_allocations = v
        .get("hot_path_allocations")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let mut regimes = Vec::new();
    if let Some(arr) = v.get("regimes").and_then(|x| x.as_array()) {
        for r in arr {
            regimes.push(ThroughputRegime {
                name: r
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?")
                    .to_string(),
                steps: r.get("steps").and_then(|x| x.as_u64()).unwrap_or(0),
                p50_us: r.get("p50_us").and_then(|x| x.as_f64()).unwrap_or(0.0),
                p95_us: r.get("p95_us").and_then(|x| x.as_f64()).unwrap_or(0.0),
                p99_us: r.get("p99_us").and_then(|x| x.as_f64()).unwrap_or(0.0),
                p99_9_us: r
                    .get("p99_9_us")
                    .or_else(|| r.get("p99.9_us"))
                    .and_then(|x| x.as_f64())
                    .unwrap_or(0.0),
                cycles_per_s: r
                    .get("cycles_per_s")
                    .and_then(|x| x.as_f64())
                    .unwrap_or(0.0),
                sop_per_s: r.get("sop_per_s").and_then(|x| x.as_f64()).unwrap_or(0.0),
                eq_t_per_s: r
                    .get("eq_t_per_s")
                    .and_then(|x| x.as_f64())
                    .unwrap_or(0.0),
                hot_path_allocations: r
                    .get("hot_path_allocations")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0),
            });
        }
    }
    ThroughputView {
        peak_heap_mb,
        peak_rss_mb,
        hot_path_allocations,
        regimes,
    }
}

/// Caminhos candidatos (cwd e relativos ao exe).
pub fn candidate_paths() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("target/aether_scorecard.json"),
        PathBuf::from("aether_scorecard.json"),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        v.push(cwd.join("target/aether_scorecard.json"));
    }
    v
}

/// Carrega o primeiro scorecard encontrado.
pub fn load_scorecard(explicit: Option<&Path>) -> Result<ScorecardView> {
    let paths: Vec<PathBuf> = if let Some(p) = explicit {
        vec![p.to_path_buf()]
    } else {
        candidate_paths()
    };
    let mut last_err = String::from("scorecard nao encontrado");
    for path in paths {
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<RawScorecard>(&text) {
                Ok(raw) => {
                    let dimensions = raw
                        .dimensions
                        .into_iter()
                        .map(|d| DimCard {
                            id: d.id,
                            name: d.name,
                            score: d.score,
                            status: d.status,
                            provenance: d.provenance,
                            metrics: parse_metrics(&d.metrics),
                        })
                        .collect();
                    let throughput = raw.throughput.as_ref().map(parse_throughput);
                    return Ok(ScorecardView {
                        path,
                        quotient: raw.quotient,
                        real_coverage: raw.real_coverage,
                        hot_path_bytes: raw.hot_path_bytes,
                        dimensions,
                        throughput,
                        load_error: None,
                    });
                }
                Err(e) => last_err = format!("{}: parse: {e}", path.display()),
            },
            Err(e) => last_err = format!("{}: {e}", path.display()),
        }
    }
    Err(AetherError::Config(last_err))
}

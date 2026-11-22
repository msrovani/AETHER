//! Placar F4 no hub: lê o artefato medido e aponta a v3 sem inventar número.
//!
//! Fonte: `data/embed_eval_last.json` (set congelado + gate). A v3 sancionada
//! (MSG 1088) só aparece como placar se existir `target/embed_eval_*.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Gate documental quando o JSON antigo não traz `hit5_min`.
const GATE_HIT5_FALLBACK: f64 = 0.974;

/// Sanção V3 (MSG 1088). Histórico — a fila foi esgotada.
pub const V3_SANCTION: &str = "V3-A sancionada (MSG 1088): Qwen3-Embedding-0.6B vs BAAI/bge-m3 no set v2. O harness mede só a cabeça densa.";

/// Decisão HITL (MSG 1093): F4 congelada; gate intacto; cadeira fechada.
pub const F4_FREEZE: &str = "HITL MSG 1093: F4 CONGELADA REPROVADA · gate 0.974 intacto · V3 esgotada · cadeira fechada · melhor placar Qwen3-Reranker-0.6B 0.9577 · sem nova medição sem sanção";

/// Leitura do último eval F4 para o cartão do Lab.
#[derive(Clone, Debug)]
pub struct F4View {
    pub present: bool,
    pub path: PathBuf,
    pub error: Option<String>,
    pub sha_short: String,
    pub n_docs: u32,
    pub n_queries: u32,
    pub dim: u32,
    pub bm25_hit5: f64,
    pub embed_hit5: f64,
    pub hybrid_hit5: f64,
    pub gate_hit5: f64,
    pub hybrid_pass: bool,
    pub misses: Vec<String>,
    pub v3_files: Vec<String>,
}

impl Default for F4View {
    fn default() -> Self {
        Self {
            present: false,
            path: PathBuf::new(),
            error: None,
            sha_short: String::new(),
            n_docs: 0,
            n_queries: 0,
            dim: 0,
            bm25_hit5: 0.0,
            embed_hit5: 0.0,
            hybrid_hit5: 0.0,
            gate_hit5: GATE_HIT5_FALLBACK,
            hybrid_pass: false,
            misses: Vec::new(),
            v3_files: Vec::new(),
        }
    }
}

#[derive(Default, Deserialize)]
struct RawF4 {
    #[serde(default)]
    sha256_set: String,
    #[serde(default)]
    n_docs: u32,
    #[serde(default)]
    n_queries: u32,
    #[serde(default)]
    dim: u32,
    #[serde(default)]
    systems: RawSystems,
    #[serde(default)]
    gate: RawGate,
}

#[derive(Default, Deserialize)]
struct RawSystems {
    #[serde(default)]
    lexical_bm25: RawHit,
    #[serde(default)]
    embed_minilm: RawHit,
    #[serde(default)]
    hybrid_rrf60: RawHit,
}

#[derive(Default, Deserialize)]
struct RawHit {
    #[serde(default)]
    hit5: f64,
    #[serde(default)]
    misses: Vec<String>,
}

#[derive(Default, Deserialize)]
struct RawGate {
    #[serde(default)]
    hit5_min: f64,
    #[serde(default)]
    hybrid_pass: bool,
}

fn workspace_root() -> PathBuf {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(m);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Carrega o placar v2 e lista artefatos v3 ainda não promovidos a `last`.
pub fn load_f4_view() -> F4View {
    let root = workspace_root();
    let path = root.join("data/embed_eval_last.json");
    let v3_files = list_v3_artifacts(&root.join("target"));
    match fs::read_to_string(&path) {
        Ok(text) => match parse_f4(&text) {
            Ok(mut view) => {
                view.present = true;
                view.path = path;
                view.v3_files = v3_files;
                view
            }
            Err(e) => F4View {
                path,
                error: Some(e),
                v3_files,
                ..F4View::default()
            },
        },
        Err(e) => F4View {
            path,
            error: Some(e.to_string()),
            v3_files,
            ..F4View::default()
        },
    }
}

fn list_v3_artifacts(dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return lines;
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|ent| ent.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with("embed_eval_")
                && name.ends_with(".json")
                && !name.contains("bm25_baseline")
        })
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("embed_eval")
            .trim_start_matches("embed_eval_")
            .to_string();
        let text = fs::read_to_string(&path).unwrap_or_default();
        lines.push(summarize_v3_text(&name, &text));
    }
    lines
}

fn summarize_v3_text(name: &str, text: &str) -> String {
    let Ok(v) = parse_f4(text) else {
        return format!("{name} ilegível");
    };
    if let Some(rr) = rerank_hit5(text) {
        let verdict = if rr >= v.gate_hit5 {
            "PASSOU"
        } else {
            "REPROVADO"
        };
        return format!("{name} denso {:.4} rerank {rr:.4} {verdict}", v.embed_hit5);
    }
    let verdict = if v.hybrid_pass && v.embed_hit5 >= v.gate_hit5 {
        "PASSOU"
    } else {
        "REPROVADO"
    };
    format!(
        "{name} embed {:.4} híbrido {:.4} {verdict}",
        v.embed_hit5, v.hybrid_hit5
    )
}

fn rerank_hit5(text: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    v.get("systems")?.get("rerank")?.get("hit5")?.as_f64()
}

/// Interpreta o JSON do `embed_eval.py --run`.
pub fn parse_f4(text: &str) -> Result<F4View, String> {
    let raw: RawF4 = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let gate = if raw.gate.hit5_min > 0.0 {
        raw.gate.hit5_min
    } else {
        GATE_HIT5_FALLBACK
    };
    let sha_short = raw.sha256_set.chars().take(8).collect();
    Ok(F4View {
        present: true,
        path: PathBuf::new(),
        error: None,
        sha_short,
        n_docs: raw.n_docs,
        n_queries: raw.n_queries,
        dim: raw.dim,
        bm25_hit5: raw.systems.lexical_bm25.hit5,
        embed_hit5: raw.systems.embed_minilm.hit5,
        hybrid_hit5: raw.systems.hybrid_rrf60.hit5,
        gate_hit5: gate,
        hybrid_pass: raw.gate.hybrid_pass,
        misses: raw.systems.hybrid_rrf60.misses,
        v3_files: Vec::new(),
    })
}

/// Texto da coluna Detalhe.
pub fn f4_detail(v: &F4View) -> String {
    if let Some(err) = &v.error {
        return format!("F4: não leu {}\n{err}\n{F4_FREEZE}", v.path.display());
    }
    if !v.present {
        return format!("Sem data/embed_eval_last.json.\n{F4_FREEZE}");
    }
    let verdict = if v.hybrid_pass { "PASSOU" } else { "REPROVADO" };
    let misses = if v.misses.is_empty() {
        "—".into()
    } else {
        v.misses.join(", ")
    };
    let v3 = if v.v3_files.is_empty() {
        format!("{V3_SANCTION} Sem artefato em target/.")
    } else {
        format!("Artefatos v3: {}", v.v3_files.join(", "))
    };
    format!(
        "{F4_FREEZE}\nF4 v2 · embed {dim}d · set {sha}\n{docs} docs · {queries} consultas\nBM25 {bm:.4} · embed {em:.4} · híbrido {hy:.4}\ngate {gate:.3} · {verdict}\nmisses híbrido: {misses}\n{v3}",
        dim = v.dim,
        sha = v.sha_short,
        docs = v.n_docs,
        queries = v.n_queries,
        bm = v.bm25_hit5,
        em = v.embed_hit5,
        hy = v.hybrid_hit5,
        gate = v.gate_hit5,
        misses = misses,
    )
}

/// Uma linha para o cartão Programa.
pub fn f4_program_line(v: &F4View) -> String {
    if !v.present {
        return "F4 CONGELADA · sem artefato v2".into();
    }
    format!(
        "F4 CONGELADA · v2 híbrido {:.4} · gate {:.3} · cadeira fechada",
        v.hybrid_hit5, v.gate_hit5
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_v2_reprovado_e_lista_misses() {
        let raw = r#"{
            "sha256_set": "c422a1a00fc18c93",
            "n_docs": 722,
            "n_queries": 142,
            "dim": 384,
            "systems": {
                "lexical_bm25": {"hit5": 0.8662, "misses": ["f01"]},
                "embed_minilm": {"hit5": 0.6761, "misses": []},
                "hybrid_rrf60": {"hit5": 0.9225, "misses": ["f08", "d115"]}
            },
            "gate": {"hit5_min": 0.974, "hybrid_pass": false, "embed_pass": false}
        }"#;
        let v = parse_f4(raw).expect("json");
        assert_eq!(v.sha_short, "c422a1a0");
        assert!((v.hybrid_hit5 - 0.9225).abs() < 1e-9);
        assert!(!v.hybrid_pass);
        assert_eq!(v.misses, vec!["f08".to_string(), "d115".to_string()]);
        assert!(f4_program_line(&v).contains("CONGELADA"));
        assert!(f4_program_line(&v).contains("cadeira fechada"));
        assert!(f4_detail(&v).contains("MSG 1093"));
    }

    #[test]
    fn resumo_v3b_mostra_rerank_e_nao_hibrido_zero() {
        let raw = r#"{
            "systems": {
                "embed_minilm": {"hit5": 0.9366, "misses": []},
                "rerank": {"hit5": 0.9437, "misses": []}
            },
            "gate": {"hit5_min": 0.974, "rerank_pass": false}
        }"#;
        let line = summarize_v3_text("bge_rerank_v2", raw);
        assert!(line.contains("0.9437"));
        assert!(line.contains("REPROVADO"));
        assert!(!line.contains("híbrido"));
    }
}

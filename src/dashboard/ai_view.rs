//! Visões da aba Testes IA: API eval + agent drive + gaps de lab.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::dashboard::gaps::{Gap, GapSeverity};

/// Uma condição de usabilidade da API (exact / paráfrase / escopo…).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ApiEvalCase {
    pub id: String,
    pub title: String,
    pub hits: u64,
    pub total: u64,
    /// `hit` = deve acertar; `abstain` = deve abster.
    pub expect: String,
    pub pass: bool,
    #[serde(default)]
    pub note: String,
}

/// Relatório agregado de `target/aether_api_eval.json`.
#[derive(Clone, Debug, Default)]
pub struct ApiEvalReport {
    pub cases: Vec<ApiEvalCase>,
    pub present: bool,
    pub path: PathBuf,
    pub cue_p50_us: u64,
    pub cue_p99_us: u64,
}

/// Um ask do test drive do agente.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentAskRow {
    pub subject: String,
    pub relation: String,
    pub action: String,
    pub exit: i32,
    #[serde(default)]
    pub detail: String,
}

/// Artefato `target/aether_agent_drive.json`.
#[derive(Clone, Debug, Default)]
pub struct AgentDriveReport {
    pub asks: Vec<AgentAskRow>,
    pub present: bool,
    pub path: PathBuf,
    pub sgdb: String,
}

#[derive(Deserialize)]
struct RawApiEval {
    #[serde(default)]
    cases: Vec<ApiEvalCase>,
    #[serde(default)]
    cue_p50_us: u64,
    #[serde(default)]
    cue_p99_us: u64,
}

#[derive(Deserialize)]
struct RawAgentDrive {
    #[serde(default)]
    asks: Vec<AgentAskRow>,
    #[serde(default)]
    sgdb: String,
}

fn workspace_root() -> PathBuf {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(m);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Caminhos dos artefatos da aba Testes IA.
pub fn ai_artifact_paths() -> (PathBuf, PathBuf) {
    let root = workspace_root();
    (
        root.join("target/aether_api_eval.json"),
        root.join("target/aether_agent_drive.json"),
    )
}

/// Carrega API eval (faltas → present=false).
pub fn load_api_eval() -> ApiEvalReport {
    let (path, _) = ai_artifact_paths();
    match read_api(&path) {
        Ok(mut r) => {
            r.present = true;
            r.path = path;
            r
        }
        Err(_) => ApiEvalReport {
            path,
            ..Default::default()
        },
    }
}

/// Carrega agent drive.
pub fn load_agent_drive() -> AgentDriveReport {
    let (_, path) = ai_artifact_paths();
    match read_agent(&path) {
        Ok(mut r) => {
            r.present = true;
            r.path = path;
            r
        }
        Err(_) => AgentDriveReport {
            path,
            ..Default::default()
        },
    }
}

fn read_api(path: &Path) -> Result<ApiEvalReport, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let raw: RawApiEval = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(ApiEvalReport {
        cases: raw.cases,
        present: true,
        path: path.to_path_buf(),
        cue_p50_us: raw.cue_p50_us,
        cue_p99_us: raw.cue_p99_us,
    })
}

fn read_agent(path: &Path) -> Result<AgentDriveReport, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let raw: RawAgentDrive = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(AgentDriveReport {
        asks: raw.asks,
        present: true,
        path: path.to_path_buf(),
        sgdb: raw.sgdb,
    })
}

/// Gaps heurísticos da onda 0.0.169 (lab + IA).
pub fn collect_wave_gaps() -> Vec<Gap> {
    let root = workspace_root();
    let mut out = Vec::new();
    if !root.join("data/rounds/20261117_b1/manifest.json").exists() {
        out.push(Gap::new(
            "ROUND_B1_MISSING",
            GapSeverity::Warn,
            "Rodada B1 ausente",
            "data/rounds/20261117_b1/manifest.json não encontrado.",
            "Rode o harvest B1 ou restaure a rodada arquivada.",
        ));
    }
    let sgdb_set = std::env::var("AETHER_SGDB_STDIO")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || std::env::var("AETHER_SGDB_URL")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
    if !sgdb_set {
        out.push(Gap::new(
            "SGDB_STDIO_UNSET",
            GapSeverity::Info,
            "SGDB não apontado no ambiente",
            "AETHER_SGDB_STDIO / AETHER_SGDB_URL vazios nesta sessão.",
            "Defina stdio:…/mcp_server.exe para o test drive falar com memória real.",
        ));
    }
    let escriba = PathBuf::from(r"C:\Users\msrov\.cache\aether\donors\qwen2_5-7b-instruct-gguf");
    if escriba.exists() {
        out.push(Gap::new(
            "ESCRIBA_READY_OFFLINE",
            GapSeverity::Info,
            "Escriba 7B pronto (offline)",
            "GGUF q4_k_m fora do crate; batch CPU; autoridade 60–120.",
            "Não entra no hub nem no T0. Use harvest/ETL offline.",
        ));
    }
    if !cfg!(feature = "api") {
        out.push(Gap::new(
            "API_FEATURE_OFF",
            GapSeverity::Info,
            "Feature api off neste binário",
            "O hub foi compilado sem --features api; o job ApiEval precisa dela.",
            "Recompile o dashboard com features dashboard,api ou rode o job via cargo test.",
        ));
    }
    out
}

/// Texto de detalhe de um caso API.
pub fn api_case_detail(c: &ApiEvalCase) -> String {
    format!(
        "{} — {}\nexpect={}  hits={}/{}  pass={}\n{}",
        c.id, c.title, c.expect, c.hits, c.total, c.pass, c.note
    )
}

/// Texto de detalhe de um ask do agente.
pub fn agent_ask_detail(a: &AgentAskRow) -> String {
    format!(
        "{} | {}\naction={}  exit={}\n{}",
        a.subject, a.relation, a.action, a.exit, a.detail
    )
}

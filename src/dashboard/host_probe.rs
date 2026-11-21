//! Sonda da máquina hospedeira + gaps heurísticos.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::affinity::L1D_BYTES;
use crate::core::core_topology::CorePlan;
use crate::dashboard::gaps::{Gap, GapSeverity};
use crate::dashboard::model::ScorecardView;
use crate::error::{AetherError, Result};

/// Relatório de hardware / ambiente.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostReport {
    pub logical_cpus: usize,
    pub physical_cpus: usize,
    pub reflex_core: Option<usize>,
    pub reserved_siblings: Vec<usize>,
    pub ram_total_mb: Option<f64>,
    pub ram_available_mb: Option<f64>,
    /// VRAM livre (MiB) via `nvidia-smi` quando houver GPU NVIDIA.
    #[serde(default)]
    pub vram_free_mb: Option<u64>,
    /// VRAM total (MiB).
    #[serde(default)]
    pub vram_total_mb: Option<u64>,
    /// Nome da GPU (primeiro adaptador).
    #[serde(default)]
    pub gpu_name: Option<String>,
    pub arch: String,
    pub os: String,
    pub affinity_feature: bool,
    pub jit_feature: bool,
    pub hw_entropy_feature: bool,
    pub mingw_on_path: bool,
    pub cargo_on_path: bool,
    pub l1d_budget_bytes: usize,
    pub gaps: Vec<GapDto>,
}

/// Gap serializável.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GapDto {
    pub code: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub remediation: String,
}

impl From<&Gap> for GapDto {
    fn from(g: &Gap) -> Self {
        Self {
            code: g.code.to_string(),
            severity: g.severity.label().to_string(),
            title: g.title.clone(),
            detail: g.detail.clone(),
            remediation: g.remediation.clone(),
        }
    }
}

/// Lê uma propriedade CIM via PowerShell (WMIC removido em Windows recentes).
fn cim_number(class: &str, prop: &str) -> Option<f64> {
    let script = format!("(Get-CimInstance -ClassName {class}).{prop} | Select-Object -First 1");
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .and_then(|v| v.parse().ok())
}

fn ram_windows() -> (Option<f64>, Option<f64>) {
    // Sem FFI novo: CIM via PowerShell (sem unsafe no dashboard).
    let total =
        cim_number("Win32_ComputerSystem", "TotalPhysicalMemory").map(|n| n / (1024.0 * 1024.0));
    let avail = cim_number("Win32_OperatingSystem", "FreePhysicalMemory").map(|n| n / 1024.0); // KB → MB
    (total, avail)
}

fn cpu_counts_cim() -> (Option<usize>, Option<usize>) {
    let logical =
        cim_number("Win32_ComputerSystem", "NumberOfLogicalProcessors").map(|n| n as usize);
    let physical = cim_number("Win32_Processor", "NumberOfCores").map(|n| n as usize);
    (logical, physical)
}

fn on_path(exe: &str) -> bool {
    std::process::Command::new("where")
        .arg(exe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// NVIDIA VRAM via `nvidia-smi` (user final / scribe policy). Ausente → None.
fn nvidia_vram() -> (Option<String>, Option<u64>, Option<u64>) {
    let Ok(out) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return (None, None, None);
    };
    if !out.status.success() {
        return (None, None, None);
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let Some(line) = line.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return (None, None, None);
    };
    let mut parts = line.split(',').map(str::trim);
    let name = parts.next().map(str::to_string);
    let total = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|n| n as u64);
    let free = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|n| n as u64);
    (name, total, free)
}

/// Coleta gaps a partir do host + scorecard opcional.
pub fn collect_gaps(report: &HostReport, scorecard: Option<&ScorecardView>) -> Vec<Gap> {
    let mut gaps = Vec::new();

    if !report.cargo_on_path {
        gaps.push(Gap::new(
            "TOOLCHAIN_PATH",
            GapSeverity::Crit,
            "cargo não está no PATH",
            "Os botões de auditoria/benchmark/demo precisam invocar cargo ou binários release.",
            "Abra um shell onde `cargo --version` funciona (ou use scripts/cargo.ps1 com MinGW).",
        ));
    } else if !report.mingw_on_path {
        gaps.push(Gap::new(
            "TOOLCHAIN_PATH",
            GapSeverity::Warn,
            "dlltool/as (MinGW) podem faltar no PATH",
            "No Windows-gnu, crates windows-sys exigem binutils MinGW para linkar demos.",
            "Coloque mingw64/bin no PATH ou use .\\scripts\\cargo.ps1.",
        ));
    }

    if !report.affinity_feature {
        gaps.push(Gap::new(
            "AFFINITY_OFF",
            GapSeverity::Info,
            "Feature affinity desligada neste build",
            "Sem pinagem de núcleo a cauda p99.9 sob carga tende a piorar.",
            "Recompile o hub/demos com --features \"dashboard,affinity\" se quiser pinning.",
        ));
    }

    gaps.push(Gap::new(
        "L1D_PRESSURE",
        GapSeverity::Info,
        "Orçamento L1d reflexivo = 32 KiB",
        format!(
            "Stores holográficos BAM T12 (~52 KB) excedem {} B e residem em L2.",
            report.l1d_budget_bytes
        ),
        "Esperado: não é regressão; só tradeoff de cache no RAG.",
    ));

    match scorecard {
        None => gaps.push(Gap::new(
            "SCORECARD_MISSING",
            GapSeverity::Warn,
            "Scorecard ausente",
            "Não há target/aether_scorecard.json carregado.",
            "Clique em “Rodar auditoria cognitiva” ou rode cargo test --release --test cognitive_audit.",
        )),
        Some(sc) if sc.dimensions.len() < 12 => gaps.push(Gap::new(
            "PARTIAL_AUDIT",
            GapSeverity::Warn,
            "Auditoria incompleta",
            format!(
                "Só {} dimensão(ões) no scorecard (esperado 12).",
                sc.dimensions.len()
            ),
            "Rode a auditoria completa pelo botão do hub.",
        )),
        Some(sc) => {
            if let Some(tp) = &sc.throughput {
                if let Some(basal) = tp.regimes.iter().find(|r| r.name.eq_ignore_ascii_case("basal"))
                {
                    if basal.p99_9_us > 1000.0 {
                        gaps.push(Gap::new(
                            "TAIL_LATENCY_HIGH",
                            GapSeverity::Warn,
                            "p99.9 Basal acima de 1 ms",
                            format!(
                                "p99.9 = {:.1} µs — típico sob contenção do SO, não perda cognitiva.",
                                basal.p99_9_us
                            ),
                            "Feche builds paralelos; habilite affinity; compare com máquina quieta (~400 µs).",
                        ));
                    }
                }
            } else {
                gaps.push(Gap::new(
                    "SCORECARD_MISSING",
                    GapSeverity::Info,
                    "Sem bloco de vazão no scorecard",
                    "Throughput ainda não foi mesclado.",
                    "Clique em “Benchmark de vazão”.",
                ));
            }
        }
    }

    gaps
}

/// Caminho do relatório de host (respeita `CARGO_TARGET_DIR`).
pub fn host_report_path() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    target.join("aether_host_report.json")
}

/// Lê `target/aether_host_report.json` se existir (sem re-sondar).
pub fn load_host_report() -> Result<HostReport> {
    let path = host_report_path();
    let raw = fs::read_to_string(&path)
        .map_err(|e| AetherError::Config(format!("host_report read {}: {e}", path.display())))?;
    serde_json::from_str(&raw).map_err(|e| AetherError::Config(format!("host_report parse: {e}")))
}

/// Executa a sonda e grava `target/aether_host_report.json`.
pub fn probe_and_save(scorecard: Option<&ScorecardView>) -> Result<HostReport> {
    let plan = CorePlan::detect();
    let (cim_logical, cim_physical) = cpu_counts_cim();
    let logical = if plan.logical > 0 {
        plan.logical
    } else if let Some(n) = cim_logical {
        n
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    };
    let physical = if plan.physical > 0 {
        plan.physical
    } else if let Some(n) = cim_physical {
        n
    } else {
        logical
    };
    let (ram_total_mb, ram_available_mb) = ram_windows();
    let (gpu_name, vram_total_mb, vram_free_mb) = nvidia_vram();

    let mut report = HostReport {
        logical_cpus: logical,
        physical_cpus: physical,
        reflex_core: plan.reflex,
        reserved_siblings: plan.reserved_siblings.clone(),
        ram_total_mb,
        ram_available_mb,
        vram_free_mb,
        vram_total_mb,
        gpu_name,
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        affinity_feature: cfg!(feature = "affinity"),
        jit_feature: cfg!(feature = "jit"),
        hw_entropy_feature: cfg!(feature = "hw-entropy"),
        mingw_on_path: on_path("dlltool.exe") || on_path("dlltool"),
        cargo_on_path: on_path("cargo.exe") || on_path("cargo"),
        l1d_budget_bytes: L1D_BYTES,
        gaps: Vec::new(),
    };

    let gaps = collect_gaps(&report, scorecard);
    report.gaps = gaps.iter().map(GapDto::from).collect();

    let path = host_report_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| AetherError::Config(format!("host_report serialize: {e}")))?;
    fs::write(&path, json).map_err(|e| AetherError::Config(format!("host_report write: {e}")))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_writes_report_with_gaps_or_ok() {
        let report = probe_and_save(None).expect("probe");
        assert!(report.logical_cpus >= 1);
        assert!(
            !report.gaps.is_empty() || report.cargo_on_path && report.affinity_feature,
            "esperado >=1 gap heurístico ou ambiente limpo"
        );
        assert!(host_report_path().is_file());
    }
}

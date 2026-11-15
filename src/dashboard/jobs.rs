//! Runner single-flight de jobs do hub (auditoria, bench, demos).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dashboard::host_probe;
use crate::dashboard::model::ScorecardView;

/// Tipo de job disparável pelos botões.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Audit,
    Bench,
    HostProbe,
    OrganismDemo,
    DaemonDemo,
}

impl JobKind {
    /// Rótulo curto para UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Audit => "Auditoria cognitiva",
            Self::Bench => "Benchmark de vazão",
            Self::HostProbe => "Sondar hardware",
            Self::OrganismDemo => "Demo organismo",
            Self::DaemonDemo => "Demo daemon",
        }
    }
}

/// Fase do job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobPhase {
    Idle,
    Running,
    Done,
    Failed,
}

impl JobPhase {
    /// Rótulo pt-BR.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }
}

/// Snapshot polled pela UI.
#[derive(Clone, Debug)]
pub struct JobSnapshot {
    pub phase: JobPhase,
    pub kind: Option<JobKind>,
    pub log: String,
}

impl Default for JobSnapshot {
    fn default() -> Self {
        Self {
            phase: JobPhase::Idle,
            kind: None,
            log: String::new(),
        }
    }
}

struct Shared {
    snap: Mutex<JobSnapshot>,
    cancel: AtomicBool,
}

/// Fila single-flight: no máximo 1 job por vez.
pub struct JobRunner {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for JobRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRunner")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

impl Default for JobRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl JobRunner {
    /// Cria runner ocioso.
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                snap: Mutex::new(JobSnapshot::default()),
                cancel: AtomicBool::new(false),
            }),
            worker: Mutex::new(None),
        }
    }

    /// Estado atual (cópia).
    pub fn snapshot(&self) -> JobSnapshot {
        self.shared
            .snap
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// True se um job está em execução.
    pub fn is_busy(&self) -> bool {
        self.snapshot().phase == JobPhase::Running
    }

    /// Pede cancelamento cooperativo (mata o processo filho).
    pub fn request_cancel(&self) {
        self.shared.cancel.store(true, Ordering::SeqCst);
    }

    /// Dispara job se idle. `scorecard` só usado pelo HostProbe.
    pub fn try_start(&self, kind: JobKind, scorecard: Option<ScorecardView>) -> bool {
        {
            let mut guard = match self.worker.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            // Limpa join handle morto.
            if let Some(h) = guard.as_ref() {
                if !h.is_finished() {
                    return false;
                }
            }
            *guard = None;

            let mut snap = match self.shared.snap.lock() {
                Ok(s) => s,
                Err(_) => return false,
            };
            if snap.phase == JobPhase::Running {
                return false;
            }
            self.shared.cancel.store(false, Ordering::SeqCst);
            *snap = JobSnapshot {
                phase: JobPhase::Running,
                kind: Some(kind),
                log: format!("iniciando {}…", kind.label()),
            };
        }

        let shared = Arc::clone(&self.shared);
        let handle = thread::spawn(move || {
            let result = match kind {
                JobKind::Audit => run_cargo(
                    &["test", "--release", "--test", "cognitive_audit", "--", "--nocapture"],
                    Duration::from_secs(900),
                    &shared,
                ),
                JobKind::Bench => run_cargo(
                    &["bench", "--bench", "throughput_tps", "--features", "m4"],
                    Duration::from_secs(900),
                    &shared,
                ),
                JobKind::HostProbe => match host_probe::probe_and_save(scorecard.as_ref()) {
                    Ok(r) => Ok(format!(
                        "host OK: {} lógico(s), {} físico(s), {} gap(s) → target/aether_host_report.json",
                        r.logical_cpus,
                        r.physical_cpus,
                        r.gaps.len()
                    )),
                    Err(e) => Err(e.to_string()),
                },
                JobKind::OrganismDemo => run_bin_or_cargo(
                    "aether-organism",
                    &["--duration", "15s", "--plain"],
                    &["run", "--release", "--bin", "aether-organism", "--", "--duration", "15s", "--plain"],
                    Duration::from_secs(120),
                    &shared,
                ),
                JobKind::DaemonDemo => run_bin_or_cargo(
                    "aether-daemon",
                    &["--duration", "10s"],
                    &["run", "--release", "--bin", "aether-daemon", "--", "--duration", "10s"],
                    Duration::from_secs(90),
                    &shared,
                ),
            };
            let mut snap = match shared.snap.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            match result {
                Ok(msg) => {
                    snap.phase = JobPhase::Done;
                    snap.log = truncate_line(&msg, 220);
                }
                Err(msg) => {
                    snap.phase = JobPhase::Failed;
                    snap.log = truncate_line(&msg, 220);
                }
            }
        });

        if let Ok(mut g) = self.worker.lock() {
            *g = Some(handle);
        }
        true
    }
}

fn truncate_line(s: &str, max: usize) -> String {
    let one = s.lines().last().unwrap_or(s).trim();
    let mut t: String = one.chars().take(max).collect();
    if one.chars().count() > max {
        t.push('…');
    }
    t
}

fn set_log(shared: &Shared, msg: impl Into<String>) {
    if let Ok(mut s) = shared.snap.lock() {
        s.log = msg.into();
    }
}

fn workspace_root() -> PathBuf {
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        return PathBuf::from(m);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn release_bin(name: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"));
    target_root.join("release").join(exe)
}

fn run_bin_or_cargo(
    bin: &str,
    bin_args: &[&str],
    cargo_args: &[&str],
    timeout: Duration,
    shared: &Shared,
) -> Result<String, String> {
    let path = release_bin(bin);
    if path.is_file() {
        set_log(shared, format!("executando {}…", path.display()));
        let mut cmd = Command::new(&path);
        cmd.args(bin_args)
            .current_dir(workspace_root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        return wait_cmd(cmd, timeout, shared, bin);
    }
    set_log(shared, format!("{bin} release ausente — cargo run…"));
    run_cargo(cargo_args, timeout, shared)
}

fn run_cargo(args: &[&str], timeout: Duration, shared: &Shared) -> Result<String, String> {
    set_log(shared, format!("cargo {}…", args.join(" ")));
    let mut cmd = Command::new("cargo");
    cmd.args(args)
        .current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    wait_cmd(cmd, timeout, shared, "cargo")
}

fn wait_cmd(
    mut cmd: Command,
    timeout: Duration,
    shared: &Shared,
    label: &str,
) -> Result<String, String> {
    use std::io::Read;

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("falha ao spawn {label}: {e}"))?;
    let start = Instant::now();
    loop {
        if shared.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{label} cancelado"));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                if let Some(ref mut s) = child.stdout {
                    let _ = s.read_to_string(&mut out);
                }
                if let Some(ref mut s) = child.stderr {
                    let _ = s.read_to_string(&mut err);
                }
                let combined = if out.is_empty() { err } else { out };
                if status.success() {
                    let tail = combined
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("ok");
                    return Ok(format!("{label} ok — {tail}"));
                }
                let tail = combined
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("erro");
                return Err(format!(
                    "{label} exit {} — {tail}",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{label} timeout ({timeout:?})"));
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(format!("{label} wait: {e}")),
        }
    }
}

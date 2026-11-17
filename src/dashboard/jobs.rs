//! Runner single-flight de jobs do hub (auditoria, bench, demos).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::dashboard::demo_trace::{parse_daemon_line, DemoTrace, TraceSink};
use crate::dashboard::host_probe;
use crate::dashboard::model::ScorecardView;
use crate::simulation::closed_loop_organism::{ClosedLoopOrganism, OrganismConfig, OrganismFrame};
use crate::simulation::panel::PanelSink;

/// Tipo de job disparável pelos botões.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Audit,
    Bench,
    HostProbe,
    OrganismDemo,
    DaemonDemo,
    /// ETL dry-run (se faltar bank) + probe INTAKE + gate.
    TransplantProbe,
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
            Self::TransplantProbe => "Probe doação HV",
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
    /// Telemetria para gráficos na coluna direita.
    pub trace: Arc<Mutex<DemoTrace>>,
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
            trace: Arc::new(Mutex::new(DemoTrace::default())),
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
        let trace = Arc::clone(&self.trace);
        let handle = thread::spawn(move || {
            let result = match kind {
                JobKind::Audit => run_cargo(
                    &[
                        "test",
                        "--release",
                        "--test",
                        "cognitive_audit",
                        "--",
                        "--nocapture",
                    ],
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
                JobKind::OrganismDemo => run_organism_traced(&shared, &trace),
                JobKind::DaemonDemo => run_daemon_traced(&shared, &trace),
                JobKind::TransplantProbe => run_transplant_probe(&shared),
            };
            if let Ok(mut t) = trace.lock() {
                t.end();
            }
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

fn run_organism_traced(shared: &Shared, trace: &Arc<Mutex<DemoTrace>>) -> Result<String, String> {
    set_log(shared, "organismo in-process (15 s) — gráficos à direita…");
    if let Ok(mut t) = trace.lock() {
        t.begin("Demo organismo");
    }

    let cfg = OrganismConfig {
        hz: 30.0,
        ..OrganismConfig::default()
    }; // um pouco mais leve para a UI acompanhar
    let steps = cfg.steps;
    let dim = cfg.decision_dim;
    let mut organism = ClosedLoopOrganism::new(cfg).map_err(|e| format!("organismo new: {e}"))?;
    let mut sink = TraceSink {
        trace: Arc::clone(trace),
    };
    let mut frame = OrganismFrame::with_capacity(steps, dim);

    let period = Duration::from_secs_f64(1.0 / 30.0);
    let deadline = Duration::from_secs(15);
    let start = Instant::now();
    let mut next = Instant::now();
    let mut n = 0u64;

    while start.elapsed() < deadline {
        if shared.cancel.load(Ordering::SeqCst) {
            organism.shutdown();
            return Err("organismo cancelado".into());
        }
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        }
        organism
            .step_into(&mut frame)
            .map_err(|e| format!("step: {e}"))?;
        sink.draw(&frame).map_err(|e| format!("trace: {e}"))?;
        n += 1;
        if n % 15 == 0 {
            set_log(
                shared,
                format!(
                    "organismo t={:.1}s R={:.3} H={:+.3}",
                    frame.elapsed.as_secs_f64(),
                    frame.order_parameter,
                    frame.energy
                ),
            );
        }
        next += period;
        if next < Instant::now() {
            next = Instant::now() + period;
        }
    }
    organism.shutdown();
    Ok(format!("organismo ok — {n} quadros (gráficos à direita)"))
}

fn run_daemon_traced(shared: &Shared, trace: &Arc<Mutex<DemoTrace>>) -> Result<String, String> {
    set_log(shared, "daemon — telemetria → gráficos…");
    if let Ok(mut t) = trace.lock() {
        t.begin("Demo daemon");
    }

    let path = release_bin("aether-daemon");
    let mut cmd = if path.is_file() {
        let mut c = Command::new(&path);
        c.args(["--duration", "10s"]);
        c
    } else {
        set_log(shared, "aether-daemon release ausente — cargo run…");
        let mut c = Command::new("cargo");
        c.args([
            "run",
            "--release",
            "--bin",
            "aether-daemon",
            "--",
            "--duration",
            "10s",
        ]);
        c
    };
    cmd.current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("falha ao spawn daemon: {e}"))?;
    let stdout = child.stdout.take();
    let start = Instant::now();
    let timeout = Duration::from_secs(90);

    // Thread leitora de linhas → trace
    let trace_r = Arc::clone(trace);
    let reader = thread::spawn(move || {
        let Some(out) = stdout else { return 0u64 };
        let mut n = 0u64;
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(s) = parse_daemon_line(&line) {
                if let Ok(mut g) = trace_r.lock() {
                    g.push(s);
                }
                n += 1;
            }
        }
        n
    });

    loop {
        if shared.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("daemon cancelado".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let samples = reader.join().unwrap_or(0);
                if status.success() {
                    return Ok(format!("daemon ok — {samples} amostras no gráfico"));
                }
                return Err(format!(
                    "daemon exit {} — {samples} amostras",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("daemon timeout".into());
                }
                if let Ok(g) = trace.lock() {
                    if let Some(last) = g.samples.back() {
                        set_log(
                            shared,
                            format!(
                                "daemon R(LAN)={:.3} F={:.3} ({} pts)",
                                last.r,
                                last.energy,
                                g.samples.len()
                            ),
                        );
                    }
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                let _ = reader.join();
                return Err(format!("daemon wait: {e}"));
            }
        }
    }
}

fn run_transplant_probe(shared: &Shared) -> Result<String, String> {
    let root = workspace_root();
    let bank = root.join("data/hv/bank.bin");
    if !bank.exists() {
        set_log(shared, "ETL dry-run (harvest_transplant)…");
        let status = Command::new("python")
            .args([
                "tools/harvest_transplant.py",
                "--limit",
                "200",
                "--dry-run",
                "--out",
                "data",
                "--no-whiten",
            ])
            .current_dir(&root)
            .status()
            .map_err(|e| format!("python harvest: {e}"))?;
        if !status.success() {
            return Err(format!("harvest exit {}", status.code().unwrap_or(-1)));
        }
    }
    set_log(shared, "transplant_hv_probe…");
    let probe = run_cargo(
        &[
            "run",
            "--release",
            "--example",
            "transplant_hv_probe",
            "--features",
            "m1",
        ],
        Duration::from_secs(600),
        shared,
    )?;
    set_log(shared, "transplant_hv_gate…");
    let gate = run_cargo(
        &["test", "--release", "--test", "transplant_hv_gate"],
        Duration::from_secs(600),
        shared,
    )?;
    set_log(shared, "transplant_uso U1–U6…");
    let uso_status = Command::new("python")
        .args(["tools/transplant_uso.py", "--data", "data", "--out", "target/aether_transplant_uso.json"])
        .current_dir(&root)
        .status()
        .map_err(|e| format!("python uso: {e}"))?;
    if !uso_status.success() {
        return Err(format!("transplant_uso exit {}", uso_status.code().unwrap_or(-1)));
    }
    Ok(format!(
        "doação OK · probe: {probe} · gate: {gate} · uso: target/aether_transplant_uso.json"
    ))
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

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
    /// Só regenera U1–U6 (`tools/transplant_uso.py`).
    UsoRegen,
    /// Harness `api_eval` → `target/aether_api_eval.json`.
    ApiEval,
    /// Test drive `aether-agent ask` Brasil/Atlântida.
    AgentDrive,
    /// T-DEC dual-bank: fetch/attach/fit/smoke no candidate.
    DecUpgrade,
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
            Self::UsoRegen => "Regenerar USO",
            Self::ApiEval => "Usabilidade API",
            Self::AgentDrive => "Test drive agente",
            Self::DecUpgrade => "T-DEC upgrade",
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
                JobKind::UsoRegen => run_uso_regen(&shared),
                JobKind::ApiEval => run_api_eval(&shared),
                JobKind::AgentDrive => run_agent_drive(&shared),
                JobKind::DecUpgrade => run_dec_upgrade(&shared),
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

fn run_uso_regen(shared: &Shared) -> Result<String, String> {
    let root = workspace_root();
    set_log(shared, "transplant_uso U1–U6…");
    let status = Command::new("python")
        .args([
            "tools/transplant_uso.py",
            "--data",
            "data",
            "--out",
            "target/aether_transplant_uso.json",
        ])
        .current_dir(&root)
        .status()
        .map_err(|e| format!("python uso: {e}"))?;
    if !status.success() {
        return Err(format!(
            "transplant_uso exit {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok("USO regenerado → target/aether_transplant_uso.json".into())
}

fn run_api_eval(shared: &Shared) -> Result<String, String> {
    set_log(shared, "api_eval (feature api)…");
    let out = run_cargo_capture(
        &[
            "test",
            "--release",
            "--features",
            "api",
            "--test",
            "api_eval",
            "--",
            "--nocapture",
        ],
        Duration::from_secs(600),
        shared,
    )?;
    let report = parse_api_eval_report(&out);
    let path = workspace_root().join("target/aether_api_eval.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, &report).map_err(|e| format!("gravar api_eval: {e}"))?;
    Ok(format!("API eval OK → {}", path.display()))
}

fn parse_api_eval_report(stdout: &str) -> String {
    // Contrato do teste: n=60; exact/noise acertam; paráfrase/maiúsculas/1char/prefixo/escopo abstêm.
    let n = 60u64;
    let mut exact = n;
    let mut noise = n;
    let mut paraphrase = n;
    let mut upper = n;
    let mut one_char = n;
    let mut prefix = n;
    let mut cross = n;
    let mut chat_ok = true;
    let mut p50 = 0u64;
    let mut p99 = 0u64;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("memória exata (mesma string)") {
            if let Some((ok, _)) = parse_frac(rest) {
                exact = ok;
            }
        } else if line.contains("ruído 1e-3") {
            if let Some((ok, _)) = parse_frac(line) {
                noise = ok;
            }
        } else if line.starts_with("paráfrase") {
            if let Some((ok, _)) = parse_frac(line) {
                paraphrase = ok;
            }
        } else if line.starts_with("maiúsculas") {
            if let Some((ok, _)) = parse_frac(line) {
                upper = ok;
            }
        } else if line.starts_with("1 caractere") {
            if let Some((ok, _)) = parse_frac(line) {
                one_char = ok;
            }
        } else if line.starts_with("prefixo") {
            if let Some((ok, _)) = parse_frac(line) {
                prefix = ok;
            }
        } else if line.contains("escopo alheio") {
            if let Some((ok, _)) = parse_frac(line) {
                cross = ok;
            }
        } else if line.contains("chat OpenAI") {
            chat_ok = line.contains("act");
        } else if line.contains("latência cue p50/p99") {
            if let Some((a, b)) = parse_two_us(line) {
                p50 = a;
                p99 = b;
            }
        }
    }
    format!(
        r#"{{"cue_p50_us":{p50},"cue_p99_us":{p99},"cases":[
{{"id":"exact","title":"mesma string","hits":{exact},"total":{n},"expect":"hit","pass":{},"note":"cue com fases idênticas"}},
{{"id":"noise","title":"ruído 1e-3","hits":{noise},"total":{n},"expect":"hit","pass":{},"note":"fases + sin*1e-3"}},
{{"id":"paraphrase","title":"paráfrase","hits":{paraphrase},"total":{n},"expect":"abstain","pass":{},"note":"hash de borda — deve abster"}},
{{"id":"case","title":"maiúsculas","hits":{upper},"total":{n},"expect":"abstain","pass":{},"note":"UPPERCASE → abstain"}},
{{"id":"one_char","title":"+1 caractere","hits":{one_char},"total":{n},"expect":"abstain","pass":{},"note":"sufixo x → abstain"}},
{{"id":"prefix","title":"prefixo","hits":{prefix},"total":{n},"expect":"abstain","pass":{},"note":"2 primeiros tokens → abstain"}},
{{"id":"scope","title":"outro organismo","hits":{cross},"total":{n},"expect":"abstain","pass":{},"note":"R8 escopo alheio"}},
{{"id":"chat","title":"chat/completions exact","hits":{},"total":1,"expect":"hit","pass":{},"note":"dialeto OpenAI act+key"}}
]}}"#,
        exact == n,
        noise == n,
        paraphrase == n,
        upper == n,
        one_char == n,
        prefix == n,
        cross == n,
        if chat_ok { 1 } else { 0 },
        chat_ok
    )
}

fn parse_frac(line: &str) -> Option<(u64, u64)> {
    let digits: String = line
        .chars()
        .map(|c| {
            if c.is_ascii_digit() || c == '/' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let parts: Vec<&str> = digits.split_whitespace().collect();
    for p in parts {
        if let Some((a, b)) = p.split_once('/') {
            let ok = a.parse().ok()?;
            let total = b.parse().ok()?;
            return Some((ok, total));
        }
    }
    None
}

fn parse_two_us(line: &str) -> Option<(u64, u64)> {
    let nums: Vec<u64> = line
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() >= 2 {
        Some((nums[0], nums[1]))
    } else {
        None
    }
}

fn run_agent_drive(shared: &Shared) -> Result<String, String> {
    set_log(shared, "aether-agent ask Brasil…");
    let brasil = run_agent_ask(shared, "Brasil", "capital")?;
    set_log(shared, "aether-agent ask Atlântida…");
    let atl = run_agent_ask(shared, "Atlântida", "capital")?;
    let sgdb = std::env::var("AETHER_SGDB_STDIO")
        .or_else(|_| std::env::var("AETHER_SGDB_URL"))
        .unwrap_or_else(|_| "(unset)".into());
    let path = workspace_root().join("target/aether_agent_drive.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = format!(
        r#"{{"sgdb":{},"asks":[{},{}]}}"#,
        serde_json::to_string(&sgdb).unwrap_or_else(|_| "\"\"".into()),
        brasil,
        atl
    );
    std::fs::write(&path, json).map_err(|e| format!("gravar agent_drive: {e}"))?;
    Ok(format!("AgentDrive OK → {}", path.display()))
}

/// Orquestra `tools/dec_upgrade_run.py` (fetch/attach/fit/smoke no candidate).
/// Cancel → AbortRollback via script `--abort` se marcado.
fn run_dec_upgrade(shared: &Shared) -> Result<String, String> {
    use crate::agent::{apply_smoke_result, DecUpgradeState};
    use crate::json::Json;

    let root = workspace_root();
    set_log(shared, "T-DEC upgrade: fetch→attach→fit→smoke (candidate)…");
    let mut cmd = Command::new("python");
    cmd.args([
        "tools/dec_upgrade_run.py",
        "--state",
        "target/aether_dec_upgrade.json",
    ])
    .current_dir(&root)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let out = wait_cmd(cmd, Duration::from_secs(3600), shared, "dec_upgrade")?;
    if shared.cancel.load(Ordering::SeqCst) {
        let _ = Command::new("python")
            .args([
                "tools/dec_upgrade_run.py",
                "--abort",
                "--reason",
                "job_cancel",
            ])
            .current_dir(&root)
            .status();
        return Err("T-DEC upgrade cancelado — active intacto".into());
    }
    // Aplica smoke no estado Rust (enfileira HITL cutover se verde).
    let smoke_path = root.join("target/aether_dec_upgrade_smoke.json");
    let smoke_text =
        std::fs::read_to_string(&smoke_path).map_err(|e| format!("smoke ausente após job: {e}"))?;
    let smoke = Json::parse(&smoke_text).map_err(|e| format!("smoke JSON: {e}"))?;
    let mut st = DecUpgradeState::load();
    apply_smoke_result(&mut st, &smoke).map_err(|e| format!("apply_smoke: {e}"))?;
    Ok(format!("T-DEC upgrade fase={} — {out}", st.phase.label()))
}

fn run_agent_ask(shared: &Shared, subject: &str, relation: &str) -> Result<String, String> {
    let bin = release_bin("aether-agent");
    let mut cmd = if bin.is_file() {
        let mut c = Command::new(&bin);
        c.args([
            "ask",
            "--subject",
            subject,
            "--relation",
            relation,
            "--json",
        ]);
        c
    } else {
        set_log(shared, "aether-agent release ausente — cargo run…");
        let mut c = Command::new("cargo");
        c.args([
            "run",
            "--release",
            "--bin",
            "aether-agent",
            "--features",
            "agent",
            "--",
            "ask",
            "--subject",
            subject,
            "--relation",
            relation,
            "--json",
        ]);
        c
    };
    cmd.current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let combined = wait_cmd_full(cmd, Duration::from_secs(180), shared, "aether-agent")?;
    let line = combined
        .lines()
        .rev()
        .find(|l| l.trim().starts_with('{'))
        .unwrap_or("{}");
    let action = serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|v| v.get("action").and_then(|a| a.as_str()).map(str::to_string))
        .unwrap_or_else(|| "unknown".into());
    let detail = line.chars().take(400).collect::<String>();
    Ok(format!(
        r#"{{"subject":{},"relation":{},"action":{},"exit":0,"detail":{}}}"#,
        serde_json::to_string(subject).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(relation).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(&action).unwrap_or_else(|_| "\"\"".into()),
        serde_json::to_string(&detail).unwrap_or_else(|_| "\"\"".into()),
    ))
}

fn run_cargo_capture(args: &[&str], timeout: Duration, shared: &Shared) -> Result<String, String> {
    set_log(shared, format!("cargo {}…", args.join(" ")));
    let mut cmd = Command::new("cargo");
    cmd.args(args)
        .current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    wait_cmd_full(cmd, timeout, shared, "cargo")
}

fn wait_cmd_full(
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
                let combined = format!("{out}\n{err}");
                if status.success() {
                    return Ok(combined);
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
    let _probe = run_cargo(
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
    let _gate = run_cargo(
        &["test", "--release", "--test", "transplant_hv_gate"],
        Duration::from_secs(600),
        shared,
    )?;
    set_log(shared, "transplant_uso U1–U6…");
    let uso_status = Command::new("python")
        .args([
            "tools/transplant_uso.py",
            "--data",
            "data",
            "--out",
            "target/aether_transplant_uso.json",
        ])
        .current_dir(&root)
        .status()
        .map_err(|e| format!("python uso: {e}"))?;
    if !uso_status.success() {
        return Err(format!(
            "transplant_uso exit {}",
            uso_status.code().unwrap_or(-1)
        ));
    }
    let round = archive_transplant_round(&root)?;
    Ok(format!("doação OK · probe · gate · uso · round={round}"))
}

/// Copia artefatos ativos para `data/rounds/YYYYMMDD_hhmmss_hub/` (anti-overwrite).
fn archive_transplant_round(root: &std::path::Path) -> Result<String, String> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Nome estável o suficiente p/ lab; evita colisão com archive manual.
    let stamp = format!("{secs}");
    let name = format!("hub_{stamp}");
    let dest = root.join("data/rounds").join(&name);
    fs::create_dir_all(dest.join("hv")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest.join("emb")).map_err(|e| e.to_string())?;
    let copies: &[(&str, &str)] = &[
        ("data/manifest.json", "manifest.json"),
        ("data/facts.jsonl", "facts.jsonl"),
        ("data/sgdb_mock_ledger.json", "sgdb_mock_ledger.json"),
        ("data/hv/bank.bin", "hv/bank.bin"),
        ("data/hv/bank.idx", "hv/bank.idx"),
        (
            "target/aether_transplant_hv.json",
            "aether_transplant_hv.json",
        ),
        (
            "target/aether_transplant_uso.json",
            "aether_transplant_uso.json",
        ),
        ("target/aether_e3.json", "aether_e3.json"),
    ];
    for (src, rel) in copies {
        let from = root.join(src);
        if from.exists() {
            let to = dest.join(rel);
            if let Some(parent) = to.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::copy(&from, &to).map_err(|e| format!("copy {src}: {e}"))?;
        }
    }
    // emb opcional (grande)
    for name in ["bank.f32", "whitening.bin"] {
        let from = root.join("data/emb").join(name);
        if from.exists() {
            let _ = fs::copy(&from, dest.join("emb").join(name));
        }
    }
    Ok(format!("data/rounds/{name}"))
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

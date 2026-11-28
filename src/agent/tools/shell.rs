//! Shell — **off** por default (A4 / G-A4). K8: HITL + kill switch + allowlist.
//!
//! Env:
//! - `AETHER_SHELL` unset/`off` → deny (`NeedsHuman`)
//! - `AETHER_SHELL=hitl|1|on|allow` → exige token HITL + argv allowlist
//! - `AETHER_SHELL=stub` → igual hitl, mas `stub` não spawna processo
//! - `AETHER_SHELL_KILL=1` → kill switch duro (vence tudo)
//!
//! Token: `hitl_token` no JSON de `goal.query` deve casar com `AETHER_HITL_TOKEN`.
//! Execução: `Command` argv direto (sem `sh -c` / `cmd /c`); cwd = sandbox.

use crate::agent::hitl::{HitlKind, HitlRequest};
use crate::agent::tool::{
    Cost, Effect, Goal, GoalKind, Risk, Tool, ToolContext, ToolError, ToolOutcome, ToolSchema,
    WorldState, AUTHORITY_QUARANTINE,
};
use crate::agent::{Claim, ExpertId, WorldPolicy};
use crate::core::certificate::Provenance;
use crate::json::Json;
use std::path::PathBuf;

/// Cap de autoridade para claims de shell (quarentena; nunca 255 automático).
pub const SHELL_AUTH_CAP: u8 = 120;

/// Timeout do spawn real (R3): reusar `run_backend_full` (timeout+kill+cap de
/// stdout). `cargo test` em primeiro build pode demorar — 120 s é o teto
/// honesto; no estouro o filho é morto e a tool devolve `Err`, nunca hang.
pub const SHELL_TIMEOUT_MS: u64 = 120_000;

/// Modo do shell (K8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellMode {
    /// Desligado — default G-A4.
    Off,
    /// Kill switch duro.
    Kill,
    /// HITL + allowlist + spawn real.
    Hitl,
    /// HITL + allowlist; binário `stub` não spawna (CI/gate).
    Stub,
}

/// Lê kill switch + `AETHER_SHELL`.
pub fn shell_mode() -> ShellMode {
    if shell_kill_switch() {
        return ShellMode::Kill;
    }
    match std::env::var("AETHER_SHELL") {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" | "allow" | "hitl" => ShellMode::Hitl,
            "stub" => ShellMode::Stub,
            "kill" | "0" | "false" | "no" | "off" | "" => ShellMode::Off,
            _ => ShellMode::Off,
        },
        Err(_) => ShellMode::Off,
    }
}

/// Kill switch: `AETHER_SHELL_KILL=1|true|on|kill`.
pub fn shell_kill_switch() -> bool {
    match std::env::var("AETHER_SHELL_KILL") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "kill"
        ),
        Err(_) => false,
    }
}

fn hitl_token_expected() -> Option<String> {
    std::env::var("AETHER_HITL_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Binários permitidos sob HITL (sem shell metachar).
fn bin_allowed(bin: &str) -> bool {
    matches!(
        bin.to_ascii_lowercase().as_str(),
        "stub" | "cargo" | "rustc" | "python" | "python3" | "py"
    )
}

/// Subcomandos `cargo` permitidos (build real controlado).
fn cargo_subcmd_ok(args: &[&str]) -> bool {
    let Some(sub) = args.first().map(|s| s.to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        sub.as_str(),
        "check" | "test" | "build" | "clippy" | "--version" | "-V" | "version"
    )
}

fn parse_shell_query(raw: &str) -> Result<(Vec<String>, Option<String>), ToolError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(ToolError::InvalidArgs(
            "shell.exec: query JSON ausente (argv+hitl_token)".into(),
        ));
    }
    let v = Json::parse(text).map_err(|e| ToolError::InvalidArgs(format!("shell json: {e}")))?;
    let argv = v
        .get("argv")
        .or_else(|| v.get("cmd"))
        .ok_or_else(|| ToolError::InvalidArgs("shell: argv/cmd ausente".into()))?;
    let list = match argv {
        Json::Arr(items) => items
            .iter()
            .map(|j| {
                j.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| ToolError::InvalidArgs("shell: argv deve ser strings".into()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Json::Str(s) => s
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        _ => {
            return Err(ToolError::InvalidArgs(
                "shell: argv deve ser array ou string".into(),
            ))
        }
    };
    if list.is_empty() {
        return Err(ToolError::InvalidArgs("shell: argv vazio".into()));
    }
    let token = v
        .get("hitl_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    Ok((list, token))
}

fn needs_human_hitl(ctx: &ToolContext, reason: &str) -> ToolError {
    let req = HitlRequest {
        request_id: ctx.trace_id,
        kind: HitlKind::ShellExec,
        reason: reason.into(),
        from_authority: AUTHORITY_QUARANTINE,
        to_authority: SHELL_AUTH_CAP,
        subject: Some("shell.exec".into()),
        timeout_ms: 0,
    };
    ToolError::NeedsHuman(format!("shell.exec HITL request={}", req.to_json()))
}

fn claim(subj: impl Into<String>, val: impl Into<String>, conf: f32) -> Claim {
    Claim::new(
        subj,
        "shell.exec",
        val,
        conf,
        AUTHORITY_QUARANTINE.min(SHELL_AUTH_CAP),
        ExpertId::Memory,
        Provenance::Inferred,
    )
}

/// `shell.exec` — deny default; K8 HITL + kill switch.
#[derive(Debug)]
pub struct ShellTool {
    root: PathBuf,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self {
            root: WorldPolicy::default().sandbox_root,
        }
    }
}

impl ShellTool {
    /// Nova tool sob a sandbox do mundo.
    pub fn new(policy: &WorldPolicy) -> Self {
        Self {
            root: policy.sandbox_root.clone(),
        }
    }

    fn gate_and_argv(
        &self,
        goal: &Goal,
        ctx: &ToolContext,
    ) -> Result<(ShellMode, Vec<String>), ToolError> {
        let mode = shell_mode();
        if mode == ShellMode::Kill {
            return Err(ToolError::Failed(
                "shell.exec kill switch (AETHER_SHELL_KILL) — 0 execução".into(),
            ));
        }
        if mode == ShellMode::Off {
            return Err(needs_human_hitl(
                ctx,
                "shell.exec desligado: AETHER_SHELL=hitl|stub + token HITL (G-A4/K8)",
            ));
        }
        let raw = goal
            .query
            .as_deref()
            .ok_or_else(|| needs_human_hitl(ctx, "shell.exec exige query JSON + hitl_token"))?;
        let (argv, presented) = parse_shell_query(raw)?;
        let expected = hitl_token_expected().ok_or_else(|| {
            needs_human_hitl(
                ctx,
                "shell.exec: AETHER_HITL_TOKEN ausente no host — sem canal HITL",
            )
        })?;
        let Some(tok) = presented.as_deref() else {
            return Err(needs_human_hitl(
                ctx,
                "shell.exec: hitl_token ausente no query",
            ));
        };
        if !token_eq(tok.trim(), &expected) {
            return Err(needs_human_hitl(ctx, "shell.exec: hitl_token inválido"));
        }
        let bin = argv[0].as_str();
        if !bin_allowed(bin) {
            return Err(ToolError::InvalidArgs(format!(
                "shell.exec binário fora da allowlist: {bin}"
            )));
        }
        if bin.eq_ignore_ascii_case("cargo")
            && !cargo_subcmd_ok(&argv[1..].iter().map(|s| s.as_str()).collect::<Vec<_>>())
        {
            return Err(ToolError::InvalidArgs(
                "shell.exec cargo: só check|test|build|clippy|--version".into(),
            ));
        }
        // Metacharacters / path escape no argv.
        for a in &argv {
            if a.contains('|')
                || a.contains('&')
                || a.contains(';')
                || a.contains('>')
                || a.contains('<')
                || a.contains('\n')
                || a.contains('`')
                || a.contains('$')
            {
                return Err(ToolError::InvalidArgs(
                    "shell.exec: metacharacter proibido no argv".into(),
                ));
            }
        }
        Ok((mode, argv))
    }
}

impl Tool for ShellTool {
    fn name(&self) -> &'static str {
        "shell.exec"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "shell.exec",
            description: "K8: shell/build sob HITL+allowlist; off default; kill switch",
            args: &["query"],
            kinds: &[GoalKind::Act],
        }
    }

    fn cost(&self) -> Cost {
        // Stub HITL é barato; spawn real ainda cabe no budget default (2s).
        Cost::local(25.0)
    }

    fn risk(&self) -> Risk {
        // Hitl/Stub: Medium para o planner (High.penalty=1.5 mata o score).
        // Effect::Irreversible ainda exige risk_ceiling=High no executor.
        match shell_mode() {
            ShellMode::Hitl | ShellMode::Stub => Risk::Medium,
            ShellMode::Off | ShellMode::Kill => Risk::High,
        }
    }

    fn affordance(&self, goal: &Goal, _state: &WorldState) -> f32 {
        if goal.relation.as_deref() == Some("shell.exec") {
            1.0
        } else if goal.kind == GoalKind::Act
            && goal
                .query
                .as_deref()
                .is_some_and(|q| q.contains("hitl_token") || q.contains("argv"))
        {
            0.2
        } else if goal.kind == GoalKind::Act
            && goal.query.as_deref().is_some_and(|q| q.contains("shell"))
        {
            // Só quando o goal cita shell: shell privilegiado não é fallback
            // genérico de qualquer efeito (não roteie "registre um fato" aqui).
            0.05
        } else {
            0.0
        }
    }

    fn preconditions(&self, goal: &Goal, state: &WorldState) -> bool {
        self.affordance(goal, state) > 0.0
    }

    fn planned_effects(&self, _goal: &Goal) -> Vec<Effect> {
        vec![Effect::Irreversible { kind: "shell.exec" }]
    }

    fn dry_run(&self, goal: &Goal, ctx: &ToolContext) -> Result<(), ToolError> {
        let _ = self.gate_and_argv(goal, ctx)?;
        Ok(())
    }

    fn call(&mut self, goal: &Goal, ctx: &mut ToolContext) -> Result<ToolOutcome, ToolError> {
        let (mode, argv) = self.gate_and_argv(goal, ctx)?;
        let bin = argv[0].as_str();

        // Stub sintético: valida caminho HITL sem spawn (gate/CI).
        if bin.eq_ignore_ascii_case("stub") {
            let detail = format!(
                "shell.exec stub ok mode={mode:?} cwd={} scope={:?} trace={}",
                self.root.display(),
                ctx.scope,
                ctx.trace_id
            );
            return Ok(ToolOutcome {
                claims: vec![claim("shell.exec", "ok", 1.0)],
                confidence: 1.0,
                detail,
                effects: vec![Effect::Irreversible { kind: "shell.exec" }],
                ..ToolOutcome::default()
            });
        }

        // AETHER_SHELL=stub: só binário stub (sem spawn acidental).
        if mode == ShellMode::Stub {
            return Err(ToolError::InvalidArgs(
                "AETHER_SHELL=stub: só argv=[\"stub\", ...]".into(),
            ));
        }

        // R3: spawn real via `run_backend_full` — timeout + kill + cap de
        // stdout, cwd = sandbox, env mínimo (PATH para cargo/python). Antes era
        // `cmd.output()` (sem timeout, buffering ilimitado, no-op de elapsed).
        let env_allow: [&str; 9] = [
            "PATH",
            "Path",
            "SystemRoot",
            "WINDIR",
            "USERPROFILE",
            "HOME",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "TMP",
        ];
        let spec = crate::agent::backend::BackendRun {
            prog: bin,
            args: &argv[1..],
            stdin_data: None,
            timeout_ms: SHELL_TIMEOUT_MS,
            max_stdout: crate::agent::backend::BACKEND_MAX_STDOUT,
            current_dir: Some(&self.root),
            env_allow: Some(&env_allow),
        };
        let started = std::time::Instant::now();
        let output = crate::agent::backend::run_backend_full(&spec);
        let elapsed = started.elapsed();
        let stdout_bytes = match output {
            Ok(b) => b,
            Err(e) => {
                // Timeout/kill/exit != 0/saída estourada → Err honesto com
                // contexto; nunca claim de "ok" nem hang.
                return Err(ToolError::Failed(format!(
                    "shell.exec {bin} falhou ({elapsed:?}): {e}"
                )));
            }
        };
        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let preview = |s: &str| {
            let t = s.trim();
            if t.len() > 240 {
                format!("{}…", &t[..240])
            } else {
                t.to_string()
            }
        };
        Ok(ToolOutcome {
            claims: vec![claim("shell.exec", "ok", 1.0)],
            confidence: 1.0,
            detail: format!(
                "shell.exec ok exit=0 argv={:?} cwd={} out={} dur={elapsed:?} scope={:?} trace={}",
                argv,
                self.root.display(),
                preview(&stdout),
                ctx.scope,
                ctx.trace_id
            ),
            effects: vec![Effect::Irreversible { kind: "shell.exec" }],
            ..ToolOutcome::default()
        })
    }

    fn escalation(&self) -> bool {
        // Hitl/Stub precisam ser selecionáveis pelo planner (score ≠ -1000).
        // Off/Kill: escalação → planner não age sozinho (G-A4).
        !matches!(shell_mode(), ShellMode::Hitl | ShellMode::Stub)
    }
}

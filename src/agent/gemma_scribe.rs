//! Gemma escriba (T-NL-OUT) — mesmo artefato da sentinela T-DEC.
//!
//! Dois plugues, um path. Nunca chama `vegaml.decide`. Opt-in `AETHER_SCRIBE=gemma`.

use super::dec_upgrade::{load_active_lock, ACTIVE_LOCK};
use super::language::{GoalSchema, LanguageCaps, LanguageError, LanguagePort, ParserPort, Style};
use super::tool::{Goal, Risk};
use super::Claim;

/// Env: liga o backend escriba (senão o bus fica em template).
pub const ENV_SCRIBE: &str = "AETHER_SCRIBE";
/// Env: tenta sidecar live (senão prosa ancorada só-claims).
pub const ENV_SCRIBE_LIVE: &str = "AETHER_SCRIBE_LIVE";

/// Política de runtime no host de produto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScribeRuntime {
    /// VRAM folgada: AND + scribe podem coexistir.
    Resident,
    /// VRAM justa: decide XOR generate (fila).
    SingleFlight,
    /// Sem scribe / orçamento insuficiente para live → template.
    TemplateOnly,
}

impl ScribeRuntime {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resident => "resident",
            Self::SingleFlight => "single_flight",
            Self::TemplateOnly => "template_only",
        }
    }
}

/// Pico AND default (0.8B + gemma-1B) MiB — host_probe pode sobrescrever.
pub const DEFAULT_AND_PEAK_MB: u64 = 4500;
/// Extra estimado para generate live (não soma segundo clone se single-flight).
pub const DEFAULT_SCRIBE_LIVE_EXTRA_MB: u64 = 500;

/// Escolhe política a partir da VRAM livre reportada (user final).
/// Caller decide opt-in (`scribe_opt_in`); esta função só dimensiona runtime.
pub fn scribe_runtime_policy(
    vram_free_mb: Option<u64>,
    and_peak_mb: u64,
    scribe_live: bool,
) -> ScribeRuntime {
    if !scribe_live {
        // Dry/ancorado não exige VRAM extra — ainda é o papel escriba do artefato.
        return ScribeRuntime::Resident;
    }
    let Some(free) = vram_free_mb else {
        return ScribeRuntime::SingleFlight;
    };
    let need_resident = and_peak_mb.saturating_add(DEFAULT_SCRIBE_LIVE_EXTRA_MB);
    if free >= need_resident {
        ScribeRuntime::Resident
    } else if free >= and_peak_mb {
        ScribeRuntime::SingleFlight
    } else {
        ScribeRuntime::TemplateOnly
    }
}

/// `AETHER_SCRIBE=gemma` (case-insensitive).
pub fn scribe_opt_in() -> bool {
    std::env::var(ENV_SCRIBE)
        .map(|v| v.eq_ignore_ascii_case("gemma") || v == "1")
        .unwrap_or(false)
}

/// Live neural opt-in.
pub fn scribe_live_opt_in() -> bool {
    std::env::var(ENV_SCRIBE_LIVE)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Path/id da escriba = sentinela active (economia de meios).
pub fn active_scribe_path() -> Option<String> {
    let lock = load_active_lock()?;
    if let Some(p) = lock
        .get("scribe_path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(p.to_string());
    }
    lock.get("sentinel")
        .and_then(|s| s.get("path").or_else(|| s.get("id")))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Garante campos scribe no lock active (cutover/revert).
pub fn ensure_scribe_fields_on_active_lock() -> Result<(), String> {
    let Some(lock) = load_active_lock() else {
        return Ok(());
    };
    let sent_path = lock
        .get("sentinel")
        .and_then(|s| s.get("path").or_else(|| s.get("id")))
        .and_then(|v| v.as_str())
        .unwrap_or("gemma-3-1b-prose")
        .to_string();
    let mut pairs: Vec<(String, crate::json::Json)> = match lock {
        crate::json::Json::Obj(v) => v
            .into_iter()
            .filter(|(k, _)| k != "scribe_enabled" && k != "scribe_path" && k != "scribe_backend")
            .collect(),
        _ => Vec::new(),
    };
    pairs.push(("scribe_enabled".into(), crate::json::Json::Bool(true)));
    pairs.push(("scribe_path".into(), crate::json::Json::str(&sent_path)));
    pairs.push((
        "scribe_backend".into(),
        crate::json::Json::str("gemma_local"),
    ));
    let out = crate::json::Json::Obj(pairs);
    let p = std::path::Path::new(ACTIVE_LOCK);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(p, out.to_string()).map_err(|e| e.to_string())
}

/// Porta escriba: generate ancorado em claims; parse delega ao ParserPort.
#[derive(Debug)]
pub struct GemmaScribePort {
    /// Path do artefato (mesmo da sentinela).
    pub artifact_path: String,
    /// Runtime escolhido.
    pub runtime: ScribeRuntime,
    /// Live tentado?
    pub live: bool,
    /// Backend label para telemetria.
    pub backend_label: &'static str,
    parser: ParserPort,
}

impl GemmaScribePort {
    /// Abre a partir do env + lock active. `None` → bus usa template.
    pub fn from_env(vram_free_mb: Option<u64>) -> Option<Self> {
        if !scribe_opt_in() {
            return None;
        }
        let live = scribe_live_opt_in();
        let runtime = scribe_runtime_policy(vram_free_mb, DEFAULT_AND_PEAK_MB, live);
        if live && runtime == ScribeRuntime::TemplateOnly {
            return None;
        }
        let path = active_scribe_path().unwrap_or_else(|| "gemma-3-1b-prose".into());
        let backend_label = if live {
            "gemma_scribe_live"
        } else {
            "gemma_scribe"
        };
        Some(Self {
            artifact_path: path,
            runtime,
            live,
            backend_label,
            parser: ParserPort::new(),
        })
    }

    /// Prosa ancorada: só reformula claims (sem fato novo). Não chama decide.
    fn generate_anchored(claims: &[Claim], style: Style) -> Result<String, LanguageError> {
        if claims.is_empty() {
            return Err(LanguageError::Empty);
        }
        let mut lines = Vec::new();
        for c in claims {
            if c.value.trim().is_empty() {
                continue;
            }
            let src = c.provenance.label();
            let line = match style {
                Style::Compact => format!(
                    "{} {}={} (fonte={} auth={})",
                    c.subject, c.relation, c.value, src, c.authority
                ),
                Style::Factual => format!(
                    "Com base na evidência, {} {} {} (fonte={}, autoridade {}).",
                    c.subject, c.relation, c.value, src, c.authority
                ),
            };
            lines.push(line);
        }
        if lines.is_empty() {
            return Err(LanguageError::Empty);
        }
        Ok(lines.join(" "))
    }

    /// Live: sidecar Python; falha → Empty (bus cai no template).
    fn generate_live(&self, claims: &[Claim], style: Style) -> Result<String, LanguageError> {
        let payload = claims_to_jsonl(claims, style);
        let out = std::process::Command::new("python")
            .args([
                "tools/gemma_scribe_generate.py",
                "--artifact",
                &self.artifact_path,
                "--style",
                match style {
                    Style::Factual => "factual",
                    Style::Compact => "compact",
                },
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let Ok(mut child) = out else {
            return Err(LanguageError::Empty);
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(payload.as_bytes());
        }
        let Ok(output) = child.wait_with_output() else {
            return Err(LanguageError::Empty);
        };
        if !output.status.success() {
            return Err(LanguageError::Empty);
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Err(LanguageError::Empty);
        }
        Ok(text)
    }
}

impl LanguagePort for GemmaScribePort {
    fn parse(&mut self, text: &str, schema: &GoalSchema) -> Result<Goal, LanguageError> {
        self.parser.parse(text, schema)
    }

    fn generate(&mut self, claims: &[Claim], style: Style) -> Result<String, LanguageError> {
        // Contrato: NUNCA vegaml.decide — só generate a partir de claims.
        if self.live && self.runtime != ScribeRuntime::TemplateOnly {
            match self.generate_live(claims, style) {
                Ok(t) => return Ok(t),
                Err(_) => {
                    // Degrada para ancorado (ainda escriba do artefato).
                }
            }
        }
        Self::generate_anchored(claims, style)
    }

    fn caps(&self) -> LanguageCaps {
        LanguageCaps {
            cost: if self.live { 3.0 } else { 1.2 },
            risk: Risk::Medium,
            latency_ms_p99: if self.live { 2500 } else { 40 },
            needs_model: true,
        }
    }
}

fn claims_to_jsonl(claims: &[Claim], style: Style) -> String {
    use crate::json::Json;
    let arr: Vec<Json> = claims
        .iter()
        .map(|c| {
            Json::obj([
                ("subject", Json::str(&c.subject)),
                ("relation", Json::str(&c.relation)),
                ("value", Json::str(&c.value)),
                ("authority", Json::num(f64::from(c.authority))),
            ])
        })
        .collect();
    Json::obj([
        (
            "style",
            Json::str(match style {
                Style::Factual => "factual",
                Style::Compact => "compact",
            }),
        ),
        ("claims", Json::Arr(arr)),
        ("rule", Json::str("only_rephrase_claims_no_new_facts")),
    ])
    .to_string()
}

/// Lê VRAM livre do host report se existir (user final).
pub fn vram_free_from_host_report() -> Option<u64> {
    let text = std::fs::read_to_string("target/aether_host_report.json").ok()?;
    // Campo opcional futuro; RAM disponível como proxy fraco se não houver VRAM.
    let j = crate::json::Json::parse(&text).ok()?;
    j.get("vram_free_mb").and_then(|v| v.as_u64()).or_else(|| {
        j.get("ram_available_mb")
            .and_then(|v| v.as_u64())
            .map(|ram| ram / 4) // proxy conservador
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ExpertId;
    use crate::core::certificate::Provenance;

    #[test]
    fn policy_vram() {
        assert_eq!(
            scribe_runtime_policy(Some(100), DEFAULT_AND_PEAK_MB, true),
            ScribeRuntime::TemplateOnly
        );
        assert_eq!(
            scribe_runtime_policy(Some(6000), DEFAULT_AND_PEAK_MB, true),
            ScribeRuntime::Resident
        );
        assert_eq!(
            scribe_runtime_policy(Some(4600), DEFAULT_AND_PEAK_MB, true),
            ScribeRuntime::SingleFlight
        );
    }

    #[test]
    fn anchored_so_claims() {
        let claims = vec![Claim::new(
            "Brasil",
            "capital",
            "Brasília",
            0.9,
            200,
            ExpertId::Memory,
            Provenance::Inferred,
        )];
        let t = GemmaScribePort::generate_anchored(&claims, Style::Factual).unwrap();
        assert!(t.to_lowercase().contains("brasil"));
        assert!(t.contains('B') || t.contains('b'));
        assert!(GemmaScribePort::generate_anchored(&[], Style::Factual).is_err());
    }
}

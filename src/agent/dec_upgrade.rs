//! T-DEC upgrade — máquina de estados dual-bank (active / candidate / previous).
//!
//! Produto só lê `active`. Candidate nunca atende Console/API. Cutover exige HITL.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::json::Json;

use super::hitl::{HitlKind, HitlRequest};
use super::hitl_queue;
use super::tool::AUTHORITY_QUARANTINE;

/// Catálogo allowlist.
pub const CATALOG_PATH: &str = "data/dec_reader_catalog.json";
/// Estado persistido do hub.
pub const STATE_PATH: &str = "target/aether_dec_upgrade.json";
/// Lock do perfil active (lido pelo DecisionPort).
pub const ACTIVE_LOCK: &str = ".vega/profiles/active/engine.lock.json";
/// Lock do candidate.
pub const CANDIDATE_LOCK: &str = ".vega/profiles/candidate/engine.lock.json";
/// Lock previous (fallback).
pub const PREVIOUS_LOCK: &str = ".vega/profiles/previous/engine.lock.json";

/// Fase da máquina de estados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecUpgradePhase {
    Idle,
    Catalog,
    HwCheck,
    HitlApproveDownload,
    Fetch,
    Attach,
    Fit,
    Smoke,
    Quarantine,
    HitlCutover,
    Active,
    AbortRollback,
}

impl DecUpgradePhase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Catalog => "catalog",
            Self::HwCheck => "hw_check",
            Self::HitlApproveDownload => "hitl_approve_download",
            Self::Fetch => "fetch",
            Self::Attach => "attach",
            Self::Fit => "fit",
            Self::Smoke => "smoke",
            Self::Quarantine => "quarantine",
            Self::HitlCutover => "hitl_cutover",
            Self::Active => "active",
            Self::AbortRollback => "abort_rollback",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "catalog" => Self::Catalog,
            "hw_check" => Self::HwCheck,
            "hitl_approve_download" => Self::HitlApproveDownload,
            "fetch" => Self::Fetch,
            "attach" => Self::Attach,
            "fit" => Self::Fit,
            "smoke" => Self::Smoke,
            "quarantine" => Self::Quarantine,
            "hitl_cutover" => Self::HitlCutover,
            "active" => Self::Active,
            "abort_rollback" => Self::AbortRollback,
            _ => return None,
        })
    }
}

/// Cor da lâmpada (banda CORE §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LampColor {
    Green,
    Yellow,
    Red,
    Off,
}

impl LampColor {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
            Self::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "green" => Self::Green,
            "yellow" => Self::Yellow,
            "red" => Self::Red,
            _ => Self::Off,
        }
    }
}

/// Lâmpadas do quadro T-DEC upgrade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecLamps {
    pub hw: LampColor,
    pub fetch: LampColor,
    pub boot: LampColor,
    pub fit: LampColor,
    pub t1: LampColor,
    pub t2: LampColor,
    pub c1: LampColor,
    pub and_gate: LampColor,
    pub quarantine: LampColor,
    pub ok_geral: LampColor,
}

impl Default for DecLamps {
    fn default() -> Self {
        Self {
            hw: LampColor::Off,
            fetch: LampColor::Off,
            boot: LampColor::Off,
            fit: LampColor::Off,
            t1: LampColor::Off,
            t2: LampColor::Off,
            c1: LampColor::Off,
            and_gate: LampColor::Off,
            quarantine: LampColor::Off,
            ok_geral: LampColor::Off,
        }
    }
}

impl DecLamps {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("hw", Json::str(self.hw.label())),
            ("fetch", Json::str(self.fetch.label())),
            ("boot", Json::str(self.boot.label())),
            ("fit", Json::str(self.fit.label())),
            ("t1", Json::str(self.t1.label())),
            ("t2", Json::str(self.t2.label())),
            ("c1", Json::str(self.c1.label())),
            ("and", Json::str(self.and_gate.label())),
            ("quarantine", Json::str(self.quarantine.label())),
            ("ok_geral", Json::str(self.ok_geral.label())),
        ])
    }

    pub fn from_json(j: &Json) -> Self {
        let get = |k: &str| {
            j.get(k)
                .and_then(|v| v.as_str())
                .map(LampColor::parse)
                .unwrap_or(LampColor::Off)
        };
        Self {
            hw: get("hw"),
            fetch: get("fetch"),
            boot: get("boot"),
            fit: get("fit"),
            t1: get("t1"),
            t2: get("t2"),
            c1: get("c1"),
            and_gate: get("and"),
            quarantine: get("quarantine"),
            ok_geral: get("ok_geral"),
        }
    }

    /// Smoke passou se T1/T2/C1/AND/Boot/Fit verdes.
    pub fn smoke_all_green(&self) -> bool {
        [
            self.boot,
            self.fit,
            self.t1,
            self.t2,
            self.c1,
            self.and_gate,
        ]
        .iter()
        .all(|c| *c == LampColor::Green)
    }
}

/// Slot do reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecSlot {
    Primary,
    Sentinel,
}

impl DecSlot {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Sentinel => "sentinel",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "primary" => Some(Self::Primary),
            "sentinel" => Some(Self::Sentinel),
            _ => None,
        }
    }
}

/// Entrada do catálogo.
#[derive(Clone, Debug)]
pub struct CatalogReader {
    pub id: String,
    pub slot: DecSlot,
    pub hf_id: String,
    pub revision: String,
    pub license: String,
    pub dtype: String,
    pub vram_mb_est: u64,
    pub disk_mb_est: u64,
    pub attach_kind: String,
    pub hw_6gb: String,
    pub nf4_vram_mb_est: Option<u64>,
    pub nf4_hw_6gb: Option<String>,
    pub label: String,
    pub default_active: bool,
}

/// Estado completo do upgrade.
#[derive(Clone, Debug)]
pub struct DecUpgradeState {
    pub phase: DecUpgradePhase,
    pub slot: Option<DecSlot>,
    pub candidate_id: Option<String>,
    pub candidate_hf: Option<String>,
    pub candidate_revision: Option<String>,
    pub dtype: Option<String>,
    pub vram_mb_est: u64,
    pub hw_compat: LampColor,
    pub lamps: DecLamps,
    pub product_bank: String,
    pub fallback_armed: bool,
    pub reason: String,
    pub smoke_path: Option<String>,
    pub hitl_download_id: Option<u64>,
    pub hitl_cutover_id: Option<u64>,
    pub updated_ms: u64,
}

impl Default for DecUpgradeState {
    fn default() -> Self {
        Self {
            phase: DecUpgradePhase::Idle,
            slot: None,
            candidate_id: None,
            candidate_hf: None,
            candidate_revision: None,
            dtype: None,
            vram_mb_est: 0,
            hw_compat: LampColor::Off,
            lamps: DecLamps::default(),
            product_bank: "active".into(),
            fallback_armed: true,
            reason: "idle".into(),
            smoke_path: None,
            hitl_download_id: None,
            hitl_cutover_id: None,
            updated_ms: now_ms(),
        }
    }
}

impl DecUpgradeState {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("dec_upgrade/1")),
            ("phase", Json::str(self.phase.label())),
            (
                "slot",
                self.slot
                    .map(|s| Json::str(s.label()))
                    .unwrap_or(Json::Null),
            ),
            ("candidate_id", opt_str(&self.candidate_id)),
            ("candidate_hf", opt_str(&self.candidate_hf)),
            ("candidate_revision", opt_str(&self.candidate_revision)),
            ("dtype", opt_str(&self.dtype)),
            ("vram_mb_est", Json::num(self.vram_mb_est as f64)),
            ("hw_compat", Json::str(self.hw_compat.label())),
            ("lamps", self.lamps.to_json()),
            ("product_bank", Json::str(&self.product_bank)),
            ("fallback_armed", Json::Bool(self.fallback_armed)),
            ("reason", Json::str(&self.reason)),
            ("smoke_path", opt_str(&self.smoke_path)),
            (
                "hitl_download_id",
                self.hitl_download_id
                    .map(|id| Json::num(id as f64))
                    .unwrap_or(Json::Null),
            ),
            (
                "hitl_cutover_id",
                self.hitl_cutover_id
                    .map(|id| Json::num(id as f64))
                    .unwrap_or(Json::Null),
            ),
            ("updated_ms", Json::num(self.updated_ms as f64)),
        ])
    }

    pub fn from_json(j: &Json) -> Self {
        let mut s = Self::default();
        if let Some(p) = j.get("phase").and_then(|v| v.as_str()) {
            s.phase = DecUpgradePhase::parse(p).unwrap_or(DecUpgradePhase::Idle);
        }
        if let Some(slot) = j.get("slot").and_then(|v| v.as_str()) {
            s.slot = DecSlot::parse(slot);
        }
        s.candidate_id = j
            .get("candidate_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        s.candidate_hf = j
            .get("candidate_hf")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        s.candidate_revision = j
            .get("candidate_revision")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        s.dtype = j.get("dtype").and_then(|v| v.as_str()).map(str::to_string);
        s.vram_mb_est = j.get("vram_mb_est").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(h) = j.get("hw_compat").and_then(|v| v.as_str()) {
            s.hw_compat = LampColor::parse(h);
        }
        if let Some(l) = j.get("lamps") {
            s.lamps = DecLamps::from_json(l);
        }
        s.product_bank = j
            .get("product_bank")
            .and_then(|v| v.as_str())
            .unwrap_or("active")
            .to_string();
        s.fallback_armed = j
            .get("fallback_armed")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        s.reason = j
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        s.smoke_path = j
            .get("smoke_path")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        s.hitl_download_id = j.get("hitl_download_id").and_then(|v| v.as_u64());
        s.hitl_cutover_id = j.get("hitl_cutover_id").and_then(|v| v.as_u64());
        s.updated_ms = j
            .get("updated_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(now_ms);
        s
    }

    pub fn save(&self) -> Result<(), String> {
        save_json(STATE_PATH, &self.to_json())
    }

    pub fn load() -> Self {
        load_json(STATE_PATH)
            .as_ref()
            .map(Self::from_json)
            .unwrap_or_default()
    }
}

fn opt_str(v: &Option<String>) -> Json {
    v.as_ref()
        .map(|s| Json::str(s.as_str()))
        .unwrap_or(Json::Null)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn save_json(path: &str, value: &Json) -> Result<(), String> {
    let p = PathBuf::from(path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&p, value.to_string()).map_err(|e| e.to_string())
}

fn load_json(path: &str) -> Option<Json> {
    let text = fs::read_to_string(path).ok()?;
    Json::parse(&text).ok()
}

/// Carrega catálogo allowlist.
pub fn load_catalog() -> Result<Vec<CatalogReader>, String> {
    let text = fs::read_to_string(CATALOG_PATH).map_err(|e| format!("{CATALOG_PATH}: {e}"))?;
    let j = Json::parse(&text).map_err(|e| format!("catalog parse: {e}"))?;
    let arr = j
        .get("readers")
        .ok_or_else(|| "catalog: falta readers".to_string())?;
    let Json::Arr(items) = arr else {
        return Err("catalog: readers não é array".into());
    };
    let mut out = Vec::new();
    for item in items {
        let slot = item
            .get("slot")
            .and_then(|v| v.as_str())
            .and_then(DecSlot::parse)
            .ok_or_else(|| "catalog: slot inválido".to_string())?;
        out.push(CatalogReader {
            id: item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            slot,
            hf_id: item
                .get("hf_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            revision: item
                .get("revision")
                .and_then(|v| v.as_str())
                .unwrap_or("main")
                .to_string(),
            license: item
                .get("license")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            dtype: item
                .get("dtype")
                .and_then(|v| v.as_str())
                .unwrap_or("bf16")
                .to_string(),
            vram_mb_est: item
                .get("vram_mb_est")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            disk_mb_est: item
                .get("disk_mb_est")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            attach_kind: item
                .get("attach_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            hw_6gb: item
                .get("hw_6gb")
                .and_then(|v| v.as_str())
                .unwrap_or("red")
                .to_string(),
            nf4_vram_mb_est: item.get("nf4_vram_mb_est").and_then(|v| v.as_u64()),
            nf4_hw_6gb: item
                .get("nf4_hw_6gb")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            label: item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            default_active: item
                .get("default_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    }
    Ok(out)
}

/// Orçamento VRAM do catálogo (default 5120).
pub fn catalog_vram_budget_mb() -> u64 {
    load_json(CATALOG_PATH)
        .and_then(|j| j.get("vram_budget_mb")?.as_u64())
        .unwrap_or(5120)
}

/// Compat HW: soma VRAM do candidate + peer ativo vs orçamento.
pub fn hw_compat_for(reader: &CatalogReader, peer_vram_mb: u64, prefer_nf4: bool) -> LampColor {
    let budget = catalog_vram_budget_mb();
    let (est, mark) = if prefer_nf4 {
        if let (Some(v), Some(m)) = (reader.nf4_vram_mb_est, reader.nf4_hw_6gb.as_deref()) {
            (v, m)
        } else {
            (reader.vram_mb_est, reader.hw_6gb.as_str())
        }
    } else {
        (reader.vram_mb_est, reader.hw_6gb.as_str())
    };
    let and_peak = est.saturating_add(peer_vram_mb);
    if and_peak > budget {
        return LampColor::Red;
    }
    match mark {
        "green" => LampColor::Green,
        "yellow" => LampColor::Yellow,
        _ => LampColor::Red,
    }
}

/// VRAM estimada do peer no slot oposto (active defaults).
pub fn peer_vram_mb(slot: DecSlot) -> u64 {
    let Ok(cat) = load_catalog() else {
        return 2048;
    };
    let want = match slot {
        DecSlot::Primary => DecSlot::Sentinel,
        DecSlot::Sentinel => DecSlot::Primary,
    };
    cat.iter()
        .find(|r| r.slot == want && r.default_active)
        .map(|r| r.vram_mb_est)
        .unwrap_or(2048)
}

/// Inicia pedido de upgrade: Catalog → HwCheck → enfileira HITL download.
pub fn begin_upgrade(slot: DecSlot, candidate_id: &str) -> Result<DecUpgradeState, String> {
    let cat = load_catalog()?;
    let reader = cat
        .iter()
        .find(|r| r.id == candidate_id && r.slot == slot)
        .ok_or_else(|| format!("candidato {candidate_id} não está na allowlist do slot"))?
        .clone();

    let prefer_nf4 = reader.hw_6gb != "green" && reader.nf4_vram_mb_est.is_some();
    let peer = peer_vram_mb(slot);
    let hw = hw_compat_for(&reader, peer, prefer_nf4);
    let mut state = DecUpgradeState {
        phase: DecUpgradePhase::HwCheck,
        slot: Some(slot),
        candidate_id: Some(reader.id.clone()),
        candidate_hf: Some(reader.hf_id.clone()),
        candidate_revision: Some(reader.revision.clone()),
        dtype: Some(if prefer_nf4 {
            "nf4".into()
        } else {
            reader.dtype.clone()
        }),
        vram_mb_est: if prefer_nf4 {
            reader.nf4_vram_mb_est.unwrap_or(reader.vram_mb_est)
        } else {
            reader.vram_mb_est
        },
        hw_compat: hw,
        product_bank: "active".into(),
        fallback_armed: true,
        ..Default::default()
    };
    state.lamps.hw = hw;
    state.reason = format!(
        "hw={} peak_and≈{}+{}MB hf={}",
        hw.label(),
        state.vram_mb_est,
        peer,
        reader.hf_id
    );
    state.updated_ms = now_ms();

    if hw == LampColor::Red {
        state.phase = DecUpgradePhase::AbortRollback;
        state.reason = format!("HW incompatível: {}", state.reason);
        state.save()?;
        return Ok(state);
    }

    let req_id = now_ms();
    let req = HitlRequest {
        request_id: req_id,
        kind: HitlKind::DecUpgradeDownload,
        reason: format!(
            "T-DEC download {} ({}) rev={} dtype={}",
            reader.id,
            reader.hf_id,
            reader.revision,
            state.dtype.as_deref().unwrap_or("bf16")
        ),
        from_authority: AUTHORITY_QUARANTINE,
        to_authority: 255,
        subject: Some(format!("dec_upgrade/download/{}", reader.id)),
        timeout_ms: 0,
    };
    hitl_queue::enqueue(&req)?;
    state.phase = DecUpgradePhase::HitlApproveDownload;
    state.hitl_download_id = Some(req_id);
    state.lamps.quarantine = LampColor::Yellow;
    state.save()?;
    Ok(state)
}

/// Após HITL download aprovado: marca Fetch e deixa job orquestrar.
pub fn mark_download_approved(state: &mut DecUpgradeState) -> Result<(), String> {
    if state.phase != DecUpgradePhase::HitlApproveDownload {
        return Err(format!(
            "fase esperada hitl_approve_download, atual={}",
            state.phase.label()
        ));
    }
    state.phase = DecUpgradePhase::Fetch;
    state.reason = "download aprovado — job DecUpgrade".into();
    state.updated_ms = now_ms();
    state.save()
}

/// Aplica resultado de smoke (JSON do tool) → Quarantine ou Abort.
pub fn apply_smoke_result(state: &mut DecUpgradeState, smoke: &Json) -> Result<(), String> {
    let lamp = |k: &str| -> LampColor {
        smoke
            .get(k)
            .and_then(|v| v.as_str())
            .map(LampColor::parse)
            .unwrap_or(LampColor::Red)
    };
    state.lamps.boot = lamp("boot");
    state.lamps.fit = lamp("fit");
    state.lamps.t1 = lamp("t1");
    state.lamps.t2 = lamp("t2");
    state.lamps.c1 = lamp("c1");
    state.lamps.and_gate = lamp("and");
    state.lamps.fetch = LampColor::Green;
    state.smoke_path = Some("target/aether_dec_upgrade_smoke.json".into());
    state.updated_ms = now_ms();

    if !state.lamps.smoke_all_green() {
        state.phase = DecUpgradePhase::AbortRollback;
        state.reason = smoke
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("smoke falhou")
            .to_string();
        state.lamps.ok_geral = LampColor::Red;
        state.fallback_armed = true;
        state.product_bank = "active".into();
        clear_candidate_marker();
        state.save()?;
        return Ok(());
    }

    state.phase = DecUpgradePhase::Quarantine;
    state.lamps.quarantine = LampColor::Yellow;
    state.reason = "smoke verde — aguarda HITL cutover".into();
    let req_id = now_ms();
    let req = HitlRequest {
        request_id: req_id,
        kind: HitlKind::DecUpgradeCutover,
        reason: format!(
            "T-DEC cutover candidate={} → active (fallback até Approve)",
            state.candidate_id.as_deref().unwrap_or("?")
        ),
        from_authority: AUTHORITY_QUARANTINE,
        to_authority: 255,
        subject: Some(format!(
            "dec_upgrade/cutover/{}",
            state.candidate_id.as_deref().unwrap_or("unknown")
        )),
        timeout_ms: 0,
    };
    hitl_queue::enqueue(&req)?;
    state.phase = DecUpgradePhase::HitlCutover;
    state.hitl_cutover_id = Some(req_id);
    state.save()
}

/// Cutover HITL: candidate → active; active → previous. Fallback desarma só aqui.
pub fn cutover_promote(state: &mut DecUpgradeState) -> Result<(), String> {
    if state.phase != DecUpgradePhase::HitlCutover {
        return Err(format!(
            "fase esperada hitl_cutover, atual={}",
            state.phase.label()
        ));
    }
    if !state.lamps.smoke_all_green() {
        return Err("cutover recusado: smoke não está verde".into());
    }

    ensure_profile_dirs()?;
    // active → previous (se existir)
    if Path::new(ACTIVE_LOCK).exists() {
        let _ = fs::remove_dir_all(".vega/profiles/previous");
        fs::rename(".vega/profiles/active", ".vega/profiles/previous")
            .map_err(|e| format!("backup active→previous: {e}"))?;
    }
    // candidate → active
    if Path::new(CANDIDATE_LOCK).exists() || Path::new(".vega/profiles/candidate").exists() {
        let _ = fs::remove_dir_all(".vega/profiles/active");
        fs::rename(".vega/profiles/candidate", ".vega/profiles/active")
            .map_err(|e| format!("promote candidate→active: {e}"))?;
    } else {
        // Sem filesystem candidate (dry-run): grava lock sintético active.
        write_synthetic_active_lock(state)?;
    }

    state.phase = DecUpgradePhase::Active;
    state.product_bank = "active".into();
    state.fallback_armed = false;
    state.lamps.quarantine = LampColor::Green;
    state.lamps.ok_geral = LampColor::Green;
    state.reason = "cutover OK — previous disponível para Reverter".into();
    state.updated_ms = now_ms();
    state.save()?;
    // Economia: sentinela active = escriba (mesmo path).
    let _ = super::gemma_scribe::ensure_scribe_fields_on_active_lock();
    Ok(())
}

/// Reverte previous → active (1-click fallback).
pub fn revert_previous(state: &mut DecUpgradeState) -> Result<(), String> {
    if !Path::new(PREVIOUS_LOCK).exists() && !Path::new(".vega/profiles/previous").exists() {
        return Err("não há previous/ para reverter".into());
    }
    let _ = fs::remove_dir_all(".vega/profiles/candidate");
    if Path::new(".vega/profiles/active").exists() {
        let _ = fs::remove_dir_all(".vega/profiles/active.bak_failed");
        let _ = fs::rename(".vega/profiles/active", ".vega/profiles/active.bak_failed");
    }
    fs::rename(".vega/profiles/previous", ".vega/profiles/active")
        .map_err(|e| format!("revert previous→active: {e}"))?;
    state.phase = DecUpgradePhase::Active;
    state.product_bank = "active".into();
    state.fallback_armed = false;
    state.lamps.ok_geral = LampColor::Yellow;
    state.reason = "revertido para previous".into();
    state.updated_ms = now_ms();
    state.save()?;
    let _ = super::gemma_scribe::ensure_scribe_fields_on_active_lock();
    Ok(())
}

/// Abort: limpa candidate; active intacto.
pub fn abort_rollback(
    state: &mut DecUpgradeState,
    reason: impl Into<String>,
) -> Result<(), String> {
    clear_candidate_marker();
    state.phase = DecUpgradePhase::AbortRollback;
    state.product_bank = "active".into();
    state.fallback_armed = true;
    state.lamps.ok_geral = LampColor::Red;
    state.reason = reason.into();
    state.updated_ms = now_ms();
    state.save()
}

fn clear_candidate_marker() {
    let _ = fs::remove_dir_all(".vega/profiles/candidate");
}

fn ensure_profile_dirs() -> Result<(), String> {
    fs::create_dir_all(".vega/profiles/active").map_err(|e| e.to_string())?;
    fs::create_dir_all(".vega/profiles/candidate").map_err(|e| e.to_string())?;
    Ok(())
}

fn write_synthetic_active_lock(state: &DecUpgradeState) -> Result<(), String> {
    ensure_profile_dirs()?;
    let lock = Json::obj([
        ("schema", Json::str("engine.lock/1")),
        ("bank", Json::str("active")),
        (
            "primary",
            if state.slot == Some(DecSlot::Primary) {
                Json::obj([
                    ("id", opt_str(&state.candidate_id)),
                    ("hf_id", opt_str(&state.candidate_hf)),
                    ("revision", opt_str(&state.candidate_revision)),
                    ("dtype", opt_str(&state.dtype)),
                ])
            } else {
                Json::obj([("id", Json::str("vega-u-0.8b"))])
            },
        ),
        (
            "sentinel",
            if state.slot == Some(DecSlot::Sentinel) {
                Json::obj([
                    ("id", opt_str(&state.candidate_id)),
                    ("hf_id", opt_str(&state.candidate_hf)),
                    ("revision", opt_str(&state.candidate_revision)),
                    ("dtype", opt_str(&state.dtype)),
                ])
            } else {
                Json::obj([("id", Json::str("gemma-3-1b-prose"))])
            },
        ),
        (
            "engine_label",
            Json::str(engine_label_from_ids(
                state
                    .candidate_id
                    .as_deref()
                    .filter(|_| state.slot == Some(DecSlot::Primary))
                    .unwrap_or("vega-u-0.8b"),
                state
                    .candidate_id
                    .as_deref()
                    .filter(|_| state.slot == Some(DecSlot::Sentinel))
                    .unwrap_or("gemma-3-1b-prose"),
            )),
        ),
        ("fingerprint", Json::str(format!("cutover-{}", now_ms()))),
        ("scribe_enabled", Json::Bool(true)),
        (
            "scribe_path",
            if state.slot == Some(DecSlot::Sentinel) {
                opt_str(&state.candidate_id)
            } else {
                Json::str("gemma-3-1b-prose")
            },
        ),
        ("scribe_backend", Json::str("gemma_local")),
    ]);
    save_json(ACTIVE_LOCK, &lock)
}

/// Label dinâmico AND (sem hardcode único).
pub fn engine_label_from_ids(primary_id: &str, sentinel_id: &str) -> String {
    format!("{primary_id} AND {sentinel_id}")
}

/// Lê engine.lock active (se existir).
pub fn load_active_lock() -> Option<Json> {
    load_json(ACTIVE_LOCK)
}

/// Paths de modelo do lock active para o DecisionPort (só active).
pub fn active_model_paths() -> (Option<String>, Option<String>, Option<String>) {
    let Some(lock) = load_active_lock() else {
        return (None, None, None);
    };
    let primary = lock
        .get("primary")
        .and_then(|p| p.get("path").or_else(|| p.get("id")))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let sentinel = lock
        .get("sentinel")
        .and_then(|p| p.get("path").or_else(|| p.get("id")))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let label = lock
        .get("engine_label")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    (primary, sentinel, label)
}

/// Resumo curto para Detalhe do hub.
pub fn detail_block(state: &DecUpgradeState) -> String {
    format!(
        "fase={} bank={} fallback={} slot={} cand={} hw={} lamps[hw={} fetch={} boot={} fit={} T1={} T2={} C1={} AND={} quar={} ok={}]\n{}",
        state.phase.label(),
        state.product_bank,
        state.fallback_armed,
        state.slot.map(|s| s.label()).unwrap_or("-"),
        state.candidate_id.as_deref().unwrap_or("-"),
        state.hw_compat.label(),
        state.lamps.hw.label(),
        state.lamps.fetch.label(),
        state.lamps.boot.label(),
        state.lamps.fit.label(),
        state.lamps.t1.label(),
        state.lamps.t2.label(),
        state.lamps.c1.label(),
        state.lamps.and_gate.label(),
        state.lamps.quarantine.label(),
        state.lamps.ok_geral.label(),
        state.reason
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_allowlist() {
        let cat = load_catalog().expect("catalog");
        assert!(cat.iter().any(|r| r.id == "vega-u-0.8b"));
        assert!(cat.iter().any(|r| r.id == "gemma-3-1b-prose"));
        assert!(cat.iter().any(|r| r.id == "qwen3.5-4b-attach"));
    }

    #[test]
    fn hw_4b_bf16_red_on_6gb_budget() {
        let cat = load_catalog().unwrap();
        let r = cat.iter().find(|x| x.id == "qwen3.5-4b-attach").unwrap();
        assert_eq!(hw_compat_for(r, 2048, false), LampColor::Red);
        assert_eq!(hw_compat_for(r, 2048, true), LampColor::Yellow);
    }

    #[test]
    fn smoke_fail_keeps_active_bank() {
        let dir = std::env::temp_dir().join(format!("aether_dec_up_{}", now_ms()));
        let _ = fs::create_dir_all(&dir);
        // Usa STATE_PATH relativo ao CWD do teste (repo root em cargo test).
        let mut st = DecUpgradeState {
            phase: DecUpgradePhase::Smoke,
            slot: Some(DecSlot::Primary),
            candidate_id: Some("vega-u-0.8b".into()),
            ..Default::default()
        };
        let smoke = Json::obj([
            ("boot", Json::str("green")),
            ("fit", Json::str("green")),
            ("t1", Json::str("red")),
            ("t2", Json::str("green")),
            ("c1", Json::str("green")),
            ("and", Json::str("green")),
            ("reason", Json::str("T1 falhou")),
        ]);
        apply_smoke_result(&mut st, &smoke).unwrap();
        assert_eq!(st.phase, DecUpgradePhase::AbortRollback);
        assert_eq!(st.product_bank, "active");
        assert!(st.fallback_armed);
        let _ = dir;
    }

    #[test]
    fn engine_label_dynamic() {
        assert_eq!(
            engine_label_from_ids("vega-u-0.8b", "gemma-3-1b-prose"),
            "vega-u-0.8b AND gemma-3-1b-prose"
        );
    }
}

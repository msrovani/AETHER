//! T-P2P (A3) — delta de evidência append-only entre dois nós.
//!
//! O contrato é o de um OPLOG Merkle-CRDT, **não** o de um `FileStorage`
//! compartilhado (NO-GO da ficha A3):
//!
//! ```text
//! nó A ──append──► Oplog (hash chain + Merkle root) ──delta──► nó B
//!    ▲                                                          │
//!    └────────────── apply idempotente por (scope, key, valor) ─┘
//! ```
//!
//! Regras travadas:
//! * oplog **append-only**: `seq` cresce por nó, `prev` é o hash da entrada
//!   anterior, `root` é o SHA-256 do conjunto ordenado de hashes;
//! * aplicação **idempotente** por `(scope, key)` + valor (replay não duplica e
//!   não reescreve);
//! * divergência de valor **nunca** vira last-write-wins: os dois valores ficam
//!   no estado e o conflito é registrado como `Claim` com relação
//!   `contradicts` (a coerência do agente trata o resto).
//!
//! Nada aqui depende de `src/knowledge/sgdb/` nem de storage externo: o nó é um
//! processo com o próprio oplog. Dois writers no mesmo `FileStorage` seguem
//! proibidos.
//!
//! Limite explícito desta fase: `delta_all` publica o oplog **próprio** do nó.
//! A malha do A3 é de dois nós (o gate mede A↔B); encaminhar entrada aprendida
//! (forwarding para 3+ nós) é a próxima extensão, não um comportamento fingido.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::core::certificate::Provenance;
use crate::core::sha256::sha256_hex;
use crate::json::Json;

use super::{Claim, ExpertId};

/// Versão do formato de delta (entra no hash de cada entrada).
pub const P2P_VERSION: u8 = 1;
/// Máximo de entradas num delta (guarda de memória; acima disso `Err`).
pub const MAX_DELTA_ENTRIES: usize = 4096;
/// Máximo de chars por campo textual de uma entrada.
pub const MAX_FIELD_CHARS: usize = 512;
/// Hash "vazio" (prefixo da primeira entrada de um oplog).
pub const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// Autoridade de um fato replicado (não inventa autoridade social).
pub const P2P_AUTH: u8 = 96;

/// Erro tipado do T-P2P (nada é descartado em silêncio).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P2pError {
    /// Nó vazio/ inválido ou acima de 64 chars.
    BadNode(String),
    /// Escopo vazio.
    BadScope,
    /// Sujeito/relação vazios ou campo acima do limite.
    BadField,
    /// Delta acima de [`MAX_DELTA_ENTRIES`].
    DeltaTooLarge(usize),
    /// Hash de entrada não bate com o conteúdo (corrupção/adulteração).
    EntryHashMismatch(u64),
    /// Merkle root não bate com as entradas.
    RootMismatch,
    /// JSON de delta ausente/ inválido.
    BadJson(String),
    /// Falha de I/O no oplog.
    Io(String),
}

impl fmt::Display for P2pError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadNode(n) => write!(f, "nó inválido: {n:?} (1..=64 chars alnum/-/_/.)"),
            Self::BadScope => write!(f, "escopo vazio"),
            Self::BadField => write!(f, "campo vazio ou acima de {MAX_FIELD_CHARS} chars"),
            Self::DeltaTooLarge(n) => write!(f, "delta com {n} entradas (máx {MAX_DELTA_ENTRIES})"),
            Self::EntryHashMismatch(seq) => write!(f, "hash da entrada seq={seq} não confere"),
            Self::RootMismatch => write!(f, "merkle root não confere"),
            Self::BadJson(why) => write!(f, "delta JSON inválido: {why}"),
            Self::Io(why) => write!(f, "oplog: {why}"),
        }
    }
}

impl std::error::Error for P2pError {}

/// Relógio vetorial `nó → último seq observado`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VClock {
    ticks: BTreeMap<String, u64>,
}

impl VClock {
    /// Relógio vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Último `seq` conhecido de `node` (0 se nunca visto).
    pub fn get(&self, node: &str) -> u64 {
        self.ticks.get(node).copied().unwrap_or(0)
    }

    /// Registra `seq` de `node`; devolve `true` se avançou.
    pub fn observe(&mut self, node: &str, seq: u64) -> bool {
        let slot = self.ticks.entry(node.to_string()).or_insert(0);
        if seq > *slot {
            *slot = seq;
            true
        } else {
            false
        }
    }

    /// Fusão ponto-a-ponto (máximo por nó).
    pub fn merge(&mut self, other: &Self) {
        for (node, seq) in &other.ticks {
            self.observe(node, *seq);
        }
    }

    /// O relógio cobre tudo que `other` já viu?
    pub fn covers(&self, other: &Self) -> bool {
        other.ticks.iter().all(|(node, seq)| self.get(node) >= *seq)
    }

    /// Nós em ordem estável.
    pub fn nodes(&self) -> Vec<(String, u64)> {
        self.ticks.iter().map(|(n, s)| (n.clone(), *s)).collect()
    }

    /// Número de nós conhecidos.
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    /// Sem nós.
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    /// JSON canônico (chaves ordenadas, `{"nó":seq}`).
    pub fn to_json(&self) -> Json {
        Json::Obj(self.ticks.iter().map(|(n, s)| (n.clone(), Json::Num(*s as f64))).collect())
    }

    /// Lê um relógio de `Json::Obj`; rejeita valores negativos/não inteiros.
    pub fn from_json(json: &Json) -> Option<Self> {
        let obj = json.as_obj()?;
        let mut clock = Self::new();
        for (node, value) in obj {
            let seq = value.as_u64()?;
            if node.trim().is_empty() {
                return None;
            }
            clock.observe(node, seq);
        }
        Some(clock)
    }
}

/// Valida um identificador de nó (1..=64 de `[A-Za-z0-9._-]`).
pub fn validate_node(node: &str) -> Result<(), P2pError> {
    let n = node.trim();
    if n.is_empty()
        || n.chars().count() > 64
        || !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(P2pError::BadNode(node.to_string()));
    }
    Ok(())
}

/// Chave de grupo `(scope, subject::relation)` normalizada — a unidade de
/// idempotência do sync.
pub fn entry_key(claim: &Claim) -> Result<String, P2pError> {
    let subject = claim.subject.trim();
    let relation = claim.relation.trim();
    if subject.is_empty() || relation.is_empty() {
        return Err(P2pError::BadField);
    }
    if subject.chars().count() > MAX_FIELD_CHARS
        || relation.chars().count() > MAX_FIELD_CHARS
        || claim.value.chars().count() > MAX_FIELD_CHARS
    {
        return Err(P2pError::BadField);
    }
    Ok(format!(
        "{}::{}",
        subject.to_lowercase(),
        relation.to_lowercase()
    ))
}

fn validate_scope(scope: &str) -> Result<(), P2pError> {
    if scope.trim().is_empty() {
        return Err(P2pError::BadScope);
    }
    Ok(())
}

/// Texto canônico hasheado de uma entrada (versão + campos + `prev`).
fn canonical(
    scope: &str,
    key: &str,
    seq: u64,
    claim: &Claim,
    prev: &str,
) -> String {
    format!(
        "v{P2P_VERSION}\nscope={}\nkey={}\nseq={seq}\nsubject={}\nrelation={}\nvalue={}\nconfidence={:.6}\nauthority={}\nsource={}\nprovenance={}\nprev={prev}",
        scope.trim(),
        key,
        claim.subject.trim(),
        claim.relation.trim(),
        claim.value.trim(),
        claim.confidence.clamp(0.0, 1.0),
        claim.authority,
        claim.source.label(),
        provenance_label(claim.provenance),
    )
}

fn provenance_label(p: Provenance) -> &'static str {
    match p {
        Provenance::Observed => "observed",
        Provenance::Inferred => "inferred",
        Provenance::Transplanted => "transplanted",
        Provenance::Axiomatic => "axiomatic",
        Provenance::Human => "human",
    }
}

fn provenance_from_label(label: &str) -> Option<Provenance> {
    match label {
        "observed" => Some(Provenance::Observed),
        "inferred" => Some(Provenance::Inferred),
        "transplanted" => Some(Provenance::Transplanted),
        "axiomatic" => Some(Provenance::Axiomatic),
        "human" => Some(Provenance::Human),
        _ => None,
    }
}

fn expert_from_label(label: &str) -> Option<ExpertId> {
    match label {
        "transplant" => Some(ExpertId::Transplant),
        "phase" => Some(ExpertId::Phase),
        "logic" => Some(ExpertId::Logic),
        "solver" => Some(ExpertId::Solver),
        "memory" => Some(ExpertId::Memory),
        "predict" => Some(ExpertId::Predict),
        _ => None,
    }
}

/// Uma entrada imutável do oplog.
#[derive(Clone, Debug, PartialEq)]
pub struct DeltaEntry {
    /// Escopo (`kb`, `kb:run:42`, …).
    pub scope: String,
    /// `subject::relation` normalizado.
    pub key: String,
    /// Sequência monotônica do nó autor.
    pub seq: u64,
    /// Evidência replicada.
    pub claim: Claim,
    /// Hash da entrada anterior no oplog do autor.
    pub prev: String,
    /// SHA-256 do `canonical(...)` com `prev`.
    pub hash: String,
}

impl DeltaEntry {
    /// Monta a entrada e sela o hash (`prev = ZERO_HASH` para a primeira).
    pub fn build(
        scope: &str,
        claim: &Claim,
        seq: u64,
        prev: &str,
    ) -> Result<Self, P2pError> {
        validate_scope(scope)?;
        let key = entry_key(claim)?;
        let hash = sha256_hex(canonical(scope, &key, seq, claim, prev).as_bytes());
        Ok(Self {
            scope: scope.trim().to_string(),
            key,
            seq,
            claim: claim.clone(),
            prev: prev.to_string(),
            hash,
        })
    }

    /// Recalcula o hash a partir do conteúdo (detecta adulteração).
    pub fn recompute_hash(&self) -> String {
        sha256_hex(canonical(&self.scope, &self.key, self.seq, &self.claim, &self.prev).as_bytes())
    }

    /// JSON de entrada (`claim` completo, nunca só o valor).
    pub fn to_json(&self) -> Json {
        Json::Obj(vec![
            ("scope".into(), Json::Str(self.scope.clone())),
            ("key".into(), Json::Str(self.key.clone())),
            ("seq".into(), Json::Num(self.seq as f64)),
            ("claim".into(), claim_json(&self.claim)),
            ("prev".into(), Json::Str(self.prev.clone())),
            ("hash".into(), Json::Str(self.hash.clone())),
        ])
    }

    /// Lê uma entrada; `None` se faltar campo obrigatório.
    pub fn from_json(json: &Json) -> Option<Self> {
        Some(Self {
            scope: json.get("scope")?.as_str()?.to_string(),
            key: json.get("key")?.as_str()?.to_string(),
            seq: json.get("seq")?.as_u64()?,
            claim: claim_from_json(json.get("claim")?)?,
            prev: json.get("prev")?.as_str()?.to_string(),
            hash: json.get("hash")?.as_str()?.to_string(),
        })
    }
}

/// `Claim` → JSON estável (labels, não códigos opacos).
pub fn claim_json(claim: &Claim) -> Json {
    Json::Obj(vec![
        ("subject".into(), Json::Str(claim.subject.clone())),
        ("relation".into(), Json::Str(claim.relation.clone())),
        ("value".into(), Json::Str(claim.value.clone())),
        ("confidence".into(), Json::Num(f64::from(claim.confidence.clamp(0.0, 1.0)))),
        ("authority".into(), Json::Num(f64::from(claim.authority))),
        ("source".into(), Json::Str(claim.source.label().to_string())),
        (
            "provenance".into(),
            Json::Str(provenance_label(claim.provenance).to_string()),
        ),
    ])
}

/// `Json` → `Claim` (valida autoridade `0..=255` e confiança `0..=1`).
pub fn claim_from_json(json: &Json) -> Option<Claim> {
    let authority = json.get("authority")?.as_u64()?;
    if authority > 255 {
        return None;
    }
    let confidence = json.get("confidence")?.as_f64()?;
    if !(0.0..=1.0).contains(&confidence) {
        return None;
    }
    Some(Claim::new(
        json.get("subject")?.as_str()?,
        json.get("relation")?.as_str()?,
        json.get("value")?.as_str()?,
        confidence as f32,
        authority as u8,
        expert_from_label(json.get("source")?.as_str()?)?,
        provenance_from_label(json.get("provenance")?.as_str()?)?,
    ))
}

/// Merkle root do conjunto de entradas: SHA-256 dos hashes **ordenados**, com
/// o número de folhas como prefixo (ordem de chegada não muda a raiz).
pub fn merkle_root(entries: &[DeltaEntry]) -> String {
    let mut hashes: Vec<&str> = entries.iter().map(|e| e.hash.as_str()).collect();
    hashes.sort_unstable();
    let mut buf = Vec::with_capacity(hashes.len() * 64 + 16);
    buf.extend_from_slice(format!("merkle:{}:", hashes.len()).as_bytes());
    for h in hashes {
        buf.extend_from_slice(h.as_bytes());
    }
    sha256_hex(&buf)
}

/// Delta de evidência: o que um nó entrega a outro.
#[derive(Clone, Debug, PartialEq)]
pub struct SgdbDelta {
    /// Nó autor do oplog.
    pub node: String,
    /// Último `seq` do nó no momento do delta.
    pub seq: u64,
    /// Relógio vetorial do autor (cobre todos os nós que ele já viu).
    pub vclock: VClock,
    /// Entradas, em ordem de `seq` (append-only).
    pub entries: Vec<DeltaEntry>,
    /// Merkle root das entradas.
    pub root: String,
}

impl SgdbDelta {
    /// Monta e sela o delta (calcula `root`).
    pub fn new(
        node: &str,
        seq: u64,
        vclock: VClock,
        entries: Vec<DeltaEntry>,
    ) -> Result<Self, P2pError> {
        validate_node(node)?;
        if entries.len() > MAX_DELTA_ENTRIES {
            return Err(P2pError::DeltaTooLarge(entries.len()));
        }
        let root = merkle_root(&entries);
        Ok(Self {
            node: node.trim().to_string(),
            seq,
            vclock,
            entries,
            root,
        })
    }

    /// Recalcula o `root` (usado após edição deliberada em teste).
    pub fn seal(&mut self) {
        self.root = merkle_root(&self.entries);
    }

    /// Verifica hash de cada entrada + Merkle root. Falha = delta recusado.
    pub fn verify(&self) -> Result<(), P2pError> {
        validate_node(&self.node)?;
        if self.entries.len() > MAX_DELTA_ENTRIES {
            return Err(P2pError::DeltaTooLarge(self.entries.len()));
        }
        for entry in &self.entries {
            if entry.recompute_hash() != entry.hash {
                return Err(P2pError::EntryHashMismatch(entry.seq));
            }
        }
        if merkle_root(&self.entries) != self.root {
            return Err(P2pError::RootMismatch);
        }
        Ok(())
    }

    /// Entradas vazias?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// JSON de transporte.
    pub fn to_json(&self) -> Json {
        Json::Obj(vec![
            ("version".into(), Json::Num(f64::from(P2P_VERSION))),
            ("node".into(), Json::Str(self.node.clone())),
            ("seq".into(), Json::Num(self.seq as f64)),
            ("vclock".into(), self.vclock.to_json()),
            (
                "entries".into(),
                Json::Arr(self.entries.iter().map(DeltaEntry::to_json).collect()),
            ),
            ("root".into(), Json::Str(self.root.clone())),
        ])
    }

    /// Lê e verifica um delta vindo da rede/arquivo.
    pub fn from_json(json: &Json) -> Result<Self, P2pError> {
        let version = json
            .get("version")
            .and_then(Json::as_u64)
            .ok_or_else(|| P2pError::BadJson("version ausente".into()))?;
        if version != u64::from(P2P_VERSION) {
            return Err(P2pError::BadJson(format!("versão {version}")));
        }
        let node = json
            .get("node")
            .and_then(Json::as_str)
            .ok_or_else(|| P2pError::BadJson("node ausente".into()))?;
        let seq = json
            .get("seq")
            .and_then(Json::as_u64)
            .ok_or_else(|| P2pError::BadJson("seq ausente".into()))?;
        let vclock = json
            .get("vclock")
            .and_then(VClock::from_json)
            .ok_or_else(|| P2pError::BadJson("vclock inválido".into()))?;
        let raw = json
            .get("entries")
            .and_then(Json::as_arr)
            .ok_or_else(|| P2pError::BadJson("entries ausente".into()))?;
        let mut entries = Vec::with_capacity(raw.len());
        for item in raw {
            let entry = DeltaEntry::from_json(item)
                .ok_or_else(|| P2pError::BadJson("entrada incompleta".into()))?;
            entries.push(entry);
        }
        let root = json
            .get("root")
            .and_then(Json::as_str)
            .ok_or_else(|| P2pError::BadJson("root ausente".into()))?;
        let delta = Self {
            node: node.to_string(),
            seq,
            vclock,
            entries,
            root: root.to_string(),
        };
        delta.verify()?;
        Ok(delta)
    }
}

/// Oplog de um nó: append-only, encadeado por hash.
#[derive(Clone, Debug)]
pub struct Oplog {
    node: String,
    path: Option<PathBuf>,
    entries: Vec<DeltaEntry>,
    vclock: VClock,
}

impl Oplog {
    /// Oplog vazio em memória.
    pub fn new(node: &str) -> Result<Self, P2pError> {
        validate_node(node)?;
        Ok(Self {
            node: node.trim().to_string(),
            path: None,
            entries: Vec::new(),
            vclock: VClock::new(),
        })
    }

    /// Abre (ou cria) o oplog JSONL em `path` e valida a cadeia inteira.
    pub fn open(path: &Path, node: &str) -> Result<Self, P2pError> {
        let mut log = Self::new(node)?;
        log.path = Some(path.to_path_buf());
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| P2pError::Io(e.to_string()))?;
            }
        }
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(P2pError::Io(e.to_string())),
        };
        let mut expected_prev = ZERO_HASH.to_string();
        for (line_no, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let json = Json::parse(line)
                .map_err(|e| P2pError::BadJson(format!("linha {}: {e}", line_no + 1)))?;
            let entry = DeltaEntry::from_json(&json)
                .ok_or_else(|| P2pError::BadJson(format!("linha {} incompleta", line_no + 1)))?;
            if entry.recompute_hash() != entry.hash {
                return Err(P2pError::EntryHashMismatch(entry.seq));
            }
            if entry.prev != expected_prev || entry.seq != log.entries.len() as u64 + 1 {
                return Err(P2pError::BadJson(format!(
                    "cadeia quebrada na linha {} (seq {})",
                    line_no + 1,
                    entry.seq
                )));
            }
            expected_prev = entry.hash.clone();
            log.vclock.observe(log.node.clone().as_str(), entry.seq);
            log.entries.push(entry);
        }
        Ok(log)
    }

    /// Nó autor.
    pub fn node(&self) -> &str {
        &self.node
    }

    /// Entradas em ordem de `seq`.
    pub fn entries(&self) -> &[DeltaEntry] {
        &self.entries
    }

    /// Último `seq` (0 se vazio).
    pub fn seq(&self) -> u64 {
        self.entries.last().map(|e| e.seq).unwrap_or(0)
    }

    /// Merkle root atual.
    pub fn root(&self) -> String {
        merkle_root(&self.entries)
    }

    /// Relógio vetorial do oplog.
    pub fn vclock(&self) -> &VClock {
        &self.vclock
    }

    /// Anexa uma claim; grava no arquivo quando aberto em disco.
    pub fn append(&mut self, scope: &str, claim: &Claim) -> Result<DeltaEntry, P2pError> {
        let seq = self.seq() + 1;
        let prev = self
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| ZERO_HASH.to_string());
        let entry = DeltaEntry::build(scope, claim, seq, &prev)?;
        if let Some(path) = &self.path {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| P2pError::Io(e.to_string()))?;
            writeln!(file, "{}", entry.to_json()).map_err(|e| P2pError::Io(e.to_string()))?;
        }
        self.vclock.observe(self.node.clone().as_str(), seq);
        self.entries.push(entry.clone());
        Ok(entry)
    }
    /// Delta com tudo que o observador `since` ainda não viu deste nó.
    pub fn delta_since(&self, since: &VClock) -> Result<SgdbDelta, P2pError> {
        let seen = since.get(&self.node);
        let entries: Vec<DeltaEntry> = self
            .entries
            .iter()
            .filter(|e| e.seq > seen)
            .cloned()
            .collect();
        if entries.len() > MAX_DELTA_ENTRIES {
            return Err(P2pError::DeltaTooLarge(entries.len()));
        }
        let mut clock = self.vclock.clone();
        clock.merge(since);
        SgdbDelta::new(&self.node, self.seq(), clock, entries)
    }

    /// Delta completo (primeiro contato).
    pub fn delta_all(&self) -> Result<SgdbDelta, P2pError> {
        self.delta_since(&VClock::new())
    }
}

/// Divergência preservada: o valor retido e o recebido, nunca um vencedor.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// Escopo.
    pub scope: String,
    /// `subject::relation` normalizado.
    pub key: String,
    /// Claim que já estava no estado (não foi sobrescrita).
    pub kept: Claim,
    /// Claim recebida com valor diferente (preservada no estado).
    pub incoming: Claim,
}

impl Conflict {
    /// Claim de conflito: relação `contradicts` (a coerência decide abstain).
    pub fn contradicts(&self) -> Claim {
        Claim::new(
            self.kept.subject.clone(),
            "contradicts",
            format!("{} ≠ {}", self.kept.value.trim(), self.incoming.value.trim()),
            self.kept
                .confidence
                .min(self.incoming.confidence)
                .clamp(0.0, 1.0),
            P2P_AUTH,
            ExpertId::Memory,
            Provenance::Observed,
        )
    }
}

/// Resultado de aplicar um delta (idempotência medida, não presumida).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Chaves novas aceitas.
    pub accepted: usize,
    /// Entradas já conhecidas (replay ou mesmo valor) — zero escrita.
    pub idempotent: usize,
    /// Valores divergentes preservados + conflito registrado.
    pub conflicts: usize,
}

/// Réplica de um nó: oplog próprio + estado multi-valor por `(scope, key)`.
#[derive(Clone, Debug)]
pub struct Replica {
    node: String,
    log: Oplog,
    store: BTreeMap<(String, String), Vec<Claim>>,
    conflicts: Vec<Conflict>,
    seen: BTreeSet<(String, u64)>,
    vclock: VClock,
}

impl Replica {
    /// Réplica em memória (oplog local, sem arquivo).
    pub fn new(node: &str) -> Result<Self, P2pError> {
        Ok(Self {
            node: node.trim().to_string(),
            log: Oplog::new(node)?,
            store: BTreeMap::new(),
            conflicts: Vec::new(),
            seen: BTreeSet::new(),
            vclock: VClock::new(),
        })
    }

    /// Réplica durável: abre o oplog JSONL e reconstrói o estado dele.
    pub fn open(path: &Path, node: &str) -> Result<Self, P2pError> {
        let log = Oplog::open(path, node)?;
        let mut replica = Self {
            node: node.trim().to_string(),
            log,
            store: BTreeMap::new(),
            conflicts: Vec::new(),
            seen: BTreeSet::new(),
            vclock: VClock::new(),
        };
        let own: Vec<DeltaEntry> = replica.log.entries().to_vec();
        let mut report = ApplyReport::default();
        for entry in own {
            replica.seen.insert((replica.node.clone(), entry.seq));
            replica.absorb(&entry, &mut report);
            replica.vclock.observe(replica.node.clone().as_str(), entry.seq);
        }
        Ok(replica)
    }

    /// Nó desta réplica.
    pub fn node(&self) -> &str {
        &self.node
    }

    /// Oplog local.
    pub fn log(&self) -> &Oplog {
        &self.log
    }

    /// Entradas conhecidas de todos os nós.
    pub fn vclock(&self) -> &VClock {
        &self.vclock
    }

    /// Estado multi-valor: nenhum writer é fundido nem sobrescrito.
    pub fn values(&self, scope: &str, subject: &str, relation: &str) -> Vec<&Claim> {
        let key = format!(
            "{}::{}",
            subject.trim().to_lowercase(),
            relation.trim().to_lowercase()
        );
        self.store
            .get(&(scope.trim().to_string(), key))
            .map(|claims| claims.iter().collect())
            .unwrap_or_default()
    }

    /// Número de chaves `(scope, key)` no estado.
    pub fn keys(&self) -> usize {
        self.store.len()
    }

    /// Conflitos preservados (divergência de valor).
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }

    /// Digest do estado inteiro (convergência comparável entre nós).
    pub fn state_root(&self) -> String {
        let mut buf = Vec::new();
        for ((scope, key), claims) in &self.store {
            let mut values: Vec<&str> = claims.iter().map(|c| c.value.trim()).collect();
            values.sort_unstable();
            for value in values {
                buf.extend_from_slice(format!("{scope}\u{1f}{key}\u{1f}{value}\n").as_bytes());
            }
        }
        sha256_hex(&buf)
    }

    /// Absorve uma entrada: insere chave nova, ignora repetição de valor, e
    /// preserva divergência (sem LWW).
    fn absorb(&mut self, entry: &DeltaEntry, report: &mut ApplyReport) {
        let key = (entry.scope.clone(), entry.key.clone());
        let exists = self.store.contains_key(&key);
        if !exists {
            self.store.insert(key, vec![entry.claim.clone()]);
            report.accepted += 1;
            return;
        }
        let same = self.store[&key]
            .iter()
            .any(|c| c.same_value(&entry.claim));
        if same {
            report.idempotent += 1;
            return;
        }
        let kept = self.store[&key][0].clone();
        if let Some(slot) = self.store.get_mut(&key) {
            slot.push(entry.claim.clone());
        }
        self.conflicts.push(Conflict {
            scope: entry.scope.clone(),
            key: entry.key.clone(),
            kept,
            incoming: entry.claim.clone(),
        });
        report.conflicts += 1;
    }

    /// Escreve uma claim no oplog próprio e no estado local.
    pub fn remember(&mut self, scope: &str, claim: &Claim) -> Result<DeltaEntry, P2pError> {
        let entry = self.log.append(scope, claim)?;
        let node = self.node.clone();
        self.seen.insert((node.clone(), entry.seq));
        let mut report = ApplyReport::default();
        self.absorb(&entry, &mut report);
        self.vclock.observe(node.as_str(), entry.seq);
        Ok(entry)
    }

    /// Delta do que este nó conhece além de `since`.
    pub fn delta_since(&self, since: &VClock) -> Result<SgdbDelta, P2pError> {
        self.log.delta_since(since)
    }

    /// Delta completo (primeiro contato / replay seguro).
    pub fn delta_all(&self) -> Result<SgdbDelta, P2pError> {
        self.log.delta_all()
    }

    /// Aplica um delta verificado; replay é idempotente.
    pub fn apply(&mut self, delta: &SgdbDelta) -> Result<ApplyReport, P2pError> {
        delta.verify()?;
        let mut report = ApplyReport::default();
        for entry in &delta.entries {
            if !self.seen.insert((delta.node.clone(), entry.seq)) {
                report.idempotent += 1;
                continue;
            }
            self.absorb(entry, &mut report);
        }
        self.vclock.merge(&delta.vclock);
        self.vclock.observe(&delta.node, delta.seq);
        Ok(report)
    }
}

/// Troca bidirecional: cada lado recebe o oplog inteiro do outro (idempotente).
pub fn sync_pair(
    a: &mut Replica,
    b: &mut Replica,
) -> Result<(ApplyReport, ApplyReport), P2pError> {
    let from_a = a.delta_all()?;
    let from_b = b.delta_all()?;
    let a_to_b = b.apply(&from_a)?;
    let b_to_a = a.apply(&from_b)?;
    Ok((a_to_b, b_to_a))
}

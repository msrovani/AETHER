//! `SgdbUndoBackend` — backend de undo **real** para as tools de escrita.
//!
//! Substitui o `MockUndo` de teste: guarda o valor corrente em memória
//! (`BTreeMap`) e um **diário append-only** em JSONL no disco, no espírito do
//! *outbox* do `neural-sgdb` (`AETHER_SGDB_OUTBOX`). Não abre conexão: o
//! backend é in-process e funciona com o banco fora do ar.
//!
//! # Contrato
//!
//! * [`UndoBackend::write`] devolve o *before-image* e registra a intenção no
//!   diário **antes** de mutar o mapa (intenção crash-safe: sem `fsync`, mas
//!   com `flush`).
//! * [`UndoBackend::insert_undo`] faz **compare-and-delete**: só apaga se o
//!   valor corrente ainda é o que o `write` emparelhado inseriu. Escritor
//!   concorrente ⇒ `Err("stale: ...")` e nada é tocado.
//! * [`UndoBackend::restore`] só reescreve o before-image se o valor esperado
//!   (última escrita registrada) ainda for o corrente; senão `Err("stale: ...")`.
//! * [`UndoBackend::stage_rewind`] recua a marca de *stage* **daquela**
//!   transação; nunca toca outra `txn` e é idempotente quando já está na marca.
//! * Autoridade `>= 200` (HITL) nunca é apagada por undo: grava *tombstone*
//!   (`{op:"tombstone", authority}`) e devolve `Err("hitl: ...")` para o host
//!   escalar. Autoridade `255` (aprovada por humano) exige token explícito
//!   ([`SgdbUndoBackend::restore_with_token`] /
//!   [`SgdbUndoBackend::write_snapshot_with_token`]), configurado em
//!   [`SgdbUndoBackend::with_token`] ou `AETHER_UNDO_TOKEN`.
//!
//! # Diário
//!
//! Uma linha JSON por operação (nunca reescrito, só anexado):
//!
//! ```text
//! {"op":"write","key":K,"prev":REC|null,"new":REC}
//! {"op":"delete","key":K,"expected":REC}
//! {"op":"restore","key":K,"prev":REC}
//! {"op":"tombstone","key":K,"authority":N}
//! ```
//!
//! `REC` é o snapshot (`text`, `object`, `authority`, `reinforced`, `scope`,
//! `provenance`) mais `tombstone`. [`SgdbUndoBackend::new`] reaplica o diário
//! ao abrir; [`SgdbUndoBackend::replay`] faz o mesmo sem abrir o backend e
//! devolve quantas operações foram aplicadas. Linhas truncadas (crash no meio
//! do append) são ignoradas.
//!
//! # Escopo
//!
//! O backend é dono de um arquivo. Dois processos no **mesmo** caminho só
//! enxergam as próprias escritas até reabrirem — a marca de *stage* e a
//! expectativa de undo são por processo. Para o hot path, o diário do SGDB
//! continua sendo a fonte durável; este backend é a camada de compensação.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::json::Json;

use super::tool::{Snapshot, UndoBackend};

/// Autoridade de uma escrita vinda da borda (quarentena, sem HITL).
pub const AUTHORITY_QUARANTINE: u8 = 128;

/// A partir daqui o undo **não apaga**: grava tombstone e escala para HITL.
pub const AUTHORITY_HITL: u8 = 200;

/// Autoridade aprovada por humano: exige token para escrita/restauração.
pub const AUTHORITY_HUMAN: u8 = 255;

/// Sufixo aplicado ao caminho do outbox do SGDB para o diário de undo.
const JOURNAL_SUFFIX: &str = ".undo";

/// Arquivo usado quando não há `AETHER_SGDB_OUTBOX` configurado.
const DEFAULT_JOURNAL: &str = "aether_undo.jsonl";

/// Registro armazenado (snapshot + marca de tombstone).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    snapshot: Snapshot,
    tombstone: bool,
}

/// Estado protegido pelo mutex.
#[derive(Clone, Debug, Default)]
struct State {
    /// Valor corrente por chave.
    records: BTreeMap<String, Record>,
    /// Valor criado pelo `write` que inseriu a chave (`prev = None`).
    insert_marks: BTreeMap<String, Snapshot>,
    /// Último valor escrito (expectativa de `restore`).
    last_write: BTreeMap<String, Snapshot>,
    /// Keys staged por transação (watermark LIFO).
    staged: BTreeMap<u64, Vec<String>>,
    /// Marca explícita por transação (índice em `staged[txn]`).
    marks: BTreeMap<u64, usize>,
    /// Token que libera operações em autoridade `255`.
    token: Option<String>,
}

/// Backend de undo durável local (JSONL append-only + `BTreeMap`).
#[derive(Debug)]
pub struct SgdbUndoBackend {
    path: PathBuf,
    state: Mutex<State>,
}

impl SgdbUndoBackend {
    /// Abre (ou cria) o diário no caminho, reaplicando o que já existe.
    ///
    /// Erros de I/O na leitura não derrubam a construção: o backend começa
    /// vazio e as escritas seguintes falham com `io: ...`. Use
    /// [`Self::try_new`] quando o host precisa do erro.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        match Self::try_new(path.clone()) {
            Ok(backend) => backend,
            Err(_) => Self {
                path,
                state: Mutex::new(State::default()),
            },
        }
    }

    /// Como [`Self::new`], mas propaga erro de leitura/criação do diretório.
    pub fn try_new(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let mut state = State::default();
        load_into(&path, &mut state)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    /// Caminho do diário derivado do ambiente.
    ///
    /// Com `AETHER_SGDB_OUTBOX` definido, o diário fica ao lado do outbox
    /// (`<outbox>.undo`) para que o *spool* do SGDB e o undo vivam juntos na
    /// mesma pasta durável. Sem a variável, cai no diretório temporário.
    pub fn from_env() -> Self {
        let path = match std::env::var("AETHER_SGDB_OUTBOX") {
            Ok(outbox) if !outbox.trim().is_empty() => {
                PathBuf::from(format!("{}{JOURNAL_SUFFIX}", outbox.trim()))
            }
            _ => std::env::temp_dir().join(DEFAULT_JOURNAL),
        };
        let backend = Self::new(path);
        if let Ok(token) = std::env::var("AETHER_UNDO_TOKEN") {
            if !token.is_empty() {
                return backend.with_token(token);
            }
        }
        backend
    }

    /// Define o token que libera operações em autoridade `255`.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        if let Ok(state) = self.state.get_mut() {
            state.token = Some(token.into());
        }
        self
    }

    /// Caminho do diário.
    pub fn journal_path(&self) -> &Path {
        &self.path
    }

    /// Reaplica o diário em um estado novo e devolve quantas linhas aplicou.
    ///
    /// É o mesmo caminho usado por [`Self::new`] ao abrir; existe separado para
    /// o teste de crash e para diagnóstico. Linhas inválidas/truncadas são
    /// ignoradas.
    pub fn replay(path: impl AsRef<Path>) -> io::Result<usize> {
        let mut state = State::default();
        load_into(path.as_ref(), &mut state)
    }

    /// Valor corrente (sem tombstones).
    pub fn get(&self, key: &str) -> Option<Snapshot> {
        let state = self.lock();
        state
            .records
            .get(key)
            .filter(|record| !record.tombstone)
            .map(|record| record.snapshot.clone())
    }

    /// `true` se a chave foi marcada como tombstone (HITL, não apagada).
    pub fn is_tombstoned(&self, key: &str) -> bool {
        self.lock()
            .records
            .get(key)
            .is_some_and(|record| record.tombstone)
    }

    /// Chaves vivas.
    pub fn len(&self) -> usize {
        self.lock()
            .records
            .values()
            .filter(|record| !record.tombstone)
            .count()
    }

    /// `true` se não há chave viva.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Escreve um snapshot completo (autoridade/escopo/proveniência do host).
    ///
    /// Sobrescrever um registro `255` exige token (ver
    /// [`Self::write_snapshot_with_token`]).
    pub fn write_snapshot(&self, key: &str, next: Snapshot) -> Result<Option<Snapshot>, String> {
        self.write_snapshot_with_token(key, next, None)
    }

    /// Como [`Self::write_snapshot`], com token para autoridade `255`.
    pub fn write_snapshot_with_token(
        &self,
        key: &str,
        next: Snapshot,
        token: Option<&str>,
    ) -> Result<Option<Snapshot>, String> {
        self.write_inner(key, token, |_prev| next)
    }

    /// Restaura `prev` sem token (recusa autoridade `255`).
    pub fn restore_plain(&self, key: &str, prev: &Snapshot) -> Result<(), String> {
        self.restore_inner(key, prev, None)
    }

    /// Restaura `prev` com token explícito (libera autoridade `255`).
    pub fn restore_with_token(
        &self,
        key: &str,
        prev: &Snapshot,
        token: Option<&str>,
    ) -> Result<(), String> {
        self.restore_inner(key, prev, token)
    }

    /// Registra keys staged por uma transação (watermark LIFO).
    pub fn stage(&self, txn: u64, keys: &[String]) {
        let mut state = self.lock();
        let entry = state.staged.entry(txn).or_default();
        entry.extend(keys.iter().cloned());
    }

    /// Registra a marca explícita de `txn` (índice no *outbox* da transação).
    pub fn stage_mark(&self, txn: u64, outbox_len: usize) {
        self.lock().marks.insert(txn, outbox_len);
    }

    /// Quantas keys a transação tem staged agora.
    pub fn staged_len(&self, txn: u64) -> usize {
        self.lock().staged.get(&txn).map(Vec::len).unwrap_or(0)
    }

    /// Trava o estado (um lock envenenado não derruba o backend).
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Núcleo do `write`: decide o próximo snapshot sob o mesmo lock.
    fn write_inner<F>(
        &self,
        key: &str,
        token: Option<&str>,
        next_of: F,
    ) -> Result<Option<Snapshot>, String>
    where
        F: FnOnce(Option<&Record>) -> Snapshot,
    {
        if key.trim().is_empty() {
            return Err("args: chave vazia".into());
        }
        let mut state = self.lock();
        let prev = state.records.get(key).cloned();
        if let Some(current) = &prev {
            if current.snapshot.authority == AUTHORITY_HUMAN && !token_ok(&state, token) {
                return Err(format!(
                    "hitl: authority={} exige token",
                    current.snapshot.authority
                ));
            }
        }
        let next = Record {
            snapshot: next_of(prev.as_ref()),
            tombstone: false,
        };
        let line = Json::obj([
            ("op", Json::str("write")),
            ("key", Json::str(key)),
            (
                "prev",
                prev.as_ref().map(record_to_json).unwrap_or(Json::Null),
            ),
            ("new", record_to_json(&next)),
        ]);
        // Intenção antes da mutação: um crash aqui deixa o diário coerente.
        self.append_line(&line)?;
        state.records.insert(key.to_string(), next.clone());
        if prev.is_none() {
            state
                .insert_marks
                .insert(key.to_string(), next.snapshot.clone());
        }
        state
            .last_write
            .insert(key.to_string(), next.snapshot.clone());
        Ok(prev.map(|record| record.snapshot))
    }

    /// Núcleo do `restore`: compare-and-swap contra a expectativa do diário.
    fn restore_inner(&self, key: &str, prev: &Snapshot, token: Option<&str>) -> Result<(), String> {
        let mut state = self.lock();
        if prev.authority == AUTHORITY_HUMAN && !token_ok(&state, token) {
            return Err("hitl: authority=255 exige token".into());
        }
        let current = state.records.get(key).cloned();
        let expected = state.last_write.get(key).cloned();
        match (current, expected) {
            (Some(current), Some(expected))
                if !current.tombstone && current.snapshot == expected => {}
            (None, None) => return Err(format!("stale: `{key}` não tem valor a restaurar")),
            _ => {
                return Err(format!(
                    "stale: `{key}` mudou desde a escrita (undo recusado)"
                ))
            }
        }
        let record = Record {
            snapshot: prev.clone(),
            tombstone: false,
        };
        let line = Json::obj([
            ("op", Json::str("restore")),
            ("key", Json::str(key)),
            ("prev", record_to_json(&record)),
        ]);
        self.append_line(&line)?;
        state.records.insert(key.to_string(), record.clone());
        state.last_write.insert(key.to_string(), record.snapshot);
        Ok(())
    }

    /// Anexa uma linha ao diário (append + `flush`, sem `fsync`).
    fn append_line(&self, line: &Json) -> Result<(), String> {
        let mut text = line.to_text();
        text.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("io: abrir diário {}: {e}", self.path.display()))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|e| format!("io: gravar diário {}: {e}", self.path.display()))
    }
}

impl UndoBackend for SgdbUndoBackend {
    fn insert_undo(&self, key: &str) -> Result<(), String> {
        let mut state = self.lock();
        let Some(current) = state.records.get(key).cloned() else {
            // Nada a apagar: insert já desfeito (ou nunca existiu).
            return Ok(());
        };
        if current.snapshot.authority >= AUTHORITY_HITL {
            // HITL: tombstone em vez de apagar; o host escala.
            if !current.tombstone {
                let line = Json::obj([
                    ("op", Json::str("tombstone")),
                    ("key", Json::str(key)),
                    (
                        "authority",
                        Json::num(f64::from(current.snapshot.authority)),
                    ),
                ]);
                self.append_line(&line)?;
                if let Some(record) = state.records.get_mut(key) {
                    record.tombstone = true;
                }
            }
            return Err(format!(
                "hitl: authority>={AUTHORITY_HITL} (chave `{key}` tombstoned)"
            ));
        }
        let Some(mark) = state.insert_marks.get(key).cloned() else {
            return Err(format!("stale: `{key}` sem marca de insert"));
        };
        if current.tombstone || current.snapshot != mark {
            return Err(format!(
                "stale: `{key}` tem valor de outro escritor (compare-and-delete recusado)"
            ));
        }
        let line = Json::obj([
            ("op", Json::str("delete")),
            ("key", Json::str(key)),
            ("expected", record_to_json(&current)),
        ]);
        self.append_line(&line)?;
        state.records.remove(key);
        state.insert_marks.remove(key);
        state.last_write.remove(key);
        Ok(())
    }

    fn restore(&self, key: &str, prev: &Snapshot) -> Result<(), String> {
        self.restore_inner(key, prev, None)
    }

    fn stage_rewind(&self, txn: u64, keys: &[String]) -> Result<(), String> {
        let mut state = self.lock();
        let len = state.staged.get(&txn).map(Vec::len).unwrap_or(0);
        // Marca explícita (`stage_mark`) ou inferida pelo tamanho do lote.
        let mark = state
            .marks
            .get(&txn)
            .copied()
            .unwrap_or_else(|| len.saturating_sub(keys.len()))
            .min(len);
        if len <= mark {
            state.marks.insert(txn, mark);
            return Ok(());
        }
        if let Some(entry) = state.staged.get_mut(&txn) {
            entry.truncate(mark);
        }
        state.marks.insert(txn, mark);
        Ok(())
    }

    fn write(&self, key: &str, text: &str) -> Result<Option<Snapshot>, String> {
        self.write_inner(key, None, |prev| {
            let (reinforced, scope, provenance) = match prev {
                Some(previous) => (
                    previous.snapshot.reinforced.saturating_add(1),
                    previous.snapshot.scope.clone(),
                    previous.snapshot.provenance.clone(),
                ),
                None => (0, String::new(), String::new()),
            };
            Snapshot {
                text: text.to_string(),
                object: text.to_string(),
                authority: AUTHORITY_QUARANTINE,
                reinforced,
                scope,
                provenance,
            }
        })
    }
}

/// Token configurado confere com o informado?
fn token_ok(state: &State, token: Option<&str>) -> bool {
    match (state.token.as_deref(), token) {
        (Some(want), Some(got)) => !want.is_empty() && want == got.trim(),
        _ => false,
    }
}

fn record_to_json(record: &Record) -> Json {
    Json::obj([
        ("text", Json::str(record.snapshot.text.as_str())),
        ("object", Json::str(record.snapshot.object.as_str())),
        ("authority", Json::num(f64::from(record.snapshot.authority))),
        ("reinforced", Json::num(record.snapshot.reinforced as f64)),
        ("scope", Json::str(record.snapshot.scope.as_str())),
        ("provenance", Json::str(record.snapshot.provenance.as_str())),
        ("tombstone", Json::Bool(record.tombstone)),
    ])
}

fn snapshot_from_json(value: &Json) -> Option<Snapshot> {
    Some(Snapshot {
        text: value.get("text")?.as_str()?.to_string(),
        object: value.get("object")?.as_str()?.to_string(),
        authority: u8::try_from(value.get("authority")?.as_u64()?).ok()?,
        reinforced: value.get("reinforced")?.as_u64()?,
        scope: value.get("scope")?.as_str()?.to_string(),
        provenance: value.get("provenance")?.as_str()?.to_string(),
    })
}

fn record_from_json(value: &Json) -> Option<Record> {
    Some(Record {
        snapshot: snapshot_from_json(value)?,
        tombstone: value.get("tombstone").and_then(Json::as_bool) == Some(true),
    })
}

/// Aplica uma linha do diário; `false` = linha desconhecida/parcial.
fn apply_line(line: &Json, state: &mut State) -> bool {
    let (Some(op), Some(key)) = (
        line.get("op").and_then(Json::as_str),
        line.get("key").and_then(Json::as_str),
    ) else {
        return false;
    };
    let key = key.to_string();
    match op {
        "write" => {
            let Some(new) = line.get("new").and_then(record_from_json) else {
                return false;
            };
            let prev = line.get("prev").and_then(record_from_json);
            state.records.insert(key.clone(), new.clone());
            if prev.is_none() {
                state.insert_marks.insert(key.clone(), new.snapshot.clone());
            }
            state.last_write.insert(key, new.snapshot);
            true
        }
        "delete" => {
            state.records.remove(&key);
            state.insert_marks.remove(&key);
            state.last_write.remove(&key);
            true
        }
        "restore" => {
            let Some(prev) = line.get("prev").and_then(record_from_json) else {
                return false;
            };
            state.records.insert(key.clone(), prev.clone());
            state.last_write.insert(key, prev.snapshot);
            true
        }
        "tombstone" => {
            let Some(authority) = line
                .get("authority")
                .and_then(Json::as_u64)
                .and_then(|n| u8::try_from(n).ok())
            else {
                return false;
            };
            if let Some(record) = state.records.get_mut(&key) {
                record.tombstone = true;
                record.snapshot.authority = authority;
            }
            true
        }
        _ => false,
    }
}

/// Lê o diário e reaplica as operações em `state`.
fn load_into(path: &Path, state: &mut State) -> io::Result<usize> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut applied = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Linha truncada (crash no meio do append) é descartada.
        let Ok(value) = Json::parse(line) else {
            continue;
        };
        if apply_line(&value, state) {
            applied += 1;
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Diário temporário com nome único; some no `Drop`.
    struct TempJournal {
        path: PathBuf,
    }

    impl TempJournal {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "aether_undo_{tag}_{}_{}.jsonl",
                std::process::id(),
                nanos
            ));
            let _ = std::fs::remove_file(&path);
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn backend(&self) -> SgdbUndoBackend {
            SgdbUndoBackend::new(self.path.clone())
        }
    }

    impl Drop for TempJournal {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn snap(text: &str, authority: u8) -> Snapshot {
        Snapshot {
            text: text.to_string(),
            object: text.to_string(),
            authority,
            reinforced: 0,
            scope: "aether/test".to_string(),
            provenance: "test".to_string(),
        }
    }

    #[test]
    fn write_devolve_before_image_e_persiste() {
        let journal = TempJournal::new("before");
        let backend = journal.backend();

        assert!(backend.write("k", "A").unwrap().is_none());
        let prev = backend.write("k", "B").unwrap().expect("before-image");
        assert_eq!(prev.text, "A");
        assert_eq!(prev.object, "A");
        assert_eq!(prev.authority, AUTHORITY_QUARANTINE);
        assert_eq!(prev.reinforced, 0);

        let now = backend.get("k").expect("valor corrente");
        assert_eq!(now.text, "B");
        assert_eq!(now.reinforced, 1, "reforço conta a reescrita");
        assert_eq!(backend.len(), 1);
    }

    #[test]
    fn insert_undo_remove_o_proprio_valor() {
        let journal = TempJournal::new("insert");
        let backend = journal.backend();
        backend.write("k", "A").unwrap();

        backend.insert_undo("k").unwrap();
        assert!(backend.get("k").is_none());
        assert!(backend.is_empty());
        // Idempotente: a chave já não existe.
        backend.insert_undo("k").unwrap();
    }

    #[test]
    fn insert_undo_recusa_valor_estrangeiro() {
        let journal = TempJournal::new("stale");
        let backend = journal.backend();
        backend.write("k", "A").unwrap();
        // Outro escritor grava por cima depois do insert.
        let prev = backend.write("k", "B").unwrap().expect("before-image");
        assert_eq!(prev.text, "A");

        let error = backend.insert_undo("k").expect_err("deve recusar");
        assert!(error.starts_with("stale"), "{error}");
        let kept = backend.get("k").expect("nada foi apagado");
        assert_eq!(kept.text, "B", "compare-and-delete não toca valor alheio");
    }

    #[test]
    fn restore_faz_roundtrip_e_recusa_stale() {
        let journal = TempJournal::new("restore");
        let backend = journal.backend();
        backend.write("k", "A").unwrap();
        let before = backend.write("k", "B").unwrap().expect("before-image");

        backend.restore("k", &before).unwrap();
        assert_eq!(backend.get("k").unwrap().text, "A");

        // Escritor concorrente muda o valor por fora do backend.
        if let Some(record) = backend.lock().records.get_mut("k") {
            record.snapshot.text = "Z".to_string();
            record.snapshot.object = "Z".to_string();
        }
        let error = backend.restore("k", &before).expect_err("deve recusar");
        assert!(error.starts_with("stale"), "{error}");
        assert_eq!(backend.get("k").unwrap().text, "Z", "valor alheio intacto");

        // Restaurar o que já foi apagado não tem expectativa: stale.
        backend.insert_undo("novo").unwrap();
        let error = backend
            .restore_plain("novo", &snap("X", AUTHORITY_QUARANTINE))
            .expect_err("deve recusar");
        assert!(error.starts_with("stale"), "{error}");
    }

    #[test]
    fn tombstone_para_authority_200() {
        let journal = TempJournal::new("tombstone");
        let backend = journal.backend();
        backend
            .write_snapshot("hitl", snap("aprovado", AUTHORITY_HITL))
            .unwrap();

        let error = backend.insert_undo("hitl").expect_err("HITL escala");
        assert!(error.contains("hitl"), "{error}");
        assert!(
            backend.is_tombstoned("hitl"),
            "registro marcado, não apagado"
        );
        assert!(backend.get("hitl").is_none(), "tombstone não é valor vivo");
        assert_eq!(backend.len(), 0);
        // Repetir não duplica tombstone nem apaga o fato.
        let error = backend.insert_undo("hitl").expect_err("HITL escala");
        assert!(error.contains("hitl"), "{error}");
        assert!(backend.is_tombstoned("hitl"));
    }

    #[test]
    fn authority_255_exige_token() {
        let journal = TempJournal::new("human");
        let backend = journal.backend().with_token("segredo");
        backend
            .write_snapshot("humano", snap("aprovado", AUTHORITY_HUMAN))
            .unwrap();

        assert!(
            backend.write("humano", "outro").is_err(),
            "sobrescrever 255 sem token é recusado"
        );
        backend
            .write_snapshot_with_token("humano", snap("v2", AUTHORITY_HUMAN), Some("segredo"))
            .unwrap();

        let prev = snap("aprovado", AUTHORITY_HUMAN);
        assert!(
            backend.restore("humano", &prev).is_err(),
            "restore 255 exige token"
        );
        backend
            .restore_with_token("humano", &prev, Some("segredo"))
            .unwrap();
        assert_eq!(backend.get("humano").unwrap().text, "aprovado");

        // Nem com token o undo apaga um fato aprovado por humano.
        let error = backend.insert_undo("humano").expect_err("HITL escala");
        assert!(error.contains("hitl"), "{error}");
        assert!(backend.is_tombstoned("humano"));
        assert!(
            backend
                .restore_with_token("humano", &prev, Some("segredo"))
                .is_err(),
            "tombstone não volta por undo"
        );
    }

    #[test]
    fn stage_rewind_e_idempotente_e_nao_toca_outra_txn() {
        let journal = TempJournal::new("stage");
        let backend = journal.backend();
        let keys = vec!["a".to_string(), "b".to_string()];
        let other = vec!["z".to_string()];

        backend.stage(7, &keys);
        backend.stage(8, &other);
        assert_eq!(backend.staged_len(7), 2);

        backend.stage_rewind(7, &keys).unwrap();
        assert_eq!(backend.staged_len(7), 0);
        assert_eq!(backend.staged_len(8), 1, "outra txn intacta");
        // Já na marca: idempotente.
        backend.stage_rewind(7, &keys).unwrap();
        assert_eq!(backend.staged_len(7), 0);

        // Marca explícita acima do tamanho atual: também é no-op.
        backend.stage_mark(8, 99);
        backend.stage_rewind(8, &other).unwrap();
        assert_eq!(backend.staged_len(8), 1);
        assert_eq!(backend.staged_len(7), 0);
    }

    #[test]
    fn replay_reconstroi_apos_reabrir() {
        let journal = TempJournal::new("replay");
        {
            let backend = journal.backend();
            backend.write("k", "A").unwrap();
            backend.write("k", "B").unwrap();
            assert_eq!(backend.len(), 1);
        }

        let applied = SgdbUndoBackend::replay(journal.path()).expect("replay");
        assert_eq!(applied, 2, "duas linhas de write no diário");

        let reopened = journal.backend();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened.get("k").unwrap().text, "B");
        // A marca de insert sobrevive ao crash: o valor mudou, então é stale.
        let error = reopened.insert_undo("k").expect_err("stale após replay");
        assert!(error.starts_with("stale"), "{error}");

        // E uma inserção nova feita depois do reopen continua desfazível.
        reopened.write("novo", "N").unwrap();
        reopened.insert_undo("novo").unwrap();
        assert!(reopened.get("novo").is_none());
        assert_eq!(reopened.len(), 1);
    }
}

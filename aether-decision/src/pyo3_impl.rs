//! Binding FFI PyO3 (feature `pyo3`): embed do `vegaml` no processo Rust.
//!
//! Decisões de desenho (plano F2→F3, decisões HITL):
//! - interpreter inicializado uma vez (`auto-initialize`), readers no mesmo
//!   processo (~4–4.5GB de 6GB);
//! - exceção Python vira `DecisionError::EngineUnavailable` — nunca panic
//!   para cima do chat (Eixo E);
//! - versões pinadas: python 3.14.10 (venv `target/vega`) + torch 2.14.1+cu126;
//! - PyO3 0.30: `Python::attach` (era `with_gil`), `cast` (era `downcast`),
//!   `IntoPyObject` (era `to_object`).
//!
//! LIMITAÇÃO DECLARADA (honestidade > comentário bonito): por enquanto o GIL
//! fica retido durante o `decide()` — as decisões do chat são serializadas
//! por turno, então o custo prático é zero hoje. Liberação real do GIL
//! (`Python::detach` em torno do forward torch) fica para a fatia de azeite
//! (Eixo D), medida com re-probe n=30 antes/depois.

use crate::config::EngineConfig;
use crate::port::DecisionPort;
use crate::types::{Answer, DecisionError, DecisionMeta, DecisionRequest, DecisionResponse};
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;

/// Pina o `site-packages` do venv no `sys.path` **antes** do `import vegaml`.
///
/// O embed (`auto-initialize`) resolve o Python base da maquina: o venv do
/// engine (`target/vega`) nao entra no `sys.path` sozinho. Sem este passo o
/// `import vegaml` falha e o port degrada para `engine_unavailable` para
/// sempre — erro que culpa o motor quando o problema e o caminho.
///
/// Ordem de resolucao (`EngineConfig::effective_site_packages`):
/// config -> `PYTHONPATH` -> derivado de `VEGA_PYTHON`.
///
/// Devolve os caminhos inseridos. `Ok(None)` = nada configurado (ai o erro
/// de import e honestamente "nada configurado", nao "motor quebrado").
pub fn pin_sys_path(py: Python<'_>, cfg: &EngineConfig) -> Result<Option<String>, DecisionError> {
    let Some(raw) = cfg.effective_site_packages() else {
        return Ok(None);
    };
    // `PYTHONPATH` pode trazer varios caminhos; o separador e o do host.
    let existing: Vec<String> = std::env::split_paths(&raw)
        .filter(|p| p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if existing.is_empty() {
        return Err(DecisionError::EngineUnavailable(format!(
            "python_site_packages nao existe: {raw} \
             (configure VEGA_SITE_PACKAGES, PYTHONPATH ou VEGA_PYTHON)"
        )));
    }
    let sys = py.import("sys").map_err(py_err_to_engine)?;
    let path = sys.getattr("path").map_err(py_err_to_engine)?;
    // Reverso para que o primeiro caminho configurado fique na frente.
    for entry in existing.iter().rev() {
        path.call_method1("insert", (0, entry))
            .map_err(py_err_to_engine)?;
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    Ok(Some(existing.join(sep)))
}

/// Port real: primário (+ sentinela quando configurada), single interpreter.
pub struct VegaPort {
    primary: Py<PyAny>,
    sentinel: Option<Py<PyAny>>,
    engine_label: String,
}

impl VegaPort {
    /// Carrega os readers no interpreter embutido. O fit de calibração é
    /// executado aqui (determinístico, ~1–2 min no start — bootstrap).
    pub fn new(cfg: &EngineConfig) -> PyResult<Self> {
        Python::attach(|py| {
            // Pin de runtime (F1): o `site-packages` do venv entra no
            // `sys.path` ANTES do import. Sem isso `vegaml` some e o port
            // abste para sempre com `engine_unavailable`.
            pin_sys_path(py, cfg)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            // G1 (Eixo D): cache de features de fit em disco. Falha de cache
            // nunca impede o bootstrap — degrada para leitura cheia.
            let tools_dir = feature_tools_dir(cfg);
            if let Some(tools) = &tools_dir {
                let _ = add_sys_path(py, tools);
            }
            let vegaml = py.import("vegaml").map_err(|e| {
                let hint = match cfg.effective_site_packages() {
                    Some(p) => format!("vegaml nao importou com site-packages '{p}'"),
                    None => "vegaml nao importou: nenhum site-packages configurado \
                             (VEGA_SITE_PACKAGES, PYTHONPATH ou VEGA_PYTHON)"
                        .to_string(),
                };
                pyo3::exceptions::PyModuleNotFoundError::new_err(format!("{hint}: {e}"))
            })?;
            let primary = match &cfg.model_primary {
                Some(m) => vegaml.call_method1("load", (m,))?,
                None => vegaml.call_method0("load")?,
            };
            if let Some(cal) = &cfg.calibration {
                install_feature_cache(py, &primary, cal, "primary", tools_dir.as_deref())?;
                install_engine_cache(py, &primary, cal, "primary", tools_dir.as_deref())?;
                fit_from_file(py, &primary, cal)?;
                flush_feature_cache(py, tools_dir.as_deref());
            }
            let sentinel = match &cfg.model_sentinel {
                Some(m) => {
                    let s = vegaml.call_method1("load", (m,))?;
                    if let Some(cal) = &cfg.calibration {
                        install_feature_cache(py, &s, cal, "sentinel", tools_dir.as_deref())?;
                        install_engine_cache(py, &s, cal, "sentinel", tools_dir.as_deref())?;
                        fit_from_file(py, &s, cal)?;
                        flush_feature_cache(py, tools_dir.as_deref());
                    }
                    Some(s)
                }
                None => None,
            };
            let engine_label = match sentinel {
                Some(_) => "vega-u-0.8b AND gemma-prose".to_string(),
                None => "vega-u-0.8b".to_string(),
            };
            Ok(Self {
                primary: primary.into(),
                sentinel: sentinel.map(Into::into),
                engine_label,
            })
        })
    }

    /// Decide num reader e converte o dict do vegaml para `Answer` tipado.
    fn decide_reader(
        py: Python<'_>,
        v: &Bound<'_, PyAny>,
        state: &str,
        questions: &serde_json::Value,
    ) -> Result<Vec<Answer>, DecisionError> {
        let questions_py = json_to_py(py, questions)
            .map_err(|e| DecisionError::Contract(format!("questions: {e}")))?;
        let out = v
            .call_method1("decide", (state, questions_py))
            .map_err(py_err_to_engine)?;
        let answers_obj = out
            .get_item("answers")
            .map_err(|e| DecisionError::Contract(format!("answers ausente: {e}")))?;
        let mut vec = Vec::new();
        for qid in questions
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            let a = answers_obj
                .get_item(&qid)
                .map_err(|e| DecisionError::Contract(format!("answer[{qid}]: {e}")))?;
            vec.push(py_answer_to_answer(py, &a, &qid)?);
        }
        Ok(vec)
    }
}

impl DecisionPort for VegaPort {
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        let t0 = std::time::Instant::now();
        let (primary_answers, sentinel_answers) = Python::attach(|py| {
            let p = Self::decide_reader(py, self.primary.bind(py), &req.state, &req.questions)?;
            let s = match &self.sentinel {
                Some(s) => Some(Self::decide_reader(
                    py,
                    s.bind(py),
                    &req.state,
                    &req.questions,
                )?),
                None => None,
            };
            Result::<_, DecisionError>::Ok((p, s))
        })?;

        // Sentinela falhou onde o primário passou = engine instável — honesto.
        if self.sentinel.is_some() != sentinel_answers.is_some() {
            return Err(DecisionError::EngineUnavailable(
                "sentinela falhou onde primário passou".into(),
            ));
        }

        let answers = match sentinel_answers {
            Some(senti) => primary_answers
                .into_iter()
                .zip(senti)
                .map(|(p, s)| {
                    debug_assert_eq!(p.question, s.question, "zip por índice: mesma ordem");
                    crate::gate::combine(&p, &s)
                })
                .collect(),
            None => primary_answers,
        };

        let conformal_set = conformal_map(&answers);
        Ok(DecisionResponse {
            answers,
            // F1-2 (MSG 0793): o vegaml expoe `prediction_set` hoje; vira o
            // conjunto conformal do lote, sem interpretacao.
            conformal_set,
            meta: DecisionMeta {
                latency_ms: t0.elapsed().as_secs_f64() * 1000.0,
                engine: self.engine_label.clone(),
                degraded: false,
                vram_used_mb: None,
            },
        })
    }
}

#[cfg(test)]
mod ffi_contract_tests {
    use super::*;
    use crate::types::{AbstainReason, Answer};

    /// F1-2 (1/2): `prediction_set` e uma sequencia do Python. Sem o braco de
    /// iteravel a conversao devolvia `None` em silencio — este teste tranca o
    /// caminho que perdeu o dado (lista de criterios, nao de bools).
    #[test]
    fn prediction_set_sequence_becomes_conformal_array() {
        pyo3::Python::attach(|py| {
            let list = pyo3::types::PyList::empty(py);
            list.append("reformular").unwrap();
            list.append("responder").unwrap();
            let v = json_from_py(py, list.as_any()).expect("sequencia deve converter");
            assert_eq!(v, serde_json::json!(["reformular", "responder"]));
        });
    }

    /// F1-2 (2/2): o conjunto do lote e chaveado por pergunta e nao inventa
    /// entrada para quem nao produziu conjunto.
    #[test]
    fn conformal_map_is_keyed_by_question_and_skips_absent() {
        let mut com_set = Answer::abstained("passou", AbstainReason::Ambiguous);
        com_set.conformal_set = Some(serde_json::json!([false, true]));
        let sem_set = Answer::abstained("route", AbstainReason::Ambiguous);

        let map = conformal_map(&[com_set, sem_set]).expect("um conjunto presente");
        assert_eq!(map["passou"], serde_json::json!([false, true]));
        assert!(map.get("route").is_none(), "nao inventamos conjunto");
    }

    /// Pin de runtime: o caminho configurado entra na FRENTE do `sys.path`,
    /// que e o que faz o `import vegaml` achar o venv do engine.
    #[test]
    fn pin_sys_path_inserts_the_configured_site_packages() {
        let dir = std::env::temp_dir().join("aether_pin_site_packages");
        std::fs::create_dir_all(&dir).unwrap();
        let mut cfg = EngineConfig::default();
        cfg.python_site_packages = Some(dir.to_string_lossy().to_string());

        pyo3::Python::attach(|py| {
            let pinned = pin_sys_path(py, &cfg)
                .expect("pin ok")
                .expect("caminho configurado e existente");
            assert!(pinned.contains("aether_pin_site_packages"), "{pinned}");
            let sys = py.import("sys").unwrap();
            let head: String = sys
                .getattr("path")
                .unwrap()
                .get_item(0)
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(head, dir.to_string_lossy(), "deve entrar na frente");
        });
    }

    /// Caminho errado falha com mensagem acionavel — nao com "motor quebrado".
    #[test]
    fn pin_sys_path_rejects_a_path_that_does_not_exist() {
        let mut cfg = EngineConfig::default();
        cfg.python_site_packages = Some("target/nao_existe_site_packages".into());
        pyo3::Python::attach(|py| {
            let err = pin_sys_path(py, &cfg).expect_err("deve falhar");
            let msg = err.to_string();
            assert!(msg.contains("nao existe"), "{msg}");
            assert!(msg.contains("target/nao_existe_site_packages"), "{msg}");
        });
    }

    /// Sem config o pin e no-op: o erro de import continua sendo do vegaml.
    #[test]
    fn pin_sys_path_is_a_noop_without_config() {
        let cfg = EngineConfig::default();
        pyo3::Python::attach(|py| {
            assert!(pin_sys_path(py, &cfg).unwrap().is_none());
        });
    }
}

/// Mapa `{pergunta: prediction_set}` — o conjunto conformal do lote.
///
/// F1-2 (MSG 0793): o vegaml devolve `prediction_set` por pergunta (lista de
/// candidatos). O `combine` do AND gate preserva o conjunto do primario, entao
/// aqui e so agregar: pergunta sem conjunto fica fora (nao inventamos `null`).
fn conformal_map(answers: &[Answer]) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for a in answers {
        if let Some(set) = &a.conformal_set {
            map.insert(a.question.clone(), set.clone());
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

fn py_err_to_engine(e: PyErr) -> DecisionError {
    // Eixo E: exceção Python NUNCA sobe como panic — vira abstain tipado.
    DecisionError::EngineUnavailable(format!("{e}"))
}

/// Diretório do cache de features (G1): `VEGA_FEATURE_CACHE` vence; default =
/// `<dir do corpus>/../tools` (o repo — o módulo `vega_disk_cache.py` mora lá).
fn feature_tools_dir(cfg: &EngineConfig) -> Option<std::path::PathBuf> {
    if let Ok(v) = std::env::var("VEGA_FEATURE_TOOLS") {
        if !v.trim().is_empty() {
            return Some(std::path::PathBuf::from(v));
        }
    }
    let cal = cfg.calibration.as_ref()?;
    let p = std::path::Path::new(cal);
    // <root>/data/<corpus>.json → <root>/tools
    p.parent()?.parent().map(|root| root.join("tools"))
}

/// Insere `dir` no `sys.path` do interpreter embutido (idempotente o bastante).
fn add_sys_path(py: Python<'_>, dir: &std::path::Path) -> PyResult<()> {
    let sys = py.import("sys")?;
    let path = sys.getattr("path")?;
    let entry = dir.to_string_lossy().to_string();
    // Já presente? Não duplica.
    let contains = path.call_method0("__contains__")?;
    let has = contains.call1((entry.clone(),))?.extract::<bool>()?;
    if !has {
        path.call_method1("insert", (0, entry))?;
    }
    Ok(())
}

/// Instala o cache de features de disco (G1) para um reader. Falha de cache
/// (módulo ausente, path errado) é ABAFADA com aviso: o bootstrap segue com
/// leitura cheia — cache é azeite, não fundação.
fn install_feature_cache(
    py: Python<'_>,
    reader: &Bound<'_, PyAny>,
    calibration: &str,
    tag: &str,
    tools_dir: Option<&std::path::Path>,
) -> PyResult<()> {
    match tools_dir {
        Some(t) if t.join("vega_disk_cache.py").exists() => {
            eprintln!("[aether-decision] G1: instalando cache de features tag={tag} tools={t:?}");
        }
        Some(t) => {
            eprintln!("[aether-decision] G1: modulo vega_disk_cache ausente em {t:?} — sem cache");
            return Ok(());
        }
        None => {
            eprintln!("[aether-decision] G1: tools_dir None — sem cache");
            return Ok(());
        }
    }
    let tools = tools_dir.unwrap();
    let cal_dir = std::path::Path::new(calibration)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_string_lossy()
        .to_string();
    let cache_dir = std::env::var("VEGA_FEATURE_CACHE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{cal_dir}{}feature_cache",
                std::path::MAIN_SEPARATOR
            )
        });
    let tools_str = tools.to_string_lossy().to_string();
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let script = format!(
        "import sys as _s; _s.path.insert(0, r'{t}'); \
import vega_disk_cache as _c; \
_c.install(_vega_reader, r'{cd}{sp}feat_{tg}.pkl'); \
import sys; print('[G1] cache instalado:', _c.stats(), flush=True)",
        t = tools_str,
        cd = cache_dir,
        sp = sep,
        tg = tag,
    );
    // Publica o reader como global temporário e roda o install.
    let globals = pyo3::types::PyDict::new(py);
    globals.set_item("_vega_reader", reader)?;
    globals.set_item("__builtins__", py.import("builtins")?)?;
    py.run(
        std::ffi::CString::new(script.as_str())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?
            .as_c_str(),
        Some(&globals),
        None,
    )
    .map_err(|e| {
        eprintln!("[aether-decision] aviso: cache de features nao instalado ({e})");
        e
    })
}

/// Fingerprint do motor para o cache de respostas (G1, camada 2): muda o
/// checkpoint ou o corpus → cache invalidado. Barato: sizes+mtimes.
fn engine_fingerprint(cfg: &EngineConfig) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in [
        cfg.model_primary.as_deref().unwrap_or("hub-default"),
        cfg.model_sentinel.as_deref().unwrap_or(""),
        cfg.calibration.as_deref().unwrap_or(""),
    ] {
        std::hash::Hash::hash(&p, &mut h);
        if let Ok(meta) = std::fs::metadata(p) {
            std::hash::Hash::hash(&meta.len(), &mut h);
            if let Ok(m) = meta.modified() {
                if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                    std::hash::Hash::hash(&d.as_secs(), &mut h);
                }
            }
        }
        std::hash::Hash::hash(&0u8, &mut h);
    }
    format!("{:016x}", std::hash::Hasher::finish(&h))
}

/// Instala o cache de RESPOSTAS do engine (G1 camada 2) — a leitura 2 do fit.
fn install_engine_cache(
    py: Python<'_>,
    reader: &Bound<'_, PyAny>,
    calibration: &str,
    tag: &str,
    tools_dir: Option<&std::path::Path>,
) -> PyResult<()> {
    let Some(tools) = tools_dir else { return Ok(()) };
    let tools_str = tools.to_string_lossy().to_string();
    let cal_dir = std::path::Path::new(calibration)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_string_lossy()
        .to_string();
    let cache_dir = std::env::var("VEGA_FEATURE_CACHE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{cal_dir}{}feature_cache",
                std::path::MAIN_SEPARATOR
            )
        });
    // O fingerprint usa a cfg do PRÓPRIO reader via closure do Python? Não:
    // o fingerprint é computado aqui (paths já são os deste reader).
    let fp = engine_fingerprint_of_reader(reader, calibration);
    let script = format!(
        "import sys as _s; _s.path.insert(0, r'{t}'); \
import vega_disk_cache as _c; \
_c.install_engine(_vega_reader, r'{cd}{sp}eng_{tg}.pkl', r'{fp}'); \
print('[G1] engine cache ok:', _c.stats(), flush=True)",
        t = tools_str,
        cd = cache_dir,
        sp = std::path::MAIN_SEPARATOR,
        tg = tag,
        fp = fp,
    );
    let globals = pyo3::types::PyDict::new(py);
    globals.set_item("_vega_reader", reader)?;
    if let Ok(b) = py.import("builtins") {
        let _ = globals.set_item("__builtins__", b);
    }
    if let Err(e) = py.run(
        match std::ffi::CString::new(script.as_str()) {
            Ok(cs) => cs,
            Err(e) => {
                eprintln!("[aether-decision] aviso: engine cache falhou ({e})");
                return Ok(());
            }
        }
        .as_c_str(),
        Some(&globals),
        None,
    ) {
        eprintln!("[aether-decision] aviso: engine cache nao instalado ({e})");
    }
    Ok(())
}

/// Fingerprint por reader: hash dos paths reais que o vegaml usou (o reader
/// sabe o próprio diretorio via config do vegaml; do lado Rust usamos os
/// caminhos conhecidos do cfg + o caminho do corpus).
fn engine_fingerprint_of_reader(
    _reader: &Bound<'_, PyAny>,
    calibration: &str,
) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&calibration, &mut h);
    if let Ok(text) = std::fs::read_to_string(calibration) {
        use std::hash::Hasher;
        h.write(text.as_bytes());
    }
    format!("{:016x}", std::hash::Hasher::finish(&h))
}

/// Persiste o cache de features (G1) — o atexit do Python NÃO roda quando o
/// processo Rust sai sem finalizar o interpreter (achado do ciclo), então o
/// hospedeiro chama o flush explicitamente após cada fit.
fn flush_feature_cache(py: Python<'_>, tools_dir: Option<&std::path::Path>) {    let Some(tools) = tools_dir else { return };
    let tools_str = tools.to_string_lossy().to_string();
    let script = format!(
        "import sys as _s; _s.path.insert(0, r'{t}'); \
import vega_disk_cache as _c; \
_n = _c.flush(); \
print('[G1] flush:', _n, 'arquivo(s)', _c.stats(), flush=True)",
        t = tools_str,
    );
    let globals = pyo3::types::PyDict::new(py);
    if let Ok(b) = py.import("builtins") {
        let _ = globals.set_item("__builtins__", b);
    }
    if let Err(e) = py.run(
        match std::ffi::CString::new(script.as_str()) {
            Ok(cs) => cs,
            Err(e) => {
                eprintln!("[aether-decision] aviso: flush de cache falhou ({e})");
                return;
            }
        }
        .as_c_str(),
        Some(&globals),
        None,
    ) {
        eprintln!("[aether-decision] aviso: flush de cache falhou ({e})");
    }
}

fn fit_from_file(py: Python<'_>, reader: &Bound<'_, PyAny>, path: &str) -> PyResult<()> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{path}: {e}")))?;
    let cal: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("json: {e}")))?;
    let examples = json_to_py(
        py,
        cal.get("examples")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("examples"))?,
    )?;
    let questions = json_to_py(
        py,
        cal.get("questions")
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>("questions"))?,
    )?;
    reader.call_method1("fit", (examples, questions))?;
    Ok(())
}

fn json_to_py(py: Python<'_>, v: &serde_json::Value) -> PyResult<Py<PyAny>> {
    match v {
        serde_json::Value::Null => {
            // Python None — PyNone::get é o caminho no PyO3 0.30 (Python::none saiu).
            pyo3::types::PyNone::get(py).to_owned().into_py_any(py)
        }
        serde_json::Value::Bool(b) => b.into_py_any(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py_any(py)
            } else {
                n.as_f64().unwrap_or(0.0).into_py_any(py)
            }
        }
        serde_json::Value::String(s) => s.into_py_any(py),
        serde_json::Value::Array(items) => {
            let list = pyo3::types::PyList::empty(py);
            for it in items {
                list.append(json_to_py(py, it)?)?;
            }
            list.into_py_any(py)
        }
        serde_json::Value::Object(map) => {
            let dict = pyo3::types::PyDict::new(py);
            for (k, val) in map {
                dict.set_item(k, json_to_py(py, val)?)?;
            }
            dict.into_py_any(py)
        }
    }
}

fn py_answer_to_answer(
    py: Python<'_>,
    a: &Bound<'_, PyAny>,
    qid: &str,
) -> Result<Answer, DecisionError> {
    let get = |k: &str| a.get_item(k).ok();

    let abstain = get("abstain")
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(false);
    let unbound = get("unbound")
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(false);
    let prediction_set = get("prediction_set").and_then(|v| json_from_py(py, &v));
    let prediction_set_len = prediction_set
        .as_ref()
        .and_then(|v| v.as_array())
        .map(|a| a.len());
    let boolean = get("boolean").and_then(|v| v.extract::<f64>().ok());

    // F1-1/F1-3 (MSG 0793): a derivacao vive em `types` para ser testada sem
    // Python (feature `pyo3` nao e necessaria para o teste de regressao).
    let choice = crate::types::derive_choice(
        get("choice").and_then(|v| v.extract::<String>().ok()),
        boolean,
    );
    let abstain_reason = crate::types::derive_abstain_reason(abstain, unbound, prediction_set_len);
    let probs = get("probs").and_then(|p| json_from_py(py, &p));
    // Confiança do readout = probabilidade da melhor opção (Eixo B: campo
    // auditável). `abstain` sem probs ⇒ None.
    let confidence = probs.as_ref().and_then(|p| {
        p.as_object()?
            .values()
            .filter_map(|v| v.as_f64())
            .fold(None::<f64>, |acc, f| Some(acc.map_or(f, |a: f64| a.max(f))))
    });

    Ok(Answer {
        question: qid.to_string(),
        choice,
        probs,
        abstain,
        abstain_reason,
        readout: get("readout").and_then(|v| v.extract::<String>().ok()),
        confidence,
        conformal_set: prediction_set,
        // `readout="ttt"` nao tem `abstain`/`prediction_set`; traz `cv_accuracy`.
        cv_accuracy: get("cv_accuracy").and_then(|v| v.extract::<f64>().ok()),
    })
}

fn json_from_py(py: Python<'_>, v: &Bound<'_, PyAny>) -> Option<serde_json::Value> {
    if let Ok(b) = v.extract::<bool>() {
        return Some(serde_json::Value::Bool(b));
    }
    if let Ok(f) = v.extract::<f64>() {
        return Some(serde_json::json!(f));
    }
    if let Ok(s) = v.extract::<String>() {
        return Some(serde_json::Value::String(s));
    }
    if let Ok(d) = v.cast::<pyo3::types::PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, val) in d.iter() {
            let k: String = k.extract().ok()?;
            map.insert(k, json_from_py(py, &val)?);
        }
        return Some(serde_json::Value::Object(map));
    }
    // F1-2: `prediction_set` e uma sequencia no vegaml (medido, MSG 0793) e o
    // conjunto conformal tem de chegar ao schema. Sem este braco a conversao
    // devolvia `None` em silencio — a mesma classe de perda do parser do B-G0
    // (MSG 0567). Fica DEPOIS do dict (que tambem itera, mas sobre chaves) e
    // aceita qualquer iteravel: list, tuple, set.
    if let Ok(iter) = v.try_iter() {
        let mut arr = Vec::new();
        for it in iter {
            arr.push(json_from_py(py, &it.ok()?)?);
        }
        return Some(serde_json::Value::Array(arr));
    }
    None
}

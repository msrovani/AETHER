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

use crate::types::{Answer, DecisionError, DecisionMeta, DecisionRequest, DecisionResponse};
use crate::port::DecisionPort;
use crate::config::EngineConfig;
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;

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
            let vegaml = py.import("vegaml")?;
            let primary = match &cfg.model_primary {
                Some(m) => vegaml.call_method1("load", (m,))?,
                None => vegaml.call_method0("load")?,
            };
            if let Some(cal) = &cfg.calibration {
                fit_from_file(py, &primary, cal)?;
            }
            let sentinel = match &cfg.model_sentinel {
                Some(m) => {
                    let s = vegaml.call_method1("load", (m,))?;
                    if let Some(cal) = &cfg.calibration {
                        fit_from_file(py, &s, cal)?;
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
            let p = Self::decide_reader(
                py,
                self.primary.bind(py),
                &req.state,
                &req.questions,
            )?;
            let s = match &self.sentinel {
                Some(s) => Some(Self::decide_reader(py, s.bind(py), &req.state, &req.questions)?),
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
        p.as_object()?.values().filter_map(|v| v.as_f64()).fold(None::<f64>, |acc, f| {
            Some(acc.map_or(f, |a: f64| a.max(f)))
        })
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

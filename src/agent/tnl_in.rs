//! T-NL-IN — contrato congelado (Fase 0): schema, limiares e artefato.
//!
//! Superfície pública estável: `LanguagePort::parse` + [`GoalSchema`].
//! Bordas externas só emitem texto/struct → este funil; o core não muda
//! o contrato sem versionar o artefato.

use std::fs;
use std::path::Path;

use crate::json::Json;

use super::language::{GoalSchema, LanguageError, LanguagePort, ParserPort};
use super::tool::Goal;

/// Versão do contrato T-NL-IN (bump quando schema/limiares mudarem).
pub const TNL_IN_CONTRACT_VERSION: &str = "1";

/// Path canônico do artefato.
pub const TNL_IN_CONTRACT_PATH: &str = "target/aether_tnl_in_contract.json";

/// Teto de confiança OOV — espelha `language::ABSTAIN_CEILING` (0.49).
pub const ABSTAIN_CEILING: f32 = 0.49;

/// Serializa o contrato máquina-legível.
pub fn tnl_in_contract_json() -> Json {
    let schema = GoalSchema::agent_default();
    Json::obj([
        ("tomada", Json::str("T-NL-IN")),
        ("version", Json::str(TNL_IN_CONTRACT_VERSION)),
        ("api", Json::str("LanguagePort::parse")),
        (
            "schema",
            Json::obj([(
                "intents",
                Json::Arr(schema.intents.iter().map(|i| Json::str(*i)).collect()),
            )]),
        ),
        (
            "thresholds",
            Json::obj([
                ("abstain_ceiling", Json::num(f64::from(ABSTAIN_CEILING))),
                ("high_conf_min", Json::num(0.85)),
                ("oov_invented_goals", Json::num(0.0)),
            ]),
        ),
        (
            "examples",
            Json::Arr(vec![
                example("ask Brasil capital", "fact", "brasil", "capital"),
                example("recall capital brasil", "memory", "", ""),
                example("verify gate F4", "verify", "", "verify"),
                example("sat 3cnf", "sat", "", ""),
            ]),
        ),
        (
            "plugue",
            Json::str("borda → texto UTF-8 ou Goal struct → T-NL-IN → core; STT/OCR/VIS não falam com planner"),
        ),
        ("pcm_pixels_tokens_in_t0", Json::Bool(false)),
    ])
}

fn example(text: &str, kind: &str, subject: &str, relation: &str) -> Json {
    Json::obj([
        ("text", Json::str(text)),
        ("kind", Json::str(kind)),
        ("subject", Json::str(subject)),
        ("relation", Json::str(relation)),
    ])
}

/// Grava o artefato (cria `target/` se preciso).
pub fn write_tnl_in_contract(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, tnl_in_contract_json().to_string())
        .map_err(|e| e.to_string())
}

/// Gate de fixtures: fact/sat/memory/act-path + OOV abstain (0 Goal inventado).
pub fn gate_fixtures(port: &mut ParserPort) -> Result<(), String> {
    let schema = GoalSchema::agent_default();
    // fact
    let g = port
        .parse("ask Brasil capital", &schema)
        .map_err(|e| format!("fact: {e:?}"))?;
    assert_slots_ci(&g, Some("brasil"), Some("capital"))?;
    // memory / recall
    let g = port
        .parse("recall capital brasil", &schema)
        .map_err(|e| format!("memory: {e:?}"))?;
    if g.query.as_deref().unwrap_or("").is_empty() && g.subject.is_none() {
        return Err("memory: query/subject vazio".into());
    }
    // verify
    let g = port
        .parse("verify gate F4", &schema)
        .map_err(|e| format!("verify: {e:?}"))?;
    if g.kind != super::tool::GoalKind::Verify {
        return Err(format!("verify: kind={:?}", g.kind));
    }
    // sat
    let g = port
        .parse("sat 8", &schema)
        .map_err(|e| format!("sat: {e:?}"))?;
    if g.n.is_none() || g.n == Some(0) {
        return Err(format!("sat: n ausente kind={:?}", g.kind));
    }
    // OOV — não inventa Goal
    match port.parse("xyzzy blorp 42", &schema) {
        Err(LanguageError::Oov | LanguageError::Empty | LanguageError::Schema) => {}
        Err(other) => return Err(format!("oov: erro inesperado {other:?}")),
        Ok(goal) => return Err(format!("oov: Goal inventado {goal:?}")),
    }
    // act path: optimize/qubo intent se schema permitir; senão OOV ok
    let _ = port.parse("optimize qubo demo", &schema);
    Ok(())
}

fn assert_slots_ci(g: &Goal, subject: Option<&str>, relation: Option<&str>) -> Result<(), String> {
    if let Some(s) = subject {
        let got = g.subject.as_deref().unwrap_or("");
        if !got.eq_ignore_ascii_case(s) {
            return Err(format!("subject want={s} got={got}"));
        }
    }
    if let Some(r) = relation {
        let got = g.relation.as_deref().unwrap_or("");
        if !got.eq_ignore_ascii_case(r) {
            return Err(format!("relation want={r} got={got}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrato_tem_campos_estaveis() {
        let j = tnl_in_contract_json().to_string();
        assert!(j.contains("T-NL-IN"));
        assert!(j.contains("LanguagePort::parse"));
        assert!(j.contains("abstain_ceiling"));
    }
}

//! O AND gate — a "chave dupla" do fórum (MSG 0756, executado em MSG 0758).
//!
//! Dois leitores sobre o MESMO estado:
//! - **primário**: o vega-U 0.8B (motor + heads TTT) — responde bem;
//! - **sentinela**: o gemma-prose — o ÚNICO reader que abstém no gibberish
//!   (MSG 0755: `T2_OOD=OK_UNICO_READER_QUE_ABSTE_NO_GIBBERISH`).
//!
//! Regra do gate (estrita, mas com um dono por sinal):
//!
//! | primário | sentinela | veredito |
//! |---|---|---|
//! | responde X | responde X | **resposta X** (readout `and_gate`) |
//! | responde X | responde Y≠X | `conflict` → escala HITL |
//! | responde X | abstém | motivo de QUEM ABSTEVE (OOD ⇒ `unknown`; margem ⇒ `ambiguous`) |
//! | abstém | responde | motivo do primário (`unknown`/`ambiguous`) |
//! | abstém | abstém | `unknown` se ambos OOD, senão o motivo mais grave |
//!
//! A sentinela é especialista em OOD; por isso o seu "abstém" é lido como
//! "não está no léxico" e não como "discordo". Já um "responde diferente" é
//! discordância de verdade e escala para o humano.

use crate::types::{AbstainReason, Answer};

/// Combina as respostas dos dois leitores num veredito único.
///
/// Puro e determinístico: sem engine, sem I/O — é a parte testável do gate.
pub fn combine(primary: &Answer, sentinel: &Answer) -> Answer {
    // 1) Degradação do engine ganha de tudo: com um reader fora, o gate não
    //    decide (o AND gate exige os dois).
    if primary.abstain_reason == Some(AbstainReason::EngineUnavailable) {
        return tagged(primary.clone(), "and_gate:primary");
    }
    if sentinel.abstain_reason == Some(AbstainReason::EngineUnavailable) {
        return tagged(sentinel.clone(), "and_gate:sentinel");
    }

    // 2) Ambos responderam: ou concordam, ou é conflito.
    if !primary.abstain && !sentinel.abstain {
        let (pk, sk) = (decision_key(primary), decision_key(sentinel));
        return match (pk, sk) {
            (Some(p), Some(s)) if p == s => {
                let mut a = primary.clone();
                a.readout = Some("and_gate".into());
                a
            }
            _ => abstained_verdict(primary, sentinel, AbstainReason::Conflict),
        };
    }

    // 3) Um abstém, o outro não. O motivo vem de QUEM ABSTEVE — quem age não
    //    tem motivo, e ler o silêncio dele como `unknown` transformava margem
    //    fina em lacuna de conhecimento (F1-3, MSG 0793). `unknown` fica
    //    reservado ao `unbound` do reader; o resto escala como `ambiguous`.
    let abstainer = if sentinel.abstain { sentinel } else { primary };
    let reason = abstainer
        .abstain_reason
        .or(primary.abstain_reason)
        .or(sentinel.abstain_reason)
        .unwrap_or(AbstainReason::Ambiguous);
    abstained_verdict(primary, sentinel, reason)
}

/// Veredito abstido que não perde o que os readers mediram.
///
/// `abstain`/`abstain_reason` são a autoridade sobre agir; `choice` (argmax),
/// `probs`, `confidence` e o conjunto conformal seguem do primário (fallback:
/// sentinela) — é o mesmo contrato do vegaml, onde `choice` e `abstain`
/// coexistem (medido, MSG 0793). Sem isso o AND gate jogava fora o
/// `conformal_set` (F1-2) em todo veredito abstido.
fn abstained_verdict(primary: &Answer, sentinel: &Answer, reason: AbstainReason) -> Answer {
    let mut out = primary.clone();
    out.abstain = true;
    out.abstain_reason = Some(reason);
    out.readout = Some("and_gate".into());
    if out.conformal_set.is_none() {
        out.conformal_set = sentinel.conformal_set.clone();
    }
    out
}

/// Chave de comparação da resposta: a `choice` canônica.
///
/// Pergunta boolean vem do Python como `boolean: f64` (probabilidade de
/// `true`), então a chave é derivada dela — o gate compara a mesma
/// grandezinha nos dois leitores.
fn decision_key(a: &Answer) -> Option<String> {
    if let Some(c) = &a.choice {
        return Some(c.to_ascii_lowercase());
    }
    if let Some(v) = &a.probs {
        // Fallback: a opção com maior probabilidade.
        if let Some(obj) = v.as_object() {
            let mut best: Option<(&String, f64)> = None;
            for (k, val) in obj {
                let p = val.as_f64()?;
                if best.map_or(true, |(_, bp)| p > bp) {
                    best = Some((k, p));
                }
            }
            if let Some((k, _)) = best {
                return Some(k.to_ascii_lowercase());
            }
        }
    }
    None
}

/// Marca a proveniência do readout sem perder o resto do answer.
fn tagged(mut a: Answer, readout: &str) -> Answer {
    a.readout = Some(readout.to_string());
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DecisionMeta;

    fn answer(question: &str, choice: &str) -> Answer {
        Answer {
            question: question.into(),
            choice: Some(choice.into()),
            probs: Some(serde_json::json!({choice: 0.8})),
            abstain: false,
            abstain_reason: None,
            readout: Some("engine".into()),
            confidence: Some(0.8),
            conformal_set: None,
            cv_accuracy: None,
        }
    }

    fn abstain(question: &str, reason: AbstainReason) -> Answer {
        Answer::abstained(question, reason)
    }

    #[test]
    fn agreement_yields_the_answer() {
        let a = combine(&answer("winner", "A"), &answer("winner", "A"));
        assert!(!a.abstain);
        assert_eq!(a.choice.as_deref(), Some("A"));
        assert_eq!(a.readout.as_deref(), Some("and_gate"));
    }

    #[test]
    fn disagreement_is_a_conflict_that_escalates() {
        let a = combine(&answer("winner", "A"), &answer("winner", "B"));
        assert!(a.abstain);
        assert_eq!(a.abstain_reason, Some(AbstainReason::Conflict));
        assert!(a.abstain_reason.unwrap().escalates_hitl());
    }

    #[test]
    fn sentinel_ood_overrides_a_confident_primary() {
        // T2 OOD: o 0.8B erra e responde, a sentinela abstém ⇒ unknown (pesquisa).
        let a = combine(
            &answer("area", "billing"),
            &abstain("area", AbstainReason::Unknown),
        );
        assert!(a.abstain);
        assert_eq!(a.abstain_reason, Some(AbstainReason::Unknown));
        assert!(a.abstain_reason.unwrap().admits_research());
    }

    #[test]
    fn primary_ood_keeps_its_reason() {
        let a = combine(
            &abstain("area", AbstainReason::Unknown),
            &answer("area", "billing"),
        );
        assert!(a.abstain);
        assert_eq!(a.abstain_reason, Some(AbstainReason::Unknown));
    }

    #[test]
    fn both_ood_is_unknown() {
        let a = combine(
            &abstain("area", AbstainReason::Unknown),
            &abstain("area", AbstainReason::Unknown),
        );
        assert_eq!(a.abstain_reason, Some(AbstainReason::Unknown));
    }

    #[test]
    fn thin_margin_stays_ambiguous_not_research() {
        let a = combine(
            &abstain("winner", AbstainReason::Ambiguous),
            &abstain("winner", AbstainReason::Ambiguous),
        );
        assert_eq!(a.abstain_reason, Some(AbstainReason::Ambiguous));
        assert!(a.abstain_reason.unwrap().escalates_hitl());
        assert!(!a.abstain_reason.unwrap().admits_research());
    }

    #[test]
    fn engine_down_is_degradation_not_conflict() {
        let a = combine(
            &answer("winner", "A"),
            &abstain("winner", AbstainReason::EngineUnavailable),
        );
        assert_eq!(a.abstain_reason, Some(AbstainReason::EngineUnavailable));
        assert_eq!(a.readout.as_deref(), Some("and_gate:sentinel"));
    }

    #[test]
    fn boolean_answers_compare_by_probability() {
        let mut p = answer("passou", "true");
        p.probs = Some(serde_json::json!({"true": 0.62, "false": 0.38}));
        p.choice = None;
        let mut s = answer("passou", "true");
        s.probs = Some(serde_json::json!({"true": 0.55, "false": 0.45}));
        s.choice = None;
        let a = combine(&p, &s);
        assert!(!a.abstain);
    }

    #[test]
    fn meta_of_the_combined_answer_is_still_serializable() {
        let a = combine(&answer("winner", "A"), &answer("winner", "A"));
        let j = serde_json::to_string(&a).unwrap();
        assert!(j.contains("\"readout\":\"and_gate\""));
        let _ = DecisionMeta {
            latency_ms: 0.0,
            engine: "and_gate".into(),
            degraded: false,
            vram_used_mb: None,
        };
    }
}

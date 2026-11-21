//! T-NL-OUT P2 — juíz textual claim↔fontes + [`CertifiedUtterance`].
//!
//! Único input legítimo de T-TTS. Sem evidência → Empty; falso settled
//! recusado quando slots da claim não aparecem no texto.

use crate::json::Json;

use super::language::{LanguageError, LanguagePort, ParserPort, Style};
use super::{Action, Claim};

/// Utterance certificada (texto + fontes + auth) — contrato T-TTS.
#[derive(Clone, Debug, PartialEq)]
pub struct CertifiedUtterance {
    /// Texto já gerado por T-NL-OUT (sem inventar).
    pub text: String,
    /// Ação que autoriza a fala (`Act`/`Cite`; `Abstain` → beep).
    pub action: Action,
    /// Autoridade mínima entre as fontes citadas.
    pub auth: u8,
    /// Rótulos de fonte (provenance/source.label).
    pub sources: Vec<String>,
    /// Confiança do juíz P2 `[0,1]`.
    pub judge_conf: f32,
}

impl CertifiedUtterance {
    /// `true` se pode alimentar T-TTS com conteúdo factual.
    pub fn may_speak_fact(&self) -> bool {
        !matches!(self.action, Action::Abstain)
            && !self.sources.is_empty()
            && !self.text.trim().is_empty()
    }

    /// JSON de telemetria / hub.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("text", Json::str(&self.text)),
            ("action", Json::str(self.action.label())),
            ("auth", Json::num(f64::from(self.auth))),
            (
                "sources",
                Json::Arr(self.sources.iter().map(|s| Json::str(s)).collect()),
            ),
            ("judge_conf", Json::num(f64::from(self.judge_conf))),
            ("may_speak_fact", Json::Bool(self.may_speak_fact())),
        ])
    }
}

/// Juíz P2: gera prosa P1 e exige que slots/fontes batam no texto.
pub fn certify_utterance(
    port: &mut ParserPort,
    claims: &[Claim],
    action: Action,
    style: Style,
) -> Result<CertifiedUtterance, LanguageError> {
    if claims.is_empty() {
        return Err(LanguageError::Empty);
    }
    let text = port.generate(claims, style)?;
    let (ok, conf, sources, auth) = judge_claim_text(claims, &text);
    if !ok {
        return Err(LanguageError::Empty);
    }
    if matches!(action, Action::Abstain) {
        // Abstain certificado só como “não sei” — sem fontes factuais.
        return Ok(CertifiedUtterance {
            text: "não sei".into(),
            action,
            auth: 0,
            sources: Vec::new(),
            judge_conf: conf,
        });
    }
    Ok(CertifiedUtterance {
        text,
        action,
        auth,
        sources,
        judge_conf: conf,
    })
}

/// Verifica que cada claim tem subject/relation/value (ou cite) no texto.
pub fn judge_claim_text(claims: &[Claim], text: &str) -> (bool, f32, Vec<String>, u8) {
    if claims.is_empty() || text.trim().is_empty() {
        return (false, 0.0, Vec::new(), 0);
    }
    let fold = fold_ascii(text);
    let mut sources = Vec::new();
    let mut auth = u8::MAX;
    let mut hits = 0usize;
    for c in claims {
        if c.value.trim().is_empty() {
            continue;
        }
        let sub = fold_ascii(&c.subject);
        let rel = fold_ascii(&c.relation);
        let val = fold_ascii(&c.value);
        let slot_ok = (!sub.is_empty() && fold.contains(&sub))
            && (!rel.is_empty() && fold.contains(&rel))
            && (!val.is_empty() && fold.contains(&val));
        let cite_ok = fold.contains("fonte=") || fold.contains(&fold_ascii(&c.source.label()));
        if slot_ok && cite_ok {
            hits += 1;
        }
        sources.push(c.source.label().to_string());
        auth = auth.min(c.authority);
    }
    if hits == 0 || hits < claims.len() {
        return (false, 0.0, sources, if auth == u8::MAX { 0 } else { auth });
    }
    let conf = (hits as f32 / claims.len() as f32).clamp(0.5, 1.0);
    (true, conf, sources, if auth == u8::MAX { 0 } else { auth })
}

fn fold_ascii(s: &str) -> String {
    s.chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| !c.is_whitespace() || true)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Claim, ExpertId};
    use crate::core::certificate::Provenance;

    fn claim() -> Claim {
        Claim::new(
            "brasil",
            "capital",
            "brasília",
            0.9,
            128,
            ExpertId::Transplant,
            Provenance::Transplanted,
        )
    }

    #[test]
    fn p2_certifica_template() {
        let mut p = ParserPort::new();
        let utt = certify_utterance(&mut p, &[claim()], Action::Cite, Style::Factual).unwrap();
        assert!(utt.may_speak_fact());
        assert!(!utt.sources.is_empty());
    }

    #[test]
    fn sem_claim_empty() {
        let mut p = ParserPort::new();
        assert!(matches!(
            certify_utterance(&mut p, &[], Action::Cite, Style::Compact),
            Err(LanguageError::Empty)
        ));
    }

    #[test]
    fn juiz_rejeita_texto_sem_slots() {
        let (ok, _, _, _) = judge_claim_text(&[claim()], "olá mundo");
        assert!(!ok);
    }
}

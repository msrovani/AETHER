//! A8 / P5 — previsão de série via reservatório congelado + metacog self_field.
//!
//! Contrato: série tipada → forecast + NRMSE; OOD/surpresa → abstain (sem inventar).
//! Autoridade da evidência ≤ [`PREDICT_AUTH_CAP`] (quarentena).

use crate::core::certificate::Provenance;
use crate::core::reservoir::{evaluate, FrozenModel, ReservoirConfig};
use crate::core::self_field::{SelfField, SelfFieldConfig, SelfFrame};
use crate::json::Json;

use super::tool::AUTHORITY_QUARANTINE;
use super::{Claim, ExpertId, ExpertOutput};

/// Cap duro de autoridade para forecast (nunca 255 sem HITL).
pub const PREDICT_AUTH_CAP: u8 = 120;

/// NRMSE holdout acima disto = OOD.
pub const NRMSE_OOD: f32 = 0.15;

/// Washout mínimo + margem para treino (`washout+8`).
pub const WASHOUT_MIN: usize = 20;

/// Horizonte máximo de forecast multi-passo.
pub const HORIZON_MAX: usize = 5;

/// Relatório de uma previsão.
#[derive(Clone, Debug)]
pub struct PredictReport {
    /// Predição 1-passo (e horizonte se pedido).
    pub yhat: Vec<f32>,
    /// NRMSE no holdout interno.
    pub nrmse: f32,
    /// Intervalo ± (calibração grosseira = nrmse).
    pub interval: f32,
    /// Motivo se abstain/ood (`None` = ok).
    pub abstain: Option<&'static str>,
}

impl PredictReport {
    /// Conversão em saída tipada do especialista.
    pub fn to_expert_output(&self) -> ExpertOutput {
        if let Some(why) = self.abstain {
            return ExpertOutput::empty(ExpertId::Predict, why);
        }
        let auth = AUTHORITY_QUARANTINE.min(PREDICT_AUTH_CAP);
        let conf = (1.0 / (1.0 + self.nrmse)).clamp(0.05, 0.99);
        let y0 = self.yhat.first().copied().unwrap_or(0.0);
        let mut claims = vec![
            Claim::new(
                "series",
                "predict.yhat",
                format!("{y0:.8}"),
                conf,
                auth,
                ExpertId::Predict,
                Provenance::Inferred,
            ),
            Claim::new(
                "series",
                "predict.nrmse",
                format!("{:.8}", self.nrmse),
                conf,
                auth,
                ExpertId::Predict,
                Provenance::Inferred,
            ),
            Claim::new(
                "series",
                "predict.interval",
                format!("{:.8}", self.interval),
                conf,
                auth,
                ExpertId::Predict,
                Provenance::Inferred,
            ),
        ];
        if self.yhat.len() > 1 {
            let joined = self
                .yhat
                .iter()
                .map(|v| format!("{v:.6}"))
                .collect::<Vec<_>>()
                .join(",");
            claims.push(Claim::new(
                "series",
                "predict.horizon",
                joined,
                conf,
                auth,
                ExpertId::Predict,
                Provenance::Inferred,
            ));
        }
        ExpertOutput {
            expert: ExpertId::Predict,
            claims,
            confidence: conf,
            certificate: None,
            detail: format!(
                "predict.series nrmse={:.6} h={} auth={auth}",
                self.nrmse,
                self.yhat.len()
            ),
            cost_us: 0,
        }
    }
}

/// Extrai `series: [f32]` e `horizon` opcional (1..=5) do JSON do goal.
pub fn parse_series_query(query: &str) -> Result<(Vec<f32>, usize), String> {
    let json = Json::parse(query.trim()).map_err(|e| format!("json: {e}"))?;
    let arr = json
        .get("series")
        .and_then(|v| v.as_arr())
        .ok_or_else(|| "series ausente".to_string())?;
    let mut series = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().enumerate() {
        let Some(x) = v.as_f64() else {
            return Err(format!("series[{i}] nao numerico"));
        };
        if !x.is_finite() {
            return Err(format!("series[{i}] nao finito"));
        }
        series.push(x as f32);
    }
    let horizon = json
        .get("horizon")
        .and_then(|h| h.as_u64().or_else(|| h.as_f64().map(|f| f as u64)))
        .unwrap_or(1)
        .clamp(1, HORIZON_MAX as u64) as usize;
    Ok((series, horizon))
}

/// Forecast 1-passo (+ horizonte) com OOD e holdout NRMSE.
pub fn forecast_series(series: &[f32], horizon: usize) -> PredictReport {
    let horizon = horizon.clamp(1, HORIZON_MAX);
    if series.iter().any(|x| !x.is_finite()) {
        return PredictReport {
            yhat: Vec::new(),
            nrmse: f32::INFINITY,
            interval: 0.0,
            abstain: Some("ood"),
        };
    }
    let washout = WASHOUT_MIN;
    let min_len = washout + 8;
    if series.len() < min_len {
        return PredictReport {
            yhat: Vec::new(),
            nrmse: f32::INFINITY,
            interval: 0.0,
            abstain: Some("ood"),
        };
    }

    // Holdout: últimos 20% (mín. 8); treino no prefixo.
    let test_len = (series.len() / 5).max(8).min(series.len() / 3);
    let train_len = series.len().saturating_sub(test_len);
    if train_len <= washout + 4 {
        return PredictReport {
            yhat: Vec::new(),
            nrmse: f32::INFINITY,
            interval: 0.0,
            abstain: Some("ood"),
        };
    }

    let n = if series.len() < 200 { 64 } else { 100 };
    let cfg = ReservoirConfig {
        n,
        degree: (n / 10).clamp(4, 12),
        spectral_radius: 0.95,
        leak: 0.5,
        input_scale: 0.5,
        seed: 0xA800_AE81,
    };

    let Ok((mut model, _)) = FrozenModel::train(cfg, series, 0, train_len, washout, 1e-6) else {
        return PredictReport {
            yhat: Vec::new(),
            nrmse: f32::INFINITY,
            interval: 0.0,
            abstain: Some("ood"),
        };
    };

    let nrmse = match evaluate(
        &mut model,
        series,
        train_len,
        test_len.saturating_sub(1).max(1),
    ) {
        Ok((nrmse, _)) if nrmse.is_finite() => nrmse,
        _ => {
            return PredictReport {
                yhat: Vec::new(),
                nrmse: f32::INFINITY,
                interval: 0.0,
                abstain: Some("ood"),
            };
        }
    };

    if nrmse > NRMSE_OOD {
        return PredictReport {
            yhat: Vec::new(),
            nrmse,
            interval: nrmse,
            abstain: Some("ood"),
        };
    }

    if metacog_should_abstain(nrmse) {
        return PredictReport {
            yhat: Vec::new(),
            nrmse,
            interval: nrmse,
            abstain: Some("abstain"),
        };
    }

    // Re-aquece no trecho completo e prevê à frente.
    model.reset();
    for &u in series {
        model.step(u);
    }
    let mut yhat = Vec::with_capacity(horizon);
    for _ in 0..horizon {
        let Ok(y) = model.predict() else {
            return PredictReport {
                yhat: Vec::new(),
                nrmse,
                interval: nrmse,
                abstain: Some("ood"),
            };
        };
        if !y.is_finite() {
            return PredictReport {
                yhat: Vec::new(),
                nrmse,
                interval: nrmse,
                abstain: Some("ood"),
            };
        }
        yhat.push(y);
        model.step(y);
    }

    PredictReport {
        yhat,
        nrmse,
        interval: nrmse,
        abstain: None,
    }
}

/// Metacog: self_field decide abster-se sob surpresa elevada (E4).
///
/// NRMSE baixo (≤0.05) nunca abstém por metacog. Acima disso, aquecimento
/// com surpresa típica (~0.04) e quadro atual `surprise=nrmse`.
pub fn metacog_should_abstain(nrmse: f32) -> bool {
    if !nrmse.is_finite() || nrmse < 0.0 {
        return true;
    }
    if nrmse <= 0.05 {
        return false;
    }
    let mut field = SelfField::with_config(SelfFieldConfig {
        warmup_steps: 4,
        surprise_ratio: 1.5,
        recall_margin_min: 0.20,
        ..SelfFieldConfig::default()
    });
    for _ in 0..8 {
        field.observe(SelfFrame::new(0.0, 0.04, 0.90, 0.0), 10.0);
    }
    let surprise = nrmse.clamp(0.0, 1.0);
    let coherence = 1.0 / (1.0 + nrmse.max(0.0));
    let margin = (1.0 - nrmse.clamp(0.0, 1.0)).max(0.0);
    field.should_abstain(margin, SelfFrame::new(0.0, surprise, coherence, 0.0))
}

/// Especialista T-PRED no MoE (série tipada → yhat + intervalo + cert).
#[derive(Debug, Default)]
pub struct PredictExpert;

impl PredictExpert {
    /// Novo especialista preditivo.
    pub fn new() -> Self {
        Self
    }
}

impl crate::agent::experts::Expert for PredictExpert {
    fn id(&self) -> ExpertId {
        ExpertId::Predict
    }

    fn probe(&self, task: &crate::agent::Task, _features: &crate::agent::TaskFeatures) -> f32 {
        match task {
            crate::agent::Task::Predict { series, .. } if series.len() >= WASHOUT_MIN => 0.95,
            crate::agent::Task::Predict { .. } => 0.4,
            _ => 0.0,
        }
    }

    fn infer(&mut self, task: &crate::agent::Task) -> ExpertOutput {
        let crate::agent::Task::Predict { series, horizon } = task else {
            return ExpertOutput::empty(self.id(), "tarefa não preditiva");
        };
        forecast_series(series, *horizon).to_expert_output()
    }

    fn learn(&mut self, _task: &crate::agent::Task, _feedback: &crate::agent::Feedback) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reservoir::mackey_glass;

    #[test]
    fn mg_curta_preve_sem_ood() {
        let series = mackey_glass(400, 17, 0.1);
        let rep = forecast_series(&series, 1);
        assert!(rep.abstain.is_none(), "{:?}", rep.abstain);
        assert!(!rep.yhat.is_empty());
        assert!(rep.nrmse < NRMSE_OOD, "nrmse={}", rep.nrmse);
    }

    #[test]
    fn serie_curta_ood() {
        let rep = forecast_series(&[1.0, 2.0, 3.0], 1);
        assert_eq!(rep.abstain, Some("ood"));
        assert!(rep.yhat.is_empty());
    }

    #[test]
    fn metacog_surpresa_alta_abstain() {
        assert!(metacog_should_abstain(0.12));
        assert!(!metacog_should_abstain(0.001));
    }
}

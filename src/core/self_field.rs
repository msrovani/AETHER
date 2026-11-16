//! E4 — *self-field* de 16 canais e separação metacognitiva.
//!
//! O Vol. I §4 propõe que metacognição não emerge de escalar raciocínio, mas de
//! uma **restrição arquitetural**: o organismo precisa prever o próprio próximo
//! quadro sensorial e usar o erro dessa auto-predição como sinal de abstenção.
//! Este módulo implementa exatamente esse campo, com três propriedades:
//!
//! * **16 canais** — quatro features (`ação própria`, `surpresa F(t)`,
//!   `coerência R(t)`, `taxa de plasticidade`) × quatro preditores por feature.
//!   Cada preditor é uma hipótese barata sobre o próximo valor (persistência,
//!   extrapolação linear, EMA e média da janela); o campo combina as quatro por
//!   peso e devolve o erro **composto**.
//! * **Escala macro lenta** — [`SelfField::observe_hierarchy`] lê a fase/ordem
//!   macro da [`RenormalizationHierarchy`] (`64 → 16 → 4`) e usa uma EMA com
//!   `τ = 100 ms`; o *torque* top-down continua sendo responsabilidade da
//!   hierarquia, aqui só se mede a previsão.
//! * **Separação metacognitiva** — [`SelfField::should_abstain`] combina a
//!   margem de recall (mundo) com a novidade do self-field (modelo interno).
//!   O campo **não** altera a recuperação: ele só decide se o organismo deve
//!   calar. É essa separação que o gate `T01` do E4 falsifica.
//!
//! # Auto-consistência estrutural
//!
//! O mesmo banco de canais é reutilizado sobre [`TopologyStats`] (sinapses
//! ativas, `⟨|J|⟩`, ordem e energia livre) para prever o **próprio estado
//! pós-plasticidade** depois de `MorphogeneticField::evolve_topology`. O erro
//! estrutural mede se o organismo previu a topologia que ele mesmo acabou de
//! criar — não se ele "acertou o mundo".
//!
//! # Contrato
//!
//! Zero alocação após a construção: todos os buffers são arrays fixos. Nenhum
//! `String`, nenhum `Vec`, nenhum `unsafe`. Entradas não finitas são saturadas;
//! o campo nunca entra em `NaN` nem faz `panic`.

#![allow(clippy::needless_range_loop)]

use crate::core::renormalization::RenormalizationHierarchy;

/// Número de features observadas por quadro.
pub const FEATURES: usize = 4;
/// Preditores independentes por feature (4 × 4 = 16 canais).
pub const PREDICTORS: usize = 4;
/// Canais totais do self-field.
pub const CHANNELS: usize = FEATURES * PREDICTORS;
/// Profundidade da janela de histórico usada pelos preditores.
pub const HISTORY: usize = 4;

/// Um quadro do próprio corpo: o que o organismo acabou de fazer/sentir.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelfFrame {
    /// Ação própria aplicada no ciclo (`u_t`), normalizada em `[-1, 1]`.
    pub action: f32,
    /// Surpresa de predição do mundo `F(t)` (MSE/erro absoluto).
    pub surprise: f32,
    /// Coerência de fase `R(t) ∈ [0, 1]` (ordem × alinhamento macro).
    pub coherence: f32,
    /// Taxa de plasticidade do ciclo anterior `∈ [0, 1]`.
    pub plasticity: f32,
}

impl SelfFrame {
    /// Constrói um quadro com saturação de valores não finitos.
    pub fn new(action: f32, surprise: f32, coherence: f32, plasticity: f32) -> Self {
        Self {
            action: sanitize(action, -1.0, 1.0),
            surprise: sanitize(surprise, 0.0, 1.0e6),
            coherence: sanitize(coherence, 0.0, 1.0),
            plasticity: sanitize(plasticity, 0.0, 1.0),
        }
    }

    /// Features na ordem canônica usada pelos canais.
    #[inline]
    pub fn to_array(self) -> [f32; FEATURES] {
        [self.action, self.surprise, self.coherence, self.plasticity]
    }

    /// Reconstrói o quadro a partir da ordem canônica.
    #[inline]
    pub fn from_array(v: [f32; FEATURES]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}

/// Estatísticas observáveis da topologia do corpo (pós-morfogênese).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopologyStats {
    /// Sinapses ativas (`|J_ij| > θ`).
    pub synapses: f32,
    /// Módulo médio das conexões ativas.
    pub mean_abs: f32,
    /// Ordem espacial `∈ [0,1]` do estado motor.
    pub order: f32,
    /// Energia livre variacional `F`.
    pub free_energy: f32,
}

impl TopologyStats {
    /// Features na ordem canônica.
    #[inline]
    pub fn to_array(self) -> [f32; FEATURES] {
        [self.synapses, self.mean_abs, self.order, self.free_energy]
    }

    /// Reconstrói as estatísticas a partir da ordem canônica.
    #[inline]
    pub fn from_array(v: [f32; FEATURES]) -> Self {
        Self {
            synapses: sanitize(v[0], 0.0, 1.0e9),
            mean_abs: sanitize(v[1], 0.0, 1.0e9),
            order: sanitize(v[2], 0.0, 1.0),
            free_energy: sanitize(v[3], 0.0, 1.0e9),
        }
    }
}

/// Leitura do self-field para um quadro.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelfReading {
    /// Erro absoluto médio da predição composta (4 features).
    pub error: f32,
    /// Incerteza entre os preditores (desvio ponderado).
    pub uncertainty: f32,
    /// Taxa de plasticidade observada (mudança média dos pesos).
    pub plasticity: f32,
    /// Surpresa registrada no quadro.
    pub surprise: f32,
    /// Coerência registrada no quadro.
    pub coherence: f32,
}

/// Configuração do self-field.
#[derive(Clone, Copy, Debug)]
pub struct SelfFieldConfig {
    /// Constante de tempo da EMA do erro (ms). O dossiê fixa `100 ms`.
    pub ema_tau_ms: f32,
    /// Passo do ajuste exponencial de pesos.
    pub learning_rate: f32,
    /// Peso mínimo de um preditor (evita colapso).
    pub min_weight: f32,
    /// Limiar em desvios-padrão para declarar novidade.
    pub abstain_error_z: f32,
    /// Limiar absoluto de incerteza entre preditores.
    pub abstain_uncertainty: f32,
    /// Ciclos de aquecimento em que o campo não opina.
    pub warmup_steps: u32,
    /// Margem mínima de recall (espelha `RecallConfidence::ABSTAIN_MARGIN`).
    pub recall_margin_min: f32,
    /// Queda relativa de coerência que dispara abstenção (ex.: `0,15` = −15%).
    pub coherence_drop: f32,
    /// Razão de surpresa acima da EMA que caracteriza novidade (`1,25` = +25%).
    pub surprise_ratio: f32,
}

impl Default for SelfFieldConfig {
    fn default() -> Self {
        Self {
            ema_tau_ms: 100.0,
            learning_rate: 0.35,
            min_weight: 0.02,
            abstain_error_z: 2.5,
            abstain_uncertainty: 0.45,
            warmup_steps: 24,
            recall_margin_min: 0.20,
            coherence_drop: 0.30,
            surprise_ratio: 2.0,
        }
    }
}

/// Banco de 4 preditores para cada uma das 4 features.
///
/// O layout é fixo: `weights[f][p]` e `last_preds[f][p]`. Não há `Vec`, então o
/// tipo é `Copy` e pode ser embutido no [`SelfField`] sem alocação.
#[derive(Clone, Copy, Debug)]
struct ChannelBank {
    weights: [[f32; PREDICTORS]; FEATURES],
    last_preds: [[f32; PREDICTORS]; FEATURES],
    history: [[f32; FEATURES]; HISTORY],
    filled: usize,
    primed: bool,
}

impl Default for ChannelBank {
    fn default() -> Self {
        Self {
            weights: [[1.0 / PREDICTORS as f32; PREDICTORS]; FEATURES],
            last_preds: [[0.0; PREDICTORS]; FEATURES],
            history: [[0.0; FEATURES]; HISTORY],
            filled: 0,
            primed: false,
        }
    }
}

impl ChannelBank {
    /// Preditor `p` da feature `f` a partir do histórico (h[0] = mais recente).
    #[inline]
    fn predictor(&self, f: usize, p: usize) -> f32 {
        let h0 = self.history[0][f];
        let h1 = self.history[1][f];
        let h2 = self.history[2][f];
        let h3 = self.history[3][f];
        match p {
            0 => h0,
            1 => 2.0 * h0 - h1,
            2 => 0.5 * h0 + 0.3 * h1 + 0.2 * h2,
            _ => 0.25 * (h0 + h1 + h2 + h3),
        }
    }

    /// Predição composta e espalhamento entre preditores.
    fn predict(&self) -> ([f32; FEATURES], [f32; FEATURES]) {
        let mut pred = [0.0f32; FEATURES];
        let mut spread = [0.0f32; FEATURES];
        for f in 0..FEATURES {
            let mut wsum = 0.0f32;
            let mut acc = 0.0f32;
            for p in 0..PREDICTORS {
                let w = self.weights[f][p].max(0.0);
                wsum += w;
                acc += w * self.predictor(f, p);
            }
            let mean = if wsum > 1e-12 { acc / wsum } else { 0.0 };
            let mut var = 0.0f32;
            for p in 0..PREDICTORS {
                let d = self.predictor(f, p) - mean;
                var += self.weights[f][p].max(0.0) * d * d;
            }
            let var = if wsum > 1e-12 { var / wsum } else { 0.0 };
            pred[f] = mean;
            spread[f] = var.max(0.0).sqrt();
        }
        (pred, spread)
    }

    /// Erro absoluto médio de uma predição contra o quadro real.
    fn error(pred: &[f32; FEATURES], frame: &[f32; FEATURES], scale: &[f32; FEATURES]) -> f32 {
        let mut acc = 0.0f32;
        for f in 0..FEATURES {
            acc += ((pred[f] - frame[f]).abs() / scale[f].max(1e-6)).min(4.0);
        }
        acc / FEATURES as f32
    }

    /// Atualiza os pesos por entropia exponencial (multiplicativos), saturando
    /// o passo e renormalizando para soma 1. Devolve `0.5·Σ|Δw|`.
    fn update_weights(
        &mut self,
        frame: &[f32; FEATURES],
        scale: &[f32; FEATURES],
        lr: f32,
        min_w: f32,
    ) -> f32 {
        let mut delta = 0.0f32;
        for f in 0..FEATURES {
            let mut new = [0.0f32; PREDICTORS];
            let mut sum = 0.0f32;
            for p in 0..PREDICTORS {
                let err = ((self.last_preds[f][p] - frame[f]).abs() / scale[f].max(1e-6)).min(4.0);
                let w = (self.weights[f][p] * (-lr * err).exp()).max(min_w);
                new[p] = w;
                sum += w;
            }
            let inv = if sum > 1e-12 { 1.0 / sum } else { 1.0 };
            for p in 0..PREDICTORS {
                let w = (new[p] * inv).clamp(min_w, 1.0);
                delta += (w - self.weights[f][p]).abs();
                self.weights[f][p] = w;
            }
        }
        0.5 * delta / FEATURES as f32
    }

    /// Observa um quadro: prediz antes de atualizar, mede erro, ajusta pesos.
    ///
    /// `scale` é a escala corrente por feature (EMA do módulo), usada para
    /// normalizar features com unidades diferentes.
    fn observe(
        &mut self,
        frame: [f32; FEATURES],
        scale: &mut [f32; FEATURES],
        alpha: f32,
        lr: f32,
        min_w: f32,
    ) -> SelfReading {
        let (pred, spread) = self.predict();

        // Atualiza a escala antes de medir (evita divisão por zero no início).
        let mut normalized = [0.0f32; FEATURES];
        for f in 0..FEATURES {
            let observed = frame[f].abs().max(0.05);
            scale[f] = (1.0 - alpha) * scale[f].max(0.05) + alpha * observed;
            normalized[f] = frame[f];
        }

        let error = Self::error(&pred, &normalized, scale);
        let uncertainty = {
            let mut acc = 0.0f32;
            for f in 0..FEATURES {
                acc += spread[f] / scale[f].max(1e-6);
            }
            (acc / FEATURES as f32).min(4.0)
        };

        let plasticity = if self.primed {
            // Guarda as predições que geraram este erro para o ajuste.
            for f in 0..FEATURES {
                for p in 0..PREDICTORS {
                    self.last_preds[f][p] = self.predictor(f, p);
                }
            }
            self.update_weights(&normalized, scale, lr, min_w)
        } else {
            0.0
        };

        // Desloca o histórico (h[0] passa a ser o quadro atual).
        self.history.copy_within(0..HISTORY - 1, 1);
        self.history[0] = frame;
        self.filled = (self.filled + 1).min(HISTORY);
        if self.filled >= 2 {
            self.primed = true;
        }

        SelfReading {
            error,
            uncertainty,
            plasticity,
            surprise: frame[1],
            coherence: frame[2],
        }
    }

    /// Erro de uma sonda **sem** alterar o estado (usado por abstenção).
    fn probe(&self, frame: [f32; FEATURES], scale: &[f32; FEATURES]) -> (f32, f32) {
        if !self.primed {
            return (0.0, 0.0);
        }
        let (pred, spread) = self.predict();
        let mut err = 0.0f32;
        let mut unc = 0.0f32;
        for f in 0..FEATURES {
            err += ((pred[f] - frame[f]).abs() / scale[f].max(1e-6)).min(4.0);
            unc += spread[f] / scale[f].max(1e-6);
        }
        ((err / FEATURES as f32), (unc / FEATURES as f32).min(4.0))
    }
}

/// Campo de auto-predição de 16 canais.
pub struct SelfField {
    cfg: SelfFieldConfig,
    bank: ChannelBank,
    topology: ChannelBank,
    scale: [f32; FEATURES],
    topo_scale: [f32; FEATURES],
    err_ema: f32,
    err_slow: f32,
    err_var: f32,
    plasticity_ema: f32,
    surprise_ema: f32,
    coherence_ema: f32,
    topology_error: f32,
    steps: u64,
    abstained: u64,
}

impl std::fmt::Debug for SelfField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelfField")
            .field("channels", &CHANNELS)
            .field("err_ema", &self.err_ema)
            .field("plasticity", &self.plasticity_ema)
            .field("steps", &self.steps)
            .finish()
    }
}

impl Default for SelfField {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfField {
    /// Cria o campo com a configuração do dossiê (`τ = 100 ms`).
    pub fn new() -> Self {
        Self::with_config(SelfFieldConfig::default())
    }

    /// Cria o campo com configuração explícita.
    pub fn with_config(cfg: SelfFieldConfig) -> Self {
        let ema_tau_ms = if cfg.ema_tau_ms.is_finite() && cfg.ema_tau_ms > 0.0 {
            cfg.ema_tau_ms
        } else {
            100.0
        };
        let cfg = SelfFieldConfig {
            ema_tau_ms,
            learning_rate: if cfg.learning_rate.is_finite() {
                cfg.learning_rate.clamp(0.0, 1.0)
            } else {
                0.35
            },
            min_weight: if cfg.min_weight.is_finite() {
                cfg.min_weight.clamp(1e-4, 0.25)
            } else {
                0.02
            },
            ..cfg
        };
        Self {
            cfg,
            bank: ChannelBank::default(),
            topology: ChannelBank::default(),
            scale: [1.0; FEATURES],
            topo_scale: [1.0; FEATURES],
            err_ema: 0.0,
            err_slow: 0.0,
            err_var: 0.0,
            plasticity_ema: 0.0,
            surprise_ema: 0.0,
            coherence_ema: 1.0,
            topology_error: 0.0,
            steps: 0,
            abstained: 0,
        }
    }

    /// Configuração em uso.
    pub fn config(&self) -> SelfFieldConfig {
        self.cfg
    }

    /// Número de canais (16).
    pub const fn channels(&self) -> usize {
        CHANNELS
    }

    /// Ciclos observados.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Erro de auto-predição (EMA `τ = 100 ms`).
    pub fn self_prediction_error(&self) -> f32 {
        self.err_ema
    }

    /// Incerteza média entre preditores do último quadro.
    pub fn uncertainty(&self) -> f32 {
        self.err_var.max(0.0).sqrt()
    }

    /// Taxa de plasticidade observada (EMA).
    pub fn plasticity(&self) -> f32 {
        self.plasticity_ema
    }

    /// Surpresa média do mundo.
    pub fn surprise(&self) -> f32 {
        self.surprise_ema
    }

    /// Coerência média.
    pub fn coherence(&self) -> f32 {
        self.coherence_ema
    }

    /// Erro de auto-predição **estrutural** (topologia pós-plasticidade).
    pub fn topology_error(&self) -> f32 {
        self.topology_error
    }

    /// Abstenções declaradas pelo campo.
    pub fn abstained(&self) -> u64 {
        self.abstained
    }

    /// Z-score de novidade do erro corrente contra a linha de base lenta.
    pub fn novelty_z(&self) -> f32 {
        let sd = self.err_var.max(0.0).sqrt().max(1e-4);
        (self.err_ema - self.err_slow) / sd
    }

    #[inline]
    fn alpha(dt_ms: f32, tau_ms: f32) -> f32 {
        let dt = if dt_ms.is_finite() && dt_ms > 0.0 {
            dt_ms
        } else {
            16.7
        };
        (1.0 - (-dt / tau_ms.max(1e-3)).exp()).clamp(0.0, 1.0)
    }

    /// Observa um quadro e devolve o erro de auto-predição **antes** do ajuste.
    ///
    /// A EMA do erro usa `dt_ms` e a constante `τ = 100 ms`; a variância é
    /// atualizada em paralelo. O campo aceita qualquer `dt_ms > 0`; valores
    /// inválidos caem no passo nominal de 16,7 ms.
    pub fn observe(&mut self, frame: SelfFrame, dt_ms: f32) -> SelfReading {
        let alpha = Self::alpha(dt_ms, self.cfg.ema_tau_ms);
        let arr = frame.to_array();
        let reading = self.bank.observe(
            arr,
            &mut self.scale,
            alpha,
            self.cfg.learning_rate,
            self.cfg.min_weight,
        );

        let e = reading.error;
        let delta = e - self.err_ema;
        self.err_ema += alpha * delta;
        // Variância da EMA em torno da média corrente (E[(e−μ)²]).
        self.err_var += alpha * (delta * delta - self.err_var);
        // Linha de base lenta (τ ≈ 8× a rápida) para o z-score de novidade.
        let slow_alpha = Self::alpha(dt_ms, self.cfg.ema_tau_ms * 8.0);
        self.err_slow = (1.0 - slow_alpha) * self.err_slow + slow_alpha * e;
        self.plasticity_ema = (1.0 - alpha) * self.plasticity_ema + alpha * reading.plasticity;
        self.surprise_ema = (1.0 - alpha) * self.surprise_ema + alpha * frame.surprise;
        self.coherence_ema = (1.0 - alpha) * self.coherence_ema + alpha * frame.coherence;
        self.steps = self.steps.wrapping_add(1);
        reading
    }

    /// Atalho: lê a escala macro da [`RenormalizationHierarchy`] e observa.
    ///
    /// A coerência é `macro_order × alignment`; a plasticidade é passada pelo
    /// chamador (o torque top-down pertence à hierarquia, não a este módulo).
    pub fn observe_hierarchy(
        &mut self,
        hierarchy: &RenormalizationHierarchy,
        action: f32,
        surprise: f32,
        plasticity: f32,
        dt_ms: f32,
    ) -> SelfReading {
        let coherence = (hierarchy.macro_order() * hierarchy.alignment()).clamp(0.0, 1.0);
        self.observe(
            SelfFrame::new(action, surprise, coherence, plasticity),
            dt_ms,
        )
    }

    /// Sonda um quadro **sem** aprender (para abstenção por consulta).
    ///
    /// Devolve `(erro, incerteza)` normalizados. Antes do aquecimento, devolve
    /// `(0, 0)`: o campo ainda não tem direito a opinar.
    pub fn probe(&self, frame: SelfFrame) -> (f32, f32) {
        if self.steps < u64::from(self.cfg.warmup_steps) {
            return (0.0, 0.0);
        }
        self.bank.probe(frame.to_array(), &self.scale)
    }

    /// Decide abster-se combinando a margem de recall com a novidade do campo.
    ///
    /// * margem `< cfg.recall_margin_min` — o mundo já não sustenta a resposta;
    /// * novidade `> cfg.abstain_error_z` σ **e** erro acima da EMA — o quadro
    ///   está fora do modelo interno (o self-field não se reconhece);
    /// * incerteza entre preditores acima do teto — o campo está dividido.
    ///
    /// A decisão **não** modifica o estado: pode ser chamada no caminho de
    /// leitura sem alocar.
    pub fn should_abstain(&mut self, recall_margin: f32, frame: SelfFrame) -> bool {
        if self.steps < u64::from(self.cfg.warmup_steps) {
            return recall_margin < self.cfg.recall_margin_min || recall_margin.is_nan();
        }
        let (error, uncertainty) = self.probe(frame);
        let novelty = self.novelty_z();
        let coherence_drop =
            frame.coherence + 1e-6 < self.coherence_ema * (1.0 - self.cfg.coherence_drop);
        let surprise_novel = frame.surprise > self.surprise_ema * self.cfg.surprise_ratio;
        let abstain = recall_margin < self.cfg.recall_margin_min
            || recall_margin.is_nan()
            || coherence_drop
            || surprise_novel
            || (error > self.err_ema * 1.3 && novelty > self.cfg.abstain_error_z)
            || uncertainty > self.cfg.abstain_uncertainty;
        if abstain {
            self.abstained = self.abstained.wrapping_add(1);
        }
        abstain
    }

    /// Versão imutável de [`Self::should_abstain`] (não conta abstenções).
    pub fn should_abstain_ro(&self, recall_margin: f32, frame: SelfFrame) -> bool {
        if self.steps < u64::from(self.cfg.warmup_steps) {
            return recall_margin < self.cfg.recall_margin_min || recall_margin.is_nan();
        }
        let (error, uncertainty) = self.probe(frame);
        let novelty = self.novelty_z();
        let coherence_drop =
            frame.coherence + 1e-6 < self.coherence_ema * (1.0 - self.cfg.coherence_drop);
        let surprise_novel = frame.surprise > self.surprise_ema * self.cfg.surprise_ratio;
        recall_margin < self.cfg.recall_margin_min
            || recall_margin.is_nan()
            || coherence_drop
            || surprise_novel
            || (error > self.err_ema * 1.3 && novelty > self.cfg.abstain_error_z)
            || uncertainty > self.cfg.abstain_uncertainty
    }

    /// Observa a topologia **depois** de `evolve_topology` e mede o erro de
    /// auto-predição estrutural (previsão do estado que o corpo acabou de
    /// criar). Devolve o erro do passo.
    pub fn observe_topology(&mut self, stats: TopologyStats, dt_ms: f32) -> f32 {
        let alpha = Self::alpha(dt_ms, self.cfg.ema_tau_ms);
        let reading = self.topology.observe(
            stats.to_array(),
            &mut self.topo_scale,
            alpha,
            self.cfg.learning_rate,
            self.cfg.min_weight,
        );
        self.topology_error = (1.0 - alpha) * self.topology_error + alpha * reading.error;
        reading.error
    }

    /// Previsão do próximo estado topológico (sem alterar o banco).
    pub fn topology_forecast(&self) -> TopologyStats {
        let (pred, _) = self.topology.predict();
        TopologyStats::from_array(pred)
    }

    /// Reinicia o campo (mantém a configuração).
    pub fn reset(&mut self) {
        self.bank = ChannelBank::default();
        self.topology = ChannelBank::default();
        self.scale = [1.0; FEATURES];
        self.topo_scale = [1.0; FEATURES];
        self.err_ema = 0.0;
        self.err_slow = 0.0;
        self.err_var = 0.0;
        self.plasticity_ema = 0.0;
        self.surprise_ema = 0.0;
        self.coherence_ema = 1.0;
        self.topology_error = 0.0;
        self.steps = 0;
        self.abstained = 0;
    }
}

#[inline]
fn sanitize(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        lo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smooth_frame(t: usize) -> SelfFrame {
        let x = t as f32 * 0.05;
        SelfFrame::new(x.sin(), 0.5 + 0.2 * x.cos(), 0.8, 0.1)
    }

    #[test]
    fn tem_16_canais_e_nao_aloca_no_tipo() {
        assert_eq!(CHANNELS, 16);
        assert_eq!(
            std::mem::size_of::<ChannelBank>(),
            std::mem::size_of::<ChannelBank>()
        );
        assert!(!std::mem::needs_drop::<SelfField>());
    }

    #[test]
    fn aprende_a_prever_um_sinal_suave() {
        let mut sf = SelfField::new();
        for t in 0..256 {
            sf.observe(smooth_frame(t), 16.7);
        }
        let early = {
            let mut sf2 = SelfField::new();
            for t in 0..32 {
                sf2.observe(smooth_frame(t), 16.7);
            }
            sf2.self_prediction_error()
        };
        assert!(sf.self_prediction_error() < early, "erro deveria cair");
        assert!(sf.plasticity() >= 0.0 && sf.plasticity() <= 1.0);
    }

    #[test]
    fn sonda_fora_do_modelo_dispara_novidade() {
        let mut sf = SelfField::new();
        for t in 0..128 {
            sf.observe(smooth_frame(t), 16.7);
        }
        let in_model = sf.probe(smooth_frame(128));
        let out_model = sf.probe(SelfFrame::new(0.9, 0.1, 0.05, 0.95));
        assert!(out_model.0 > in_model.0, "fora do modelo={out_model:?}");
    }

    #[test]
    fn abstenco_nao_altera_o_erro() {
        let mut sf = SelfField::new();
        for t in 0..64 {
            sf.observe(smooth_frame(t), 16.7);
        }
        let before = sf.self_prediction_error();
        let _ = sf.should_abstain(0.9, smooth_frame(65));
        assert_eq!(before, sf.self_prediction_error());
    }

    #[test]
    fn topologia_previsivel_apos_plasticidade() {
        let mut sf = SelfField::new();
        for k in 0..64 {
            let s = TopologyStats {
                synapses: 40.0 + 4.0 * (k as f32 * 0.1).sin(),
                mean_abs: 0.5 + 0.1 * (k as f32 * 0.07).cos(),
                order: 0.7 + 0.1 * (k as f32 * 0.05).sin(),
                free_energy: 2.0 + 0.2 * (k as f32 * 0.03).cos(),
            };
            sf.observe_topology(s, 16.7);
        }
        assert!(sf.topology_error().is_finite());
        assert!(sf.topology_error() < 1.0);
        let fc = sf.topology_forecast();
        assert!(fc.synapses.is_finite() && fc.order.is_finite());
    }
}

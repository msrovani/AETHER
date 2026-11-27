//! D1 · T-SENSOR (`docs/TOMADAS_FUTURO.md` §D1; `CORE_TOMADAS` T-SENSOR).
//!
//! Evento/host/IoT **estruturado** → série tipada → T-PRED. **Classe D**:
//! adaptador + **backend virtual determinístico** (`synthetic`, seed) para gates;
//! hardware real pluga depois **sem mudar fio/disjuntor/filtros/DB**. `feature
//! sensor` off; `AETHER_SENSOR=1`. **Sem bytes no T0.** auth≤120.
//!
//! ```text
//! [sensor real/sintético] → SensorReading tipado → T-MOE/T-PRED → T-COH → Action
//! ```

use super::tool::ToolError;

/// D1 auth cap.
pub const SENSOR_AUTH_CAP: u8 = 120;
/// `series` máximo (anti-DoS; W6).
pub const MAX_SERIES: usize = 4096;

/// Leitura de sensor tipada (POD; não PCM/bytes no core).
#[derive(Clone, Debug, PartialEq)]
pub struct SensorReading {
    /// Id do sensor.
    pub sensor_id: String,
    /// timestamp ms.
    pub timestamp_ms: u64,
    /// valor numérico.
    pub value: f64,
    /// unidade (ex. "C", "Hz", "Pa", "rpm").
    pub unit: String,
}

impl SensorReading {
    /// Disjuntor **schema break → reject**; valor deve ser finito.
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.sensor_id.is_empty() {
            return Err(ToolError::InvalidArgs(
                "schema break: sensor_id vazio".into(),
            ));
        }
        if !self.value.is_finite() {
            return Err(ToolError::InvalidArgs(
                "schema break: value não-finito".into(),
            ));
        }
        Ok(())
    }
}

/// **SUGESTÃO/MÉDIO:** `Display`.
impl std::fmt::Display for SensorReading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}={}{} @{}",
            self.sensor_id, self.value, self.unit, self.timestamp_ms
        )
    }
}

/// Backend do sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorBackend {
    /// Desligado (no-op honesto).
    Off,
    /// **Virtual determinístico** (`splitmix64`, seed) — classe D sem HW.
    Synthetic,
    /// Backend real externo via `AETHER_SENSOR_CMD`.
    Sidecar,
}

/// Porta D1 (default off).
#[derive(Clone, Debug)]
pub struct SensorPort {
    /// Ligada (feature `sensor` + `AETHER_SENSOR=1`).
    pub enabled: bool,
    /// Backend.
    pub backend: SensorBackend,
    /// Seed para `Synthetic`.
    pub seed: u64,
}

impl Default for SensorPort {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: SensorBackend::Off,
            seed: 0x0D10_0001,
        }
    }
}

impl SensorPort {
    /// Off.
    pub fn off() -> Self {
        Self::default()
    }

    /// `AETHER_SENSOR=1` + feature `sensor`; `AETHER_SENSOR_BACKEND=synthetic|sidecar`.
    pub fn from_env() -> Self {
        let env_on = std::env::var("AETHER_SENSOR")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let backend = match std::env::var("AETHER_SENSOR_BACKEND").as_deref() {
            Ok("synthetic") => SensorBackend::Synthetic,
            Ok("sidecar") => SensorBackend::Sidecar,
            _ => SensorBackend::Synthetic, // classe D sem HW: virtual por default
        };
        Self {
            enabled: env_on && cfg!(feature = "sensor"),
            backend,
            seed: 0x0D10_0001,
        }
    }

    /// Leitura única. schema break→reject; tomada off→`Err` honesto; **nunca inventa HW**.
    pub fn read(&self, sensor_id: &str, timestamp_ms: u64) -> Result<SensorReading, ToolError> {
        if sensor_id.is_empty() {
            return Err(ToolError::InvalidArgs(
                "schema break: sensor_id vazio".into(),
            ));
        }
        if !self.enabled {
            return Err(ToolError::Precondition(
                "T-SENSOR off (feature sensor / AETHER_SENSOR)".into(),
            ));
        }
        match self.backend {
            SensorBackend::Off => Err(ToolError::Precondition("T-SENSOR backend off".into())),
            SensorBackend::Synthetic => Ok(self.synthetic(sensor_id, timestamp_ms, 0)),
            SensorBackend::Sidecar => self.sidecar(sensor_id, timestamp_ms),
        }
    }

    /// Série (para gates ou backend). **W6:** `MAX_SERIES`, overflow `checked_add`,
    /// cada leitura `.validate()`, `Err` honesto (não "só válidas" silencioso).
    pub fn series(
        &self,
        sensor_id: &str,
        n: usize,
        start_ms: u64,
        step_ms: u64,
    ) -> Result<Vec<SensorReading>, ToolError> {
        if n > MAX_SERIES {
            return Err(ToolError::InvalidArgs(format!(
                "n {n} > MAX_SERIES {MAX_SERIES}"
            )));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let offset = (i as u64)
                .checked_mul(step_ms.max(1))
                .ok_or_else(|| ToolError::InvalidArgs("step overflow".into()))?;
            let ts = start_ms
                .checked_add(offset)
                .ok_or_else(|| ToolError::InvalidArgs("timestamp overflow".into()))?;
            let r = self.read(sensor_id, ts)?;
            r.validate()?; // W6
            out.push(r);
        }
        Ok(out)
    }

    /// **Virtual**: senoide determinística + ruído `splitmix64`, só para gates.
    fn synthetic(&self, sensor_id: &str, timestamp_ms: u64, _i: u64) -> SensorReading {
        let mut st = self.seed ^ fnv(sensor_id) ^ timestamp_ms;
        let noise =
            (crate::core::silicon_entropy::splitmix64(&mut st) & 0xFFFF) as f64 / 65536.0 - 0.5;
        let phase = (timestamp_ms % 1000) as f64 / 1000.0 * std::f64::consts::TAU;
        SensorReading {
            sensor_id: sensor_id.into(),
            timestamp_ms,
            value: phase.sin() + noise * 0.05,
            unit: "unit".into(),
        }
    }

    /// Sidecar `AETHER_SENSOR_CMD`, **seguro** (`backend::run_backend_cmd`): sem shell;
    /// `sensor_id` sanitizado; timeout+limite; `value` **`is_finite`** + `.validate()`. W1+W6+W8.
    fn sidecar(&self, sensor_id: &str, timestamp_ms: u64) -> Result<SensorReading, ToolError> {
        let cmd = std::env::var("AETHER_SENSOR_CMD").map_err(|_| {
            ToolError::Precondition(
                "T-SENSOR enabled sem backend (AETHER_SENSOR_CMD; binário concreto)".into(),
            )
        })?;
        let out = super::backend::run_backend_cmd(
            &cmd,
            Some(sensor_id),
            None,
            super::backend::BACKEND_TIMEOUT_MS,
            super::backend::BACKEND_MAX_STDOUT,
        )
        .map_err(|e| ToolError::Failed(format!("sensor backend: {e}")))?;
        let text = String::from_utf8_lossy(&out).trim().to_string();
        let value = text
            .parse::<f64>()
            .map_err(|_| ToolError::Failed(format!("sensor value não numérico: {text}")))?;
        // W6: NaN/inf → schema break; não propaga para T-PRED
        if !value.is_finite() {
            return Err(ToolError::InvalidArgs(format!(
                "schema break: value não-finito ({text})"
            )));
        }
        let r = SensorReading {
            sensor_id: sensor_id.into(),
            timestamp_ms,
            value,
            unit: "unit".into(),
        };
        r.validate()?; // W6
        Ok(r)
    }

    /// Série de valores para **T-PRED** (`f32`, sem inventar; só leituras válidas).
    pub fn series_values(
        &self,
        sensor_id: &str,
        n: usize,
        start_ms: u64,
        step_ms: u64,
    ) -> Result<Vec<f32>, ToolError> {
        Ok(self
            .series(sensor_id, n, start_ms, step_ms)?
            .into_iter()
            .map(|r| r.value as f32)
            .collect())
    }
}

fn fnv(s: &str) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

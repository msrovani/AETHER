//! Gate **D1** — T-SENSOR; classe D independente de HW; backend **virtual sintético**
//! determinístico + sidecar plugável; schema break→reject; série→T-PRED; feature `sensor`.
//! NOTA: teste único (muta env).

#![cfg(feature = "agent")]

use aether_engine::agent::sensor::{SensorBackend, SensorPort};

#[test]
fn d1_sensor_virtual_serie_tpred_schema() {
    // tomada off → Err honesto; schema break → reject
    let off = SensorPort::off();
    assert!(off.read("s1", 0).is_err());
    let p = SensorPort { enabled: true, backend: SensorBackend::Synthetic, seed: 42 };
    assert!(p.read("", 0).is_err(), "schema break sensor_id vazio");

    // virtual sintético determinístico (classe D sem HW)
    let r1 = p.read("s1", 1000).unwrap();
    let r2 = p.read("s1", 1000).unwrap();
    assert_eq!(r1, r2, "sintético determinístico");
    assert!(r1.value.is_finite());
    // série
    let series = p.series("s1", 16, 0, 100).unwrap();
    assert_eq!(series.len(), 16);
    let vals = p.series_values("s1", 16, 0, 100).unwrap();
    assert_eq!(vals.len(), 16, "série tipada para T-PRED");

    // sidecar honest Err sem backend
    let sc = SensorPort { enabled: true, backend: SensorBackend::Sidecar, seed: 0 };
    std::env::remove_var("AETHER_SENSOR_CMD");
    assert!(sc.read("s1", 0).is_err());
    std::env::set_var("AETHER_SENSOR_CMD", "definitely-not-a-real-binary-xyz");
    assert!(sc.read("s1", 0).is_err());
    std::env::remove_var("AETHER_SENSOR_CMD");
}

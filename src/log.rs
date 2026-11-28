//! Log estruturado JSON-lines (T14 mitigado — **sem dependência nova**).
//!
//! O plano pedia `tracing`; a mitigação preguiçosa: um `event()` mínimo que
//! emite JSON-lines no **stderr**, off por default (`AETHER_LOG=1` liga) —
//! zero custo no caminho normal, telemetria real quando o operador quiser.
//! Escalar para `tracing` depois é trocar o corpo de `event()` apenas.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: OnceLock<bool> = OnceLock::new();

/// `AETHER_LOG=1|true` liga a emissão (default off — zero custo em prod).
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        std::env::var("AETHER_LOG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Emite uma linha JSON no stderr: `{"ts":ms,"c":componente,"m":mensagem}`.
///
/// Mensagem **sem segredos** (nunca token/credencial — caller garante).
pub fn event(component: &str, msg: &str) {
    if !enabled() {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    eprintln!(
        "{{\"ts\":{},\"c\":{},\"m\":{}}}",
        ts,
        crate::json::Json::str(component).to_text(),
        crate::json::Json::str(msg).to_text()
    );
}

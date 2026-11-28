//! Trava global de **env** para testes (R14 — causa raiz dos flakes).
//!
//! Variáveis de ambiente são **por processo**: o harness roda os testes de um
//! alvo em paralelo no mesmo processo, então gates que fazem
//! `std::env::set_var("AETHER_*")` correm entre si (um teste lê o env que o
//! outro acabou de escrever). Esta trava serializa o acesso ao env dentro do
//! alvo; entre alvos não há corrida (processos separados).
//!
//! Uso: primeira linha do teste que muta env —
//! `let _env = aether_engine::test_env::env_test_lock();`
//!
//! `#[doc(hidden)]`: é infra de teste, não contrato público da lib.

#![doc(hidden)]

use std::sync::{Mutex, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Guarda o lock do env para o escopo do teste (recupera de poison).
pub fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    let mutex = ENV_LOCK.get_or_init(|| Mutex::new(()));
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    }
}

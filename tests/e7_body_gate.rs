//! E7 — gate do corpo `S0` (telemetria real + 4 atuadores), Vol. II §5/§6/§10.
//!
//! H1: a entropia da distribuição de metas não colapsa (`> 0,5` nats).
//! H2: a preferência é por dificuldade intermediária (U invertido,
//!     Spearman `> 0,5`).
//! H3: desconectar o efeito dos atuadores derruba a entropia em `≥ 30%`.

use aether_engine::core::s0_body::run_s0;

#[test]
fn e7_corpo_s0_anti_passeio_h1_h2_h3() {
    let cycles = 100_000u64;
    let on = run_s0(cycles, true, 0xA37E_0E74);
    let off = run_s0(cycles, false, 0xA37E_0E75);
    let drop = 1.0 - off.entropy / on.entropy.max(1e-9);
    let spearman = on.spearman_inverted_u();

    println!(
        "[E7] H1 entropy_on={:.4} entropy_off={:.4} | H2 spearman={:.4} | H3 drop={:.3}",
        on.entropy, off.entropy, spearman, drop
    );
    println!(
        "[E7] atuadores throttle={} afinidade={} pausa={} alarmes={}",
        on.actuator_counts.0, on.actuator_counts.1, on.actuator_counts.2, on.alarms
    );
    println!(
        "[E7] frequencia_on={:?}",
        on.frequency.map(|v| (v * 1000.0) as i32)
    );

    assert_eq!(on.cycles, cycles);
    assert!(on.entropy > 0.5, "H1: entropia {:.4} <= 0.5", on.entropy);
    assert!(
        spearman > 0.5,
        "H2: Spearman {spearman:.4} <= 0.5 (sem preferencia intermediaria)"
    );
    assert!(
        drop >= 0.30,
        "H3: queda {drop:.3} < 0.30 (corpo nao restringe a agencia)"
    );

    // Os quatro atuadores precisam ter disparado de verdade.
    assert!(on.actuator_counts.0 > 0, "throttling nunca disparou");
    assert!(on.actuator_counts.1 > 0, "afinidade nunca migrou");
    assert!(on.actuator_counts.2 > 0, "pausa nunca disparou");
    assert!(on.alarms > 0, "alarme nunca disparou");

    // Ablação não é "sem telemetria": os sentidos continuam, só o efeito some.
    assert!(off.entropy < on.entropy);
}

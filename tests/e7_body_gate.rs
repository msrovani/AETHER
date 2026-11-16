//! E7 — gate do corpo `S0` (telemetria real + 4 atuadores), Vol. II §5/§6/§10.
//!
//! H1: a entropia da distribuição de metas não colapsa (`> 0,5` nats).
//! H2: a preferência é por dificuldade intermediária (U invertido,
//!     Spearman `> 0,5`).
//! H3: desconectar o efeito dos atuadores derruba a entropia em `≥ 30%`.
//!
//! O gate usa [`run_s0_fixed`]: a telemetria é **dado de entrada**, não uma
//! medição de wall-clock. Assim o resultado depende só de `(cycles, seed, 3a
//! amostra de referência)` — antes, `run_s0` reamostrava o host a cada 10k
//! ciclos e a queda H3 variava de 0,55 a 0,70 com a carga da máquina
//! (MSG_0213/0215). O caminho com host real continua em `run_s0` e no
//! `examples/s0_body_probe.rs`.

use aether_engine::core::s0_body::{run_s0_fixed, HostSample};

/// Host de referência sintético: latências medianas de uma máquina saudável.
/// Congelado de propósito — é o contrato do gate.
const REF: HostSample = HostSample {
    jitter_us: 10.0,
    cpu_us: 220.0,
    disk_us: 450.0,
    net_us: 70.0,
};

/// Sementes do gate: o valor reportado é a mediana, mas **todas** precisam
/// passar nas três hipóteses.
///
/// Atenção: `LcgState::new` faz `state | 1`, então par/ímpar consecutivos
/// colapsam no mesmo estado — as seeds abaixo são distintas depois do `| 1`.
const SEEDS: [u64; 8] = [
    0xA37E_0E74,
    0x1234_5678,
    0x0BAD_F00D,
    0xDEAD_BEEF,
    0x5EED_1234,
    0x7777_7777,
    0xC0FF_EE00,
    0x9E37_79B9,
];

#[test]
fn e7_corpo_s0_anti_passeio_h1_h2_h3() {
    let cycles = 100_000u64;
    let mut drops = Vec::with_capacity(SEEDS.len());

    for (k, &seed) in SEEDS.iter().enumerate() {
        // Braço (a): atuadores ligados.
        let on = run_s0_fixed(cycles, true, seed, REF);
        // Braço (b): ablação H3 — mesmos sentidos, atos sem consequência.
        let off = run_s0_fixed(cycles, false, seed, REF);

        let drop = 1.0 - off.entropy / on.entropy.max(1e-9);
        let spearman = on.spearman_inverted_u();
        drops.push(drop);

        println!(
            "[E7] seed[{k}]={seed:#x} H1 entropy_on={:.4} entropy_off={:.4} | H2 spearman={:.4} | H3 drop={:.3}",
            on.entropy, off.entropy, spearman, drop
        );
        println!(
            "[E7] seed[{k}] atuadores throttle={} afinidade={} pausa={} alarmes={}",
            on.actuator_counts.0, on.actuator_counts.1, on.actuator_counts.2, on.alarms
        );

        assert_eq!(on.cycles, cycles);
        assert_eq!(off.cycles, cycles);
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

    let min = drops.iter().copied().fold(f32::INFINITY, f32::min);
    let max = drops.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    println!(
        "[E7] dispersao H3 em {} seeds: min={:.3} max={:.3} amplitude={:.3}",
        SEEDS.len(),
        min,
        max,
        max - min
    );
}

/// O gate **não** pode depender da fonte de telemetria: com a mesma amostra, a
/// execução é bit-a-bit reprodutível.
#[test]
fn e7_gate_reproduz_com_telemetria_fixa() {
    let a = run_s0_fixed(50_000, true, 0xA37E_0E74, REF);
    let b = run_s0_fixed(50_000, true, 0xA37E_0E74, REF);
    assert_eq!(a.entropy.to_bits(), b.entropy.to_bits());
    assert_eq!(a.frequency, b.frequency);
    assert_eq!(a.success, b.success);
    assert_eq!(a.actuator_counts, b.actuator_counts);
    assert_eq!(a.alarms, b.alarms);
}

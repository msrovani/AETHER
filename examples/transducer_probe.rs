//! Sonda do transdutor universal: 12 s de organismo sintético a 60 Hz.
//!
//! Demonstra o ciclo físico completo da borda Zero-UI, sem hardware e sem rede:
//! linha de base saudável -> rajada de anomalia (2 s) -> recuperação.
//! Imprime `F(t)` e a latência de detecção em milissegundos.
//!
//! ```text
//! cargo run --release --example transducer_probe --features m8
//! ```

use aether_engine::transducer_io::{
    FieldNormalizer, PhaseMapper, SurpriseTracker, SyntheticTransducer, TransducerIo, FIELD_DIM,
    PHASE_CHANNELS,
};

const HZ: f32 = 60.0;
const SECONDS: f32 = 12.0;
const BURST_START_S: f32 = 4.0;
const BURST_SECONDS: f32 = 2.0;
const ALERT: f32 = 0.5;

fn sparkline(value: f32, width: usize) -> String {
    const BARS: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let level = ((value.clamp(0.0, 1.0) * (BARS.len() - 1) as f32).round()) as usize;
    let filled = ((value.clamp(0.0, 1.0) * width as f32).round() as usize).min(width);
    let mut line = String::with_capacity(width);
    for i in 0..width {
        if i < filled {
            line.push(BARS[level]);
        } else {
            line.push(' ');
        }
    }
    line
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = SyntheticTransducer::new(FIELD_DIM, HZ, 0xA3E7)?;
    let mut normalizer = FieldNormalizer::new(FIELD_DIM)?;
    let mut tracker = SurpriseTracker::new(FIELD_DIM)?;
    let mapper = PhaseMapper::new(FIELD_DIM)?;

    let mut raw = vec![0.0f32; FIELD_DIM];
    let mut field = vec![0.0f32; FIELD_DIM];
    let mut phases = [0.0f32; PHASE_CHANNELS];

    source.schedule_burst(BURST_START_S, BURST_SECONDS, 1.0);

    let total_ticks = (SECONDS * HZ) as usize;
    let burst_tick = (BURST_START_S * HZ) as usize;
    let burst_end = ((BURST_START_S + BURST_SECONDS) * HZ) as usize;
    let mut detection_ms: Option<f32> = None;
    let mut peak = 0.0f32;

    println!("AETHER transducer_probe | {FIELD_DIM} canais | {HZ} Hz | {SECONDS}s");
    println!(
        "rajada: t={BURST_START_S}s..{}s  alerta F>{ALERT}",
        BURST_START_S + BURST_SECONDS
    );
    println!();
    println!("    t       F   campo");
    println!("  -----------------------------");

    for tick in 0..total_ticks {
        source.sample_into(&mut raw)?;
        normalizer.update_into(&raw, &mut field)?;
        let surprise = tracker.observe(&field)?;
        mapper.map_into(&field, &mut phases)?;

        if tick >= burst_tick && tick < burst_end {
            peak = peak.max(surprise);
            if detection_ms.is_none() && surprise > ALERT {
                detection_ms = Some((tick - burst_tick) as f32 * 1000.0 / HZ);
            }
        }

        if tick % 6 == 0 {
            let t = tick as f32 / HZ;
            let mark = if tick >= burst_tick && tick < burst_end {
                '!'
            } else {
                ' '
            };
            println!(
                "{mark} {:5.2}s  {:5.3}  {}",
                t,
                surprise,
                sparkline(surprise, 40)
            );
        }
    }

    let first_phase = phases[0];
    println!();
    println!("pico da rajada: F={peak:.3}");
    match detection_ms {
        Some(ms) => println!("detectado em {ms:.1} ms (janela fisica de {HZ:.0} Hz)"),
        None => println!("anomalia NAO detectada (pico abaixo de {ALERT})"),
    }
    println!("fase do canal 0 apos recuperacao: {first_phase:.3} rad");
    Ok(())
}

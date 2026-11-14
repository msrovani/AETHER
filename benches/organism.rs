//! Benchmark do quadro completo do organismo (Módulo 4).
//!
//! Meta da Seção 3: quadro completo (3 nós + onda + colapso) ≤ 8 ms
//! (60 Hz com folga). Não usa TUI — mede `step()` puro.

use std::time::Duration;

use aether_engine::simulation::closed_loop_organism::{ClosedLoopOrganism, OrganismConfig};
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_organism(c: &mut Criterion) {
    let mut group = c.benchmark_group("organism");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10);

    let cfg = OrganismConfig {
        // Perturbação fora do horizonte do bench: mede o quadro em regime.
        perturb_after: Duration::from_secs(3600),
        ..Default::default()
    };
    let mut organism = ClosedLoopOrganism::new(cfg).expect("organismo");

    group.bench_function("frame_60hz", |b| b.iter(|| organism.step().expect("step")));

    group.finish();
    organism.shutdown();
}

criterion_group!(benches, bench_organism);
criterion_main!(benches);

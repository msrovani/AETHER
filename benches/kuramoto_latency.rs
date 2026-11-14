//! Benchmarks do Módulo 3 — codec de 40 bytes e latência UDP em loopback.
//!
//! * `kuramoto_codec_40b` — custo de `encode`/`decode` do pacote de fase.
//! * `kuramoto_udp40_loopback` — ida e volta real (`send` + `recv`) de um
//!   datagrama de 40 bytes entre dois sockets em `127.0.0.1`.
//!
//! Meta de referência: `send_to` → `recv_from` em loopback ≤ 200 µs (p99).

use std::hint::black_box;
use std::net::UdpSocket;
use std::time::Duration;

use aether_engine::core::kuramoto_field::{PhasePacket, PACKET_SIZE};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn sample_packet() -> PhasePacket {
    PhasePacket {
        timestamp_ns: 0x0123_4567_89AB_CDEF,
        node_id: 3,
        phase: [0.125, 1.5, 3.0, 6.25],
        xi: [-1.5, 0.0, 2.25],
    }
}

fn bench_codec(c: &mut Criterion) {
    let pkt = sample_packet();
    let wire = pkt.encode();

    let mut group = c.benchmark_group("kuramoto_codec_40b");
    group.throughput(Throughput::Bytes(PACKET_SIZE as u64));
    group.bench_function("encode", |bh| bh.iter(|| black_box(pkt.encode())));
    group.bench_function("decode", |bh| {
        bh.iter(|| black_box(PhasePacket::decode(black_box(&wire)).expect("decode")))
    });
    group.finish();
}

fn bench_loopback(c: &mut Criterion) {
    let a = UdpSocket::bind("127.0.0.1:0").expect("bind a");
    let b = UdpSocket::bind("127.0.0.1:0").expect("bind b");
    a.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("timeout a");
    b.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("timeout b");
    a.connect(b.local_addr().expect("addr b"))
        .expect("connect a");
    b.connect(a.local_addr().expect("addr a"))
        .expect("connect b");

    let wire = sample_packet().encode();
    let mut buf = [0u8; 64];

    let mut group = c.benchmark_group("kuramoto_udp40_loopback");
    group.throughput(Throughput::Bytes(PACKET_SIZE as u64));
    group.sample_size(50);
    group.bench_function("ping_pong", |bh| {
        bh.iter(|| {
            a.send(black_box(&wire)).expect("send");
            let n = b.recv(&mut buf).expect("recv");
            black_box(n)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_codec, bench_loopback);
criterion_main!(benches);

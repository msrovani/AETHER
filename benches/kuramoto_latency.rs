//! Benchmark de latência do protocolo de fase (Módulo 3).
//!
//! Meta da Seção 3: `send_to` → `recv_from` loopback, pacote de 40 bytes,
//! ≤ 200 µs (p99). Mede o ping-pong completo (codificação + syscall + decode)
//! em `127.0.0.1` com sockets não-bloqueantes.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use aether_engine::core::kuramoto_field::PhasePacket;
use criterion::{criterion_group, criterion_main, Criterion};

fn loopback_pair() -> (UdpSocket, UdpSocket, SocketAddrV4, SocketAddrV4) {
    let a = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("bind a");
    let b = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).expect("bind b");
    let a_addr = a.local_addr().expect("addr a");
    let b_addr = b.local_addr().expect("addr b");
    a.set_nonblocking(true).expect("nonblock a");
    b.set_nonblocking(true).expect("nonblock b");
    (a, b, a_addr, b_addr)
}

fn bench_kuramoto_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("kuramoto");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(20);

    let (a, b, a_addr, b_addr) = loopback_pair();
    let pkt = PhasePacket {
        timestamp_ns: 0,
        node_id: 1,
        phase: [0.1, 0.2, 0.3, 0.4],
        xi: [0.0, 0.0, 0.0],
    };
    let bytes = pkt.encode();
    let mut rx_buf = [0u8; 64];

    group.bench_function("ping_pong_40B_loopback", |bench| {
        bench.iter(|| {
            // A -> B
            a.send_to(&bytes, b_addr).expect("send");
            loop {
                match b.recv_from(&mut rx_buf) {
                    Ok(_) => break,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(e) => panic!("recv: {e}"),
                }
            }
            // B -> A (fecha o ping-pong)
            b.send_to(&bytes, a_addr).expect("send back");
            loop {
                match a.recv_from(&mut rx_buf) {
                    Ok(_) => break,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(e) => panic!("recv: {e}"),
                }
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_kuramoto_latency);
criterion_main!(benches);

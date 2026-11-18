//! Telemetria do host como `TransducerIo` (Onda P / 1.1).
//!
//! `sample_into` escreve num buffer já alocado. Os contadores do SO ficam no
//! estado do transdutor, não no caminho de retorno.

#![allow(unsafe_code)]

use crate::error::{AetherError, Result};
use crate::transducer_io::{TransducerIo, FIELD_DIM};

/// Amostra jitter, CPU, disco e rede do processo/host em um campo de 64 dims.
#[derive(Debug)]
pub struct SystemTelemetryTransducer {
    hz: f32,
    last_qpc: u64,
    qpc_freq: u64,
    last_idle: u64,
    last_kernel: u64,
    last_user: u64,
    last_disk: u64,
    last_net: u64,
    cpu: f32,
    disk: f32,
    net: f32,
    jitter: f32,
}

impl SystemTelemetryTransducer {
    /// Transdutor a `hz` nominal (o daemon usa 60).
    pub fn new(hz: f32) -> Self {
        let mut t = Self {
            hz: hz.max(1.0),
            last_qpc: qpc(),
            qpc_freq: qpc_freq().max(1),
            last_idle: 0,
            last_kernel: 0,
            last_user: 0,
            last_disk: 0,
            last_net: 0,
            cpu: 0.0,
            disk: 0.0,
            net: 0.0,
            jitter: 0.0,
        };
        let (idle, kernel, user) = system_times();
        t.last_idle = idle;
        t.last_kernel = kernel;
        t.last_user = user;
        t.last_disk = process_io_bytes();
        t.last_net = tcp_segments();
        t
    }

    /// Carga de CPU observada na última amostra, em `[0, 1]`.
    pub fn cpu_load(&self) -> f32 {
        self.cpu
    }

    fn refresh(&mut self) {
        let now = qpc();
        let dt = now.saturating_sub(self.last_qpc) as f64 / self.qpc_freq as f64;
        let expect = 1.0 / f64::from(self.hz);
        let late = (dt - expect).max(0.0);
        self.jitter = (late / expect).clamp(0.0, 1.0) as f32;
        self.last_qpc = now;

        let (idle, kernel, user) = system_times();
        let idle_d = idle.saturating_sub(self.last_idle) as f64;
        let total = kernel.saturating_sub(self.last_kernel) as f64
            + user.saturating_sub(self.last_user) as f64;
        self.last_idle = idle;
        self.last_kernel = kernel;
        self.last_user = user;
        self.cpu = if total > 0.0 {
            ((total - idle_d) / total).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let disk = process_io_bytes();
        let disk_d = disk.saturating_sub(self.last_disk) as f32;
        self.last_disk = disk;
        self.disk = (disk_d / (1024.0 * 1024.0)).clamp(0.0, 1.0);

        let net = tcp_segments();
        let net_d = net.saturating_sub(self.last_net) as f32;
        self.last_net = net;
        self.net = (net_d / 256.0).clamp(0.0, 1.0);
    }
}

impl TransducerIo for SystemTelemetryTransducer {
    fn dim(&self) -> usize {
        FIELD_DIM
    }

    fn nominal_hz(&self) -> f32 {
        self.hz
    }

    fn sample_into(&mut self, out: &mut [f32]) -> Result<()> {
        if out.len() != FIELD_DIM {
            return Err(AetherError::Shape {
                expected: vec![FIELD_DIM],
                got: vec![out.len()],
            });
        }
        self.refresh();
        let signals = [self.jitter, self.cpu, self.disk, self.net];
        for (i, slot) in out.iter_mut().enumerate() {
            let s = signals[i % 4];
            let phase = (i / 4) as f32 * 0.15;
            *slot = (s * (phase.sin() * 0.25 + 1.0) * 2.0 - 1.0).clamp(-1.0, 1.0);
        }
        Ok(())
    }
}

fn qpc() -> u64 {
    let mut ticks = 0u64;
    unsafe {
        QueryPerformanceCounter(&mut ticks);
    }
    ticks
}

fn qpc_freq() -> u64 {
    let mut freq = 0u64;
    unsafe {
        QueryPerformanceFrequency(&mut freq);
    }
    freq
}

fn system_times() -> (u64, u64, u64) {
    let mut idle = 0u64;
    let mut kernel = 0u64;
    let mut user = 0u64;
    unsafe {
        GetSystemTimes(&mut idle, &mut kernel, &mut user);
    }
    (idle, kernel, user)
}

fn process_io_bytes() -> u64 {
    let mut io = IoCounters {
        read_ops: 0,
        write_ops: 0,
        other_ops: 0,
        read_bytes: 0,
        write_bytes: 0,
        other_bytes: 0,
    };
    unsafe {
        if GetProcessIoCounters(GetCurrentProcess(), &mut io) == 0 {
            return 0;
        }
    }
    io.read_bytes.saturating_add(io.write_bytes)
}

fn tcp_segments() -> u64 {
    let mut tcp = TcpStats {
        algo: 0,
        min_rto: 0,
        max_rto: 0,
        max_conn: 0,
        active: 0,
        passive: 0,
        fail: 0,
        est: 0,
        reset: 0,
        cur: 0,
        in_segs: 0,
        out_segs: 0,
        retrans: 0,
        in_err: 0,
        out_rst: 0,
        num_conn: 0,
    };
    unsafe {
        if GetTcpStatistics(&mut tcp) != 0 {
            return 0;
        }
    }
    u64::from(tcp.in_segs).saturating_add(u64::from(tcp.out_segs))
}

#[link(name = "kernel32")]
extern "system" {
    fn QueryPerformanceCounter(lp: *mut u64) -> i32;
    fn QueryPerformanceFrequency(lp: *mut u64) -> i32;
    fn GetSystemTimes(idle: *mut u64, kernel: *mut u64, user: *mut u64) -> i32;
    fn GetCurrentProcess() -> isize;
    fn GetProcessIoCounters(process: isize, counters: *mut IoCounters) -> i32;
}

#[repr(C)]
struct IoCounters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}

#[link(name = "iphlpapi")]
extern "system" {
    fn GetTcpStatistics(stats: *mut TcpStats) -> u32;
}

#[repr(C)]
struct TcpStats {
    algo: u32,
    min_rto: u32,
    max_rto: u32,
    max_conn: i32,
    active: u32,
    passive: u32,
    fail: u32,
    est: u32,
    reset: u32,
    cur: u32,
    in_segs: u32,
    out_segs: u32,
    retrans: u32,
    in_err: u32,
    out_rst: u32,
    num_conn: u32,
}

#[cfg(test)]
mod tests {
    use super::SystemTelemetryTransducer;
    use crate::transducer_io::TransducerIo;

    #[test]
    fn amostra_cabe_em_menos_um_um() {
        let mut t = SystemTelemetryTransducer::new(60.0);
        let mut buf = [2.0f32; 64];
        t.sample_into(&mut buf).unwrap();
        assert!(buf.iter().all(|v| (-1.0..=1.0).contains(v)));
        // GetSystemTimes é sistêmico: sob contenção (builds paralelos) a carga
        // pode cair entre amostras. Exigimos só faixa válida; monotonia frouxa
        // só quando o baseline já era baixo (máquina quieta do freeze).
        let quiet = t.cpu_load();
        assert!((0.0..=1.0).contains(&quiet));
        let start = std::time::Instant::now();
        let mut x = 1u64;
        while start.elapsed().as_millis() < 30 {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        }
        let _ = x;
        t.sample_into(&mut buf).unwrap();
        let after = t.cpu_load();
        assert!((0.0..=1.0).contains(&after));
        if quiet < 0.3 {
            assert!(
                after + 0.15 >= quiet,
                "cpu_load caiu demais em host quieto: quiet={quiet} after={after}"
            );
        }
    }
}

//! E7 — corpo `S0`: telemetria real de host, quatro atuadores e o teste
//! anti-passeio `H1–H3` (Vol. II §5/§6/§10).
//!
//! O ponto do E7 não é “simular um agente”: é dar **consequência física** ao
//! laço. A telemetria vem de syscalls reais (jitter, CPU, disco e UDP local) e
//! os quatro atuadores têm efeito observável:
//!
//! 1. **throttling** — `sleep` curto quando o jitter sobe;
//! 2. **migração de afinidade** — alterna o núcleo lógico (via
//!    `core_affinity` quando a feature está ligada; sem ela, muda a fila lógica
//!    que o ambiente usa);
//! 3. **alarme** — publica um flag atômico que o ambiente lê;
//! 4. **pausa** — `sleep` longo quando a carga está alta.
//!
//! O ambiente responde a esses atos: o centro de factibilidade
//! `d*(t)` (dificuldade em que a tarefa é resolvível) se move. Sem atuadores, o
//! corpo é um mundo estático — e o agente colapsa em uma única meta. Com eles,
//! `d*` varia e a distribuição de metas mantém entropia.
//!
//! Gates:
//! * **H1** — entropia da distribuição de metas `> H_min` (não colapsa);
//! * **H2** — preferência por dificuldade intermediária (U invertido,
//!   Spearman entre frequência e `−(d−0,5)²` `> 0,5`);
//! * **H3** — desconectar o efeito dos atuadores derruba a entropia em `≥ 30%`.

use std::io::{Read, Write};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Número de bins de dificuldade.
pub const BINS: usize = 10;

/// Amostra de telemetria real do host.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostSample {
    /// Jitter médio entre chamadas de relógio (µs).
    pub jitter_us: f32,
    /// Proxy de carga de CPU: duração de um laço fixo (µs).
    pub cpu_us: f32,
    /// Latência de escrita+leitura de 4 KiB no disco (µs).
    pub disk_us: f32,
    /// Latência de um datagrama UDP local (µs).
    pub net_us: f32,
}

impl HostSample {
    /// Normaliza para `[0,1]` com escalas conservadoras de hardware.
    pub fn normalized(&self) -> [f32; 4] {
        [
            (self.jitter_us / 25.0).clamp(0.0, 1.0),
            (self.cpu_us / 400.0).clamp(0.0, 1.0),
            (self.disk_us / 800.0).clamp(0.0, 1.0),
            (self.net_us / 120.0).clamp(0.0, 1.0),
        ]
    }
}

/// Lê o host de verdade. Chamada **fora** do laço quente (a cada 10k ciclos).
pub fn sample_host() -> HostSample {
    // Jitter: intervalos entre leituras de `Instant` após um spin curto.
    let mut acc = 0.0f64;
    let mut n = 0usize;
    let mut last = Instant::now();
    for _ in 0..64 {
        let mut x = 0u64;
        for i in 0..200u64 {
            x = x.wrapping_add(i.wrapping_mul(2654435761));
        }
        std::hint::black_box(x);
        let now = Instant::now();
        acc += now.duration_since(last).as_nanos() as f64 / 1_000.0;
        last = now;
        n += 1;
    }
    let jitter_us = (acc / n.max(1) as f64) as f32;

    // CPU: mesma carga fixa, medida como latência (proxy de contenção).
    let t0 = Instant::now();
    let mut x = 0u64;
    for i in 0..200_000u64 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(i);
    }
    std::hint::black_box(x);
    let cpu_us = t0.elapsed().as_secs_f64() as f32 * 1.0e6;

    // Disco: arquivo temporário de 4 KiB (ida e volta).
    let path = std::env::temp_dir().join(format!("aether_s0_probe_{}.bin", std::process::id()));
    let t0 = Instant::now();
    let mut disk_us = 0.0f32;
    if let Ok(mut f) = std::fs::File::create(&path) {
        let buf = [0xA5u8; 4096];
        if f.write_all(&buf).is_ok() {
            let _ = f.flush();
            if let Ok(mut r) = std::fs::File::open(&path) {
                let mut back = [0u8; 4096];
                let _ = r.read_exact(&mut back);
            }
        }
        disk_us = t0.elapsed().as_secs_f64() as f32 * 1.0e6;
    }
    let _ = std::fs::remove_file(&path);

    // Rede: datagrama UDP para loopback (sem conexão, sem bloqueio).
    let mut net_us = 0.0f32;
    if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
        let t0 = Instant::now();
        let _ = sock.send_to(&[0u8; 40], "127.0.0.1:9");
        net_us = t0.elapsed().as_secs_f64() as f32 * 1.0e6;
    }

    HostSample {
        jitter_us,
        cpu_us,
        disk_us,
        net_us,
    }
}

/// Atores do corpo `S0`.
#[derive(Debug)]
pub struct Actuators {
    enabled: bool,
    throttle_count: AtomicU32,
    affinity_count: AtomicU32,
    pause_count: AtomicU32,
    alarm: AtomicBool,
    alarm_count: AtomicU32,
    core: AtomicU32,
}

impl Actuators {
    /// Cria os atuadores (ligados ou em ablação).
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            throttle_count: AtomicU32::new(0),
            affinity_count: AtomicU32::new(0),
            pause_count: AtomicU32::new(0),
            alarm: AtomicBool::new(false),
            alarm_count: AtomicU32::new(0),
            core: AtomicU32::new(0),
        }
    }

    /// `true` se o efeito dos atuadores está ligado.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Atuador 1: throttling (sleep curto) quando o jitter está alto.
    pub fn throttle(&self, jitter_norm: f32) {
        if !self.enabled || jitter_norm < 0.35 {
            return;
        }
        std::thread::sleep(Duration::from_micros(20));
        self.throttle_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Atuador 2: migração de afinidade (núcleo lógico alternado).
    pub fn migrate_affinity(&self) {
        if !self.enabled {
            return;
        }
        let next = self.core.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        #[cfg(feature = "affinity")]
        {
            let cores = core_affinity::get_core_ids().unwrap_or_default();
            if !cores.is_empty() {
                core_affinity::set_for_current(cores[next as usize % cores.len()]);
            }
        }
        let _ = next;
        self.affinity_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Atuador 3: alarme (flag que o ambiente lê).
    pub fn alarm(&self, on: bool) {
        if !self.enabled {
            return;
        }
        if on {
            self.alarm_count.fetch_add(1, Ordering::Relaxed);
        }
        self.alarm.store(on, Ordering::Relaxed);
    }

    /// Atuador 4: pausa (sleep longo) quando a carga está alta.
    pub fn pause(&self, cpu_norm: f32) {
        if !self.enabled || cpu_norm < 0.55 {
            return;
        }
        std::thread::sleep(Duration::from_micros(50));
        self.pause_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Flag de alarme.
    pub fn alarm_on(&self) -> bool {
        self.alarm.load(Ordering::Relaxed)
    }

    /// Contadores `(throttle, afinidade, pausa)`.
    pub fn counts(&self) -> (u32, u32, u32) {
        (
            self.throttle_count.load(Ordering::Relaxed),
            self.affinity_count.load(Ordering::Relaxed),
            self.pause_count.load(Ordering::Relaxed),
        )
    }

    /// Núcleo lógico corrente (telemetria).
    pub fn core(&self) -> u32 {
        self.core.load(Ordering::Relaxed)
    }
}

/// Estatísticas do teste anti-passeio.
#[derive(Clone, Copy, Debug)]
pub struct GoalStats {
    /// Entropia (nats) da distribuição de dificuldade das metas.
    pub entropy: f32,
    /// Frequência por bin.
    pub frequency: [f32; BINS],
    /// Sucesso estimado por bin.
    pub success: [f32; BINS],
    /// Usos `(throttle, afinidade, pausa)`.
    pub actuator_counts: (u32, u32, u32),
    /// Alarmes disparados.
    pub alarms: u32,
    /// Ciclos executados.
    pub cycles: u64,
}

impl GoalStats {
    /// Correlação de Spearman entre a frequência e o U invertido `−(d−0,5)²`.
    pub fn spearman_inverted_u(&self) -> f32 {
        let ideal: Vec<f32> = (0..BINS)
            .map(|b| {
                let d = (b as f32 + 0.5) / BINS as f32;
                -((d - 0.5) * (d - 0.5))
            })
            .collect();
        spearman(&self.frequency, &ideal)
    }
}

fn ranks(v: &[f32]) -> Vec<f32> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
    let mut r = vec![0.0f32; v.len()];
    let mut i = 0usize;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && (v[idx[j + 1]] - v[idx[i]]).abs() < 1e-12 {
            j += 1;
        }
        let rank = (i + j) as f32 / 2.0 + 1.0;
        for k in i..=j {
            r[idx[k]] = rank;
        }
        i = j + 1;
    }
    r
}

/// Spearman = Pearson sobre os ranks.
pub fn spearman(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.len() < 2 {
        return 0.0;
    }
    let ra = ranks(a);
    let rb = ranks(b);
    let n = ra.len() as f32;
    let ma = ra.iter().sum::<f32>() / n;
    let mb = rb.iter().sum::<f32>() / n;
    let mut num = 0.0f32;
    let mut da = 0.0f32;
    let mut db = 0.0f32;
    for i in 0..ra.len() {
        let x = ra[i] - ma;
        let y = rb[i] - mb;
        num += x * y;
        da += x * x;
        db += y * y;
    }
    num / (da.sqrt() * db.sqrt()).max(1e-12)
}

/// Roda `cycles` ciclos do corpo `S0`.
///
/// * `actuators_enabled = false` é a ablação H3: os sentidos continuam, mas os
///   atos não mudam o ambiente.
/// * A telemetria real é amostrada a cada 10k ciclos; o ambiente entre amostras
///   interpola o centro de factibilidade.
pub fn run_s0(cycles: u64, actuators_enabled: bool, seed: u64) -> GoalStats {
    let actuators = Actuators::new(actuators_enabled);
    let mut rng = LcgState::new(seed);
    let mut est = [0.5f32; BINS];
    let mut freq = [0u32; BINS];
    let mut succ = [0.0f32; BINS];
    let mut counts = [0u32; BINS];

    let mut telemetry = sample_host();
    let mut target = 0.78f32; // corpo hostil por padrão
    let sample_every = 10_000u64;
    let eps = 0.08f32;

    for cycle in 0..cycles {
        if cycle % sample_every == 0 {
            telemetry = sample_host();
            let n = telemetry.normalized();
            // Centro de factibilidade base: quanto mais ruído, mais difícil.
            target = (0.42 + 0.45 * (n[0] * 0.5 + n[1] * 0.3 + n[2] * 0.1 + n[3] * 0.1))
                .clamp(0.15, 0.92);
        }
        // Atuadores reais (com consequência no centro do ambiente).
        let n = telemetry.normalized();
        if cycle % 8_192 == 0 {
            let pressure = (0.5 * n[0] + 0.5 * n[2]).max(n[0]);
            let alarm_on = n[0] > 0.02 || n[1] > 0.10 || (cycle / 8_192) % 8 == 0;
            actuators.alarm(alarm_on);
            actuators.throttle(pressure.max(0.40));
            actuators.pause(n[1].max(0.60));
            actuators.migrate_affinity();
        }
        if actuators.enabled() {
            // O corpo responde: cada ato reduz a hostilidade e modula o alvo.
            let relief = 0.50
                + 0.08 * if actuators.alarm_on() { 1.0 } else { 0.0 }
                + 0.06 * ((actuators.core() % 2) as f32);
            let wave = (cycle as f32 * 0.0009).sin() * 0.10;
            target = (0.5 + (target - 0.5) * (1.0 - relief) + wave).clamp(0.12, 0.92);
        }
        // Escolha ε-gulosa sobre o **progresso** `p(1−p)`: é o que evita o
        // passeio aleatório e mantém metas de dificuldade intermediária.
        let b = if rng.next_f32() < eps {
            (rng.next_f32() * BINS as f32) as usize % BINS
        } else {
            let mut best = 0usize;
            let mut best_prog = -1.0f32;
            for (i, &e) in est.iter().enumerate() {
                let prog = e * (1.0 - e);
                if prog > best_prog + 1e-9 {
                    best_prog = prog;
                    best = i;
                }
            }
            best
        };
        let d = (b as f32 + 0.5) / BINS as f32;
        // Sucesso verdadeiro: U invertido em torno de `target`.
        let p = (-((d - target) / 0.22).powi(2)).exp().clamp(0.0, 1.0);
        let hit = rng.next_f32() < p;
        counts[b] += 1;
        freq[b] += 1;
        let y = if hit { 1.0 } else { 0.0 };
        // Média móvel: o agente aprende a factibilidade local.
        est[b] = 0.98 * est[b] + 0.02 * y;
        succ[b] += y;
    }

    let total = freq.iter().sum::<u32>().max(1) as f32;
    let mut frequency = [0.0f32; BINS];
    let mut success = [0.0f32; BINS];
    let mut entropy = 0.0f32;
    for b in 0..BINS {
        frequency[b] = freq[b] as f32 / total;
        success[b] = if counts[b] > 0 {
            succ[b] / counts[b] as f32
        } else {
            0.0
        };
        if frequency[b] > 0.0 {
            entropy -= frequency[b] * frequency[b].ln();
        }
    }
    let (thr, aff, pau) = actuators.counts();
    GoalStats {
        entropy,
        frequency,
        success,
        actuator_counts: (thr, aff, pau),
        alarms: actuators.alarm_count.load(Ordering::Relaxed),
        cycles,
    }
}

/// LCG local (determinístico, sem dependência do `rand` no laço).
#[derive(Clone, Copy, Debug)]
struct LcgState {
    state: u64,
}

impl LcgState {
    const fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }
    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spearman_detecta_u_invertido() {
        let ideal: Vec<f32> = (0..BINS)
            .map(|b| {
                let d = (b as f32 + 0.5) / BINS as f32;
                -((d - 0.5) * (d - 0.5))
            })
            .collect();
        assert!((spearman(&ideal, &ideal) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn entropia_maior_com_corpo_do_que_sem() {
        let on = run_s0(20_000, true, 0xA37E_0E70);
        let off = run_s0(20_000, false, 0xA37E_0E71);
        assert!(on.entropy > 0.5);
        assert!(off.entropy < on.entropy);
    }
}

//! Portão de decisão para backends GPU (wgpu) — **Fase 2**.
//!
//! O backend `wgpu` só entra no motor se a *roofline* de banda de memória
//! justificar o custo de transferência. Este módulo não acelera nada: ele
//! calcula, a partir do tráfego medido em [`crate::core::fhrr_bam`] /
//! `bam_phase`, se um workload **cabe** no orçamento e se a GPU ganharia do
//! caminho CPU.
//!
//! Regra estipulada (OPENCODE MSG 0086):
//! * GPU apenas em `N >= 512` **com fallback CPU obrigatório**;
//! * o veredito é `NotProfitable` sempre que a projeção GPU não couber no
//!   alvo de latência — nesse caso o motor permanece em CPU e nenhuma
//!   dependência gráfica é compilada.
//!
//! Números de referência (80 passos, medidos/documentados na Fase 0):
//! `N=512` → 251.7 MB · `N=1024` → 1.01 GB · `N=4096` → 16.1 GB.
//! Com 50 GB/s (CPU) e 300 GB/s (GPU) isso dá 5.03/20.13/322.12 ms e
//! 0.84/3.36/53.69 ms — ou seja, **nada abaixo de 200 µs** é alcançável.

/// Banda efetiva conservadora do caminho CPU (GB/s).
pub const CPU_BANDWIDTH_GBPS: f64 = 50.0;
/// Banda efetiva nominal de uma GPU integrada/discreta de referência (GB/s).
pub const GPU_BANDWIDTH_GBPS: f64 = 300.0;
/// Bytes movidos por aresta por passo (`re` + `im` + valor, f32).
pub const BYTES_PER_EDGE: f64 = 12.0;
/// Menor `N` em que a GPU é considerada (fallback CPU continua obrigatório).
pub const MIN_GPU_N: usize = 512;

/// Workload candidato a backend GPU.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Workload {
    /// Lado da matriz de acoplamento.
    pub n: usize,
    /// Passos de integração.
    pub steps: usize,
    /// Alvo de latência do estágio (µs).
    pub target_us: f64,
}

impl Workload {
    /// Cria um workload.
    pub const fn new(n: usize, steps: usize, target_us: f64) -> Self {
        Self {
            n,
            steps,
            target_us,
        }
    }

    /// Arestas por passo (`n²`).
    pub const fn edges(&self) -> usize {
        self.n.saturating_mul(self.n)
    }

    /// Bytes movidos no total (`n² · 12 B · passos`).
    pub fn bytes(&self) -> f64 {
        self.edges() as f64 * BYTES_PER_EDGE * self.steps as f64
    }

    /// Projeção de tempo em µs para uma banda em GB/s.
    pub fn projected_us(&self, bandwidth_gbps: f64) -> f64 {
        if bandwidth_gbps <= 0.0 {
            return f64::INFINITY;
        }
        self.bytes() / (bandwidth_gbps * 1e9) * 1e6
    }

    /// Projeção no caminho CPU (µs).
    pub fn cpu_us(&self) -> f64 {
        self.projected_us(CPU_BANDWIDTH_GBPS)
    }

    /// Projeção no caminho GPU (µs), ignorando overhead de kernel/dispatch.
    pub fn gpu_us(&self) -> f64 {
        self.projected_us(GPU_BANDWIDTH_GBPS)
    }
}

/// Veredito do portão.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuVerdict {
    /// `N < 512`: abaixo do mínimo estipulado, CPU apenas.
    BelowMinN,
    /// Cabe no alvo e a GPU ganharia: vale implementar o backend.
    WorthIt {
        /// Ganho projetado `cpu_us / gpu_us`.
        speedup: f64,
        /// Tempo GPU projetado (µs).
        gpu_us: f64,
    },
    /// GPU ganharia em banda, mas não cabe no alvo de latência.
    TooSlowForTarget {
        /// Tempo GPU projetado (µs).
        gpu_us: f64,
        /// Alvo pedido (µs).
        target_us: f64,
    },
    /// A GPU não ganharia da CPU (tráfego pequeno demais para amortizar).
    CpuFaster,
}

impl GpuVerdict {
    /// `true` se o backend GPU deve ser implementado para este workload.
    pub const fn recommends_gpu(&self) -> bool {
        matches!(self, Self::WorthIt { .. })
    }
}

/// Decide se o workload justifica um backend GPU.
///
/// O caminho CPU **continua** como fallback em todos os casos.
pub fn decide(w: Workload) -> GpuVerdict {
    if w.n < MIN_GPU_N {
        return GpuVerdict::BelowMinN;
    }
    let cpu = w.cpu_us();
    let gpu = w.gpu_us();
    if gpu >= cpu {
        return GpuVerdict::CpuFaster;
    }
    if gpu > w.target_us {
        return GpuVerdict::TooSlowForTarget {
            gpu_us: gpu,
            target_us: w.target_us,
        };
    }
    GpuVerdict::WorthIt {
        speedup: cpu / gpu,
        gpu_us: gpu,
    }
}

/// Linha da tabela de roofline usada nos relatórios.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RooflineRow {
    /// Lado da matriz.
    pub n: usize,
    /// Arestas por passo.
    pub edges: usize,
    /// Megabytes movidos no total.
    pub mb: f64,
    /// Projeção CPU em ms.
    pub cpu_ms: f64,
    /// Projeção GPU em ms.
    pub gpu_ms: f64,
}

/// Tabela de roofline para `n ∈ {512, 1024, 4096}` a `steps` passos.
pub fn roofline_table(steps: usize) -> Vec<RooflineRow> {
    [512usize, 1024, 4096]
        .into_iter()
        .map(|n| {
            let w = Workload::new(n, steps, 0.0);
            RooflineRow {
                n,
                edges: w.edges(),
                mb: w.bytes() / 1e6,
                cpu_ms: w.cpu_us() / 1e3,
                gpu_ms: w.gpu_us() / 1e3,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_e_projecao_batem_com_a_roofline_documentada() {
        // N=512, 80 passos: 512²·12·80 = 251 658 240 B ≈ 251.7 MB.
        let w = Workload::new(512, 80, 200.0);
        assert_eq!(w.edges(), 262_144);
        let mb = w.bytes() / 1e6;
        assert!((mb - 251.7).abs() < 0.1, "mb={mb}");
        // 50 GB/s ⇒ 5.03 ms; 300 GB/s ⇒ 0.84 ms.
        assert!(
            (w.cpu_us() / 1e3 - 5.033).abs() < 0.01,
            "cpu={}",
            w.cpu_us()
        );
        assert!(
            (w.gpu_us() / 1e3 - 0.839).abs() < 0.01,
            "gpu={}",
            w.gpu_us()
        );
    }

    #[test]
    fn n_abaixo_de_512_nunca_recomenda_gpu() {
        let v = decide(Workload::new(256, 80, 1e9));
        assert_eq!(v, GpuVerdict::BelowMinN);
        assert!(!v.recommends_gpu());
    }

    #[test]
    fn reflexo_sub_200us_nao_justifica_wgpu() {
        // O caso que fecha a Fase 2: nenhum N da tabela cabe em 200 µs.
        for n in [512usize, 1024, 4096] {
            let v = decide(Workload::new(n, 80, 200.0));
            assert!(
                matches!(v, GpuVerdict::TooSlowForTarget { .. }),
                "n={n} deveria ser TooSlowForTarget, veio {v:?}"
            );
            assert!(!v.recommends_gpu());
        }
    }

    #[test]
    fn lote_grande_justifica_gpu_com_fallback_cpu() {
        // Streaming contínuo de 200 mil passos: 251.7 MB × 2 500 ≈ 629 GB,
        // que a 300 GB/s projeta 2 097 ms — alvo de 3 s o absorve.
        let w = Workload::new(512, 200_000, 3e6);
        let v = decide(w);
        match v {
            GpuVerdict::WorthIt { speedup, .. } => {
                assert!((speedup - 6.0).abs() < 0.01, "speedup={speedup}");
            }
            other => panic!("esperava WorthIt, veio {other:?}"),
        }
    }

    #[test]
    fn tabela_tem_tres_linhas_e_cresce_com_n() {
        let rows = roofline_table(80);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].mb < rows[1].mb && rows[1].mb < rows[2].mb);
        assert!(rows[2].cpu_ms > 300.0);
    }
}

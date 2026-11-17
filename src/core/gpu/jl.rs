//! GEMM do transplante na GPU (G1 do `docs/GPU_PLAN.md`).
//!
//! O censo (`examples/gpu_census.rs`) mediu **39,3 GFLOP em 13,4 s** de CPU
//! para 10⁴ embeddings (`d = 384 → 10.240 bits`), a **2,9 GFLOP/s**. O kernel
//! abaixo faz a mesma conta com um thread por `(bit, linha)`:
//!
//! ```text
//! para cada bit b e linha r:  acc = Σ_j mat[b][j] · x[r][j]   (j ascendente)
//!                             bit_b(r) = acc ≥ 0
//! ```
//!
//! * a soma é **j ascendente**, como no caminho CPU, então o resultado é o
//!   mesmo bit a bit (o teste `gpu_jl_gate` compara e exige 0 divergências);
//! * lançamento em `(bits, n_rows)`: threads consecutivas compartilham o mesmo
//!   `bit` (broadcast da matriz pela L1) e depois a mesma linha;
//! * a matriz é gerada **na CPU** pelo mesmo `JlProjector::fill_matrix`
//!   (mesmo RNG/gauss do caminho CPU) e sobe uma vez por sessão.

use super::GpuOff;

/// Erro do caminho GPU (nunca fatal: o chamador cai para CPU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JlGpuError {
    /// Backend indisponível (ver [`GpuOff`]).
    Off(GpuOff),
    /// Buffer curto para o resultado.
    ShortOut,
}

impl From<GpuOff> for JlGpuError {
    fn from(off: GpuOff) -> Self {
        Self::Off(off)
    }
}

/// Largura do *workgroup* (threads) do kernel.
pub const WG: u32 = 64;

/// Kernel **v2**: coalescido, sem átomos, 32 acumuladores por thread.
///
/// A matriz vem **transposta** (`d × bits`): as 32 colunas de cada thread são
/// 32 `f32` consecutivos, então um warp lê linhas de cache inteiras. A soma
/// continua j-ascendente, então o bit é idêntico ao caminho CPU.
#[cfg(feature = "gpu")]
const SHADER_V2: &str = r#"
struct Params {
    n_rows: u32,
    d: u32,
    bits: u32,
    words: u32,   // palavras de 32 bits por linha (2 x u64 words)
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mat: array<f32>;
@group(0) @binding(2) var<storage, read> x: array<f32>;
@group(0) @binding(3) var<storage, read_write> out: array<u32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let word = gid.x;
    let row = gid.y;
    if (word >= p.words || row >= p.n_rows) {
        return;
    }
    var acc: array<f32, 32>;
    for (var b: u32 = 0u; b < 32u; b = b + 1u) {
        acc[b] = 0.0;
    }
    let xbase = row * p.d;
    let mbase = word * 32u;
    for (var j: u32 = 0u; j < p.d; j = j + 1u) {
        let xv = x[xbase + j];
        let rb = j * p.bits + mbase;
        for (var b: u32 = 0u; b < 32u; b = b + 1u) {
            acc[b] = acc[b] + mat[rb + b] * xv;
        }
    }
    var packed: u32 = 0u;
    for (var b: u32 = 0u; b < 32u; b = b + 1u) {
        if (acc[b] >= 0.0) {
            packed = packed | (1u << b);
        }
    }
    out[row * p.words + word] = packed;
}
"#;

/// Qual kernel usar.
///
/// * [`JlKernel::V1`] — 1 thread por `(bit, linha)`, matriz row-major, `atomicOr`.
///   Foi o primeiro a validar (bit-a-bit, 3× no RTX 3050).
/// * [`JlKernel::V2`] — 1 thread por `(palavra de 32 bits, linha)`, matriz
///   **transposta** (`d × bits`), 32 acumuladores, sem átomos (coalescido).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JlKernel {
    /// Kernel original (átomos, row-major).
    V1,
    /// Kernel coalescido (sem átomos, matriz transposta).
    V2,
}

/// Kernel padrão do caminho GPU.
pub const DEFAULT_KERNEL: JlKernel = JlKernel::V2;

/// Transpõe a matriz JL (`bits × d` → `d × bits`), sem alocar.
///
/// É o layout que o kernel v2 exige; faça isso **uma vez por sessão** e reusa
/// o buffer entre lotes (o custo é ~15,7 MB de cópia).
pub fn transpose_into(row_major: &[f32], bits: usize, d: usize, out: &mut [f32]) {
    assert!(row_major.len() >= bits * d && out.len() >= bits * d);
    for bit in 0..bits {
        for j in 0..d {
            out[j * bits + bit] = row_major[bit * d + j];
        }
    }
}

#[cfg(feature = "gpu")]
const SHADER_V1: &str = r#"
struct Params {
    n_rows: u32,
    d: u32,
    bits: u32,
    words: u32,   // palavras de 64 bits por linha
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mat: array<f32>;
@group(0) @binding(2) var<storage, read> x: array<f32>;
@group(0) @binding(3) var<storage, read_write> out: array<atomic<u32>>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // eixo x = bit (bit-major: o workgroup compartilha o bit), eixo y = linha
    let bit = gid.x;
    let row = gid.y;
    if (bit >= p.bits || row >= p.n_rows) {
        return;
    }
    let mbase = bit * p.d;
    let xbase = row * p.d;
    var acc: f32 = 0.0;
    for (var j: u32 = 0u; j < p.d; j = j + 1u) {
        acc = acc + mat[mbase + j] * x[xbase + j];
    }
    if (acc >= 0.0) {
        let word = bit >> 5u;
        let shift = bit & 31u;
        atomicOr(&out[(row * p.words) * 2u + word], 1u << shift);
    }
}
"#;
#[cfg(feature = "gpu")]
mod imp {
    use super::{JlGpuError, JlKernel, DEFAULT_KERNEL, SHADER_V1, SHADER_V2, WG};
    use crate::core::gpu;

    /// Roda o GEMM do JL. `out_words` tem `n_rows · words` `u64`.
    ///
    /// Devolve `Err` quando não há device — o chamador **deve** cair para CPU.
    pub fn project_into(
        mat: &[f32],
        rows: &[f32],
        n_rows: usize,
        d: usize,
        bits: usize,
        out_words: &mut [u64],
    ) -> Result<(), JlGpuError> {
        project_into_with(DEFAULT_KERNEL, false, mat, rows, n_rows, d, bits, out_words)
    }

    /// Igual a [project_into], com orce_off (teste determinístico do fallback).
    #[allow(clippy::too_many_arguments)]
    pub fn project_into_with(
        kernel: JlKernel,
        force_off: bool,
        mat: &[f32],
        rows: &[f32],
        n_rows: usize,
        d: usize,
        bits: usize,
        out_words: &mut [u64],
    ) -> Result<(), JlGpuError> {
        let words = bits.div_ceil(64);
        if out_words.len() < n_rows * words {
            return Err(JlGpuError::ShortOut);
        }
        let (device, queue, _name) = gpu::device_with(force_off)?;

        // --- buffers ---
        use wgpu::util::DeviceExt;
        let mat_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jl-mat"),
            contents: bytemuck::cast_slice(mat),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let x_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jl-x"),
            contents: bytemuck::cast_slice(rows),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let zero = vec![0u8; n_rows * words * 8];
        let out_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jl-out"),
            contents: &zero,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jl-staging"),
            size: (n_rows * words * 8) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // v1 usa palavras de 64 bits; v2 usa palavras de 32 bits (2× words)
        let w32 = match kernel {
            JlKernel::V1 => words,
            JlKernel::V2 => words * 2,
        };
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jl-params"),
            contents: bytemuck::cast_slice(&[n_rows as u32, d as u32, bits as u32, w32 as u32]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // --- pipeline ---
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jl-shader"),
            source: wgpu::ShaderSource::Wgsl(
                match kernel {
                    JlKernel::V1 => SHADER_V1,
                    JlKernel::V2 => SHADER_V2,
                }
                .into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jl-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jl-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("jl-pipe"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jl-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mat_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: x_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: out_buf.as_entire_binding(),
                },
            ],
        });

        // --- dispatch ---
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jl-enc"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("jl-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let gx = (w32 as u32).div_ceil(WG);
            pass.dispatch_workgroups(gx, n_rows as u32, 1);
        }
        encoder.copy_buffer_to_buffer(&out_buf, 0, &staging, 0, (n_rows * words * 8) as u64);
        queue.submit(Some(encoder.finish()));

        // --- readback ---
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::Maintain::Wait);
        if rx.recv().map(|r| r.is_err()).unwrap_or(true) {
            return Err(JlGpuError::Off(super::GpuOff::NoDevice));
        }
        let view = slice.get_mapped_range();
        let words32: &[u32] = bytemuck::cast_slice(&view);
        for (i, w) in out_words.iter_mut().enumerate().take(n_rows * words) {
            let lo = u64::from(words32[i * 2]);
            let hi = u64::from(words32[i * 2 + 1]);
            *w = lo | (hi << 32);
        }
        drop(view);
        staging.unmap();
        Ok(())
    }
}

#[cfg(feature = "gpu")]
pub use imp::{project_into, project_into_with};

/// Sem a feature: recusa explícita (o chamador cai para CPU).
/// Assinatura espelha a variante `feature = "gpu"` (8 args); allow local.
#[cfg(not(feature = "gpu"))]
#[allow(clippy::too_many_arguments)]
pub fn project_into_with(
    _kernel: JlKernel,
    _force_off: bool,
    _mat: &[f32],
    _rows: &[f32],
    _n_rows: usize,
    _d: usize,
    _bits: usize,
    _out_words: &mut [u64],
) -> Result<(), JlGpuError> {
    Err(JlGpuError::Off(GpuOff::NoFeature))
}

/// Sem a feature: recusa explícita (o chamador cai para CPU).
#[cfg(not(feature = "gpu"))]
pub fn project_into(
    _mat: &[f32],
    _rows: &[f32],
    _n_rows: usize,
    _d: usize,
    _bits: usize,
    _out_words: &mut [u64],
) -> Result<(), JlGpuError> {
    Err(JlGpuError::Off(GpuOff::NoFeature))
}

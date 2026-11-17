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

const SHADER: &str = r#"
struct Params {
    n_rows: u32,
    d: u32,
    bits: u32,
    words: u32,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> mat: array<f32>;
@group(0) @binding(2) var<storage, read> x: array<f32>;
@group(0) @binding(3) var<storage, read_write> out: array<atomic<u32>>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // eixo x = bit, eixo y = linha (bit-major: o workgroup compartilha o bit)
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
    use super::{JlGpuError, SHADER, WG};
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
        project_into_with(false, mat, rows, n_rows, d, bits, out_words)
    }

    /// Igual a [project_into], com orce_off (teste determinístico do fallback).
    #[allow(clippy::too_many_arguments)]
    pub fn project_into_with(
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
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jl-params"),
            contents: bytemuck::cast_slice(&[
                n_rows as u32,
                d as u32,
                bits as u32,
                words as u32,
            ]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // --- pipeline ---
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jl-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
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
            let gx = (bits as u32).div_ceil(WG);
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
#[cfg(not(feature = "gpu"))]
pub fn project_into_with(
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

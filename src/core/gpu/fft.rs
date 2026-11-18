//! FFT em lote (G2). O laço de 60 Hz não chama este módulo.
//!
//! O caminho CPU é `rustfft` (referência do gate). O caminho GPU é um
//! Cooley–Tukey iterativo in-place, feature `gpu`, fora do default.
//! `force_off` devolve erro e o chamador usa a CPU — nunca `panic`.

use num_complex::Complex;
use rustfft::FftPlanner;

use super::GpuOff;

/// Erro do lote. Ausência de GPU não é fatal para o organismo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FftGpuError {
    /// Backend indisponível.
    Off(GpuOff),
    /// `n` não é potência de 2, ou o buffer não cabe o lote.
    BadShape,
}

impl From<GpuOff> for FftGpuError {
    fn from(off: GpuOff) -> Self {
        Self::Off(off)
    }
}

/// FFT direta (sem 1/n) de `batch` vetores de comprimento `n`.
pub fn fft_batch_cpu(
    input: &[Complex<f32>],
    n: usize,
    batch: usize,
) -> Result<Vec<Complex<f32>>, FftGpuError> {
    if n == 0 || !n.is_power_of_two() || input.len() != batch.saturating_mul(n) {
        return Err(FftGpuError::BadShape);
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut out = input.to_vec();
    for chunk in out.chunks_mut(n) {
        fft.process(chunk);
    }
    Ok(out)
}

/// Bit-reversal por linha. O kernel iterativo assume ordem natural na saída.
#[cfg(feature = "gpu")]
fn bit_reverse_rows(input: &[Complex<f32>], n: usize, batch: usize) -> Vec<Complex<f32>> {
    let bits = n.trailing_zeros();
    let mut out = vec![Complex::new(0.0, 0.0); input.len()];
    for b in 0..batch {
        let base = b * n;
        for i in 0..n {
            let j = i.reverse_bits() >> (usize::BITS - bits);
            out[base + j] = input[base + i];
        }
    }
    out
}

/// Intercala `re, im` para o buffer da GPU.
#[cfg(feature = "gpu")]
fn pack_f32(rows: &[Complex<f32>]) -> Vec<f32> {
    let mut raw = Vec::with_capacity(rows.len() * 2);
    for c in rows {
        raw.push(c.re);
        raw.push(c.im);
    }
    raw
}

#[cfg(feature = "gpu")]
fn unpack_f32(raw: &[f32]) -> Vec<Complex<f32>> {
    raw.chunks(2).map(|p| Complex::new(p[0], p[1])).collect()
}

/// Tenta a FFT na GPU. Sem feature, sem adapter ou com `force_off`, devolve [`FftGpuError::Off`].
pub fn fft_batch_gpu(
    input: &[Complex<f32>],
    n: usize,
    batch: usize,
    force_off: bool,
) -> Result<Vec<Complex<f32>>, FftGpuError> {
    if n == 0 || !n.is_power_of_two() || batch == 0 || input.len() != batch * n {
        return Err(FftGpuError::BadShape);
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = (input, force_off);
        return Err(FftGpuError::Off(if force_off {
            GpuOff::Disabled
        } else {
            GpuOff::NoFeature
        }));
    }
    #[cfg(feature = "gpu")]
    {
        gpu_fft(input, n, batch, force_off)
    }
}

#[cfg(feature = "gpu")]
fn gpu_fft(
    input: &[Complex<f32>],
    n: usize,
    batch: usize,
    force_off: bool,
) -> Result<Vec<Complex<f32>>, FftGpuError> {
    use wgpu::util::DeviceExt;

    let pipe = fft_pipe(force_off)?;
    let device = &pipe.device;
    let queue = &pipe.queue;
    let reversed = bit_reverse_rows(input, n, batch);
    let packed = pack_f32(&reversed);
    let bytes = packed.len() * 4;

    let buf_a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fft-a"),
        contents: bytemuck::cast_slice(&packed),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let buf_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fft-b"),
        size: bytes as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fft-stage"),
        size: bytes as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let stages = n.trailing_zeros();
    let mut params = Vec::with_capacity(stages as usize);
    let mut binds = Vec::with_capacity(stages as usize);
    let mut len = 2u32;
    for stage in 0..stages {
        let param = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("fft-params"),
            contents: bytemuck::cast_slice(&[n as u32, batch as u32, len, 0u32]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (src, dst) = if stage % 2 == 0 {
            (&buf_a, &buf_b)
        } else {
            (&buf_b, &buf_a)
        };
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fft-bind"),
            layout: &pipe.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: param.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst.as_entire_binding(),
                },
            ],
        });
        params.push(param);
        binds.push(bind);
        len <<= 1;
    }

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("fft-enc"),
    });
    let groups = ((n / 2) as u32).div_ceil(64);
    for bind in &binds {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("fft-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipe.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.dispatch_workgroups(groups, batch as u32, 1);
    }
    let final_buf = if stages % 2 == 1 { &buf_b } else { &buf_a };
    encoder.copy_buffer_to_buffer(final_buf, 0, &staging, 0, bytes as u64);
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let mapped = slice.get_mapped_range();
    let raw: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
    drop(mapped);
    staging.unmap();
    let _ = params;
    Ok(unpack_f32(&raw))
}

#[cfg(feature = "gpu")]
fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[cfg(feature = "gpu")]
fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[cfg(feature = "gpu")]
struct FftPipe {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

#[cfg(feature = "gpu")]
fn fft_pipe(force_off: bool) -> Result<&'static FftPipe, FftGpuError> {
    if force_off {
        return Err(FftGpuError::Off(GpuOff::Disabled));
    }
    static PIPE: std::sync::OnceLock<Result<FftPipe, GpuOff>> = std::sync::OnceLock::new();
    match PIPE.get_or_init(build_fft_pipe) {
        Ok(pipe) => Ok(pipe),
        Err(off) => Err((*off).into()),
    }
}

#[cfg(feature = "gpu")]
fn build_fft_pipe() -> Result<FftPipe, GpuOff> {
    let (device, queue, _) = super::device_with(false)?;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fft-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fft-bgl"),
        entries: &[
            uniform_entry(0),
            storage_entry(1, true),
            storage_entry(2, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fft-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("fft-pipe"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: "main",
        compilation_options: Default::default(),
        cache: None,
    });
    Ok(FftPipe {
        device,
        queue,
        pipeline,
        bgl,
    })
}

#[cfg(feature = "gpu")]
const SHADER: &str = r#"
struct Params {
    n: u32,
    batch: u32,
    len: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec2<f32>>;

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let half = p.len >> 1u;
    let butterflies = p.n >> 1u;
    let butterfly = gid.x;
    let batch = gid.y;
    if (butterfly >= butterflies || batch >= p.batch) {
        return;
    }
    let pos = butterfly % half;
    let block = butterfly / half;
    let base = batch * p.n + block * p.len;
    let i0 = base + pos;
    let i1 = i0 + half;
    let a = src[i0];
    let b = src[i1];
    let ang = -6.283185307179586 * f32(pos) / f32(p.len);
    let w = vec2<f32>(cos(ang), sin(ang));
    let v = cmul(w, b);
    dst[i0] = a + v;
    dst[i1] = a - v;
}
"#;

//! JL **tiled** (G1-v3 do `docs/GPU_PLAN.md`) â€” GEMM do transplante em lote.
//!
//! O v2 (`jl.rs`) Ã© coalescido e sem Ã¡tomos, mas cada linha relÃª a matriz
//! inteira: **31 GB** para 2.000 linhas, ~530 ms. Aqui o bloco carrega o tile da
//! matriz em *shared memory* e o reusa para 64 linhas â€” o trÃ¡fego cai 64Ã—.
//!
//! ```text
//! bloco = (8 palavras de 32 bits) Ã— (64 linhas), 16 warps (512 threads)
//! por chunk de 16 j: sM[8][32][16] â† mat (coalescido por lane = bit)
//!                     sX[64][16]   â† x   (coalescido por j)
//! cada lane: 32 acumuladores, j ascendente (bit-idÃªntico ao caminho CPU)
//! ```
//!
//! A matriz **sobe uma vez por sessÃ£o** ([`JlSession::set_matrix`]): o
//! `fill + transposiÃ§Ã£o` (~84 ms) sai do caminho de cada lote, e o buffer fica
//! residente na GPU.
//!
//! Para a chamada avulsa continua valendo `jl::project_into` (v2).

use super::GpuOff;

/// Erro do caminho GPU (nunca fatal: o chamador cai para CPU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JlTiledError {
    /// Backend indisponÃ­vel (ver [`GpuOff`]).
    Off(GpuOff),
    /// Nenhuma matriz carregada na sessÃ£o.
    NoMatrix,
    /// Buffer curto (ou `d`/`bits` zerados / `rows` nÃ£o mÃºltiplo).
    Shape,
}

impl From<GpuOff> for JlTiledError {
    fn from(off: GpuOff) -> Self {
        Self::Off(off)
    }
}

#[cfg(feature = "gpu")]
const SHADER: &str = include_str!("jl_tiled.wgsl");

#[cfg(feature = "gpu")]
mod sess {
    use super::JlTiledError;
    use crate::core::gpu;

    const WPB: u32 = 8;
    const RT: u32 = 32;

    /// Estado persistente do GEMM: device, pipeline, matriz transposta e
    /// buffers de lote.
    pub struct Session {
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: String,
        pipeline: wgpu::ComputePipeline,
        layout: wgpu::BindGroupLayout,
        params: wgpu::Buffer,
        mat: Option<wgpu::Buffer>,
        x: wgpu::Buffer,
        out: wgpu::Buffer,
        stage: wgpu::Buffer,
        bind: Option<wgpu::BindGroup>,
        pub d: usize,
        pub bits: usize,
        cap_rows: usize,
    }

    impl Session {
        pub fn new(force_off: bool) -> Result<Self, JlTiledError> {
            let (device, queue, adapter) = gpu::device_with(force_off)?;
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("jl-tiled-shader"),
                source: wgpu::ShaderSource::Wgsl(super::SHADER.into()),
            });
            let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..4u32)
                .map(|b| wgpu::BindGroupLayoutEntry {
                    binding: b,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: if b == 0 {
                            wgpu::BufferBindingType::Uniform
                        } else {
                            wgpu::BufferBindingType::Storage { read_only: b < 3 }
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                })
                .collect();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jl-tiled-layout"),
                entries: &entries,
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jl-tiled-pl"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("jl-tiled-pipe"),
                layout: Some(&pl),
                module: &module,
                entry_point: "main",
                compilation_options: Default::default(),
                cache: None,
            });
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-params"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let x = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-x"),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let out = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-out"),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let stage = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-stage"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Ok(Self {
                device,
                queue,
                adapter,
                pipeline,
                layout,
                params,
                mat: None,
                x,
                out,
                stage,
                bind: None,
                d: 0,
                bits: 0,
                cap_rows: 0,
            })
        }

        pub fn adapter(&self) -> &str {
            &self.adapter
        }

        /// Sobe a matriz transposta (`d Ã— bits`) uma vez por sessÃ£o.
        pub fn set_matrix(
            &mut self,
            mat_t: &[f32],
            d: usize,
            bits: usize,
        ) -> Result<(), JlTiledError> {
            if d == 0 || bits == 0 || bits % 32 != 0 || mat_t.len() < d * bits {
                return Err(JlTiledError::Shape);
            }
            if self.d == d && self.bits == bits && self.mat.is_some() {
                self.queue.write_buffer(
                    self.mat.as_ref().expect("mat"),
                    0,
                    bytemuck::cast_slice(&mat_t[..d * bits]),
                );
                return Ok(());
            }
            self.mat = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-mat"),
                size: (d * bits * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.queue.write_buffer(
                self.mat.as_ref().expect("mat"),
                0,
                bytemuck::cast_slice(&mat_t[..d * bits]),
            );
            self.d = d;
            self.bits = bits;
            self.bind = None;
            Ok(())
        }

        /// (Re)dimensiona os buffers do lote.
        fn ensure_rows(&mut self, rows: usize) -> Result<(), JlTiledError> {
            let cap = rows.next_power_of_two().max(64);
            if self.cap_rows >= rows && self.bind.is_some() {
                return Ok(());
            }
            let words = self.bits / 32;
            self.x = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-x"),
                size: (cap * self.d * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.out = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-out"),
                size: (cap * words * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.stage = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jl-tiled-stage"),
                size: (cap * words * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mat = self.mat.as_ref().ok_or(JlTiledError::NoMatrix)?;
            self.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("jl-tiled-bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: mat.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.x.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.out.as_entire_binding(),
                    },
                ],
            }));
            self.cap_rows = cap;
            Ok(())
        }

        /// Projeta `n_rows` linhas em bits.
        pub fn project_into(
            &mut self,
            rows: &[f32],
            n_rows: usize,
            out_words: &mut [u64],
        ) -> Result<(), JlTiledError> {
            let (d, bits) = (self.d, self.bits);
            if d == 0 || bits == 0 {
                return Err(JlTiledError::NoMatrix);
            }
            let words64 = bits / 64;
            if n_rows == 0 || rows.len() < n_rows * d || out_words.len() < n_rows * words64 {
                return Err(JlTiledError::Shape);
            }
            self.ensure_rows(n_rows)?;
            let words32 = bits / 32;
            self.queue.write_buffer(
                &self.params,
                0,
                bytemuck::cast_slice(&[n_rows as u32, d as u32, bits as u32, words32 as u32]),
            );
            self.queue
                .write_buffer(&self.x, 0, bytemuck::cast_slice(&rows[..n_rows * d]));

            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("jl-tiled-enc"),
                });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("jl-tiled-pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, self.bind.as_ref().ok_or(JlTiledError::NoMatrix)?, &[]);
                let gx = (words32 as u32).div_ceil(WPB);
                let gy = (n_rows as u32).div_ceil(RT);
                pass.dispatch_workgroups(gx, gy, 1);
            }
            let bytes = (n_rows * words32 * 4) as u64;
            enc.copy_buffer_to_buffer(&self.out, 0, &self.stage, 0, bytes);
            self.queue.submit(Some(enc.finish()));

            let slice = self.stage.slice(..bytes);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            self.device.poll(wgpu::Maintain::Wait);
            if rx.recv().map(|r| r.is_err()).unwrap_or(true) {
                return Err(JlTiledError::Off(gpu::GpuOff::NoDevice));
            }
            let view = slice.get_mapped_range();
            let w32: &[u32] = bytemuck::cast_slice(&view);
            for (i, w) in out_words.iter_mut().enumerate().take(n_rows * words64) {
                let lo = u64::from(w32[i * 2]);
                let hi = u64::from(w32[i * 2 + 1]);
                *w = lo | (hi << 32);
            }
            drop(view);
            self.stage.unmap();
            Ok(())
        }
    }
}

/// SessÃ£o do GEMM JL tiled â€” **o caminho para lotes**.
///
/// ```text
/// let mut s = JlSession::new()?;
/// s.set_matrix(&mat_t, 384, 10_240)?;   // uma vez por sessão
/// s.project_into(&rows, n, &mut out)?;  // por lote
/// ```
pub struct JlSession {
    #[cfg(feature = "gpu")]
    inner: sess::Session,
}

impl std::fmt::Debug for JlSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JlSession")
            .field("adapter", &self.adapter())
            .field("d", &self.d())
            .field("bits", &self.bits())
            .finish()
    }
}

impl JlSession {
    /// Abre a sessÃ£o (paga a compilaÃ§Ã£o do shader uma vez).
    pub fn new() -> Result<Self, JlTiledError> {
        Self::with_force_off(false)
    }

    /// Igual a [JlSession::new], com `force_off` (teste do fallback).
    pub fn with_force_off(force_off: bool) -> Result<Self, JlTiledError> {
        #[cfg(feature = "gpu")]
        {
            Ok(Self {
                inner: sess::Session::new(force_off)?,
            })
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = force_off;
            Err(JlTiledError::Off(GpuOff::NoFeature))
        }
    }

    /// Nome do adapter em uso (vazio sem a feature).
    pub fn adapter(&self) -> &str {
        #[cfg(feature = "gpu")]
        {
            self.inner.adapter()
        }
        #[cfg(not(feature = "gpu"))]
        {
            ""
        }
    }

    /// `d` da matriz carregada (0 = nenhuma).
    pub fn d(&self) -> usize {
        #[cfg(feature = "gpu")]
        {
            self.inner.d
        }
        #[cfg(not(feature = "gpu"))]
        {
            0
        }
    }

    /// `bits` da matriz carregada (0 = nenhuma).
    pub fn bits(&self) -> usize {
        #[cfg(feature = "gpu")]
        {
            self.inner.bits
        }
        #[cfg(not(feature = "gpu"))]
        {
            0
        }
    }

    /// Sobe a matriz **transposta** (`d Ã— bits`, layout de
    /// [`super::jl::transpose_into`]) uma vez por sessÃ£o.
    pub fn set_matrix(&mut self, mat_t: &[f32], d: usize, bits: usize) -> Result<(), JlTiledError> {
        #[cfg(feature = "gpu")]
        {
            self.inner.set_matrix(mat_t, d, bits)
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (mat_t, d, bits);
            Err(JlTiledError::Off(GpuOff::NoFeature))
        }
    }

    /// Projeta `n_rows` linhas (`rows` Ã© `n_rows Ã— d`, `out_words` Ã©
    /// `n_rows Ã— bits/64`).
    pub fn project_into(
        &mut self,
        rows: &[f32],
        n_rows: usize,
        out_words: &mut [u64],
    ) -> Result<(), JlTiledError> {
        #[cfg(feature = "gpu")]
        {
            self.inner.project_into(rows, n_rows, out_words)
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (rows, n_rows, out_words);
            Err(JlTiledError::Off(GpuOff::NoFeature))
        }
    }
}

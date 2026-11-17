//! G3 do `docs/GPU_PLAN.md` â€” **sweep de instÃ¢ncias Ising em lote** na GPU.
//!
//! O censo (`examples/gpu_census.rs`) mediu **123 ms** para 10â´ instÃ¢ncias de
//! `n=20` no solver auditado (0,38 GFLOP). Cada instÃ¢ncia Ã© independente, entÃ£o
//! o paralelismo natural Ã© **uma thread por instÃ¢ncia** â€” sem `atomic`, sem
//! comunicaÃ§Ã£o entre threads.
//!
//! O kernel replica o contrato do `ThermodynamicIsingSolver` em `f32`:
//! integraÃ§Ã£o simplÃ©tica (Euler), rampa linear `a(t)`, paredes inelÃ¡sticas,
//! elitismo sobre a trajetÃ³ria, **polimento guloso** (`descend`) e **fuga
//! determinÃ­stica** com flip Ãºnico â€” os mesmos passos do caminho auditado:
//!
//! ```text
//! hx_i   = Î£_j J_ijÂ·x_j + h_i
//! y_i   += dtÂ·(âˆ’(a0âˆ’a)Â·x_i + c0Â·hx_i)
//! x_i    = clamp(x_i + dtÂ·a0Â·y_i, âˆ’1, 1)      (y_i = 0 na parede)
//! s_i    = sign(x_i)  (sign(0) = +1)
//! E(s)   = âˆ’Â½Â·sáµ€Js âˆ’ háµ€s ; guarda o melhor
//! descend: flip do menor Î”H_i = 2Â·s_iÂ·(g_i + h_i), atÃ© nÃ£o reduzir
//! fuga:    se E(polido) â‰¥ E(s_0), testa flip Ãºnico em cada i e fica com o melhor
//! ```
//!
//! # SessÃ£o, nÃ£o chamada avulsa
//!
//! Compilar o shader custa **250â€“550 ms** (medido): uma chamada avulsa gasta
//! mais nisso do que no lote inteiro. Por isso o caminho de produÃ§Ã£o Ã©
//! [`IsingSweep`], que guarda device, pipeline e buffers entre lotes â€” o custo
//! por lote passa a ser cÃ³pia + dispatch + leitura.
//!
//! # Gate (`tests/gpu_ising_sweep_gate.rs`)
//!
//! `f32` no GPU e `f64` na CPU â‡’ a trajetÃ³ria **nÃ£o** Ã© bit-idÃªntica. O gate Ã©
//! **estatÃ­stico**, como o plano exige: com `J`/`h` inteiros (energias exatas
//! em `f32`), `|Î”E| â‰¤ 1e-3` por instÃ¢ncia em â‰¥95% dos casos, teste KS entre as
//! distribuiÃ§Ãµes de Ã³timos e razÃ£o â‰¥2Ã— medida.
//!
//! **O `digest` do E2 e o `bsb_compacto_igual_ao_solver_auditado` nÃ£o valem no
//! caminho GPU** â€” bit-a-bit morre por construÃ§Ã£o (reduÃ§Ã£o em `f32`).

use super::GpuOff;

/// Maior `n` suportado pelo kernel (arrays fixos de `MAX_N`).
pub const MAX_N: usize = 32;

/// Erros do sweep (nunca fatais: o chamador roda em CPU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsingGpuError {
    /// Backend indisponÃ­vel.
    Off(GpuOff),
    /// `n > MAX_N`, `k == 0`, `steps == 0` ou buffers curtos.
    InvalidShape,
}

impl From<GpuOff> for IsingGpuError {
    fn from(off: GpuOff) -> Self {
        Self::Off(off)
    }
}

#[cfg(feature = "gpu")]
const SHADER: &str = include_str!("ising.wgsl");

#[cfg(feature = "gpu")]
mod sess {
    use super::{IsingGpuError, MAX_N, SHADER};
    use crate::core::gpu;

    const STORAGE_IN: wgpu::BufferUsages =
        wgpu::BufferUsages::STORAGE.union(wgpu::BufferUsages::COPY_DST);
    const STORAGE_RW: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
        .union(wgpu::BufferUsages::COPY_DST)
        .union(wgpu::BufferUsages::COPY_SRC);
    const STAGING: wgpu::BufferUsages =
        wgpu::BufferUsages::MAP_READ.union(wgpu::BufferUsages::COPY_DST);

    fn storage(device: &wgpu::Device, label: &str, bytes: u64, rw: bool) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes,
            usage: if rw { STORAGE_RW } else { STORAGE_IN },
            mapped_at_creation: false,
        })
    }

    /// Estado persistente do sweep (device, pipeline e buffers por capacidade).
    ///
    /// Criar o pipeline custa **250â€“550 ms**; reusar esta sessÃ£o entre lotes Ã© a
    /// diferenÃ§a entre "0,2Ã—" e o ganho real medido.
    pub struct Session {
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: String,
        pipeline: wgpu::ComputePipeline,
        layout: wgpu::BindGroupLayout,
        params: wgpu::Buffer,
        j: wgpu::Buffer,
        h: wgpu::Buffer,
        init: wgpu::Buffer,
        c0: wgpu::Buffer,
        e: wgpu::Buffer,
        s: wgpu::Buffer,
        e_stage: wgpu::Buffer,
        s_stage: wgpu::Buffer,
        bind: Option<wgpu::BindGroup>,
        cap_k: usize,
        cap_n: usize,
    }

    impl Session {
        /// Abre a sessÃ£o (paga a compilaÃ§Ã£o do shader uma vez).
        pub fn new(force_off: bool) -> Result<Self, IsingGpuError> {
            let (device, queue, adapter) = gpu::device_with(force_off)?;
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ising-shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..7u32)
                .map(|b| wgpu::BindGroupLayoutEntry {
                    binding: b,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: if b == 0 {
                            wgpu::BufferBindingType::Uniform
                        } else {
                            wgpu::BufferBindingType::Storage { read_only: b < 5 }
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                })
                .collect();
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ising-layout"),
                entries: &entries,
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ising-pl"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("ising-pipe"),
                layout: Some(&pl),
                module: &module,
                entry_point: "main",
                compilation_options: Default::default(),
                cache: None,
            });
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ising-params"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let j0 = storage(&device, "ising-j", 16, false);
            let h0 = storage(&device, "ising-h", 16, false);
            let i0 = storage(&device, "ising-init", 16, false);
            let c0b = storage(&device, "ising-c0", 16, false);
            let e0 = storage(&device, "ising-e", 4, true);
            let s0 = storage(&device, "ising-s", 16, true);
            let es = storage(&device, "ising-e-stage", 4, false);
            let ss = storage(&device, "ising-s-stage", 16, false);
            let mut s = Self {
                device,
                queue,
                adapter,
                pipeline,
                layout,
                params,
                j: j0,
                h: h0,
                init: i0,
                c0: c0b,
                e: e0,
                s: s0,
                e_stage: es,
                s_stage: ss,
                bind: None,
                cap_k: 0,
                cap_n: 0,
            };
            s.allocate(1, 1)?;
            Ok(s)
        }

        /// Nome do adapter em uso (telemetria).
        pub fn adapter(&self) -> &str {
            &self.adapter
        }

        /// (Re)dimensiona os buffers para `k` instÃ¢ncias de `n` sÃ­tios.
        fn allocate(&mut self, k: usize, n: usize) -> Result<(), IsingGpuError> {
            let cap_k = k.next_power_of_two().max(64);
            if self.cap_k >= k && self.cap_n == n {
                return Ok(());
            }
            let dev = &self.device;
            let kn = (cap_k * n) as u64;
            let knn = (cap_k * n * n) as u64;
            self.j = storage(dev, "ising-j", knn * 4, false);
            self.h = storage(dev, "ising-h", kn * 4, false);
            self.init = storage(dev, "ising-init", kn * 8, false);
            self.c0 = storage(dev, "ising-c0", cap_k as u64 * 4, false);
            self.e = storage(dev, "ising-e", cap_k as u64 * 4, true);
            self.s = storage(dev, "ising-s", kn * 4, true);
            self.e_stage = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ising-e-stage"),
                size: cap_k as u64 * 4,
                usage: STAGING,
                mapped_at_creation: false,
            });
            self.s_stage = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ising-s-stage"),
                size: kn * 4,
                usage: STAGING,
                mapped_at_creation: false,
            });
            self.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ising-bind"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.j.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.h.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.init.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.c0.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.e.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: self.s.as_entire_binding(),
                    },
                ],
            }));
            self.cap_k = cap_k;
            self.cap_n = n;
            Ok(())
        }

        /// Roda `k` instÃ¢ncias (`n â‰¤ MAX_N`) e devolve `(energias, estados)`.
        #[allow(clippy::too_many_arguments)]
        pub fn sweep(
            &mut self,
            kmat: &[f32],
            hmat: &[f32],
            init: &[f32],
            c0: &[f32],
            n: usize,
            k: usize,
            steps: usize,
            out_energy: &mut [f32],
            out_states: &mut [i32],
        ) -> Result<(), IsingGpuError> {
            if n == 0 || n > MAX_N || k == 0 || steps == 0 {
                return Err(IsingGpuError::InvalidShape);
            }
            if kmat.len() < k * n * n
                || hmat.len() < k * n
                || init.len() < k * n * 2
                || c0.len() < k
                || out_energy.len() < k
                || out_states.len() < k * n
            {
                return Err(IsingGpuError::InvalidShape);
            }
            self.allocate(k, n)?;
            let q = &self.queue;
            q.write_buffer(
                &self.params,
                0,
                bytemuck::cast_slice(&[n as u32, steps as u32, k as u32, 0u32]),
            );
            q.write_buffer(&self.j, 0, bytemuck::cast_slice(&kmat[..k * n * n]));
            q.write_buffer(&self.h, 0, bytemuck::cast_slice(&hmat[..k * n]));
            q.write_buffer(&self.init, 0, bytemuck::cast_slice(&init[..k * n * 2]));
            q.write_buffer(&self.c0, 0, bytemuck::cast_slice(&c0[..k]));

            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("ising-enc"),
                });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("ising-pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(
                    0,
                    self.bind.as_ref().ok_or(IsingGpuError::InvalidShape)?,
                    &[],
                );
                pass.dispatch_workgroups(k as u32, 1, 1);
            }
            enc.copy_buffer_to_buffer(&self.e, 0, &self.e_stage, 0, (k * 4) as u64);
            enc.copy_buffer_to_buffer(&self.s, 0, &self.s_stage, 0, (k * n * 4) as u64);
            self.queue.submit(Some(enc.finish()));

            let device = &self.device;
            let read = |buf: &wgpu::Buffer| -> Option<Vec<u8>> {
                let slice = buf.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
                device.poll(wgpu::Maintain::Wait);
                rx.recv().ok()?.ok()?;
                let v = slice.get_mapped_range().to_vec();
                buf.unmap();
                Some(v)
            };
            let ebytes = read(&self.e_stage).ok_or(IsingGpuError::Off(gpu::GpuOff::NoDevice))?;
            let sbytes = read(&self.s_stage).ok_or(IsingGpuError::Off(gpu::GpuOff::NoDevice))?;
            let ev: &[f32] = bytemuck::cast_slice(&ebytes);
            let sv: &[i32] = bytemuck::cast_slice(&sbytes);
            out_energy[..k].copy_from_slice(&ev[..k]);
            out_states[..k * n].copy_from_slice(&sv[..k * n]);
            Ok(())
        }
    }
}

/// SessÃ£o de sweep Ising na GPU â€” **o caminho recomendado para lotes**.
///
/// Compila o kernel uma vez e reusa buffers; cada [`IsingSweep::sweep_into`]
/// paga sÃ³ cÃ³pia + dispatch + leitura.
pub struct IsingSweep {
    #[cfg(feature = "gpu")]
    inner: sess::Session,
}

impl IsingSweep {
    /// Abre a sessÃ£o (paga a compilaÃ§Ã£o do shader uma vez).
    pub fn new() -> Result<Self, IsingGpuError> {
        Self::with_force_off(false)
    }

    /// Igual a [IsingSweep::new], com `force_off` (teste do fallback).
    pub fn with_force_off(force_off: bool) -> Result<Self, IsingGpuError> {
        #[cfg(feature = "gpu")]
        {
            Ok(Self {
                inner: sess::Session::new(force_off)?,
            })
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = force_off;
            Err(IsingGpuError::Off(GpuOff::NoFeature))
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

    /// Roda `k` instÃ¢ncias (`n â‰¤ MAX_N`) e devolve `(energias, estados)`.
    ///
    /// * `kmat` â€” `[k, n, n]` acoplamentos (simÃ©tricos, diagonal zero).
    /// * `hmat` â€” `[k, n]` campo externo.
    /// * `init` â€” `[k, n, 2]` pares `(x_0, y_0)` por sÃ­tio.
    /// * `c0` â€” `[k]` ganho `0.5 / (âˆšn Â· std(J))` (o mesmo do solver).
    ///
    /// Devolve a energia `H(s)` do estado emergente (pÃ³s-polimento e fuga), com
    /// a mesma fÃ³rmula do caminho auditado.
    #[allow(clippy::too_many_arguments)]
    pub fn sweep_into(
        &mut self,
        kmat: &[f32],
        hmat: &[f32],
        init: &[f32],
        c0: &[f32],
        n: usize,
        k: usize,
        steps: usize,
        out_energy: &mut [f32],
        out_states: &mut [i32],
    ) -> Result<(), IsingGpuError> {
        #[cfg(feature = "gpu")]
        {
            self.inner
                .sweep(kmat, hmat, init, c0, n, k, steps, out_energy, out_states)
        }
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (kmat, hmat, init, c0, n, k, steps, out_energy, out_states);
            Err(IsingGpuError::Off(GpuOff::NoFeature))
        }
    }
}

impl std::fmt::Debug for IsingSweep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsingSweep")
            .field("adapter", &self.adapter())
            .finish()
    }
}

/// Sweep avulso (cria a sessÃ£o a cada chamada: paga a compilaÃ§Ã£o do shader).
///
/// Para lotes repetidos use [`IsingSweep`].
#[cfg(feature = "gpu")]
#[allow(clippy::too_many_arguments)]
pub fn sweep_into(
    kmat: &[f32],
    hmat: &[f32],
    init: &[f32],
    c0: &[f32],
    n: usize,
    k: usize,
    steps: usize,
    out_energy: &mut [f32],
    out_states: &mut [i32],
) -> Result<(), IsingGpuError> {
    IsingSweep::new()?.sweep_into(kmat, hmat, init, c0, n, k, steps, out_energy, out_states)
}

/// Sweep avulso com `force_off` (teste determinÃ­stico do fallback).
#[cfg(feature = "gpu")]
#[allow(clippy::too_many_arguments)]
pub fn sweep_into_with(
    force_off: bool,
    kmat: &[f32],
    hmat: &[f32],
    init: &[f32],
    c0: &[f32],
    n: usize,
    k: usize,
    steps: usize,
    out_energy: &mut [f32],
    out_states: &mut [i32],
) -> Result<(), IsingGpuError> {
    IsingSweep::with_force_off(force_off)?
        .sweep_into(kmat, hmat, init, c0, n, k, steps, out_energy, out_states)
}

/// Sem a feature: recusa explÃ­cita (o chamador fica na CPU).
#[cfg(not(feature = "gpu"))]
#[allow(clippy::too_many_arguments)]
pub fn sweep_into(
    _kmat: &[f32],
    _hmat: &[f32],
    _init: &[f32],
    _c0: &[f32],
    _n: usize,
    _k: usize,
    _steps: usize,
    _out_energy: &mut [f32],
    _out_states: &mut [i32],
) -> Result<(), IsingGpuError> {
    Err(IsingGpuError::Off(GpuOff::NoFeature))
}

/// Sem a feature: recusa explÃ­cita (variante com `force_off`).
#[cfg(not(feature = "gpu"))]
#[allow(clippy::too_many_arguments)]
pub fn sweep_into_with(
    _force_off: bool,
    _kmat: &[f32],
    _hmat: &[f32],
    _init: &[f32],
    _c0: &[f32],
    _n: usize,
    _k: usize,
    _steps: usize,
    _out_energy: &mut [f32],
    _out_states: &mut [i32],
) -> Result<(), IsingGpuError> {
    Err(IsingGpuError::Off(GpuOff::NoFeature))
}

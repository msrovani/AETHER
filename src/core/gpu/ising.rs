//! G3 do `docs/GPU_PLAN.md` — **sweep de instâncias Ising em lote** na GPU.
//!
//! O censo (`examples/gpu_census.rs`) mediu **123 ms** para 10⁴ instâncias de
//! `n=20` no solver auditado (0,38 GFLOP). Cada instância é independente, então
//! o paralelismo natural é **uma thread por instância** — sem `atomic`, sem
//! comunicação entre threads.
//!
//! O kernel replica o contrato do `ThermodynamicIsingSolver` em `f32`:
//! integração simplética (Euler), rampa linear `a(t)`, paredes inelásticas,
//! elitismo sobre a trajetória, **polimento guloso** (`descend`) e **fuga
//! determinística** com flip único — os mesmos passos do caminho auditado:
//!
//! ```text
//! hx_i   = Σ_j J_ij·x_j + h_i
//! y_i   += dt·(−(a0−a)·x_i + c0·hx_i)
//! x_i    = clamp(x_i + dt·a0·y_i, −1, 1)      (y_i = 0 na parede)
//! s_i    = sign(x_i)  (sign(0) = +1)
//! E(s)   = −½·sᵀJs − hᵀs ; guarda o melhor
//! descend: flip do menor ΔH_i = 2·s_i·(g_i + h_i), até não reduzir
//! fuga:    se E(polido) ≥ E(s_0), testa flip único em cada i e fica com o melhor
//! ```
//!
//! # Gate (`tests/gpu_ising_sweep_gate.rs`)
//!
//! `f32` no GPU e `f64` na CPU ⇒ a trajetória **não** é bit-idêntica. O gate é
//! **estatístico**, como o plano exige: com `J`/`h` inteiros (energias exatas
//! em `f32`), `|ΔE| ≤ 1e-3` por instância em ≥95% dos casos, teste KS entre as
//! distribuições de ótimos e razão ≥2× medida.
//!
//! **O `digest` do E2 e o `bsb_compacto_igual_ao_solver_auditado` não valem no
//! caminho GPU** — bit-a-bit morre por construção (redução em `f32`).

use super::GpuOff;

/// Maior `n` suportado pelo kernel (arrays fixos de `MAX_N`).
pub const MAX_N: usize = 32;

/// Erros do sweep (nunca fatais: o chamador roda em CPU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsingGpuError {
    /// Backend indisponível.
    Off(GpuOff),
    /// `n > MAX_N`, `k == 0` ou buffers curtos.
    InvalidShape,
}

impl From<GpuOff> for IsingGpuError {
    fn from(off: GpuOff) -> Self {
        Self::Off(off)
    }
}

#[cfg(feature = "gpu")]
const SHADER: &str = r#"
struct P {
    n: u32,
    steps: u32,
    k: u32,
    pad: u32,
};

@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> jm: array<f32>;
@group(0) @binding(2) var<storage, read> hm: array<f32>;
@group(0) @binding(3) var<storage, read> init: array<f32>;
@group(0) @binding(4) var<storage, read> c0m: array<f32>;
@group(0) @binding(5) var<storage, read_write> oute: array<f32>;
@group(0) @binding(6) var<storage, read_write> outs: array<i32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let inst = gid.x;
    if (inst >= p.k) {
        return;
    }
    let n = p.n;

    var x: array<f32, 32>;
    var y: array<f32, 32>;
    var hx: array<f32, 32>;
    var g: array<f32, 32>;
    var s: array<i32, 32>;
    var best: array<i32, 32>;
    var work: array<i32, 32>;

    let jbase = inst * n * n;
    let hbase = inst * n;
    let ibase = inst * n * 2u;

    for (var i: u32 = 0u; i < n; i = i + 1u) {
        x[i] = init[ibase + 2u * i];
        y[i] = init[ibase + 2u * i + 1u];
        s[i] = select(-1, 1, x[i] >= 0.0);
        best[i] = s[i];
    }

    // E(s_0) — a referência da fuga determinística.
    var e0: f32 = 0.0;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        var acc: f32 = 0.0;
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            acc = acc + jm[jbase + i * n + j] * f32(s[j]);
        }
        e0 = e0 - 0.5 * f32(s[i]) * acc;
    }
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        e0 = e0 - hm[hbase + i] * f32(s[i]);
    }
    var e_best: f32 = e0;

    let a0: f32 = 1.0;
    let dt: f32 = 0.05;
    let c0 = c0m[inst];
    let steps_f = f32(p.steps);

    for (var k: u32 = 0u; k < p.steps; k = k + 1u) {
        let a = a0 * f32(k + 1u) / steps_f;
        let drift = a0 - a;
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            var acc: f32 = 0.0;
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                acc = acc + jm[jbase + i * n + j] * x[j];
            }
            hx[i] = acc + hm[hbase + i];
        }
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            y[i] = y[i] + dt * (-drift * x[i] + c0 * hx[i]);
            let xn = x[i] + dt * a0 * y[i];
            if (xn > 1.0) {
                x[i] = 1.0;
                y[i] = 0.0;
            } else if (xn < -1.0) {
                x[i] = -1.0;
                y[i] = 0.0;
            } else {
                x[i] = xn;
            }
        }
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            s[i] = select(-1, 1, x[i] >= 0.0);
        }
        var e: f32 = 0.0;
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            var acc: f32 = 0.0;
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                acc = acc + jm[jbase + i * n + j] * f32(s[j]);
            }
            e = e - 0.5 * f32(s[i]) * acc;
        }
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            e = e - hm[hbase + i] * f32(s[i]);
        }
        if (e < e_best) {
            e_best = e;
            for (var i: u32 = 0u; i < n; i = i + 1u) {
                best[i] = s[i];
            }
        }
    }

    // ---------- polimento: descend(best) ----------
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        work[i] = best[i];
    }
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        var acc: f32 = 0.0;
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            acc = acc + jm[jbase + i * n + j] * f32(work[j]);
        }
        g[i] = acc;
    }
    for (var step: u32 = 0u; step < 8u * n; step = step + 1u) {
        var pick: i32 = -1;
        var pick_delta: f32 = -1e-12;
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            let delta = 2.0 * f32(work[i]) * (g[i] + hm[hbase + i]);
            if (delta < pick_delta) {
                pick_delta = delta;
                pick = i32(i);
            }
        }
        if (pick < 0) {
            break;
        }
        let pi = u32(pick);
        let delta_s = -2.0 * f32(work[pi]);
        work[pi] = -work[pi];
        for (var k: u32 = 0u; k < n; k = k + 1u) {
            g[k] = g[k] + jm[jbase + k * n + pi] * delta_s;
        }
    }
    var e_work: f32 = 0.0;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        var acc: f32 = 0.0;
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            acc = acc + jm[jbase + i * n + j] * f32(work[j]);
        }
        e_work = e_work - 0.5 * f32(work[i]) * acc;
    }
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        e_work = e_work - hm[hbase + i] * f32(work[i]);
    }

    // ---------- fuga determinística (só se o polimento não bateu E(s_0)) ----------
    if (e_work >= e0) {
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            best[i] = work[i];
        }
        var best_e = e_work;
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                s[j] = best[j];
            }
            s[i] = -s[i];
            // descend(s) a partir do flip
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                var acc: f32 = 0.0;
                for (var k: u32 = 0u; k < n; k = k + 1u) {
                    acc = acc + jm[jbase + j * n + k] * f32(s[k]);
                }
                g[j] = acc;
            }
            for (var step: u32 = 0u; step < 8u * n; step = step + 1u) {
                var pick: i32 = -1;
                var pick_delta: f32 = -1e-12;
                for (var j: u32 = 0u; j < n; j = j + 1u) {
                    let delta = 2.0 * f32(s[j]) * (g[j] + hm[hbase + j]);
                    if (delta < pick_delta) {
                        pick_delta = delta;
                        pick = i32(j);
                    }
                }
                if (pick < 0) {
                    break;
                }
                let pj = u32(pick);
                let delta_s = -2.0 * f32(s[pj]);
                s[pj] = -s[pj];
                for (var k: u32 = 0u; k < n; k = k + 1u) {
                    g[k] = g[k] + jm[jbase + k * n + pj] * delta_s;
                }
            }
            var e_try: f32 = 0.0;
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                var acc: f32 = 0.0;
                for (var k: u32 = 0u; k < n; k = k + 1u) {
                    acc = acc + jm[jbase + j * n + k] * f32(s[k]);
                }
                e_try = e_try - 0.5 * f32(s[j]) * acc;
            }
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                e_try = e_try - hm[hbase + j] * f32(s[j]);
            }
            if (e_try < best_e) {
                best_e = e_try;
                for (var j: u32 = 0u; j < n; j = j + 1u) {
                    work[j] = s[j];
                }
            }
        }
    }

    // ---------- saída ----------
    var e_out: f32 = 0.0;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        var acc: f32 = 0.0;
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            acc = acc + jm[jbase + i * n + j] * f32(work[j]);
        }
        e_out = e_out - 0.5 * f32(work[i]) * acc;
    }
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        e_out = e_out - hm[hbase + i] * f32(work[i]);
    }
    oute[inst] = e_out;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        outs[inst * n + i] = work[i];
    }
}
"#;

#[cfg(feature = "gpu")]
mod imp {
    use super::{IsingGpuError, MAX_N, SHADER};
    use crate::core::gpu;

    /// Roda `k` instâncias (`n ≤ MAX_N`) e devolve `(energias, estados)`.
    ///
    /// * `kmat` — `[k, n, n]` acoplamentos (simétricos, diagonal zero).
    /// * `hmat` — `[k, n]` campo externo.
    /// * `init` — `[k, n, 2]` pares `(x_0, y_0)` por sítio.
    /// * `c0` — `[k]` ganho `0.5 / (√n · std(J))` (o mesmo do solver).
    ///
    /// Devolve a energia `H(s)` do estado emergente (pós-polimento e fuga), com a
    /// mesma fórmula do caminho auditado.
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
        let (device, queue, _name) = gpu::device_with(force_off)?;
        use wgpu::util::DeviceExt;

        let mk = |label: &str, data: &[u8], usage: wgpu::BufferUsages| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            })
        };
        let j_buf = mk(
            "ising-j",
            bytemuck::cast_slice(&kmat[..k * n * n]),
            wgpu::BufferUsages::STORAGE,
        );
        let h_buf = mk(
            "ising-h",
            bytemuck::cast_slice(&hmat[..k * n]),
            wgpu::BufferUsages::STORAGE,
        );
        let i_buf = mk(
            "ising-init",
            bytemuck::cast_slice(&init[..k * n * 2]),
            wgpu::BufferUsages::STORAGE,
        );
        let c_buf = mk(
            "ising-c0",
            bytemuck::cast_slice(&c0[..k]),
            wgpu::BufferUsages::STORAGE,
        );
        let e_size = (k * 4) as u64;
        let s_size = (k * n * 4) as u64;
        let e_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ising-e"),
            size: e_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let s_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ising-s"),
            size: s_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let e_stage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ising-e-stage"),
            size: e_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let s_stage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ising-s-stage"),
            size: s_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params = mk(
            "ising-params",
            bytemuck::cast_slice(&[n as u32, steps as u32, k as u32, 0u32]),
            wgpu::BufferUsages::UNIFORM,
        );

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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ising-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: j_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: h_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: i_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: c_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: e_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: s_buf.as_entire_binding(),
                },
            ],
        });

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ising-enc"),
        });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ising-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((k as u32).div_ceil(64), 1, 1);
        }
        enc.copy_buffer_to_buffer(&e_buf, 0, &e_stage, 0, e_size);
        enc.copy_buffer_to_buffer(&s_buf, 0, &s_stage, 0, s_size);
        queue.submit(Some(enc.finish()));

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
        let ebytes = read(&e_stage).ok_or(IsingGpuError::Off(gpu::GpuOff::NoDevice))?;
        let sbytes = read(&s_stage).ok_or(IsingGpuError::Off(gpu::GpuOff::NoDevice))?;
        let ev: &[f32] = bytemuck::cast_slice(&ebytes);
        let sv: &[i32] = bytemuck::cast_slice(&sbytes);
        out_energy[..k].copy_from_slice(&ev[..k]);
        out_states[..k * n].copy_from_slice(&sv[..k * n]);
        Ok(())
    }

    /// Sweep com o backend padrão.
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
        sweep_into_with(
            false, kmat, hmat, init, c0, n, k, steps, out_energy, out_states,
        )
    }
}

#[cfg(feature = "gpu")]
pub use imp::{sweep_into, sweep_into_with};

/// Sem a feature: recusa explícita (o chamador fica na CPU).
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

/// Sem a feature: recusa explícita (variante com `force_off`).
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

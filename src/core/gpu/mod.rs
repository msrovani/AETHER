//! Backend GPU **opt-in** (`--features gpu`) — plano em `docs/GPU_PLAN.md`.
//!
//! Regras do plano, codificadas aqui:
//!
//! * a feature é **opt-in** e fica fora do `default`; sem ela o crate é
//!   CPU-only completo;
//! * **fallback obrigatório**: ausência de adapter (ou `AETHER_GPU=0`) degrada
//!   para CPU com aviso, **nunca** `panic`;
//! * GPU só para *lote* (G0: JL 1362×, FFT 7,4×, sweep Ising 929×); o laço de
//!   60 Hz, o `record` do F1, o M5 e o solver do E2 **não** passam por aqui.
//!
//! O censo que autoriza (ou mata) cada workload é `examples/gpu_census.rs`.
//!
//! # unsafe
//!
//! O único unsafe do módulo é o Waker no-op de [lock_on] (padrão da
//! biblioteca padrão para executor mínimo). Mesmo precedente de
//! core::spsc_ring; nada de ponteiro solto, nada de I/O.
#![allow(unsafe_code)]

pub mod jl;

#[cfg(feature = "gpu")]
use std::future::Future;
#[cfg(feature = "gpu")]
use std::pin::pin;
#[cfg(feature = "gpu")]
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// Motivo de a GPU estar indisponível (telemetria, nunca erro fatal).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuOff {
    /// `AETHER_GPU=0` desligou explicitamente.
    Disabled,
    /// A feature `gpu` não foi compilada.
    NoFeature,
    /// Nenhum adapter (driver ausente, VM, CI sem GPU).
    NoAdapter,
    /// O adapter existe mas o device não pôde ser criado.
    NoDevice,
}

impl GpuOff {
    /// Rótulo estável (uma linha, sem alocação).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "AETHER_GPU=0",
            Self::NoFeature => "feature gpu desligada",
            Self::NoAdapter => "nenhum adapter",
            Self::NoDevice => "device indisponível",
        }
    }
}

/// Executa um futuro até completar (micro-executor: sem dependência externa).
#[cfg(feature = "gpu")]
fn block_on<F: Future>(fut: F) -> F::Output {
    fn noop_raw() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw()
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    let waker = unsafe { Waker::from_raw(noop_raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// Estado do backend para telemetria/diagnóstico.
#[derive(Clone, Debug)]
pub struct GpuStatus {
    /// Há device utilizável?
    pub available: bool,
    /// Nome do adapter (quando há).
    pub adapter: Option<String>,
    /// Motivo, quando indisponível.
    pub off: Option<GpuOff>,
}

impl GpuStatus {
    /// Resumo de uma linha (para log/relatório).
    pub fn summary(&self) -> String {
        match (&self.adapter, self.off) {
            (Some(name), _) => format!("gpu: {name}"),
            (None, Some(off)) => format!("cpu-only: {}", off.label()),
            (None, None) => "cpu-only".to_string(),
        }
    }
}

#[cfg(feature = "gpu")]
mod backend {
    use super::{block_on, GpuOff, GpuStatus};
    use std::sync::Mutex;

    /// (device, queue, nome do adapter) — criados uma vez por processo.
    pub type Gpu = (wgpu::Device, wgpu::Queue, String);

    static CACHE: Mutex<Option<Result<(), GpuOff>>> = Mutex::new(None);

    fn env_disabled() -> bool {
        matches!(std::env::var("AETHER_GPU").as_deref(), Ok("0") | Ok("off"))
    }

    /// Liga o backend (uma vez) e devolve o device compartilhado.
    pub fn device() -> Result<Gpu, GpuOff> {
        device_with(false)
    }

    /// Igual a [device], mas com orce_off (teste determinístico do fallback).
    pub fn device_with(force_off: bool) -> Result<Gpu, GpuOff> {
        if force_off || env_disabled() {
            return Err(GpuOff::Disabled);
        }
        let mut slot = CACHE.lock().map_err(|_| GpuOff::NoDevice)?;
        if *slot == Some(Err(GpuOff::NoAdapter)) {
            return Err(GpuOff::NoAdapter);
        }
        if *slot == Some(Err(GpuOff::NoDevice)) {
            return Err(GpuOff::NoDevice);
        }
        // `wgpu::Instance` + adapter são baratos; o device é criado aqui.
        let instance = wgpu::Instance::default();
        let mut adapters = instance.enumerate_adapters(wgpu::Backends::all());
        // Prefere GPU discreta; depois integrada; nunca software (WARP/llvmpipe).
        adapters.retain(|a| a.get_info().device_type != wgpu::DeviceType::Cpu);
        adapters.sort_by_key(|a| match a.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            _ => 2,
        });
        let Some(adapter) = adapters.into_iter().next() else {
            *slot = Some(Err(GpuOff::NoAdapter));
            return Err(GpuOff::NoAdapter);
        };
        let info = adapter.get_info();
        let device = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("aether-gpu"),
                ..Default::default()
            },
            None,
        ));
        match device {
            Ok((device, queue)) => {
                *slot = Some(Ok(()));
                Ok((device, queue, info.name))
            }
            Err(_) => {
                *slot = Some(Err(GpuOff::NoDevice));
                Err(GpuOff::NoDevice)
            }
        }
    }

    /// `GpuStatus` a partir do device.
    pub fn status() -> GpuStatus {
        status_with(false)
    }

    /// Igual a [status], com orce_off (teste determinístico).
    pub fn status_with(force_off: bool) -> GpuStatus {
        match device_with(force_off) {
            Ok((_, _, name)) => GpuStatus {
                available: true,
                adapter: Some(name),
                off: None,
            },
            Err(off) => GpuStatus {
                available: false,
                adapter: None,
                off: Some(off),
            },
        }
    }
}

/// Consulta o estado do backend (nunca falha, nunca aloca no caminho quente).
#[cfg(feature = "gpu")]
pub fn status() -> GpuStatus {
    backend::status()
}

/// Estado com orce_off (teste determinístico do fallback, sem mexer em env).
#[cfg(feature = "gpu")]
pub fn status_with(force_off: bool) -> GpuStatus {
    backend::status_with(force_off)
}

/// Sem a feature: sempre CPU-only, com motivo explícito.
#[cfg(not(feature = "gpu"))]
pub fn status() -> GpuStatus {
    status_with(false)
}

/// Sem a feature: orce_off só troca o rótulo do motivo.
#[cfg(not(feature = "gpu"))]
pub fn status_with(force_off: bool) -> GpuStatus {
    GpuStatus {
        available: false,
        adapter: None,
        off: Some(
            if force_off || matches!(std::env::var("AETHER_GPU").as_deref(), Ok("0") | Ok("off")) {
                GpuOff::Disabled
            } else {
                GpuOff::NoFeature
            },
        ),
    }
}

/// Device compartilhado (só com a feature).
#[cfg(feature = "gpu")]
pub fn device() -> Result<backend::Gpu, GpuOff> {
    backend::device()
}

/// Device com orce_off (teste determinístico do fallback).
#[cfg(feature = "gpu")]
pub fn device_with(force_off: bool) -> Result<backend::Gpu, GpuOff> {
    backend::device_with(force_off)
}

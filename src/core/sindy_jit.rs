//! JIT Cranelift para as equações SINDy descobertas.
//!
//! Quando o [`SindyEquationDiscoverer`] sintetiza uma matriz esparsa `Ξ`
//! nova, este módulo compila **apenas os termos ativos** (`Ξ_ct ≠ 0`) em
//! código de máquina nativo
//!
//! ```text
//! extern "C" fn(x: *const f32, dx: *mut f32)   // dx = Θ(x)·Ξ,  x,dx ∈ R²
//! ```
//!
//! e publica o ponteiro com um **único `store` atômico** que o integrador
//! ([`SindyIntegrator`]) lê a cada avaliação — o laço cognitivo nunca é
//! interrompido, bloqueado ou serializado com o compilador.
//!
//! ## Arquitetura
//!
//! ```text
//!  thread Koopman-SINDy (núcleo secundário)        thread reflexiva (RT)
//!  ┌──────────────────────────────┐                ┌────────────────────┐
//!  │ discoverer.fit_window()      │                │ SindyIntegrator    │
//!  │ SindyJit::publish(&disc)     │  AtomicUsize   │  eval / rk4_step   │
//!  │   ├ seqlock(Ξ f32)  ─────────┼───────────────►│  kernel? JIT : interp
//!  │   └ Cranelift → ptr ─────────┼── store(Rel) ─►│  calls += 1 (Rel)  │
//!  └──────────────────────────────┘                └────────────────────┘
//! ```
//!
//! * **Sem JIT** (feature `jit` desligada, ou Cranelift falhou): o
//!   integrador avalia os mesmos coeficientes por um interpretador nativo
//!   (leitura *seqlock*, sem alocação) — resultado numericamente equivalente,
//!   apenas mais lento; [`JitStats::last_error`] registra o motivo.
//! * **Reclamação de código**: versões aposentadas só são liberadas quando o
//!   contador `calls` (incrementado *depois* de cada avaliação) avança além do
//!   valor registrado na troca ⇒ nenhuma avaliação em voo usa o código
//!   liberado. **Contrato**: um único avaliador por `SindyJit`
//!   ([`SindyJit::new`] devolve exatamente um [`SindyIntegrator`], não
//!   `Clone`).
//! * Ao descartar o [`SindyJit`], o ponteiro é zerado (o integrador cai no
//!   interpretador) e o código restante é **vazado** de propósito (um
//!   avaliador pode estar em voo); custo: uma arena pequena por instância.
//!
//! ## Limites
//!
//! * Precisão `f32` nos coeficientes/termos (o descobridor ajusta em `f64`).
//! * Só o *RHS* de 2 variáveis (`SINDY_TERMS = 10`, grau ≤ 3).
//! * Verificado neste ambiente: `x86_64-pc-windows-gnu`, Cranelift 0.113.

#![allow(unsafe_code)] // transmute usize→fn e free_memory; invariantes acima.

use std::sync::atomic::{fence, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::core::koopman_sindy::{SindyEquationDiscoverer, SINDY_TERMS};
use crate::error::{AetherError, Result};

/// Assinatura do kernel gerado: `dx = f(x)`, com `x`/`dx` de 2 `f32`.
pub type SindyKernelFn = extern "C" fn(*const f32, *mut f32);

/// Expoentes `(a, b)` de `x1^a · x2^b` por termo da biblioteca.
pub const TERM_EXPONENTS: [(u8, u8); SINDY_TERMS] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (2, 0),
    (1, 1),
    (0, 2),
    (3, 0),
    (2, 1),
    (1, 2),
    (0, 3),
];

/// Número de coeficientes (`2 × 10`).
const NCOEF: usize = 2 * SINDY_TERMS;

/// `true` se o crate foi compilado com o backend Cranelift.
pub const fn jit_compiled_in() -> bool {
    cfg!(feature = "jit")
}

/// Estado compartilhado entre publicador e avaliador (sem `Mutex`).
struct Shared {
    /// Endereço do kernel nativo (`0` = usar interpretador).
    kernel: AtomicUsize,
    /// Versão publicada (`0` = sem equações).
    generation: AtomicU64,
    /// Avaliações concluídas (marcador de quiescência).
    calls: AtomicU64,
    /// *Seqlock* dos coeficientes `f32` (par = estável).
    seq: AtomicU32,
    coeffs: [AtomicU32; NCOEF],
}

impl Shared {
    fn new() -> Self {
        Self {
            kernel: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            seq: AtomicU32::new(0),
            coeffs: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    /// Publica `Ξ` (apenas o publicador chama; escritor único).
    fn store_coeffs(&self, xi: &[f32; NCOEF]) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        for (slot, v) in self.coeffs.iter().zip(xi) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// Lê `Ξ` de forma consistente (retenta se o escritor estiver ativo).
    #[inline]
    fn load_coeffs(&self, out: &mut [f32; NCOEF]) {
        loop {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            for (o, slot) in out.iter_mut().zip(&self.coeffs) {
                *o = f32::from_bits(slot.load(Ordering::Relaxed));
            }
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == s1 {
                return;
            }
        }
    }
}

/// Avalia `dx = Θ(x)·Ξ` por interpretação (referência e *fallback*).
#[inline]
pub fn eval_interpreted(xi: &[f32; NCOEF], x: &[f32; 2], dx: &mut [f32; 2]) {
    let (x1, x2) = (x[0], x[1]);
    let p1 = [1.0, x1, x1 * x1, x1 * x1 * x1];
    let p2 = [1.0, x2, x2 * x2, x2 * x2 * x2];
    for (c, d) in dx.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        for (t, &(a, b)) in TERM_EXPONENTS.iter().enumerate() {
            let w = xi[c * SINDY_TERMS + t];
            if w != 0.0 {
                acc += w * p1[a as usize] * p2[b as usize];
            }
        }
        *d = acc;
    }
}

/// Lado do avaliador (laço cognitivo): lê o ponteiro atômico a cada chamada.
#[derive(Debug)]
pub struct SindyIntegrator {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("kernel", &self.kernel.load(Ordering::Relaxed))
            .field("generation", &self.generation.load(Ordering::Relaxed))
            .finish()
    }
}

impl SindyIntegrator {
    /// `dx = f(x)` — JIT se publicado, senão interpretador (zero alocação).
    #[inline]
    pub fn eval(&self, x: &[f32; 2], dx: &mut [f32; 2]) {
        let k = self.shared.kernel.load(Ordering::Acquire);
        if k != 0 {
            // SAFETY: `k` foi produzido por `SindyJit` a partir de
            // `get_finalized_function` de um módulo vivo; o código só é
            // liberado após `calls` avançar (ver `SindyJit::reclaim`).
            let f = unsafe { std::mem::transmute::<usize, SindyKernelFn>(k) };
            f(x.as_ptr(), dx.as_mut_ptr());
        } else {
            let mut xi = [0.0f32; NCOEF];
            self.shared.load_coeffs(&mut xi);
            eval_interpreted(&xi, x, dx);
        }
        self.shared.calls.fetch_add(1, Ordering::Release);
    }

    /// Um passo de Runge–Kutta 4 `x ← x + dt·(k1+2k2+2k3+k4)/6`.
    #[inline]
    pub fn rk4_step(&self, x: &mut [f32; 2], dt: f32) {
        let mut k1 = [0.0f32; 2];
        let mut k2 = [0.0f32; 2];
        let mut k3 = [0.0f32; 2];
        let mut k4 = [0.0f32; 2];
        self.eval(x, &mut k1);
        let t = [x[0] + 0.5 * dt * k1[0], x[1] + 0.5 * dt * k1[1]];
        self.eval(&t, &mut k2);
        let t = [x[0] + 0.5 * dt * k2[0], x[1] + 0.5 * dt * k2[1]];
        self.eval(&t, &mut k3);
        let t = [x[0] + dt * k3[0], x[1] + dt * k3[1]];
        self.eval(&t, &mut k4);
        for i in 0..2 {
            x[i] += dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
        }
    }

    /// Versão publicada (`0` = ainda sem equações).
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    /// `true` se há equações publicadas.
    pub fn has_equations(&self) -> bool {
        self.generation() > 0
    }

    /// `true` se a versão corrente roda como código nativo gerado.
    pub fn is_jit_active(&self) -> bool {
        self.shared.kernel.load(Ordering::Acquire) != 0
    }
}

/// Estatísticas do publicador.
#[derive(Clone, Debug, Default)]
pub struct JitStats {
    /// Versões publicadas (JIT ou interpretadas).
    pub generations: u64,
    /// Compilações Cranelift bem-sucedidas.
    pub compiles: u64,
    /// Falhas de compilação (caiu no interpretador).
    pub failures: u64,
    /// Publicações ignoradas por `Ξ` inalterado.
    pub skipped_unchanged: u64,
    /// Módulos aposentados já liberados.
    pub retired_freed: u64,
    /// Latência da última compilação (µs).
    pub last_compile_us: u64,
    /// Termos ativos na última versão.
    pub active_terms: usize,
    /// Último erro do backend, se houve.
    pub last_error: Option<String>,
}

/// Resultado de uma publicação.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishOutcome {
    /// `Ξ` equivalente à versão corrente: nada feito.
    Unchanged,
    /// Nova versão compilada para código nativo.
    Jitted {
        /// Versão publicada.
        generation: u64,
        /// Termos ativos compilados.
        active_terms: usize,
    },
    /// Nova versão publicada no interpretador (sem JIT).
    Interpreted {
        /// Versão publicada.
        generation: u64,
    },
}

#[cfg(feature = "jit")]
mod backend {
    //! Backend Cranelift (somente com a feature `jit`).
    use super::*;
    use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, UserFuncName, Value};
    use cranelift_codegen::settings::{self, Configurable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{default_libcall_names, Linkage, Module};

    /// Módulo JIT vivo + endereço do kernel.
    pub(super) struct JitCode {
        module: JITModule,
        ptr: usize,
    }

    impl JitCode {
        pub(super) fn addr(&self) -> usize {
            self.ptr
        }

        /// Libera a memória executável.
        ///
        /// # Safety
        /// Nenhuma thread pode estar executando (nem chamar depois) o kernel.
        pub(super) unsafe fn free(self) {
            // SAFETY: contrato repassado ao chamador.
            unsafe { self.module.free_memory() }
        }
    }

    fn cfg_err<E: std::fmt::Display>(what: &str, e: E) -> AetherError {
        AetherError::Config(format!("SINDy-JIT: {what}: {e}"))
    }

    /// `x^n` com cache (`n ∈ 1..=3`).
    fn power(b: &mut FunctionBuilder<'_>, base: Value, cache: &mut [Option<Value>; 4], n: u8) -> Value {
        cache[1] = Some(base);
        for k in 2..=n as usize {
            if cache[k].is_none() {
                let prev = cache[k - 1].expect("potencia anterior");
                cache[k] = Some(b.ins().fmul(prev, base));
            }
        }
        cache[n as usize].expect("potencia")
    }

    /// Compila os termos ativos de `Ξ` em `extern "C" fn(*const f32, *mut f32)`.
    pub(super) fn compile(xi: &[f32; NCOEF]) -> Result<JitCode> {
        let mut flags = settings::builder();
        flags
            .set("opt_level", "speed")
            .map_err(|e| cfg_err("opt_level", e))?;
        flags
            .set("is_pic", "false")
            .map_err(|e| cfg_err("is_pic", e))?;
        let isa = cranelift_native::builder()
            .map_err(|e| cfg_err("isa nativa", e))?
            .finish(settings::Flags::new(flags))
            .map_err(|e| cfg_err("isa finish", e))?;
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_ty = module.target_config().pointer_type();
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty));
        sig.params.push(AbiParam::new(ptr_ty));
        let id = module
            .declare_function("aether_sindy_rhs", Linkage::Export, &sig)
            .map_err(|e| cfg_err("declare", e))?;

        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        ctx.func.name = UserFuncName::user(0, id.as_u32());
        let mut fctx = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
            let block = b.create_block();
            b.append_block_params_for_function_params(block);
            b.switch_to_block(block);
            b.seal_block(block);
            let (xin, xout) = (b.block_params(block)[0], b.block_params(block)[1]);
            let mf = MemFlags::trusted();
            let x1 = b.ins().load(types::F32, mf, xin, 0);
            let x2 = b.ins().load(types::F32, mf, xin, 4);
            let mut p1: [Option<Value>; 4] = [None; 4];
            let mut p2: [Option<Value>; 4] = [None; 4];
            for c in 0..2usize {
                let mut acc: Option<Value> = None;
                for (t, &(ea, eb)) in TERM_EXPONENTS.iter().enumerate() {
                    let w = xi[c * SINDY_TERMS + t];
                    if w == 0.0 {
                        continue;
                    }
                    let wv = b.ins().f32const(w);
                    let term = match (ea, eb) {
                        (0, 0) => wv,
                        (a, 0) => {
                            let m = power(&mut b, x1, &mut p1, a);
                            b.ins().fmul(m, wv)
                        }
                        (0, e) => {
                            let m = power(&mut b, x2, &mut p2, e);
                            b.ins().fmul(m, wv)
                        }
                        (a, e) => {
                            let ma = power(&mut b, x1, &mut p1, a);
                            let me = power(&mut b, x2, &mut p2, e);
                            let m = b.ins().fmul(ma, me);
                            b.ins().fmul(m, wv)
                        }
                    };
                    acc = Some(match acc {
                        Some(s) => b.ins().fadd(s, term),
                        None => term,
                    });
                }
                let out = acc.unwrap_or_else(|| b.ins().f32const(0.0));
                b.ins().store(mf, out, xout, (c * 4) as i32);
            }
            b.ins().return_(&[]);
            b.finalize();
        }
        module
            .define_function(id, &mut ctx)
            .map_err(|e| cfg_err("define", e))?;
        module.clear_context(&mut ctx);
        module
            .finalize_definitions()
            .map_err(|e| cfg_err("finalize", e))?;
        let ptr = module.get_finalized_function(id) as usize;
        Ok(JitCode { module, ptr })
    }
}

#[cfg(not(feature = "jit"))]
mod backend {
    //! Stub seguro: sem Cranelift, toda publicação cai no interpretador.
    use super::*;

    pub(super) struct JitCode;

    impl JitCode {
        pub(super) fn addr(&self) -> usize {
            0
        }

        /// # Safety
        /// Sem efeito (stub).
        pub(super) unsafe fn free(self) {}
    }

    pub(super) fn compile(_xi: &[f32; NCOEF]) -> Result<JitCode> {
        Err(AetherError::Config(
            "SINDy-JIT: crate compilado sem a feature `jit`".into(),
        ))
    }
}

use backend::JitCode;

/// Lado do publicador (thread Koopman-SINDy).
pub struct SindyJit {
    shared: Arc<Shared>,
    current: Option<JitCode>,
    /// Módulos aposentados + valor de `calls` no momento da troca.
    retired: Vec<(JitCode, u64)>,
    last_xi: [f32; NCOEF],
    rel_tol: f32,
    stats: JitStats,
}

impl std::fmt::Debug for SindyJit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SindyJit")
            .field("shared", &self.shared)
            .field("retired", &self.retired.len())
            .field("stats", &self.stats)
            .finish()
    }
}

impl SindyJit {
    /// Cria o publicador e o **único** avaliador associado.
    pub fn new() -> (Self, SindyIntegrator) {
        let shared = Arc::new(Shared::new());
        (
            Self {
                shared: Arc::clone(&shared),
                current: None,
                retired: Vec::new(),
                last_xi: [0.0; NCOEF],
                rel_tol: 1e-4,
                stats: JitStats::default(),
            },
            SindyIntegrator { shared },
        )
    }

    /// Tolerância relativa abaixo da qual `Ξ` é considerada inalterada.
    pub fn set_rel_tolerance(&mut self, rel_tol: f32) {
        if rel_tol.is_finite() && rel_tol >= 0.0 {
            self.rel_tol = rel_tol;
        }
    }

    /// Estatísticas acumuladas.
    pub fn stats(&self) -> &JitStats {
        &self.stats
    }

    /// Publica a `Ξ` corrente do descobridor.
    pub fn publish(&mut self, d: &SindyEquationDiscoverer) -> Result<PublishOutcome> {
        self.publish_coefficients(d.coefficients())
    }

    /// Publica `Ξ` explícita (`[ẋ1; ẋ2] × 10 termos`).
    pub fn publish_coefficients(
        &mut self,
        xi: &[[f64; SINDY_TERMS]; 2],
    ) -> Result<PublishOutcome> {
        let mut flat = [0.0f32; NCOEF];
        for (c, row) in xi.iter().enumerate() {
            for (t, &v) in row.iter().enumerate() {
                if !v.is_finite() || v.abs() > f32::MAX as f64 {
                    return Err(AetherError::Degenerate(
                        "SINDy-JIT: coeficiente nao finito".into(),
                    ));
                }
                flat[c * SINDY_TERMS + t] = v as f32;
            }
        }
        self.reclaim();
        let generation = self.shared.generation.load(Ordering::Relaxed);
        if generation > 0 && self.is_equivalent(&flat) {
            self.stats.skipped_unchanged += 1;
            return Ok(PublishOutcome::Unchanged);
        }
        let active = flat.iter().filter(|v| **v != 0.0).count();

        // 1. Coeficientes (para o interpretador e para auditoria).
        self.shared.store_coeffs(&flat);
        self.last_xi = flat;

        // 2. Compila e troca o ponteiro de forma atômica.
        let t0 = Instant::now();
        let compiled = backend::compile(&flat);
        let new_gen = generation + 1;
        self.stats.generations += 1;
        self.stats.active_terms = active;
        match compiled {
            Ok(code) => {
                self.stats.compiles += 1;
                self.stats.last_compile_us = t0.elapsed().as_micros() as u64;
                self.shared.kernel.store(code.addr(), Ordering::Release);
                self.shared.generation.store(new_gen, Ordering::Release);
                if let Some(old) = self.current.replace(code) {
                    let at = self.shared.calls.load(Ordering::Acquire);
                    self.retired.push((old, at));
                }
                Ok(PublishOutcome::Jitted {
                    generation: new_gen,
                    active_terms: active,
                })
            }
            Err(e) => {
                self.stats.failures += 1;
                self.stats.last_error = Some(e.to_string());
                // Mantém coerência: sem kernel novo, o interpretador serve Ξ nova.
                self.shared.kernel.store(0, Ordering::Release);
                self.shared.generation.store(new_gen, Ordering::Release);
                if let Some(old) = self.current.take() {
                    let at = self.shared.calls.load(Ordering::Acquire);
                    self.retired.push((old, at));
                }
                Ok(PublishOutcome::Interpreted { generation: new_gen })
            }
        }
    }

    /// `Ξ` nova é equivalente à última publicada (mesmo suporte, Δ pequeno)?
    fn is_equivalent(&self, new: &[f32; NCOEF]) -> bool {
        let mut scale = 0.0f32;
        for (a, b) in self.last_xi.iter().zip(new) {
            if (*a == 0.0) != (*b == 0.0) {
                return false;
            }
            scale = scale.max(a.abs()).max(b.abs());
        }
        let tol = self.rel_tol * scale.max(f32::MIN_POSITIVE);
        self.last_xi
            .iter()
            .zip(new)
            .all(|(a, b)| (a - b).abs() <= tol)
    }

    /// Libera módulos aposentados sem avaliação em voo (ver docs do módulo).
    pub fn reclaim(&mut self) {
        let now = self.shared.calls.load(Ordering::Acquire);
        let mut i = 0;
        while i < self.retired.len() {
            if now > self.retired[i].1 {
                let (code, _) = self.retired.swap_remove(i);
                // SAFETY: `calls` avançou desde a troca ⇒ a avaliação que
                // poderia estar usando este kernel já terminou, e o ponteiro
                // publicado agora é outro.
                unsafe { code.free() };
                self.stats.retired_freed += 1;
            } else {
                i += 1;
            }
        }
    }
}

impl Drop for SindyJit {
    fn drop(&mut self) {
        // O avaliador pode estar em voo: cai no interpretador e o código
        // restante é vazado de propósito (ver docs do módulo).
        self.shared.kernel.store(0, Ordering::Release);
        for (code, _) in self.retired.drain(..) {
            std::mem::forget(code);
        }
        if let Some(code) = self.current.take() {
            std::mem::forget(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xi_exemplo() -> [[f64; SINDY_TERMS]; 2] {
        let mut xi = [[0.0f64; SINDY_TERMS]; 2];
        // ẋ1 = x2 ; ẋ2 = -x1 + 0.5 x2 - 0.5 x1² x2 (Van der Pol-like)
        xi[0][2] = 1.0;
        xi[1][1] = -1.0;
        xi[1][2] = 0.5;
        xi[1][7] = -0.5;
        xi
    }

    #[test]
    fn jit_e_interpretador_coincidem() {
        let (mut jit, integ) = SindyJit::new();
        let out = jit.publish_coefficients(&xi_exemplo()).expect("publish");
        assert!(matches!(
            out,
            PublishOutcome::Jitted { .. } | PublishOutcome::Interpreted { .. }
        ));
        assert_eq!(integ.is_jit_active(), jit_compiled_in());
        let mut flat = [0.0f32; NCOEF];
        for c in 0..2 {
            for t in 0..SINDY_TERMS {
                flat[c * SINDY_TERMS + t] = xi_exemplo()[c][t] as f32;
            }
        }
        for k in 0..50 {
            let x = [k as f32 * 0.07 - 1.7, 0.9 - k as f32 * 0.05];
            let mut a = [0.0f32; 2];
            let mut b = [0.0f32; 2];
            integ.eval(&x, &mut a);
            eval_interpreted(&flat, &x, &mut b);
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5);
        }
    }

    #[test]
    fn publicacao_identica_e_ignorada() {
        let (mut jit, _integ) = SindyJit::new();
        jit.publish_coefficients(&xi_exemplo()).expect("p1");
        let o = jit.publish_coefficients(&xi_exemplo()).expect("p2");
        assert_eq!(o, PublishOutcome::Unchanged);
        assert_eq!(jit.stats().skipped_unchanged, 1);
    }
}

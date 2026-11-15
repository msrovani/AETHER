//! Fila circular SPSC *lock-free* (um produtor, um consumidor).
//!
//! Liga a thread reflexiva (núcleo isolado) às threads secundárias
//! (Koopman-SINDy, SwarmSpeciation) sem `Mutex`, sem alocação por mensagem e
//! sem *syscall*: `try_push`/`try_pop` são `O(1)` com um `load` *acquire* e um
//! `store` *release* por operação.
//!
//! * Capacidade `N` potência de dois (mascaramento em vez de módulo).
//! * Índices `head`/`tail` em linhas de cache separadas (`align(64)`) para
//!   evitar *false sharing*.
//! * `T: Copy + Send`: mensagens são POD; nada a destruir no descarte.
//! * O tipo expõe **apenas** [`SpscProducer`] e [`SpscConsumer`] (cada um
//!   `Send`, nenhum `Clone`): a restrição SPSC é imposta pelo sistema de tipos.

#![allow(unsafe_code)] // UnsafeCell/MaybeUninit do buffer; invariantes abaixo.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Índice isolado em sua própria linha de cache.
#[repr(align(64))]
#[derive(Debug, Default)]
struct CachePadded<T>(T);

/// Estado compartilhado entre produtor e consumidor.
struct Inner<T, const N: usize> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Próxima posição de leitura (escrita só pelo consumidor).
    head: CachePadded<AtomicUsize>,
    /// Próxima posição de escrita (escrita só pelo produtor).
    tail: CachePadded<AtomicUsize>,
}

// SAFETY: cada slot é escrito por exatamente uma thread (produtor) antes da
// publicação via `tail.store(Release)` e lido por exatamente uma (consumidor)
// após `tail.load(Acquire)`; `T: Send` garante a transferência entre threads.
unsafe impl<T: Send, const N: usize> Sync for Inner<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Inner<T, N> {}

impl<T, const N: usize> std::fmt::Debug for Inner<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpscInner")
            .field("capacity", &N)
            .field("head", &self.head.0.load(Ordering::Relaxed))
            .field("tail", &self.tail.0.load(Ordering::Relaxed))
            .finish()
    }
}

/// Construtor da fila SPSC de capacidade `N` (potência de dois).
#[derive(Debug)]
pub struct SpscRingBuffer<T: Copy + Send, const N: usize> {
    inner: Arc<Inner<T, N>>,
}

impl<T: Copy + Send, const N: usize> SpscRingBuffer<T, N> {
    const VALID: () = assert!(
        N >= 2 && N.is_power_of_two(),
        "capacidade deve ser potencia de dois >= 2"
    );

    /// Aloca o buffer (única alocação da fila).
    pub fn new() -> Self {
        let () = Self::VALID;
        let buf: Vec<UnsafeCell<MaybeUninit<T>>> = (0..N)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();
        Self {
            inner: Arc::new(Inner {
                buf: buf.into_boxed_slice(),
                head: CachePadded(AtomicUsize::new(0)),
                tail: CachePadded(AtomicUsize::new(0)),
            }),
        }
    }

    /// Separa nos dois extremos (produtor, consumidor).
    pub fn split(self) -> (SpscProducer<T, N>, SpscConsumer<T, N>) {
        (
            SpscProducer {
                inner: Arc::clone(&self.inner),
            },
            SpscConsumer { inner: self.inner },
        )
    }
}

impl<T: Copy + Send, const N: usize> Default for SpscRingBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Extremo produtor (único).
#[derive(Debug)]
pub struct SpscProducer<T: Copy + Send, const N: usize> {
    inner: Arc<Inner<T, N>>,
}

/// Extremo consumidor (único).
#[derive(Debug)]
pub struct SpscConsumer<T: Copy + Send, const N: usize> {
    inner: Arc<Inner<T, N>>,
}

impl<T: Copy + Send, const N: usize> SpscProducer<T, N> {
    /// Enfileira `value`; devolve `Err(value)` se a fila estiver cheia.
    #[inline]
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        let inner = &*self.inner;
        let tail = inner.tail.0.load(Ordering::Relaxed);
        let head = inner.head.0.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= N {
            return Err(value);
        }
        // SAFETY: slot `tail & (N-1)` está livre (`tail - head < N`) e só o
        // produtor escreve nele até a publicação abaixo.
        unsafe {
            (*inner.buf[tail & (N - 1)].get()).write(value);
        }
        inner.tail.0.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Elementos atualmente na fila (estimativa).
    pub fn len(&self) -> usize {
        let t = self.inner.tail.0.load(Ordering::Relaxed);
        let h = self.inner.head.0.load(Ordering::Acquire);
        t.wrapping_sub(h)
    }

    /// `true` se vazia (estimativa).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Capacidade `N`.
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<T: Copy + Send, const N: usize> SpscConsumer<T, N> {
    /// Desenfileira o próximo elemento, se houver.
    #[inline]
    pub fn try_pop(&mut self) -> Option<T> {
        let inner = &*self.inner;
        let head = inner.head.0.load(Ordering::Relaxed);
        let tail = inner.tail.0.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        // SAFETY: `head != tail` ⇒ o slot foi publicado pelo produtor
        // (Release/Acquire) e só o consumidor o lê antes de liberar abaixo.
        let v = unsafe { (*inner.buf[head & (N - 1)].get()).assume_init() };
        inner.head.0.store(head.wrapping_add(1), Ordering::Release);
        Some(v)
    }

    /// Elementos atualmente na fila (estimativa).
    pub fn len(&self) -> usize {
        let t = self.inner.tail.0.load(Ordering::Acquire);
        let h = self.inner.head.0.load(Ordering::Relaxed);
        t.wrapping_sub(h)
    }

    /// `true` se vazia (estimativa).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_e_cheia() {
        let (mut p, mut c) = SpscRingBuffer::<u32, 4>::new().split();
        for i in 0..4 {
            assert!(p.try_push(i).is_ok());
        }
        assert_eq!(p.try_push(99), Err(99));
        for i in 0..4 {
            assert_eq!(c.try_pop(), Some(i));
        }
        assert_eq!(c.try_pop(), None);
    }

    #[test]
    fn transferencia_entre_threads_preserva_ordem() {
        let (mut p, mut c) = SpscRingBuffer::<u64, 64>::new().split();
        let n = 200_000u64;
        let h = std::thread::spawn(move || {
            let mut i = 0u64;
            while i < n {
                if p.try_push(i).is_ok() {
                    i += 1;
                } else {
                    std::hint::spin_loop();
                }
            }
        });
        let mut expect = 0u64;
        while expect < n {
            if let Some(v) = c.try_pop() {
                assert_eq!(v, expect);
                expect += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        h.join().expect("produtor");
    }
}

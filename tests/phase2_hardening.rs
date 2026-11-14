//! probe temporário (M6)

#![cfg(feature = "m6")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use aether_engine::thermodynamic_dreamer::{DreamerConfig, ThermodynamicDreamer};

struct Counting;

static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.store(true, Ordering::SeqCst);
    ALLOCS.store(0, Ordering::SeqCst);
    let out = f();
    ARMED.store(false, Ordering::SeqCst);
    (out, ALLOCS.load(Ordering::SeqCst))
}

#[test]
fn probe_before() {
    const N: usize = 32;
    let mut j = vec![0.0f32; N * N];
    for i in 0..N {
        for k in 0..N {
            if i != k {
                j[i * N + k] = ((i * 31 + k * 17) % 7) as f32 * 0.03 - 0.09;
            }
        }
    }
    let h = vec![0.02f32; N];
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    // warmup
    let _ = dreamer.imagine_futures(&j, &h, 0.5, 32, 40, 0.05);
    let (_, allocs32) = count_allocs(|| dreamer.imagine_futures(&j, &h, 0.5, 32, 40, 0.05));
    let (_, allocs4) = count_allocs(|| dreamer.imagine_futures(&j, &h, 0.5, 4, 40, 0.05));
    println!("ALLOCS_BEFORE samples=32 -> {allocs32}; samples=4 -> {allocs4}");
}

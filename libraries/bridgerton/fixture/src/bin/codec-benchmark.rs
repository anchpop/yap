use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn main() {
    let counter = bridge_fixture::Counter::new();
    let cards = vec![counter.sample_card(); 1_000];
    let size = bridgerton::value::encode(&cards).unwrap().len();
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..100 {
        let bytes = bridgerton::value::encode(&cards).unwrap();
        let decoded: Vec<bridge_fixture::Card> = bridgerton::value::decode(&bytes).unwrap();
        assert_eq!(decoded, cards);
        std::hint::black_box(decoded);
    }
    let micros = start.elapsed().as_micros() as f64 / 100.0;
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) / 100;
    let allocated = BYTES.load(Ordering::Relaxed) / 100;
    println!(
        "{{\"cards\":1000,\"encoded_bytes\":{size},\"roundtrip_us\":{micros},\"allocations\":{allocations},\"allocated_bytes\":{allocated}}}"
    );
}

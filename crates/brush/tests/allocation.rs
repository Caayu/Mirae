//! No allocation per input sample or per dab once a stroke's buffers and
//! tiles exist (invariants F1, F2). Measured with a counting allocator that
//! only counts on the test's own thread, so the harness and rayon workers
//! do not interfere.
//!
//! Two known, bounded exceptions are documented in docs/performance.md and
//! deliberately not exercised per sample here: rayon's job injector
//! allocates one queue block per ~63 parallel batches (amortized, inside
//! rayon), and `StrokeBuffer::take_dirty` hands its set away, so the next
//! frame's dirty marks allocate fresh `BTreeSet` nodes (per frame).
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use glam::Vec2;
use tela_brush::dynamics::Curve;
use tela_brush::{BrushInput, BrushPreset, StampContext, StrokeBuffer, StrokeProcessor};
use tela_color::Color;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

struct Counting;

fn record() {
    // `try_with`: thread-local storage may already be torn down.
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        }
    });
}

// SAFETY: forwards every call to the system allocator unchanged; the
// bookkeeping uses const-initialized thread-locals, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: same contract as our caller's; forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations_during(f: impl FnOnce()) -> u64 {
    ALLOCATIONS.with(|n| n.set(0));
    COUNTING.with(|on| on.set(true));
    f();
    COUNTING.with(|on| on.set(false));
    ALLOCATIONS.with(|n| n.get())
}

/// Back-and-forth stroke over the same region: after the first pass every
/// tile exists and every buffer has grown, so later passes must not
/// allocate.
fn scribble(size: f32, parallel: bool) -> u64 {
    let preset = BrushPreset::plain(size);
    let ctx = StampContext {
        tip: preset.tip.prepare(),
        parallel,
        ..StampContext::default()
    };
    let mut buffer = StrokeBuffer::new(Color::BLACK, ctx);
    let mut proc = StrokeProcessor::new();
    let sample = |i: usize| {
        let phase = (i % 200) as f32 / 200.0;
        let x = 300.0 + 400.0 * (phase * std::f32::consts::TAU).sin();
        BrushInput::from_position(Vec2::new(x, 300.0), i as f64 / 200.0).with_pressure(0.8)
    };
    proc.begin(&preset, &Curve::linear(), 1.0, sample(0), &mut buffer);
    for i in 1..1200 {
        proc.push(sample(i), &mut buffer);
    }
    let n = allocations_during(|| {
        for i in 1200..4000 {
            proc.push(sample(i), &mut buffer);
        }
    });
    proc.finish(&mut buffer);
    n
}

#[test]
fn processing_and_stamping_do_not_allocate_per_sample() {
    // Sequential stamping (every batch below the parallel threshold, and
    // the path the stamp benchmark measures): exactly zero.
    assert_eq!(scribble(12.0, false), 0, "small brush, sequential");
    assert_eq!(scribble(400.0, false), 0, "large brush, sequential");
    // Parallel stamping: only rayon's amortized injector blocks, far below
    // one allocation per 100 samples (2800 samples here).
    let parallel = scribble(400.0, true);
    assert!(
        parallel <= 28,
        "large brush, parallel: {parallel} allocations"
    );
}

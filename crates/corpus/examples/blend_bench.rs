//! Composition cost per blend mode (phase 4C).
//!
//! A 2048² document whose layers all have content in every tile (opaque
//! and translucent pixels mixed). The bottom layer is Normal; every other
//! layer uses the mode being measured. Reports milliseconds per composed
//! 256×256 tile for stacks of 2 and 10 layers, the ratio to Normal, and
//! allocations during composition (must be 0).
//!
//! `cargo run --release -p tela-corpus --example blend_bench`
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE};
use tela_document::{BlendMode, ComposeScratch, Document, Placement};

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const SIZE: u32 = 2048;
const REPS: usize = 5;

fn document(layers: usize, mode: BlendMode) -> Document {
    let mut d = Document::new(SIZE, SIZE);
    let mut all = DirtyTiles::new();
    all.mark_rect(IRect::from_size(SIZE, SIZE));
    let mut id = d.active_layer();
    for n in 0..layers {
        if n > 0 {
            id = d.add_layer(Placement::Top).unwrap();
            d.set_blend_mode(id, mode).unwrap();
            d.set_opacity(id, 0.8).unwrap();
        }
        let seed = n as u32 * 37;
        d.modify_tiles(id, &all, |_, t| {
            for y in 0..TILE_SIZE {
                for x in 0..TILE_SIZE {
                    let v = (x * 7 + y * 13 + seed) % 256;
                    // Mostly opaque, some translucent pixels.
                    let a = if (x + y + seed) % 5 == 0 { 128 } else { 255 };
                    let p = |c: u32| (c * a / 255) as u8;
                    *t.get_mut(x, y) = [p(v), p(255 - v), p((v * 3) % 256), a as u8];
                }
            }
        })
        .unwrap();
    }
    d
}

fn ms_per_tile(d: &Document) -> (f64, u64) {
    let tiles: Vec<_> = IRect::from_size(SIZE, SIZE).tiles().collect();
    let mut scratch = ComposeScratch::new();
    let before = ALLOCS.load(Ordering::Relaxed);
    let t = Instant::now();
    for _ in 0..REPS {
        for &c in &tiles {
            d.compose_tile_into(c, None, &mut scratch);
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1e3 / (REPS * tiles.len()) as f64;
    (ms, ALLOCS.load(Ordering::Relaxed) - before)
}

fn main() {
    println!("| mode | 2 layers ms/tile | × Normal | 10 layers ms/tile | × Normal | allocations |");
    println!("|---|---:|---:|---:|---:|---:|");
    let (n2, _) = ms_per_tile(&document(2, BlendMode::Normal));
    let (n10, _) = ms_per_tile(&document(10, BlendMode::Normal));
    for mode in BlendMode::ALL {
        let (t2, a2) = ms_per_tile(&document(2, mode));
        let (t10, a10) = ms_per_tile(&document(10, mode));
        println!(
            "| {} | {t2:.3} | {:.2} | {t10:.3} | {:.2} | {} |",
            mode.name(),
            t2 / n2,
            t10 / n10,
            a2 + a10
        );
    }
}

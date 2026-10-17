//! Layer workflow costs (phase 5B): merge down at both depths (small and
//! 4096², Normal, a blend mode, baked opacity, mask + clipping), its undo
//! and redo, the memory it holds and peaks at, and reordering and
//! renaming in a 100-layer document.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example layer_bench
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerId, LowerMask, Placement};

/// Counts live bytes and remembers the highest value.
struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

fn grew(by: isize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size() as isize);
        // SAFETY: same contract as our caller's; forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        grew(new_size as isize - layout.size() as isize);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const MIB: f64 = 1024.0 * 1024.0;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Fills `tiles` of a layer with translucent content that differs per `k`.
fn fill(d: &mut Document, id: LayerId, tiles: &DirtyTiles, k: u32) {
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..256u32 {
                for x in 0..256u32 {
                    let (px, py) = (ox + x as i32, oy + y as i32);
                    let s = ((px as f32 * 0.013 + k as f32).sin() + (py as f32 * 0.019).cos()) * 0.25 + 0.5;
                    let a = 0.15 + 0.85 * s;
                    let rgb = [s, 1.0 - s, 0.2 + 0.1 * k as f32 % 0.7];
                    *t.get_mut(x, y) = P::from_premul(PremulRgba::new(rgb[0] * a, rgb[1] * a, rgb[2] * a, a));
                }
            }
        })
        .unwrap();
    });
}

fn grid(side: i32) -> DirtyTiles {
    let mut t = DirtyTiles::new();
    for y in 0..side {
        for x in 0..side {
            t.mark(TileCoord { x, y });
        }
    }
    t
}

#[derive(Clone, Copy)]
enum Case {
    Normal,
    SoftLight60,
    BakedOpacity,
    MaskAndClip,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Case::Normal => "Normal over Normal",
            Case::SoftLight60 => "Soft Light 60 % over Normal",
            Case::BakedOpacity => "lower opacity 60 % (baked)",
            Case::MaskAndClip => "upper clipped, both masked (applied)",
        }
    }
}

/// Two dense layers of `side × side` tiles, set up for `case`; the upper
/// one is returned.
fn pair(side: i32, depth: PixelDepth, case: Case) -> (Document, LayerId) {
    let px = side as u32 * 256;
    let mut d = Document::with_depth(px, px, depth);
    let lower = d.active_layer();
    let tiles = grid(side);
    fill(&mut d, lower, &tiles, 1);
    let upper = d.add_layer(Placement::Top).unwrap();
    fill(&mut d, upper, &tiles, 2);
    match case {
        Case::Normal => {}
        Case::SoftLight60 => {
            d.set_blend_mode(upper, BlendMode::SoftLight).unwrap();
            d.set_opacity(upper, 0.6).unwrap();
        }
        Case::BakedOpacity => {
            d.set_opacity(lower, 0.6).unwrap();
        }
        Case::MaskAndClip => {
            d.set_clipped(upper, true).unwrap();
            for id in [lower, upper] {
                d.add_mask(id).unwrap();
                d.modify_mask(id, &tiles, |_, t| {
                    for (i, v) in t.pixels_mut().iter_mut().enumerate() {
                        *v = (i % 256) as u8;
                    }
                })
                .unwrap();
            }
        }
    }
    (d, upper)
}

fn merges() {
    println!("## Merge down");
    println!();
    println!(
        "| size | depth | case | merge ms | undo ms | redo ms | step MiB | live +MiB | peak +MiB | document MiB |"
    );
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for (side, label) in [(1, "256² (1 tile)"), (16, "4096²")] {
        for depth in PixelDepth::ALL {
            for case in [
                Case::Normal,
                Case::SoftLight60,
                Case::BakedOpacity,
                Case::MaskAndClip,
            ] {
                let (mut d, upper) = pair(side, depth, case);
                let history = d.history_bytes();
                let base = live();
                PEAK.store(base, Ordering::Relaxed);
                let t = Instant::now();
                d.merge_down(upper, LowerMask::Apply).unwrap();
                let merge = ms(t);
                let peak = PEAK.load(Ordering::Relaxed) - base;
                let after = live() - base;
                let step = d.history_bytes() - history;
                let doc_bytes: usize = d
                    .layers()
                    .map(|l| l.pixels().tile_count() * 256 * 256 * depth.bytes_per_pixel())
                    .sum();
                let t = Instant::now();
                d.undo().unwrap();
                let undo = ms(t);
                let t = Instant::now();
                d.redo().unwrap();
                let redo = ms(t);
                println!(
                    "| {label} | {} | {} | {merge:.2} | {undo:.3} | {redo:.3} | {:.1} | {:.1} | {:.1} | {:.1} |",
                    depth.bits(),
                    case.name(),
                    step as f64 / MIB,
                    after as f64 / MIB,
                    peak as f64 / MIB,
                    doc_bytes as f64 / MIB,
                );
            }
        }
    }
    println!();
}

fn reorder_and_rename() {
    println!("## Reorder and rename (100 layers, one tile each, 8-bit)");
    println!();
    let mut d = Document::new(4096, 4096);
    let mut ids = vec![d.active_layer()];
    for _ in 1..100 {
        ids.push(d.add_layer(Placement::Top).unwrap());
    }
    let one = grid(1);
    for (k, &id) in ids.iter().enumerate() {
        fill(&mut d, id, &one, k as u32);
    }
    let (bottom, top) = (ids[0], ids[99]);
    let mut moves = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        d.move_layer(bottom, Placement::Top).unwrap();
        moves.push(ms(t));
        d.move_layer(bottom, Placement::Bottom).unwrap();
    }
    let mut renames = Vec::new();
    for i in 0..200 {
        // Alternate layers so every rename is its own step.
        let id = if i % 2 == 0 { top } else { bottom };
        let t = Instant::now();
        d.set_name(id, format!("name {i}")).unwrap();
        renames.push(ms(t));
    }
    println!("| operation | median ms |");
    println!("|---|---:|");
    println!(
        "| move bottom → top (drag and drop) | {:.4} |",
        median(moves)
    );
    println!("| rename | {:.4} |", median(renames));
    println!();
}

fn main() {
    merges();
    reorder_and_rename();
}

//! 8-bit against 16-bit storage (phase 4E, ADR 0018): memory, composition,
//! strokes, history and depth conversion, measured on whole documents
//! rather than isolated pixel loops.
//!
//! Composition: 2048² documents whose layers have content in every tile
//! (the mixed opaque/translucent filler of `blend_bench`), ms per composed
//! 256×256 tile, for 1 and 10 layers, blend modes, a stored mask and
//! clipping groups, with allocations counted. Strokes: real brush-engine
//! strokes across the document, small (8 px) and large (200 px):
//! preview, commit with the history copy (alpha free and locked), undo
//! and redo, and the bytes history holds. Conversion: a 10-layer document
//! 8 → 16 and back, with undo. Memory: buffer sizes and whole documents
//! (10/50/100 layers, full and sparse).
//!
//! `cargo run --release -p tela-corpus --example depth_bench`
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use glam::Vec2;
use tela_brush::dynamics::BrushProperty;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{
    BlendMode, ComposeScratch, Document, LayerId, Overlay, OverlayPaint, Placement,
};

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);

// SAFETY: every method forwards unchanged to the system allocator; the
// counter is a relaxed atomic increment with no other effect.
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
const MIB: f64 = 1024.0 * 1024.0;
const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

fn all_tiles() -> DirtyTiles {
    let mut all = DirtyTiles::new();
    all.mark_rect(IRect::from_size(SIZE, SIZE));
    all
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The `blend_bench` filler (8-bit values, stored exactly at either
/// depth).
fn fill(d: &mut Document, id: LayerId, seed: u32) {
    with_pixel_type!(d.depth(), P => d.modify_tiles(id, &all_tiles(), |_, t: &mut Tile<P>| {
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                let v = (x * 7 + y * 13 + seed) % 256;
                let a = if (x + y + seed).is_multiple_of(5) { 128 } else { 255 };
                let p = |c: u32| (c * a / 255) as u8;
                *t.get_mut(x, y) = P::from_rgba8([p(v), p(255 - v), p((v * 3) % 256), a as u8]);
            }
        }
    }))
    .unwrap();
}

fn full_mask(d: &mut Document, id: LayerId) {
    d.add_mask(id).unwrap();
    d.modify_mask(id, &all_tiles(), |_, t| {
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = (i % 255) as u8;
        }
    })
    .unwrap();
}

#[derive(Clone, Copy)]
struct Stack {
    layers: usize,
    mode: BlendMode,
    clip: bool,
    mask: bool,
}

fn document(depth: PixelDepth, s: Stack) -> Document {
    let mut d = Document::with_depth(SIZE, SIZE, depth);
    let base = d.active_layer();
    fill(&mut d, base, 0);
    if s.mask {
        full_mask(&mut d, base);
    }
    for n in 1..s.layers {
        let id = d.add_layer(Placement::Top).unwrap();
        fill(&mut d, id, n as u32 * 37);
        d.set_blend_mode(id, s.mode).unwrap();
        d.set_opacity(id, 0.8).unwrap();
        d.set_clipped(id, s.clip).unwrap();
    }
    d
}

fn compose(d: &Document, overlay: Option<&Overlay>, tiles: &[TileCoord]) -> (f64, u64) {
    let mut scratch = ComposeScratch::new();
    let mut times = Vec::with_capacity(REPS);
    let before = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..REPS {
        let t = Instant::now();
        for &c in tiles {
            d.compose_tile_into(c, overlay, &mut scratch);
        }
        times.push(t.elapsed().as_secs_f64() * 1e3 / tiles.len() as f64);
    }
    (median(times), ALLOCS.load(Ordering::Relaxed) - before)
}

/// A soft round stroke of `size` px across the document's diagonal.
fn stroke(size: f32) -> StrokeBuffer {
    let mut preset = BrushPreset::plain(size);
    preset.properties.opacity = BrushProperty::constant(0.8);
    preset.properties.hardness = BrushProperty::constant(0.3);
    let mut e = BrushEngine::new(preset);
    e.begin_stroke(
        BrushInput::from_position(Vec2::new(40.0, 60.0), 0.0),
        1.0,
        7,
    );
    for i in 1..=40 {
        let t = i as f32 / 40.0;
        let p = Vec2::new(40.0 + t * 1960.0, 60.0 + t * 1900.0);
        e.add_input_at(BrushInput::from_position(p, f64::from(t)), f64::from(t));
    }
    e.end_stroke().expect("a stroke was begun")
}

fn commit<P: StoragePixel>(d: &mut Document, id: LayerId, s: &StrokeBuffer, alpha: PixelAlpha) {
    d.modify_tiles(id, s.touched(), |c, t: &mut Tile<P>| {
        s.commit_tile(c, t, alpha)
    })
    .unwrap();
}

fn ratio(a: f64, b: f64) -> String {
    format!("{:.2}", b / a)
}

fn composition() {
    println!("## Composition (2048², every tile filled; ms per 256×256 tile)\n");
    println!("| stack | 8-bit | 16-bit | 16 / 8 | allocations (8 / 16) |");
    println!("|---|---:|---:|---:|---:|");
    let tiles: Vec<TileCoord> = all_tiles().iter().collect();
    let plain = |layers, mode| Stack {
        layers,
        mode,
        clip: false,
        mask: false,
    };
    let rows = [
        ("1 layer", plain(1, BlendMode::Normal)),
        ("10 layers, Normal", plain(10, BlendMode::Normal)),
        ("10 layers, Multiply", plain(10, BlendMode::Multiply)),
        ("2 layers, top Screen", plain(2, BlendMode::Screen)),
        ("2 layers, top Luminosity", plain(2, BlendMode::Luminosity)),
        (
            "1 layer + fully stored mask",
            Stack {
                mask: true,
                ..plain(1, BlendMode::Normal)
            },
        ),
        (
            "base + 1 clipped, Normal",
            Stack {
                clip: true,
                ..plain(2, BlendMode::Normal)
            },
        ),
        (
            "base + 9 clipped, Multiply",
            Stack {
                clip: true,
                ..plain(10, BlendMode::Multiply)
            },
        ),
    ];
    for (name, stack) in rows {
        let (a, aa) = compose(&document(PixelDepth::Rgba8, stack), None, &tiles);
        let (b, ba) = compose(&document(PixelDepth::Rgba16, stack), None, &tiles);
        println!(
            "| {name} | {a:.3} | {b:.3} | {} | {aa} / {ba} |",
            ratio(a, b)
        );
    }
}

struct StrokeTimes {
    preview: f64,
    commit: f64,
    locked: f64,
    undo: f64,
    redo: f64,
    held: usize,
    reserved: usize,
}

fn stroke_times(depth: PixelDepth, s: &StrokeBuffer) -> StrokeTimes {
    let one = Stack {
        layers: 1,
        mode: BlendMode::Normal,
        clip: false,
        mask: false,
    };
    let base = document(depth, one);
    let l = base.active_layer();
    let touched: Vec<TileCoord> = s.touched().iter().collect();
    let paint = |c: TileCoord, px: &mut [PremulRgba]| s.paint_over_tile(c, px, PixelAlpha::Free);
    let covers = |c: TileCoord| s.touched().contains(c);
    let o = Overlay {
        target: l,
        paint: OverlayPaint::Pixels(&paint),
        covers: &covers,
    };
    let (preview, _) = compose(&base, Some(&o), &touched);
    let (mut commit_ms, mut locked_ms, mut undo, mut redo) = (vec![], vec![], vec![], vec![]);
    let (mut held, mut reserved) = (0, 0);
    for _ in 0..REPS {
        for (alpha, out) in [
            (PixelAlpha::Free, &mut commit_ms),
            (PixelAlpha::Preserved, &mut locked_ms),
        ] {
            let mut d = base.clone();
            let (h0, r0) = (d.history_bytes(), d.history_reserved_bytes());
            let t = Instant::now();
            with_pixel_type!(depth, P => commit::<P>(&mut d, l, s, alpha));
            out.push(t.elapsed().as_secs_f64() * 1e3);
            if alpha == PixelAlpha::Free {
                held = d.history_bytes() - h0;
                reserved = d.history_reserved_bytes() - r0;
                let t = Instant::now();
                d.undo().unwrap();
                undo.push(t.elapsed().as_secs_f64() * 1e6);
                let t = Instant::now();
                d.redo().unwrap();
                redo.push(t.elapsed().as_secs_f64() * 1e6);
            }
        }
    }
    StrokeTimes {
        preview,
        commit: median(commit_ms),
        locked: median(locked_ms),
        undo: median(undo),
        redo: median(redo),
        held,
        reserved,
    }
}

fn strokes() {
    println!("\n## Strokes on a filled 2048² layer (soft round across the diagonal)\n");
    println!(
        "| brush | tiles | bits | preview ms/tile | commit ms (incl. history copy) | commit, alpha locked ms | undo µs | redo µs | history bytes held |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for size in [8.0, 200.0] {
        let s = stroke(size);
        for depth in PixelDepth::ALL {
            let t = stroke_times(depth, &s);
            println!(
                "| {size} px | {} | {} | {:.3} | {:.2} | {:.2} | {:.0} | {:.0} | {} ({:.1} MiB; reserved {:.1} MiB) |",
                s.touched().len(),
                depth.bits(),
                t.preview,
                t.commit,
                t.locked,
                t.undo,
                t.redo,
                t.held,
                t.held as f64 / MIB,
                t.reserved as f64 / MIB,
            );
        }
    }
}

fn conversion() {
    println!("\n## Depth conversion of a 10-layer 2048² document (640 tiles)\n");
    println!("| operation | ms | history reserved after | allocations |");
    println!("|---|---:|---:|---:|");
    let stack = Stack {
        layers: 10,
        mode: BlendMode::Normal,
        clip: false,
        mask: false,
    };
    for (from, to) in [
        (PixelDepth::Rgba8, PixelDepth::Rgba16),
        (PixelDepth::Rgba16, PixelDepth::Rgba8),
    ] {
        let mut convert = Vec::new();
        let mut undo = Vec::new();
        let mut reserved = 0;
        let mut allocs = 0;
        for _ in 0..3 {
            let mut d = document(from, stack);
            let r0 = d.history_reserved_bytes();
            let a0 = ALLOCS.load(Ordering::Relaxed);
            let t = Instant::now();
            d.convert_depth(to);
            convert.push(t.elapsed().as_secs_f64() * 1e3);
            allocs = ALLOCS.load(Ordering::Relaxed) - a0;
            reserved = d.history_reserved_bytes() - r0;
            let t = Instant::now();
            d.undo().unwrap();
            undo.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!(
            "| {} → {} | {:.1} | {:.1} MiB | {allocs} |",
            from.bits(),
            to.bits(),
            median(convert),
            reserved as f64 / MIB
        );
        println!(
            "| undo {} → {} | {:.3} | — | — |",
            from.bits(),
            to.bits(),
            median(undo)
        );
    }
}

fn memory() {
    println!("\n## Memory\n");
    let tile = |d: PixelDepth| TILE_PIXELS * d.bytes_per_pixel();
    println!("| item | 8-bit | 16-bit |");
    println!("|---|---:|---:|");
    println!(
        "| pixel tile 256 × 256 × 4 channels | {} B ({} KiB) | {} B ({} KiB) |",
        tile(PixelDepth::Rgba8),
        tile(PixelDepth::Rgba8) / 1024,
        tile(PixelDepth::Rgba16),
        tile(PixelDepth::Rgba16) / 1024
    );
    println!(
        "| mask tile (8-bit at every depth) | {0} KiB | {0} KiB |",
        TILE_PIXELS / 1024
    );
    let scratch = (3 * 16 + 4 + 4) * TILE_PIXELS;
    println!(
        "| ComposeScratch (f32 at every depth) | {:.2} MiB | {:.2} MiB |",
        scratch as f64 / MIB,
        scratch as f64 / MIB
    );
    // A full 4000 × 3000 layer has 16 × 12 tiles.
    let full = 16 * 12;
    for layers in [1usize, 10, 50, 100] {
        println!(
            "| {layers} full layers, 4000 × 3000 ({} tiles; computed) | {:.0} MiB | {:.0} MiB |",
            layers * full,
            (layers * full * tile(PixelDepth::Rgba8)) as f64 / MIB,
            (layers * full * tile(PixelDepth::Rgba16)) as f64 / MIB
        );
    }
    // Sparse: each layer holds one 200 px stroke (measured documents).
    let s = stroke(200.0);
    for layers in [10usize, 50, 100] {
        let mut sizes = Vec::new();
        for depth in PixelDepth::ALL {
            let mut d = Document::with_depth(SIZE, SIZE, depth);
            let mut id = d.active_layer();
            for i in 0..layers {
                if i > 0 {
                    id = d.add_layer(Placement::Top).unwrap();
                }
                with_pixel_type!(depth, P => commit::<P>(&mut d, id, &s, PixelAlpha::Free));
            }
            let bytes: usize = d
                .layers()
                .map(|l| l.pixels().tile_count() * tile(depth))
                .sum();
            let tiles: usize = d.layers().map(|l| l.pixels().tile_count()).sum();
            sizes.push((tiles, bytes, d.history_bytes()));
        }
        println!(
            "| {layers} sparse layers, one 200 px stroke each ({} tiles; measured) | {:.0} MiB (+ {:.0} MiB history) | {:.0} MiB (+ {:.0} MiB history) |",
            sizes[0].0,
            sizes[0].1 as f64 / MIB,
            sizes[0].2 as f64 / MIB,
            sizes[1].1 as f64 / MIB,
            sizes[1].2 as f64 / MIB,
        );
    }
}

fn main() {
    memory();
    composition();
    strokes();
    conversion();
}

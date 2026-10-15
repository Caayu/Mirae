//! Cost of masks, clipping and alpha lock (phase 4D).
//!
//! Composition: a 2048² document whose layers have content in every tile
//! (the same mixed opaque/translucent filler as `blend_bench`); ms per
//! composed 256×256 tile and allocations, for plain stacks, masks (empty
//! and fully stored) and clipping groups of 1 and 5 clipped layers.
//! Strokes: a real brush-engine stroke across the document, previewed on
//! pixels and on a mask, committed with alpha free and preserved, and a
//! mask stroke's commit, undo and redo with the bytes history keeps.
//!
//! `cargo run --release -p tela-corpus --example mask_clip_bench`
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use glam::Vec2;
use tela_brush::dynamics::BrushProperty;
use tela_brush::preset::PaintMode;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, TileCoord};
use tela_color::PremulRgba;
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

fn all_tiles() -> DirtyTiles {
    let mut all = DirtyTiles::new();
    all.mark_rect(IRect::from_size(SIZE, SIZE));
    all
}

fn fill(d: &mut Document, id: LayerId, seed: u32) {
    d.modify_tiles(id, &all_tiles(), |_, t| {
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                let v = (x * 7 + y * 13 + seed) % 256;
                let a = if (x + y + seed).is_multiple_of(5) {
                    128
                } else {
                    255
                };
                let p = |c: u32| (c * a / 255) as u8;
                *t.get_mut(x, y) = [p(v), p(255 - v), p((v * 3) % 256), a as u8];
            }
        }
    })
    .unwrap();
}

fn full_mask(d: &mut Document, id: LayerId) {
    d.add_mask(id).unwrap();
    d.modify_mask(id, &all_tiles(), |_, t| {
        for y in 0..TILE_SIZE {
            for x in 0..TILE_SIZE {
                // Never all 255, so every tile stays stored.
                *t.get_mut(x, y) = ((x * 3 + y) % 255) as u8;
            }
        }
    })
    .unwrap();
}

/// A base plus `clipped` clipped layers in `mode`, or plain layers when
/// `clip` is false; `masks` gives the base (and the first clipped layer)
/// a fully stored mask.
fn document(layers: usize, clip: bool, mode: BlendMode, masks: bool) -> Document {
    let mut d = Document::new(SIZE, SIZE);
    let base = d.active_layer();
    fill(&mut d, base, 0);
    if masks {
        full_mask(&mut d, base);
    }
    for n in 1..layers {
        let id = d.add_layer(Placement::Top).unwrap();
        fill(&mut d, id, n as u32 * 37);
        d.set_blend_mode(id, mode).unwrap();
        d.set_opacity(id, 0.8).unwrap();
        d.set_clipped(id, clip).unwrap();
        if masks && n == 1 {
            full_mask(&mut d, id);
        }
    }
    d
}

fn ms_per_tile(d: &Document, overlay: Option<&Overlay>, tiles: &[TileCoord]) -> (f64, u64) {
    let mut scratch = ComposeScratch::new();
    let before = ALLOCS.load(Ordering::Relaxed);
    let t = Instant::now();
    for _ in 0..REPS {
        for &c in tiles {
            d.compose_tile_into(c, overlay, &mut scratch);
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1e3 / (REPS * tiles.len()) as f64;
    (ms, ALLOCS.load(Ordering::Relaxed) - before)
}

/// A wide soft stroke crossing the document diagonally.
fn stroke(mode: PaintMode) -> StrokeBuffer {
    let mut preset = BrushPreset::plain(120.0);
    preset.compositing.mode = mode;
    preset.properties.opacity = BrushProperty::constant(0.8);
    let mut e = BrushEngine::new(preset);
    e.begin_stroke(
        BrushInput::from_position(Vec2::new(40.0, 40.0), 0.0),
        1.0,
        1,
    );
    for i in 1..=200 {
        let f = i as f32 / 200.0;
        let p = Vec2::new(40.0 + f * 1960.0, 40.0 + f * 1960.0);
        e.add_input_at(BrushInput::from_position(p, f64::from(f)), f64::from(f));
    }
    e.end_stroke().unwrap()
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let tiles: Vec<TileCoord> = IRect::from_size(SIZE, SIZE).tiles().collect();
    let n = BlendMode::Normal;
    let m = BlendMode::Multiply;
    println!("Composition (2048², every tile full), ms per tile:\n");
    println!("| stack | ms/tile | allocations |");
    println!("|---|---:|---:|");
    let cases: [(&str, usize, bool, BlendMode, bool); 10] = [
        ("1 layer", 1, false, n, false),
        ("2 layers (4C baseline: Normal)", 2, false, n, false),
        ("2 layers, top Multiply", 2, false, m, false),
        ("1 layer + full mask", 1, false, n, true),
        ("base + 1 clipped (Normal)", 2, true, n, false),
        ("base + 1 clipped (Multiply)", 2, true, m, false),
        ("base + 5 clipped (Normal)", 6, true, n, false),
        ("base + 5 clipped (Multiply)", 6, true, m, false),
        ("6 layers unclipped (Normal)", 6, false, n, false),
        (
            "masks + clipping (base and clipped masked)",
            2,
            true,
            n,
            true,
        ),
    ];
    for (name, layers, clip, mode, masks) in cases {
        let d = document(layers, clip, mode, masks);
        let (ms, allocs) = ms_per_tile(&d, None, &tiles);
        println!("| {name} | {ms:.3} | {allocs} |");
    }
    {
        let mut d = document(1, false, n, false);
        let l = d.active_layer();
        d.add_mask(l).unwrap();
        let (ms, allocs) = ms_per_tile(&d, None, &tiles);
        let stored = d.layer(l).unwrap().mask().unwrap().tile_count();
        println!("| 1 layer + new (empty) mask, {stored} tiles stored | {ms:.3} | {allocs} |");
    }

    // Strokes.
    let s = stroke(PaintMode::Paint);
    // Only erasing changes a new, all-revealing mask.
    let erase = stroke(PaintMode::Erase);
    let touched: Vec<TileCoord> = s.touched().iter().collect();
    println!(
        "\nStroke: 120 px soft round across the diagonal, {} tiles.\n",
        touched.len()
    );
    println!("| operation | ms/tile | allocations |");
    println!("|---|---:|---:|");
    let mut d = document(1, false, n, false);
    let l = d.active_layer();
    d.add_mask(l).unwrap();
    let pixels = |c: TileCoord, px: &mut [PremulRgba]| s.paint_over_tile(c, px, PixelAlpha::Free);
    let locked =
        |c: TileCoord, px: &mut [PremulRgba]| s.paint_over_tile(c, px, PixelAlpha::Preserved);
    let mask = |c: TileCoord, v: &mut [f32]| s.paint_alpha_tile(c, v);
    let covers = |c: TileCoord| s.touched().contains(c);
    for (name, paint) in [
        ("preview on pixels", OverlayPaint::Pixels(&pixels)),
        (
            "preview on pixels, alpha locked",
            OverlayPaint::Pixels(&locked),
        ),
        ("preview on the mask", OverlayPaint::Mask(&mask)),
    ] {
        let o = Overlay {
            target: l,
            paint,
            covers: &covers,
        };
        let (ms, allocs) = ms_per_tile(&d, Some(&o), &touched);
        println!("| {name} | {ms:.3} | {allocs} |");
    }
    for (name, alpha) in [
        ("commit, alpha free", PixelAlpha::Free),
        ("commit, alpha locked", PixelAlpha::Preserved),
    ] {
        let mut times = Vec::new();
        for _ in 0..REPS {
            let mut copy = document(1, false, n, false);
            let id = copy.active_layer();
            let t = Instant::now();
            copy.modify_tiles(id, s.touched(), |c, tile| s.commit_tile(c, tile, alpha))
                .unwrap();
            times.push(t.elapsed().as_secs_f64() * 1e3 / touched.len() as f64);
        }
        println!("| {name} (incl. history copy) | {:.3} | — |", median(times));
    }

    // Mask stroke history.
    let mut commit = Vec::new();
    let mut undo = Vec::new();
    let mut redo = Vec::new();
    let mut bytes = 0;
    for _ in 0..REPS {
        let mut copy = document(1, false, n, false);
        let id = copy.active_layer();
        copy.add_mask(id).unwrap();
        let before = copy.history_bytes();
        let t = Instant::now();
        copy.modify_mask(id, erase.touched(), |c, tile| {
            erase.commit_alpha_tile(c, tile)
        })
        .unwrap();
        commit.push(t.elapsed().as_secs_f64() * 1e3);
        let stored = copy.layer(id).unwrap().mask().unwrap().tile_count();
        let t = Instant::now();
        copy.undo().unwrap();
        undo.push(t.elapsed().as_secs_f64() * 1e6);
        bytes = copy.history_bytes() - before;
        let t = Instant::now();
        copy.redo().unwrap();
        redo.push(t.elapsed().as_secs_f64() * 1e6);
        assert_eq!(copy.layer(id).unwrap().mask().unwrap().tile_count(), stored);
    }
    println!(
        "\nMask erase stroke: commit {:.2} ms, undo {:.0} us, redo {:.0} us (medians of {REPS}); \
         history holds {bytes} bytes after undo ({} mask tiles of {} bytes).",
        median(commit),
        median(undo),
        median(redo),
        bytes / (TILE_SIZE * TILE_SIZE) as usize,
        TILE_SIZE * TILE_SIZE,
    );
    println!(
        "Compose scratch: {} bytes (acc, layer, group: 16 B/px; mask: 4 B/px; out: 4 B/px).",
        (3 * 16 + 4 + 4) * (TILE_SIZE * TILE_SIZE) as usize
    );
}

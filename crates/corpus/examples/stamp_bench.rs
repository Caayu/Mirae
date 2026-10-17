//! Rasterization benchmark: dabs per second, allocations, tiles touched and
//! composition cost.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example stamp_bench
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use glam::Vec2;
use tela_brush::scatter::ScatterMode;
use tela_brush::texture::{CanvasTexture, TextureMode, TexturePattern, TextureSettings};
use tela_brush::tip::Tip;
use tela_brush::{Dab, DabSink, PixelAlpha, StampContext, StrokeBuffer};
use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{ComposeScratch, Document, Overlay, OverlayPaint, Placement};

/// Counts allocations so "no allocation per dab" is measured, not assumed.
struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same contract as our caller's; forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Hard,
    Soft,
    Textured,
    Scatter,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Hard => "hard round",
            Self::Soft => "soft round",
            Self::Textured => "textured",
            Self::Scatter => "scatter",
        }
    }
}

/// Dabs along a line at 10% spacing, as the engine would place them.
fn dabs(kind: Kind, size: f32, count: usize) -> Vec<Dab> {
    let style = tela_brush::scatter::ScatterStyle {
        mode: ScatterMode::Radial,
        ..Default::default()
    };
    let rng = tela_brush::rng::StrokeRng::new(1);
    let mut out = Vec::with_capacity(count * 2);
    for i in 0..count {
        let base = Dab {
            center: Vec2::new(
                300.0 + i as f32 * size * 0.1,
                300.0 + (i as f32 * 0.37).sin() * 20.0,
            ),
            radius: size / 2.0,
            hardness: if kind == Kind::Soft { 0.0 } else { 0.9 },
            flow: 0.5,
            texture: if kind == Kind::Textured { 1.0 } else { 0.0 },
            ..Dab::default()
        };
        let amount = if kind == Kind::Scatter { 1.0 } else { 0.0 };
        style.apply(&base, amount, Vec2::X, &rng, i as u64, &mut out);
    }
    out
}

fn context(kind: Kind) -> StampContext {
    StampContext {
        tip: Tip::default().prepare(),
        texture: (kind == Kind::Textured).then(|| {
            CanvasTexture::new(&TextureSettings {
                pattern: TexturePattern::PaperRough,
                mode: TextureMode::Height,
                ..Default::default()
            })
        }),
        anti_aliasing: true,
        ..Default::default()
    }
}

fn main() {
    println!("| brush | size px | dabs | dabs/s | Mpx/s | tiles/dab | allocs/dab (hot path) |");
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for kind in [Kind::Hard, Kind::Soft, Kind::Textured, Kind::Scatter] {
        for size in [5.0f32, 25.0, 100.0, 250.0, 500.0] {
            let count = ((40_000_000.0 / (size * size)) as usize).clamp(200, 20_000);
            let batch = dabs(kind, size, count);
            // Warm-up run allocates every tile the stroke touches; the
            // measured run reuses a buffer that already owns them.
            let mut warm = StrokeBuffer::new(Color::BLACK, context(kind));
            for d in &batch {
                warm.dab(d);
            }
            let mut buf = warm;
            let before = ALLOCS.load(Ordering::Relaxed);
            let t = Instant::now();
            for d in &batch {
                buf.dab(d);
            }
            let secs = t.elapsed().as_secs_f64();
            let allocs = ALLOCS.load(Ordering::Relaxed) - before;
            let per_dab = allocs as f64 / batch.len() as f64;
            let px: f64 = batch
                .iter()
                .map(|d| (2.0 * d.radius as f64 + 2.0).powi(2))
                .sum();
            let tiles_per_dab: f64 = batch
                .iter()
                .map(|d| {
                    let r = d.radius + 1.0;
                    tela_canvas::IRect::enclosing(
                        d.center.x - r,
                        d.center.y - r,
                        d.center.x + r,
                        d.center.y + r,
                    )
                    .tiles()
                    .count() as f64
                })
                .sum::<f64>()
                / batch.len() as f64;
            println!(
                "| {} | {size:.0} | {} | {:.0} | {:.1} | {:.2} | {:.3} |",
                kind.name(),
                batch.len(),
                batch.len() as f64 / secs,
                px / secs / 1e6,
                tiles_per_dab,
                per_dab,
            );
        }
    }

    // Single thread vs per-tile parallel, with realistic batches: a pen at
    // 2000 px/s sampled at 200 Hz moves 10 px per sample.
    println!(
        "\nsingle-thread vs parallel ({} hardware threads), batches of one input sample:\n",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
    println!("| brush | size px | dabs/batch | single dabs/s | parallel dabs/s | speed-up |");
    println!("|---|---:|---:|---:|---:|---:|");
    for kind in [Kind::Hard, Kind::Soft, Kind::Textured] {
        for size in [5.0f32, 25.0, 100.0, 250.0, 500.0] {
            let per_batch = ((10.0 / (0.1 * size)).round() as usize).max(1);
            let count = ((40_000_000.0 / (size * size)) as usize).clamp(200, 20_000);
            let batch = dabs(kind, size, count);
            let run = |parallel: bool| {
                let ctx = StampContext {
                    parallel,
                    ..context(kind)
                };
                // Warm: tiles exist, as in the middle of a stroke.
                let mut buf = StrokeBuffer::new(Color::BLACK, ctx);
                buf.dabs(&batch);
                let t = Instant::now();
                for chunk in batch.chunks(per_batch) {
                    buf.dabs(chunk);
                }
                batch.len() as f64 / t.elapsed().as_secs_f64()
            };
            let (single, parallel) = (run(false), run(true));
            println!(
                "| {} | {size:.0} | {per_batch} | {single:.0} | {parallel:.0} | {:.2}x |",
                kind.name(),
                parallel / single
            );
        }
    }

    // Composition: one stroke previewed over a two-layer document.
    let mut doc = Document::new(2048, 2048);
    let bottom = doc.active_layer();
    let top = doc.add_layer(Placement::Top).unwrap();
    let mut diagonal = DirtyTiles::new();
    diagonal.mark_rect(IRect::from_size(2048, 2048));
    for layer in [bottom, top] {
        doc.modify_tiles(layer, &diagonal, |c, t| {
            let (ox, oy) = c.origin();
            for i in 0..TILE_SIZE {
                if ox == oy {
                    *t.get_mut(i, i) = [20u8, 30, 40, 255];
                }
            }
        })
        .unwrap();
    }
    let mut stroke = StrokeBuffer::new(Color::BLACK, context(Kind::Soft));
    for d in dabs(Kind::Soft, 120.0, 400) {
        stroke.dab(&d);
    }
    let paint =
        |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, PixelAlpha::Free);
    let covers = |c: TileCoord| stroke.touched().contains(c);
    let overlay = Overlay {
        target: top,
        paint: OverlayPaint::Pixels(&paint),
        covers: &covers,
    };
    let coords: Vec<TileCoord> = stroke.touched().iter().collect();
    let mut scratch = ComposeScratch::new();
    let reps = 20;
    let before = ALLOCS.load(Ordering::Relaxed);
    let t = Instant::now();
    for _ in 0..reps {
        for &c in &coords {
            doc.compose_tile_into(c, Some(&overlay), &mut scratch);
        }
    }
    let per_tile = t.elapsed().as_secs_f64() / (reps * coords.len()) as f64;
    let allocs = ALLOCS.load(Ordering::Relaxed) - before;
    println!(
        "\ncomposition (2 layers + stroke overlay): {:.3} ms per 256x256 tile, {} allocations in {} tiles",
        per_tile * 1e3,
        allocs,
        reps * coords.len()
    );
    color_costs();
}

/// Phase 5A: what the paint colour costs. Stamping never reads the colour
/// (the stroke is an alpha mask); it enters only where the mask is
/// composited, in the preview (`paint_over_tile`) and the commit
/// (`commit_tile`). Both are measured with black and a colour with
/// independent channels, at both storage depths.
fn color_costs() {
    println!(
        "\npaint colour (soft 120 px stroke, 400 dabs; preview = 2-layer composition with the stroke overlay):\n"
    );
    println!("| depth | colour | preview ms / tile | commit ms / tile | allocations |");
    println!("|---|---|---:|---:|---:|");
    for depth in PixelDepth::ALL {
        for (name, color) in [
            ("black", Color::BLACK),
            ("37,149,213", Color::from_rgb8([37, 149, 213])),
        ] {
            let (preview, commit, allocs) = with_pixel_type!(depth, P => color_cost::<P>(color));
            println!(
                "| {}-bit | {name} | {:.3} | {:.3} | {allocs} |",
                depth.bits(),
                preview * 1e3,
                commit * 1e3
            );
        }
    }
}

fn color_cost<P: StoragePixel>(color: Color) -> (f64, f64, u64) {
    let mut doc = Document::with_depth(2048, 2048, P::DEPTH);
    let bottom = doc.active_layer();
    let top = doc.add_layer(Placement::Top).unwrap();
    let mut all = DirtyTiles::new();
    all.mark_rect(IRect::from_size(2048, 2048));
    for layer in [bottom, top] {
        doc.modify_tiles(layer, &all, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for i in 0..TILE_SIZE {
                if ox == oy {
                    *t.get_mut(i, i) = P::from_rgba8([20, 30, 40, 255]);
                }
            }
        })
        .unwrap();
    }
    let mut stroke = StrokeBuffer::new(color, context(Kind::Soft));
    for d in dabs(Kind::Soft, 120.0, 400) {
        stroke.dab(&d);
    }
    let paint =
        |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px, PixelAlpha::Free);
    let covers = |c: TileCoord| stroke.touched().contains(c);
    let overlay = Overlay {
        target: top,
        paint: OverlayPaint::Pixels(&paint),
        covers: &covers,
    };
    let coords: Vec<TileCoord> = stroke.touched().iter().collect();
    let mut scratch = ComposeScratch::new();
    let mut tile: Tile<P> = Tile::new();
    let reps = 20;
    let before = ALLOCS.load(Ordering::Relaxed);
    let t = Instant::now();
    for _ in 0..reps {
        for &c in &coords {
            doc.compose_tile_into(c, Some(&overlay), &mut scratch);
        }
    }
    let preview = t.elapsed().as_secs_f64() / (reps * coords.len()) as f64;
    let t = Instant::now();
    for _ in 0..reps {
        for &c in &coords {
            stroke.commit_tile(c, &mut tile, PixelAlpha::Free);
        }
    }
    let commit = t.elapsed().as_secs_f64() / (reps * coords.len()) as f64;
    let allocs = ALLOCS.load(Ordering::Relaxed) - before;
    std::hint::black_box(&tile);
    (preview, commit, allocs)
}

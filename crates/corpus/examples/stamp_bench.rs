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
use tela_brush::{Dab, DabSink, StampContext, StrokeBuffer};
use tela_canvas::TileCoord;
use tela_color::{PremulRgba, Rgba};
use tela_document::{ComposeScratch, Document, Overlay};

/// Counts allocations so "no allocation per dab" is measured, not assumed.
struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);

// SAFETY: forwards to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
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
            let mut warm = StrokeBuffer::new(Rgba::BLACK, context(kind));
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

    // Composition: one stroke previewed over a two-layer document.
    let mut doc = Document::new(2048, 2048);
    doc.layers.push(tela_document::RasterLayer::new("Layer 2"));
    for layer in &mut doc.layers {
        for y in 0..2048 {
            layer.pixels.set_pixel(y, y, [20, 30, 40, 255]);
        }
    }
    let mut stroke = StrokeBuffer::new(Rgba::BLACK, context(Kind::Soft));
    for d in dabs(Kind::Soft, 120.0, 400) {
        stroke.dab(&d);
    }
    let paint = |c: TileCoord, px: &mut [PremulRgba]| stroke.paint_over_tile(c, px);
    let covers = |c: TileCoord| stroke.touched().contains(c);
    let overlay = Overlay {
        above_layer: 1,
        paint: &paint,
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
}

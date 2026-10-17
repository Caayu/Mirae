//! Gradient costs (phase 5F): one preview frame (every tile the gradient
//! covers composed through the compositor with the preview overlay, as the
//! app does while dragging), the commit (one history step), undo and redo,
//! the commit's real peak memory above the document and what its history
//! step holds, for 256², 2048² and 4096² canvases at both depths: linear,
//! radial, angular and reflected gradients, two and eight stops, dither on
//! and off, a feathered selection and a mask.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example gradient_bench            # ~2 min
//! cargo run --release -p tela-corpus --example gradient_bench -- --quick # 256² and 2048² only
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;

use tela_color::{Color, PixelDepth, PremulRgba};
use tela_document::{
    ComposeScratch, Document, FillTarget, Gradient, GradientExtend, GradientGeometry,
    GradientPaint, GradientShape, GradientStop, GradientStops, LayerPart, Mixing, Overlay,
    OverlayPaint, SelectionOp, SelectionShape,
};

/// Counts live bytes, the highest value and allocations.
struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

fn grew(by: isize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size() as isize);
        ALLOCS.fetch_add(1, Ordering::Relaxed);
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
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const MIB: f64 = 1024.0 * 1024.0;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    shape: GradientShape,
    stops: usize,
    mixing: Mixing,
    dither: bool,
    selection: bool,
    mask: bool,
}

const fn case(name: &'static str, shape: GradientShape) -> Case {
    Case {
        name,
        shape,
        stops: 2,
        mixing: Mixing::Perceptual,
        dither: true,
        selection: false,
        mask: false,
    }
}

fn stops(n: usize) -> GradientStops {
    let colours = [
        Color::new(1.0, 0.0, 0.0),
        Color::new(1.0, 0.6, 0.0),
        Color::new(0.9, 0.9, 0.1),
        Color::new(0.1, 0.8, 0.2),
        Color::new(0.0, 0.6, 0.9),
        Color::new(0.2, 0.2, 0.8),
        Color::new(0.6, 0.1, 0.7),
        Color::WHITE,
    ];
    if n == 2 {
        return GradientStops::two(Color::BLACK, Color::WHITE);
    }
    GradientStops::new(
        (0..n)
            .map(|i| GradientStop::new(i as f32 / (n - 1) as f32, colours[i % colours.len()], 1.0)),
    )
    .expect("stops")
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    println!("## Gradients (phase 5F)\n");
    println!(
        "One thread. The gradient spans the canvas diagonally (every tile changes). \
         'preview' = one frame: every covered tile composed through the compositor with \
         the preview overlay (what the app redraws per pointer move); 'commit' = \
         Document::apply_gradient (paint, quantize, history); 'peak' = the allocator's \
         real peak above the document during the commit, 'growth' what stays (the \
         layer's new tiles), 'history' the bytes the step holds; 'preview allocs' = \
         allocations during the preview frame (the overlay itself allocates none).\n"
    );
    println!(
        "| canvas | bits | case | tiles | render ms | preview ms | Mpx/s | preview allocs | commit ms | undo ms | redo ms | peak MiB | growth MiB | history MiB |"
    );
    println!("|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let cases = [
        case("linear, 2 stops", GradientShape::Linear),
        Case {
            dither: false,
            ..case("linear, 2 stops, no dither", GradientShape::Linear)
        },
        Case {
            mixing: Mixing::Standard,
            ..case("linear, 2 stops, standard", GradientShape::Linear)
        },
        Case {
            stops: 8,
            ..case("linear, 8 stops", GradientShape::Linear)
        },
        case("radial", GradientShape::Radial),
        case("angular", GradientShape::Angular),
        case("reflected", GradientShape::Reflected),
        Case {
            selection: true,
            ..case("linear, feathered selection", GradientShape::Linear)
        },
        Case {
            mask: true,
            ..case("linear, mask", GradientShape::Linear)
        },
    ];
    let sizes: &[u32] = if quick {
        &[256, 2048]
    } else {
        &[256, 2048, 4096]
    };
    for &size in sizes {
        for depth in PixelDepth::ALL {
            for c in &cases {
                run(size, depth, c);
            }
        }
    }
}

fn run(size: u32, depth: PixelDepth, c: &Case) {
    let mut d = Document::with_depth(size, size, depth);
    let id = d.active_layer();
    if c.mask {
        d.add_mask(id).expect("mask");
    }
    if c.selection {
        let q = size as f32 / 4.0;
        d.select(
            &SelectionShape::Rect([q, q], [3.0 * q, 3.0 * q]),
            false,
            SelectionOp::Replace,
        );
        d.feather_selection(20.0);
    }
    let s = f64::from(size);
    let geometry = match c.shape {
        GradientShape::Linear | GradientShape::Reflected if c.shape == GradientShape::Reflected => {
            GradientGeometry {
                shape: c.shape,
                start: [s / 2.0, s / 2.0],
                end: [s * 0.9, s * 0.8],
            }
        }
        GradientShape::Radial | GradientShape::Angular => GradientGeometry {
            shape: c.shape,
            start: [s / 2.0, s / 2.0],
            end: [s * 0.9, s / 2.0],
        },
        _ => GradientGeometry {
            shape: c.shape,
            start: [s * 0.1, s * 0.1],
            end: [s * 0.9, s * 0.9],
        },
    };
    let g = Gradient::new(
        geometry,
        &stops(c.stops),
        c.mixing,
        GradientExtend::Clamp,
        c.dither,
    );
    let part = if c.mask {
        LayerPart::Mask
    } else {
        LayerPart::Pixels
    };
    let target = FillTarget { layer: id, part };
    // With a mask: grey pixels to show, and a mask that hides everything
    // first, so revealing changes it.
    if c.mask {
        let grey = Gradient::new(
            GradientGeometry {
                shape: GradientShape::Linear,
                start: [0.0, 0.0],
                end: [1.0, 0.0],
            },
            &GradientStops::two(Color::new(0.5, 0.5, 0.5), Color::new(0.5, 0.5, 0.5)),
            Mixing::Standard,
            GradientExtend::Clamp,
            false,
        );
        let pixels = FillTarget {
            layer: id,
            part: LayerPart::Pixels,
        };
        d.apply_gradient(pixels, &grey, GradientPaint::Draw)
            .expect("grey");
        let hide = Gradient::new(
            GradientGeometry {
                shape: GradientShape::Linear,
                start: [0.0, 0.0],
                end: [1.0, 0.0],
            },
            &GradientStops::two(Color::BLACK, Color::BLACK),
            Mixing::Standard,
            GradientExtend::Clamp,
            false,
        );
        d.apply_gradient(target, &hide, GradientPaint::Erase)
            .expect("hide");
    }

    // One preview frame.
    let mut scratch = ComposeScratch::new();
    let (tiles, preview_ms, preview_allocs) = {
        let p = d
            .gradient_preview(target, &g, GradientPaint::Draw)
            .expect("preview");
        let pixels = |t, px: &mut [PremulRgba]| p.pixels(t, px);
        let mask = |t, v: &mut [f32]| p.mask(t, v);
        let covers = |t| p.covers(t);
        let overlay = Overlay {
            target: id,
            paint: if c.mask {
                OverlayPaint::Mask(&mask)
            } else {
                OverlayPaint::Pixels(&pixels)
            },
            covers: &covers,
        };
        let coords: Vec<_> = p.tiles().iter().collect();
        let a0 = ALLOCS.load(Ordering::Relaxed);
        let t = Instant::now();
        for &coord in &coords {
            d.compose_tile_into(coord, Some(&overlay), &mut scratch);
        }
        (coords.len(), ms(t), ALLOCS.load(Ordering::Relaxed) - a0)
    };
    let mpx = tiles as f64 * 65536.0 / (preview_ms / 1000.0) / 1e6;
    // The gradient alone (positions and colours), without painting.
    let render_ms = {
        let mut out = vec![PremulRgba::TRANSPARENT; 65536];
        let t = Instant::now();
        for coord in tela_canvas::IRect::new(0, 0, size as i32, size as i32).tiles() {
            g.render_tile(coord, &mut out);
        }
        ms(t)
    };

    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let history_before = d.history_bytes();
    let t = Instant::now();
    d.apply_gradient(target, &g, GradientPaint::Draw)
        .expect("commit");
    let commit_ms = ms(t);
    let peak = (PEAK.load(Ordering::Relaxed) - base) as f64 / MIB;
    let growth = (LIVE.load(Ordering::Relaxed) - base) as f64 / MIB;
    let history = (d.history_bytes() - history_before) as f64 / MIB;
    let t = Instant::now();
    d.undo().expect("undo");
    let undo_ms = ms(t);
    let t = Instant::now();
    d.redo().expect("redo");
    let redo_ms = ms(t);
    println!(
        "| {size}² | {} | {} | {tiles} | {render_ms:.1} | {preview_ms:.1} | {mpx:.1} | {preview_allocs} | {commit_ms:.1} | {undo_ms:.2} | {redo_ms:.2} | {peak:.1} | {growth:.1} | {history:.1} |",
        depth.bits(),
        c.name,
    );
}

//! Fill costs (phase 5E): the region alone and the whole fill (region,
//! paint, history), undo, the tiles read and written, the temporary
//! memory the region computation holds and the real peak above the
//! document while filling, for 256², 2048² and 4096² canvases at both
//! depths: a whole empty layer, the background around line art and a
//! small closed shape through the composed reference, and a soft gradient
//! at tolerance 0, 10 % and 50 %.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example fill_bench            # ~1 min
//! cargo run --release -p tela-corpus --example fill_bench -- --quick # 256² and 2048² only
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{
    ConnectedFill, Document, FillPaint, FillReference, FillTarget, LayerId, LayerPart, Placement,
    Tolerance,
};

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

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn paint_layer(d: &mut Document, id: LayerId, f: impl Fn(i32, i32) -> PremulRgba) {
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(0, 0, d.width() as i32, d.height() as i32));
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..TILE_SIZE as i32 {
                for x in 0..TILE_SIZE as i32 {
                    let p = f(ox + x, oy + y);
                    if p.a > 0.0 {
                        *t.get_mut(x as u32, y as u32) = P::from_premul(p);
                    }
                }
            }
        })
        .unwrap();
    });
}

/// Layers: "gradient" (a soft colour field), "line" (anti-aliased rings,
/// radius 40, every 128 px), "fill" (empty, the target of most cases).
struct Doc {
    d: Document,
    gradient: LayerId,
    fill: LayerId,
}

fn document(side: u32, depth: PixelDepth) -> Doc {
    let mut d = Document::with_depth(side, side, depth);
    let gradient = d.active_layer();
    paint_layer(&mut d, gradient, |x, y| {
        let v = |k: i32| (0.5 + 0.5 * ((x * k + y * (7 - k)) as f32 / 900.0).sin()).clamp(0.0, 1.0);
        PremulRgba::new(v(2), v(3), v(5), 1.0)
    });
    let line = d.add_layer(Placement::Top).unwrap();
    paint_layer(&mut d, line, |x, y| {
        let (dx, dy) = ((x % 128) as f32 - 63.5, (y % 128) as f32 - 63.5);
        let a = (1.5 - ((dx * dx + dy * dy).sqrt() - 40.0).abs()).clamp(0.0, 1.0);
        PremulRgba::new(0.0, 0.0, 0.0, a)
    });
    let fill = d.add_layer(Placement::Top).unwrap();
    Doc { d, gradient, fill }
}

struct Case {
    name: &'static str,
    target: fn(&Doc) -> LayerId,
    reference: FillReference,
    seed: fn(u32) -> (i32, i32),
    tenths: u16,
    hide_gradient: bool,
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    println!("## Fill (phase 5E)\n");
    println!(
        "One thread. 'region' = Document::fill_region alone; 'fill' = fill_connected \
         (region, paint into the target, history); tiles read = reference tiles decoded \
         or composed; written = tiles changed; temp = what the region computation holds \
         (distance maps, bitsets, span stack, edge); peak = the allocator's real peak \
         above the document during the fill, of which 'growth' stays (the layer's new \
         tiles) and the rest is transient; history = bytes the step holds.\n"
    );
    println!(
        "| canvas | bits | case | tolerance | region px | region ms | fill ms | undo ms | tiles read | maps | written | temp MiB | peak MiB | growth MiB | history MiB |"
    );
    println!("|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let cases = [
        Case {
            name: "empty layer, whole (target)",
            target: |d| d.fill,
            reference: FillReference::Target,
            seed: |_| (1, 1),
            tenths: 100,
            hide_gradient: false,
        },
        Case {
            name: "around line art (composite)",
            target: |d| d.fill,
            reference: FillReference::Composite,
            seed: |_| (1, 1),
            tenths: 100,
            hide_gradient: true,
        },
        Case {
            name: "inside one ring (composite)",
            target: |d| d.fill,
            reference: FillReference::Composite,
            seed: |s| {
                (
                    (s / 2 / 128 * 128 + 64) as i32,
                    (s / 2 / 128 * 128 + 64) as i32,
                )
            },
            tenths: 100,
            hide_gradient: true,
        },
        Case {
            name: "gradient (target)",
            target: |d| d.gradient,
            reference: FillReference::Target,
            seed: |s| ((s / 2) as i32, (s / 3) as i32),
            tenths: 0,
            hide_gradient: false,
        },
        Case {
            name: "gradient (target)",
            target: |d| d.gradient,
            reference: FillReference::Target,
            seed: |s| ((s / 2) as i32, (s / 3) as i32),
            tenths: 100,
            hide_gradient: false,
        },
        Case {
            name: "gradient (target)",
            target: |d| d.gradient,
            reference: FillReference::Target,
            seed: |s| ((s / 2) as i32, (s / 3) as i32),
            tenths: 500,
            hide_gradient: false,
        },
    ];
    let sides: &[u32] = if quick {
        &[256, 2048]
    } else {
        &[256, 2048, 4096]
    };
    for &side in sides {
        for depth in PixelDepth::ALL {
            let base = document(side, depth);
            for case in &cases {
                let mut doc = Doc {
                    d: base.d.clone(),
                    gradient: base.gradient,
                    fill: base.fill,
                };
                if case.hide_gradient {
                    doc.d.set_visible(doc.gradient, false).unwrap();
                }
                let target = FillTarget {
                    layer: (case.target)(&doc),
                    part: LayerPart::Pixels,
                };
                let fill = ConnectedFill {
                    seed: (case.seed)(side),
                    reference: case.reference,
                    tolerance: Tolerance::from_tenths(case.tenths),
                    anti_alias: true,
                };
                let t = Instant::now();
                let region = doc.d.fill_region(target, &fill).unwrap();
                let region_ms = ms(t);
                let stats = region.stats();
                drop(region);
                let history_before = doc.d.history_bytes();
                let live = LIVE.load(Ordering::Relaxed);
                PEAK.store(live, Ordering::Relaxed);
                let t = Instant::now();
                let dirty = doc
                    .d
                    .fill_connected(target, &fill, FillPaint::Color(Color::new(0.9, 0.2, 0.4)))
                    .unwrap();
                let fill_ms = ms(t);
                let peak = (PEAK.load(Ordering::Relaxed) - live) as f64 / MIB;
                let growth = (LIVE.load(Ordering::Relaxed) - live) as f64 / MIB;
                let history = (doc.d.history_bytes() - history_before) as f64 / MIB;
                let t = Instant::now();
                doc.d.undo().unwrap();
                let undo_ms = ms(t);
                println!(
                    "| {side}² | {} | {} | {:.1} % | {} | {region_ms:.1} | {fill_ms:.1} | {undo_ms:.2} | {} | {} | {} | {:.1} | {peak:.1} | {growth:.1} | {history:.1} |",
                    depth.bits(),
                    case.name,
                    f32::from(case.tenths) / 10.0,
                    stats.pixels,
                    stats.tiles_read,
                    stats.maps,
                    dirty.len(),
                    stats.temp_bytes as f64 / MIB,
                );
            }
        }
    }
}

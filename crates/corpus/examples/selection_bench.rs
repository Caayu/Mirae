//! Selection costs (phase 5C): creating rectangles, ellipses and lassos,
//! combining, select all, invert, deselect, feathering (1, 10, 50 px on a
//! small and a large selection), tracing the outline, a stroke's preview
//! and commit with and without a selection, and the memory selections and
//! their history hold. A 4096² 8-bit document.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example selection_bench
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use glam::Vec2;
use tela_brush::{Dab, DabSink, PixelAlpha, StampContext, StrokeBuffer};
use tela_canvas::{TILE_SIZE, Tile};
use tela_color::{Color, PremulRgba};
use tela_document::{Document, SelectionOp, SelectionShape};

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

const SIZE: u32 = 4096;
const MIB: f64 = 1024.0 * 1024.0;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Median of `runs` timings of `f` on a fresh copy of `doc`.
fn time(doc: &Document, runs: usize, f: impl Fn(&mut Document)) -> f64 {
    let mut v: Vec<f64> = (0..runs)
        .map(|_| {
            let mut d = doc.clone();
            let t = Instant::now();
            f(&mut d);
            ms(t)
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> SelectionShape {
    SelectionShape::Rect([x0, y0], [x1, y1])
}

fn ellipse(r: f32) -> SelectionShape {
    SelectionShape::Ellipse {
        center: [2048.3, 2047.7],
        radii: [r, r * 0.7],
        rotation: 0.4,
    }
}

/// A lasso of `n` points around a wobbly circle of radius `r`.
fn lasso(n: usize, r: f32) -> SelectionShape {
    SelectionShape::Polygon(
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / n as f32;
                let rr = r * (1.0 + 0.08 * (7.0 * t).sin());
                [2048.0 + rr * t.cos(), 2048.0 + rr * t.sin()]
            })
            .collect(),
    )
}

fn with(shape: &SelectionShape, aa: bool) -> Document {
    let mut d = Document::new(SIZE, SIZE);
    d.select(shape, aa, SelectionOp::Replace);
    d
}

/// A 500 px soft stroke across the document's middle, dabs at 10 px.
fn stroke() -> StrokeBuffer {
    let mut s = StrokeBuffer::new(
        Color::new(0.2, 0.4, 0.8),
        StampContext {
            anti_aliasing: true,
            ..StampContext::default()
        },
    );
    for i in 0..300 {
        s.dab(&Dab {
            center: Vec2::new(
                500.0 + i as f32 * 10.0,
                2048.0 + (i as f32 * 0.05).sin() * 300.0,
            ),
            radius: 250.0,
            opacity: 0.8,
            flow: 0.3,
            hardness: 0.3,
            ..Dab::default()
        });
    }
    s
}

fn main() {
    let empty = Document::new(SIZE, SIZE);
    println!("4096² 8-bit document; median of runs, ms.\n");
    println!("| operation | small | large |");
    println!("|---|---:|---:|");
    let small_rect = rect(1000.2, 1000.7, 1200.4, 1150.1);
    let large_rect = rect(100.0, 100.0, 4000.0, 3990.0);
    println!(
        "| rectangle (200×150 / 3900²) | {:.2} | {:.2} |",
        time(&empty, 9, |d| {
            let _ = d.select(&small_rect, false, SelectionOp::Replace);
        }),
        time(&empty, 5, |d| {
            let _ = d.select(&large_rect, false, SelectionOp::Replace);
        })
    );
    println!(
        "| ellipse AA (r 100 / 1900) | {:.2} | {:.2} |",
        time(&empty, 9, |d| {
            let _ = d.select(&ellipse(100.0), true, SelectionOp::Replace);
        }),
        time(&empty, 5, |d| {
            let _ = d.select(&ellipse(1900.0), true, SelectionOp::Replace);
        })
    );
    println!(
        "| ellipse hard (r 100 / 1900) | {:.2} | {:.2} |",
        time(&empty, 9, |d| {
            let _ = d.select(&ellipse(100.0), false, SelectionOp::Replace);
        }),
        time(&empty, 5, |d| {
            let _ = d.select(&ellipse(1900.0), false, SelectionOp::Replace);
        })
    );
    println!(
        "| lasso AA, 2 000 points (r 150 / 1800) | {:.2} | {:.2} |",
        time(&empty, 9, |d| {
            let _ = d.select(&lasso(2000, 150.0), true, SelectionOp::Replace);
        }),
        time(&empty, 5, |d| {
            let _ = d.select(&lasso(2000, 1800.0), true, SelectionOp::Replace);
        })
    );
    let base_small = with(&ellipse(300.0), true);
    let base_large = with(&ellipse(1900.0), true);
    for (op, name) in [
        (SelectionOp::Add, "add"),
        (SelectionOp::Subtract, "subtract"),
        (SelectionOp::Intersect, "intersect"),
    ] {
        println!(
            "| {name} an AA ellipse (r 250 on r 300 / r 1600 on r 1900) | {:.2} | {:.2} |",
            time(&base_small, 9, |d| {
                let _ = d.select(&ellipse(250.0), true, op);
            }),
            time(&base_large, 5, |d| {
                let _ = d.select(&ellipse(1600.0), true, op);
            })
        );
    }
    println!(
        "| select all / invert it / deselect | {:.3} / {:.3} / {:.3} | |",
        time(&empty, 9, |d| {
            let _ = d.select_all();
        }),
        time(&with(&large_rect, false), 9, |d| {
            let _ = d.invert_selection();
        }),
        time(&base_large, 9, |d| {
            let _ = d.deselect();
        }),
    );
    let all_minus = {
        let mut d = Document::new(SIZE, SIZE);
        d.select_all();
        d.select(&small_rect, false, SelectionOp::Subtract);
        d
    };
    println!(
        "| invert (select all minus 200×150) | {:.3} | |",
        time(&all_minus, 9, |d| {
            let _ = d.invert_selection();
        })
    );
    for px in [1.0, 10.0, 50.0] {
        println!(
            "| feather {px} px (r 100 ellipse / r 1900 ellipse) | {:.2} | {:.2} |",
            time(&with(&ellipse(100.0), true), 5, |d| {
                let _ = d.feather_selection(px);
            }),
            time(&base_large, 3, |d| {
                let _ = d.feather_selection(px);
            })
        );
    }
    let mut full_feathered = Document::new(SIZE, SIZE);
    full_feathered.select_all();
    println!(
        "| feather 50 px, everything selected | {:.3} | |",
        time(&full_feathered, 5, |d| {
            let _ = d.feather_selection(50.0);
        })
    );
    let outline = |d: &Document| {
        let t = Instant::now();
        let loops = d.selection().unwrap().outline();
        (ms(t), loops.iter().map(Vec::len).sum::<usize>())
    };
    let (a, na) = outline(&with(&ellipse(100.0), true));
    let (b, nb) = outline(&base_large);
    println!("| outline (r 100 / r 1900 ellipse; points {na} / {nb}) | {a:.2} | {b:.2} |");
    let (c, nc) = outline(&all_minus);
    println!("| outline, everything minus 200×150 ({nc} points) | {c:.2} | |");

    // Strokes through a selection: preview per tile and commit.
    println!("\nStroke: 300 dabs of 500 px, flow 0.3 (soft), over the document's middle.\n");
    println!("| selection | preview ms / tile | commit ms / tile | tiles |");
    println!("|---|---:|---:|---:|");
    let s = stroke();
    let feathered = {
        let mut d = with(&ellipse(1200.0), true);
        d.feather_selection(50.0);
        d
    };
    let mut all = Document::new(SIZE, SIZE);
    all.select_all();
    for (name, doc) in [
        ("none", &empty),
        ("everything", &all),
        ("hard rectangle 3900²", &with(&large_rect, false)),
        ("AA ellipse r 1900", &base_large),
        ("feathered ellipse (50 px)", &feathered),
    ] {
        let tiles = s.touched().clone();
        let mut px = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
        let t = Instant::now();
        for _ in 0..3 {
            for c in tiles.iter() {
                px.fill(PremulRgba::new(0.2, 0.2, 0.2, 0.5));
                s.paint_over_tile_within(c, &mut px, PixelAlpha::Free, doc.selection_tile(c));
            }
        }
        let preview = ms(t) / (3 * tiles.len()) as f64;
        let mut d = doc.clone();
        let l = d.active_layer();
        let t = Instant::now();
        d.modify_tiles_in_selection(l, &tiles, |c, tile: &mut Tile<[u8; 4]>, within| {
            s.commit_tile_within(c, tile, PixelAlpha::Free, within)
        })
        .unwrap();
        let commit = ms(t) / tiles.len() as f64;
        println!("| {name} | {preview:.3} | {commit:.3} | {} |", tiles.len());
    }

    // Memory: live bytes held by the selection and by history.
    println!("\nMemory (live bytes the selection adds; history after the step).\n");
    println!("| selection | stored tiles | selection MiB | history MiB |");
    println!("|---|---:|---:|---:|");
    let mem = |name: &str, f: &dyn Fn(&mut Document)| {
        let mut d = Document::new(SIZE, SIZE);
        let before = LIVE.load(Ordering::Relaxed);
        f(&mut d);
        let held = LIVE.load(Ordering::Relaxed) - before;
        let tiles = d.selection().map_or(0, |s| s.stored_tile_count());
        println!(
            "| {name} | {tiles} | {:.2} (live delta incl. history {:.2}) | {:.2} |",
            tiles as f64 * 65536.0 / MIB,
            held as f64 / MIB,
            d.history_bytes() as f64 / MIB
        );
    };
    mem("none", &|_| {});
    mem("small rectangle", &|d| {
        let _ = d.select(&small_rect, false, SelectionOp::Replace);
    });
    mem("select all", &|d| {
        let _ = d.select_all();
    });
    mem("select all, invert", &|d| {
        d.select_all();
        d.invert_selection();
    });
    mem("large AA ellipse", &|d| {
        let _ = d.select(&ellipse(1900.0), true, SelectionOp::Replace);
    });
    mem("large ellipse feathered 50 px", &|d| {
        d.select(&ellipse(1900.0), true, SelectionOp::Replace);
        d.feather_selection(50.0);
    });
    mem("10 rectangles added", &|d| {
        for i in 0..10 {
            let o = i as f32 * 300.0;
            d.select(&rect(o, o, o + 400.0, o + 300.0), false, SelectionOp::Add);
        }
    });
    println!(
        "\npeak live bytes: {:.1} MiB",
        PEAK.load(Ordering::Relaxed) as f64 / MIB
    );
}

//! Transform costs (phase 5D): taking the source (the snapshot), one full
//! preview (every tile the transform touches composed through the
//! preview overlay, as the app does after a change), the commit, undo and
//! redo, and the memory the source, the preview and the history hold, for
//! content of 256², 2048² and 4096² pixels at both depths, without a
//! selection and through a hard and a feathered selection; moves, scales
//! (50 %, 200 %) and rotations (15°, 45°).
//!
//! ```sh
//! cargo run --release -p tela-corpus --example transform_bench            # ~3 min
//! cargo run --release -p tela-corpus --example transform_bench -- --quick # 256² and 2048² only
//! BREAKDOWN=1 …                                                         # also the per-tile function alone
//! ```
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{
    ComposeScratch, Document, Interpolation, LayerId, Overlay, OverlayPaint, SelectionOp,
    SelectionShape, Transform, TransformParams,
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

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

/// A document whose layer holds `side`² pixels of soft, coloured,
/// partly transparent content at (64, 64).
fn document(side: i32, depth: PixelDepth) -> (Document, LayerId) {
    let canvas = (side + 128) as u32;
    let mut d = Document::with_depth(canvas, canvas, depth);
    let id = d.active_layer();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(64, 64, 64 + side, 64 + side));
    with_pixel_type!(depth, P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..TILE_SIZE as i32 {
                for x in 0..TILE_SIZE as i32 {
                    let (px, py) = (ox + x - 64, oy + y - 64);
                    if !(0..side).contains(&px) || !(0..side).contains(&py) {
                        continue;
                    }
                    let a = 0.3 + 0.7 * ((px * 7 + py * 3) % 97) as f32 / 96.0;
                    let p = PremulRgba::new(
                        a * (px % 256) as f32 / 255.0,
                        a * (py % 256) as f32 / 255.0,
                        a * 0.5,
                        a,
                    );
                    *t.get_mut(x as u32, y as u32) = P::from_premul(p);
                }
            }
        })
        .unwrap();
    });
    (d, id)
}

/// A named change of the identity transform.
type Op = (&'static str, fn(&mut TransformParams));

#[derive(Clone, Copy)]
enum Sel {
    None,
    Hard,
    Feathered,
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    println!("## Transform (phase 5D)\n");
    println!(
        "Ryzen-class CPU, one thread. Content side² px of soft coloured pixels; \
         'preview' = every tile the transform touches composed through the preview \
         overlay once (what a change of the transform costs on screen); memory = \
         source snapshot (with its pyramid), preview scratch, history held after the \
         commit, and the peak above the document while committing.\n"
    );
    println!(
        "| content | bits | selection | transform | tiles | source ms | source MiB | preview ms | commit ms | undo ms | redo ms | history MiB | peak extra MiB |"
    );
    println!("|---:|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let sides: &[i32] = if quick {
        &[256, 2048]
    } else {
        &[256, 2048, 4096]
    };
    for &side in sides {
        for depth in PixelDepth::ALL {
            let sels: &[Sel] = if side == 4096 {
                &[Sel::None]
            } else {
                &[Sel::None, Sel::Hard, Sel::Feathered]
            };
            for &sel in sels {
                let ops: Vec<Op> = vec![
                    ("move 37, 21", |p| p.translate = [37.0, 21.0]),
                    ("scale 50 %", |p| p.scale = [0.5, 0.5]),
                    ("scale 200 %", |p| p.scale = [2.0, 2.0]),
                    ("rotate 15°", |p| p.rotation = 15f64.to_radians()),
                    ("rotate 45°", |p| p.rotation = 45f64.to_radians()),
                ];
                for (name, op) in ops {
                    if side == 4096 && name == "scale 200 %" {
                        continue;
                    }
                    run(side, depth, sel, name, op);
                }
            }
        }
    }
}

fn run(side: i32, depth: PixelDepth, sel: Sel, name: &str, op: fn(&mut TransformParams)) {
    let (mut d, id) = document(side, depth);
    let r = IRect::new(
        64 + side / 8,
        64 + side / 8,
        64 + side * 7 / 8,
        64 + side * 7 / 8,
    );
    match sel {
        Sel::None => {}
        Sel::Hard => {
            d.select(
                &SelectionShape::Rect([r.x0 as f32, r.y0 as f32], [r.x1 as f32, r.y1 as f32]),
                false,
                SelectionOp::Replace,
            );
        }
        Sel::Feathered => {
            d.select(
                &SelectionShape::Ellipse {
                    center: [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0],
                    radii: [r.width() as f32 / 2.0, r.height() as f32 / 3.0],
                    rotation: 0.3,
                },
                true,
                SelectionOp::Replace,
            );
            d.feather_selection(8.0);
        }
    }
    let base = live();
    PEAK.store(base, Ordering::Relaxed);

    let t = Instant::now();
    let source = d.transform_source(id).unwrap();
    let source_ms = ms(t);
    let mut p = TransformParams::identity(source.centre());
    op(&mut p);
    let transform = Transform {
        params: p,
        interpolation: Interpolation::Bicubic,
    };
    let tiles = source.affected_tiles(&p);

    // One full preview, as the app composes it.
    let mut scratch = ComposeScratch::new();
    let doc = &d;
    let src = &source;
    let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
        src.transform_pixels_tile(&transform, c, &doc.selection_tile(c), px);
    };
    let mask = |c: TileCoord, v: &mut [f32]| {
        src.transform_mask_tile(&transform, c, v);
    };
    let covers = |c: TileCoord| tiles.contains(c);
    let overlay = Overlay {
        target: id,
        paint: OverlayPaint::Layer {
            pixels: &pixels,
            mask: &mask,
        },
        covers: &covers,
    };
    let t = Instant::now();
    for c in tiles.iter() {
        std::hint::black_box(doc.compose_tile_into(c, Some(&overlay), &mut scratch));
    }
    let preview_ms = ms(t);
    if std::env::var("BREAKDOWN").is_ok() {
        let mut px = vec![PremulRgba::TRANSPARENT; (TILE_SIZE * TILE_SIZE) as usize];
        let t = Instant::now();
        for c in tiles.iter() {
            px.fill(PremulRgba::TRANSPARENT);
            src.transform_pixels_tile(&transform, c, &doc.selection_tile(c), &mut px);
        }
        eprintln!("{name}: render only {:.1} ms", ms(t));
    }
    let source_mib = source.bytes() as f64 / MIB;
    let scratch_bytes = std::mem::size_of_val(scratch.out.as_slice()) * 9;
    drop(scratch);

    let t = Instant::now();
    d.apply_transform(&source, &transform).unwrap();
    let commit_ms = ms(t);
    let peak = (PEAK.load(Ordering::Relaxed) - base) as f64 / MIB;
    drop(source);
    let history = d.history_bytes() as f64 / MIB;
    let t = Instant::now();
    d.undo().unwrap();
    let undo_ms = ms(t);
    let t = Instant::now();
    d.redo().unwrap();
    let redo_ms = ms(t);
    let _ = scratch_bytes;
    println!(
        "| {side}² | {} | {} | {name} | {} | {source_ms:.1} | {source_mib:.1} | {preview_ms:.1} | {commit_ms:.1} | {undo_ms:.2} | {redo_ms:.2} | {history:.1} | {peak:.1} |",
        depth.bits(),
        match sel {
            Sel::None => "none",
            Sel::Hard => "hard",
            Sel::Feathered => "feathered 8 px",
        },
        tiles.len(),
    );
}

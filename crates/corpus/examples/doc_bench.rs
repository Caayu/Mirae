//! Consolidated document benchmark (phase 4G): composition against layer
//! count, depth and features; full redraws of realistic sparse, partial
//! and dense documents with their memory and file costs; history costs.
//!
//! ```sh
//! cargo run --release -p tela-corpus --example doc_bench            # full, ~2 min
//! cargo run --release -p tela-corpus --example doc_bench -- --quick # gate, ~10 s
//! ```
//!
//! Exits with an error if composition allocates (invariant F3), so the
//! quick run is part of `scripts/check.ps1 -Bench`.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};
use std::time::Instant;

use glam::Vec2;
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{DirtyTiles, IRect, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, ComposeScratch, Document, LayerId, Placement};

/// Counts allocations and live bytes.
struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: same contract as our caller's; forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const MIB: f64 = 1024.0 * 1024.0;

fn allocs() -> u64 {
    ALLOCS.load(Ordering::Relaxed)
}

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn hash(x: i32, y: i32, k: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ k;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Fills `coords` of layer `id` with translucent painted content that
/// differs per layer `k` (soft bands, alpha 0 … 1).
fn fill(d: &mut Document, id: LayerId, coords: &[TileCoord], k: u32) {
    let mut tiles = DirtyTiles::new();
    for &c in coords {
        tiles.mark(c);
    }
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..256u32 {
                for x in 0..256u32 {
                    let (px, py) = (ox + x as i32, oy + y as i32);
                    let s = ((px as f32 * 0.011 + k as f32).sin() + (py as f32 * 0.017).cos()) * 0.5 + 0.5;
                    let a = (s * 0.9 + 0.1).min(1.0);
                    let n = (hash(px, py, k) % 9) as f32 / 255.0;
                    let rgb = [0.2 + 0.5 * s, 0.6 - 0.3 * s + n, 0.3 + 0.02 * k as f32 % 0.6];
                    *t.get_mut(x, y) = P::from_premul(PremulRgba::new(rgb[0].min(1.0) * a, rgb[1].min(1.0) * a, rgb[2] * a, a));
                }
            }
        })
        .unwrap();
    });
}

fn mask(d: &mut Document, id: LayerId, coords: &[TileCoord]) {
    d.add_mask(id).unwrap();
    let mut tiles = DirtyTiles::new();
    for &c in coords {
        tiles.mark(c);
    }
    d.modify_mask(id, &tiles, |c, t| {
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = ((i as i32 + c.x * 31) % 256) as u8;
        }
    })
    .unwrap();
}

#[derive(Clone, Copy, PartialEq)]
enum Config {
    Normal,
    Multiply,
    SoftLight,
    Mask,
    Clipping,
    MaskClipping,
}

impl Config {
    const ALL: [Config; 6] = [
        Config::Normal,
        Config::Multiply,
        Config::SoftLight,
        Config::Mask,
        Config::Clipping,
        Config::MaskClipping,
    ];
    fn name(self) -> &'static str {
        match self {
            Config::Normal => "Normal",
            Config::Multiply => "Multiply",
            Config::SoftLight => "Soft Light",
            Config::Mask => "Normal + mask",
            Config::Clipping => "clipping (every 2nd)",
            Config::MaskClipping => "mask + clipping",
        }
    }
}

/// `n` layers that all draw in tile (0, 0).
fn one_tile_stack(n: u32, depth: PixelDepth, config: Config) -> Document {
    let mut d = Document::with_depth(256, 256, depth);
    let c = [TileCoord { x: 0, y: 0 }];
    for k in 0..n {
        let id = if k == 0 {
            d.active_layer()
        } else {
            d.add_layer(Placement::Top).unwrap()
        };
        fill(&mut d, id, &c, k);
        match config {
            Config::Multiply => drop(d.set_blend_mode(id, BlendMode::Multiply).unwrap()),
            Config::SoftLight => drop(d.set_blend_mode(id, BlendMode::SoftLight).unwrap()),
            Config::Mask => mask(&mut d, id, &c),
            Config::Clipping if k % 2 == 1 => drop(d.set_clipped(id, true).unwrap()),
            Config::MaskClipping => {
                mask(&mut d, id, &c);
                if k % 2 == 1 {
                    d.set_clipped(id, true).unwrap();
                }
            }
            _ => {}
        }
        d.set_opacity(id, 0.85).unwrap();
    }
    d
}

/// Times `compose_tile_into` of the given tiles: median ms per tile over
/// `runs`, and the allocations it made.
fn time_compose(
    d: &Document,
    tiles: &[TileCoord],
    runs: usize,
    scratch: &mut ComposeScratch,
) -> (f64, u64) {
    let mut times = Vec::with_capacity(runs);
    let before = allocs();
    for _ in 0..runs {
        let t = Instant::now();
        for &c in tiles {
            std::hint::black_box(d.compose_tile_into(c, None, scratch));
        }
        times.push(t.elapsed().as_secs_f64() * 1000.0 / tiles.len() as f64);
    }
    // `times` was reserved before; nothing in the loop allocates unless
    // composition does.
    (median(times), allocs() - before)
}

fn layer_scaling(quick: bool, failures: &mut u64) {
    println!("### Composition cost of one tile against the layer count\n");
    println!("Every layer draws in the tile (worst case for that tile), opacity 0.85.\n");
    println!("| layers | bits | layers' mode | ms / tile | µs / layer | Mpx/s | allocations |");
    println!("|---:|---:|---|---:|---:|---:|---:|");
    let counts: &[u32] = &[1, 10, 50, 100];
    let depths: &[PixelDepth] = if quick {
        &[PixelDepth::Rgba8]
    } else {
        &PixelDepth::ALL
    };
    let runs = if quick { 3 } else { 15 };
    let mut scratch = ComposeScratch::new();
    for &depth in depths {
        for config in Config::ALL {
            if quick
                && !matches!(
                    config,
                    Config::Normal | Config::SoftLight | Config::MaskClipping
                )
            {
                continue;
            }
            for &n in counts {
                let d = one_tile_stack(n, depth, config);
                let (ms, a) = time_compose(&d, &[TileCoord { x: 0, y: 0 }], runs, &mut scratch);
                *failures += a;
                println!(
                    "| {n} | {} | {} | {ms:.3} | {:.1} | {:.0} | {a} |",
                    depth.bits(),
                    config.name(),
                    ms * 1000.0 / f64::from(n),
                    65_536.0 / (ms / 1000.0) / 1e6
                );
            }
        }
    }
    println!();
}

/// A 4096² document of `layers` layers covering `coverage` of the canvas
/// each: mostly Normal, some Multiply/Screen/Overlay, every 5th clipped,
/// every 7th masked.
fn realistic(layers: u32, coverage: f32, depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(4096, 4096, depth);
    let all: Vec<TileCoord> = IRect::from_size(4096, 4096).tiles().collect();
    for k in 0..layers {
        let id = if k == 0 {
            d.active_layer()
        } else {
            d.add_layer(Placement::Top).unwrap()
        };
        let coords: Vec<TileCoord> = all
            .iter()
            .copied()
            .filter(|c| (hash(c.x, c.y, k) % 1000) as f32 / 1000.0 < coverage)
            .collect();
        fill(&mut d, id, &coords, k);
        let mode = match k % 10 {
            3 => BlendMode::Multiply,
            6 => BlendMode::Screen,
            8 => BlendMode::Overlay,
            _ => BlendMode::Normal,
        };
        d.set_blend_mode(id, mode).unwrap();
        if k % 5 == 4 {
            d.set_clipped(id, true).unwrap();
        }
        if k % 7 == 6 {
            let some: Vec<TileCoord> = coords.iter().copied().step_by(3).collect();
            mask(&mut d, id, &some);
        }
    }
    d.set_history_limits(tela_document::HistoryLimits {
        max_steps: 0,
        max_bytes: 0,
    });
    d
}

fn pixel_bytes(d: &Document) -> (usize, usize) {
    let per = d.depth().bytes_per_pixel() * 65_536;
    let px = d.layers().map(|l| l.pixels().tile_count() * per).sum();
    let m = d
        .layers()
        .map(|l| l.mask().map_or(0, |m| m.tile_count()) * 65_536)
        .sum();
    (px, m)
}

fn realistic_documents(quick: bool, failures: &mut u64) {
    println!("### Realistic 4096² documents\n");
    println!(
        "Mostly Normal layers, every 10th of three in Multiply/Screen/Overlay, every 5th clipped, every 7th with a mask. Full redraw = every tile any visible layer draws in (opening the document, toggling a layer); stroke frame = 4 tiles (a brush update).\n"
    );
    println!(
        "| document | layers | bits | stored tiles | pixels MiB | masks MiB | full redraw ms | ms / tile | stroke frame ms | allocations | save s | load s | file MiB |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let cases: &[(&str, u32, f32)] = if quick {
        &[("sparse 5 %", 50, 0.05)]
    } else {
        &[
            ("sparse 5 %", 10, 0.05),
            ("sparse 5 %", 50, 0.05),
            ("sparse 5 %", 100, 0.05),
            ("partial 30 %", 10, 0.3),
            ("partial 30 %", 50, 0.3),
            ("dense 100 %", 10, 1.0),
        ]
    };
    let depths: &[PixelDepth] = if quick {
        &[PixelDepth::Rgba8]
    } else {
        &PixelDepth::ALL
    };
    let mut scratch = ComposeScratch::new();
    for &(name, layers, coverage) in cases {
        for &depth in depths {
            let d = realistic(layers, coverage, depth);
            let content: Vec<TileCoord> = d.content_tiles().iter().collect();
            let stored: usize = d.layers().map(|l| l.pixels().tile_count()).sum();
            let (px, m) = pixel_bytes(&d);
            let runs = if quick { 1 } else { 3 };
            let (ms, a) = time_compose(&d, &content, runs, &mut scratch);
            let frame: Vec<TileCoord> = [(7, 7), (8, 7), (7, 8), (8, 8)]
                .map(|(x, y)| TileCoord { x, y })
                .to_vec();
            let (fms, fa) = time_compose(&d, &frame, if quick { 3 } else { 9 }, &mut scratch);
            *failures += a + fa;
            let (save, load, size) = if quick {
                (0.0, 0.0, 0.0)
            } else {
                let t = Instant::now();
                let bytes = tela_io::write_document(
                    &d,
                    Cursor::new(Vec::new()),
                    &tela_io::SaveOptions::default(),
                )
                .unwrap()
                .into_inner();
                let save = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let back = tela_io::read_document(Cursor::new(&bytes), &tela_io::Limits::default())
                    .unwrap();
                let load = t.elapsed().as_secs_f64();
                assert_eq!(back.document.layer_count(), d.layer_count());
                (save, load, bytes.len() as f64 / MIB)
            };
            println!(
                "| {name} | {layers} | {} | {stored} | {:.0} | {:.0} | {:.0} | {ms:.3} | {:.2} | {} | {save:.2} | {load:.2} | {size:.0} |",
                depth.bits(),
                px as f64 / MIB,
                m as f64 / MIB,
                ms * content.len() as f64,
                fms * 4.0,
                a + fa,
            );
        }
    }
    println!();
}

fn stroke(size: f32, from: Vec2, to: Vec2) -> StrokeBuffer {
    stroke_with(BrushPreset::plain(size), from, to)
}

fn stroke_with(preset: BrushPreset, from: Vec2, to: Vec2) -> StrokeBuffer {
    let mut engine = BrushEngine::new(preset);
    let steps = (from.distance(to) / 2.0).ceil() as usize;
    engine.begin_stroke(BrushInput::from_position(from, 0.0), 1.0, 7);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        engine.add_input_at(
            BrushInput::from_position(from.lerp(to, t), f64::from(t)),
            f64::from(t),
        );
    }
    engine.end_stroke().expect("a stroke was begun")
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// A history operation; returns the tiles it changed.
type Op = Box<dyn Fn(&mut Document) -> usize>;

fn history(quick: bool) {
    println!("### History\n");
    println!(
        "4096² document, one fully painted layer (and a second for removal). Commit includes the history copy; bytes = what the history holds after the step / reserves for it.\n"
    );
    println!(
        "| operation | bits | tiles | commit ms | undo ms | redo ms | held MiB | reserved MiB |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|");
    let depths: &[PixelDepth] = if quick {
        &[PixelDepth::Rgba8]
    } else {
        &PixelDepth::ALL
    };
    let all: Vec<TileCoord> = IRect::from_size(4096, 4096).tiles().collect();
    for &depth in depths {
        let mut base = Document::with_depth(4096, 4096, depth);
        let l1 = base.active_layer();
        fill(&mut base, l1, &all, 1);
        let l2 = base.add_layer(Placement::Top).unwrap();
        fill(&mut base, l2, &all[..64], 2);
        base.set_active_layer(l1).unwrap();
        let ops: Vec<(&str, Op)> = vec![
            (
                "stroke 25 px × 1000 px",
                Box::new(move |d: &mut Document| {
                    let s = stroke(25.0, Vec2::new(500.0, 500.0), Vec2::new(1500.0, 900.0));
                    let n = s.touched().len();
                    with_pixel_type!(d.depth(), P => d.modify_tiles(l1, s.touched(), |c, t: &mut Tile<P>| s.commit_tile(c, t, PixelAlpha::Free))).unwrap();
                    n
                }),
            ),
            (
                "stroke 500 px × 3000 px",
                Box::new(move |d: &mut Document| {
                    let s = stroke(500.0, Vec2::new(300.0, 2000.0), Vec2::new(3700.0, 2200.0));
                    let n = s.touched().len();
                    with_pixel_type!(d.depth(), P => d.modify_tiles(l1, s.touched(), |c, t: &mut Tile<P>| s.commit_tile(c, t, PixelAlpha::Free))).unwrap();
                    n
                }),
            ),
            (
                "mask stroke 500 px × 3000 px",
                Box::new(move |d: &mut Document| {
                    if d.layer(l1).unwrap().mask().is_none() {
                        d.add_mask(l1).unwrap();
                    }
                    // Erasing hides: a new mask reveals everything already.
                    let mut p = BrushPreset::plain(500.0);
                    p.compositing.mode = tela_brush::preset::PaintMode::Erase;
                    let s = stroke_with(p, Vec2::new(300.0, 1000.0), Vec2::new(3700.0, 1200.0));
                    let n = s.touched().len();
                    d.modify_mask(l1, s.touched(), |c, t| s.commit_alpha_tile(c, t))
                        .unwrap();
                    n
                }),
            ),
            (
                "clear layer",
                Box::new(move |d: &mut Document| {
                    let n = d.layer(l2).unwrap().pixels().tile_count();
                    d.clear_layer(l2).unwrap();
                    n
                }),
            ),
            (
                "remove layer",
                Box::new(move |d: &mut Document| {
                    let n = d.layer(l2).unwrap().pixels().tile_count();
                    d.remove_layer(l2).unwrap();
                    n
                }),
            ),
            (
                "convert depth",
                Box::new(move |d: &mut Document| {
                    let n: usize = d.layers().map(|l| l.pixels().tile_count()).sum();
                    let to = if d.depth() == PixelDepth::Rgba8 {
                        PixelDepth::Rgba16
                    } else {
                        PixelDepth::Rgba8
                    };
                    d.convert_depth(to);
                    n
                }),
            ),
        ];
        for (name, op) in &ops {
            let mut d = base.clone();
            if name.starts_with("mask") {
                d.add_mask(l1).unwrap();
            }
            let (h0, r0) = (d.history_bytes(), d.history_reserved_bytes());
            let t = Instant::now();
            let tiles = op(&mut d);
            let commit = ms(t);
            let (h1, r1) = (d.history_bytes(), d.history_reserved_bytes());
            let t = Instant::now();
            d.undo().unwrap();
            let undo = ms(t);
            let t = Instant::now();
            d.redo().unwrap();
            let redo = ms(t);
            println!(
                "| {name} | {} | {tiles} | {commit:.1} | {undo:.2} | {redo:.2} | {:.1} | {:.1} |",
                depth.bits(),
                (h1 - h0) as f64 / MIB,
                (r1 - r0) as f64 / MIB
            );
        }
    }
    println!();
}

fn memory() {
    println!("### Memory of the building blocks\n");
    let before = live();
    let scratch = ComposeScratch::new();
    let scratch_bytes = live() - before;
    drop(scratch);
    let tile8: Tile<[u8; 4]> = Tile::new();
    let tile16: Tile<[u16; 4]> = Tile::new();
    let mask: Tile<u8> = Tile::new();
    println!("| item | bytes |");
    println!("|---|---:|");
    println!("| pixel tile, 8-bit | {} |", size_of_val(tile8.pixels()));
    println!("| pixel tile, 16-bit | {} |", size_of_val(tile16.pixels()));
    println!("| mask tile | {} |", size_of_val(mask.pixels()));
    println!("| ComposeScratch (measured) | {scratch_bytes} |");
    let empty = live();
    let mut d = Document::new(4096, 4096);
    for _ in 0..99 {
        d.add_layer(Placement::Top).unwrap();
    }
    d.set_history_limits(tela_document::HistoryLimits {
        max_steps: 0,
        max_bytes: 0,
    });
    println!(
        "| 100 empty layers (headers, names, empty maps) | {} |",
        live() - empty
    );
    println!();
}

fn main() {
    let quick = std::env::args().any(|a| a == "--quick");
    let mut failures = 0;
    memory();
    layer_scaling(quick, &mut failures);
    realistic_documents(quick, &mut failures);
    history(quick);
    if failures > 0 {
        eprintln!("composition allocated {failures} times (invariant F3)");
        std::process::exit(1);
    }
    println!("composition allocations: 0");
}

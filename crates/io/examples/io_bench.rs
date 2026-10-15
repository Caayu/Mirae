//! Save/load benchmark for `.mirae` documents: time, size, throughput and
//! peak extra memory, for small, large, sparse, 8/16-bit, masked and
//! clipped documents, and a comparison of compression levels.
//!
//! ```sh
//! cargo run --release -p tela-io --example io_bench
//! ```
//!
//! Throughput is raw pixel data (decoded tile bytes) per second. Peak
//! extra memory is the largest amount allocated during the operation
//! beyond what was live before it (save) or what the loaded document
//! holds (load), measured by a counting allocator.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, Placement};
use tela_io::{Compression, Limits, SaveOptions, load_document, save_document};

/// Tracks live and peak allocated bytes.
struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

fn grow(by: isize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: forwards to the system allocator unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grow(layout.size() as isize);
        // SAFETY: same contract as our caller's; forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        grow(new_size as isize - layout.size() as isize);
        // SAFETY: `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}

fn peak() -> isize {
    PEAK.load(Ordering::Relaxed)
}

/// How a layer's tiles are filled.
#[derive(Clone, Copy)]
enum Content {
    /// Opaque gradient with ±3 levels of pixel noise (painted texture).
    Painted,
    /// Translucent soft bands with transparent gaps (strokes).
    Strokes,
    /// One flat colour (a fill).
    Flat,
}

fn hash(x: i32, y: i32, k: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b) ^ k;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

fn pixel(content: Content, x: i32, y: i32, k: u32) -> PremulRgba {
    match content {
        Content::Painted => {
            let n = (hash(x, y, k) % 7) as f32 - 3.0;
            let v = |base: f32| ((base + n) / 255.0).clamp(0.0, 1.0);
            PremulRgba::new(
                v(40.0 + (x % 4096) as f32 / 20.0),
                v(90.0 + (y % 4096) as f32 / 30.0),
                v(120.0 + k as f32),
                1.0,
            )
        }
        Content::Strokes => {
            let s = ((x as f32 * 0.013 + k as f32).sin() + (y as f32 * 0.021).cos()) * 0.5;
            let a = (s - 0.1).clamp(0.0, 1.0);
            PremulRgba::new(0.8 * a, 0.3 * a, 0.2 * a, a)
        }
        Content::Flat => PremulRgba::new(0.9, 0.85, 0.7, 1.0),
    }
}

fn fill(
    d: &mut Document,
    id: tela_document::LayerId,
    coords: &[TileCoord],
    content: Content,
    k: u32,
) {
    let mut tiles = DirtyTiles::new();
    for &c in coords {
        tiles.mark(c);
    }
    with_pixel_type!(d.depth(), P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, oy) = c.origin();
            for y in 0..256 {
                for x in 0..256 {
                    *t.get_mut(x, y) = P::from_premul(pixel(content, ox + x as i32, oy + y as i32, k));
                }
            }
        })
        .unwrap();
    });
}

struct Spec {
    name: &'static str,
    side: u32,
    layers: u32,
    /// Fraction of each layer's tiles that hold paint.
    coverage: f32,
    masks: bool,
    clipping: bool,
}

fn build(spec: &Spec, depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(spec.side, spec.side, depth);
    let n = spec.side.div_ceil(256) as i32;
    let all: Vec<TileCoord> = (0..n)
        .flat_map(|y| (0..n).map(move |x| TileCoord { x, y }))
        .collect();
    let mut id = d.active_layer();
    for k in 0..spec.layers {
        if k > 0 {
            id = d.add_layer(Placement::Top).unwrap();
        }
        let content = [Content::Painted, Content::Strokes, Content::Flat][k as usize % 3];
        let coords: Vec<TileCoord> = all
            .iter()
            .copied()
            .filter(|c| (hash(c.x, c.y, k) % 1000) as f32 / 1000.0 < spec.coverage)
            .collect();
        fill(&mut d, id, &coords, content, k);
        if spec.masks {
            d.add_mask(id).unwrap();
            let mut tiles = DirtyTiles::new();
            for &c in &coords {
                tiles.mark(c);
            }
            d.modify_mask(id, &tiles, |c, t| {
                for (i, v) in t.pixels_mut().iter_mut().enumerate() {
                    *v = ((i as i32 % 256 + c.x * 7) % 256) as u8;
                }
            })
            .unwrap();
        }
        if spec.clipping && k % 2 == 1 {
            d.set_clipped(id, true).unwrap();
            d.set_blend_mode(id, BlendMode::Multiply).unwrap();
        }
    }
    d
}

fn raw_bytes(d: &Document) -> u64 {
    let px = d.depth().bytes_per_pixel() as u64 * 65_536;
    d.layers()
        .map(|l| {
            l.pixels().tile_count() as u64 * px
                + l.mask().map_or(0, |m| m.tile_count()) as u64 * 65_536
        })
        .sum()
}

const MIB: f64 = 1024.0 * 1024.0;

struct Measured {
    file: u64,
    save: f64,
    load: f64,
    save_peak: f64,
    load_peak: f64,
}

fn measure(d: &Document, path: &Path, compression: Compression) -> Measured {
    let options = SaveOptions {
        compression,
        ..SaveOptions::default()
    };
    let before = live();
    reset_peak();
    let t = Instant::now();
    save_document(d, path, &options).unwrap();
    let save = t.elapsed().as_secs_f64();
    let save_peak = (peak() - before) as f64 / MIB;
    let file = std::fs::metadata(path).unwrap().len();
    let before = live();
    reset_peak();
    let t = Instant::now();
    let loaded = load_document(path, &Limits::default()).unwrap();
    let load = t.elapsed().as_secs_f64();
    let kept = live() - before;
    let load_peak = (peak() - before - kept) as f64 / MIB;
    assert_eq!(loaded.document.layer_count(), d.layer_count());
    drop(loaded);
    Measured {
        file,
        save,
        load,
        save_peak,
        load_peak,
    }
}

fn main() {
    let dir = std::env::temp_dir().join(format!("mirae-io-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bench.mirae");
    let specs = [
        Spec {
            name: "small 1000² ×3",
            side: 1000,
            layers: 3,
            coverage: 0.5,
            masks: false,
            clipping: false,
        },
        Spec {
            name: "4096² ×10 full",
            side: 4096,
            layers: 10,
            coverage: 1.0,
            masks: false,
            clipping: false,
        },
        Spec {
            name: "4096² ×10 sparse 10 %",
            side: 4096,
            layers: 10,
            coverage: 0.1,
            masks: false,
            clipping: false,
        },
        Spec {
            name: "4096² ×10 full, masks + clipping",
            side: 4096,
            layers: 10,
            coverage: 1.0,
            masks: true,
            clipping: true,
        },
    ];
    println!("default compression {:?}\n", Compression::default());
    println!(
        "| document | bits | tiles | raw MiB | file MiB | ratio | save s | save MB/s | load s | load MB/s | save peak extra MiB | load peak extra MiB |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for spec in &specs {
        for depth in PixelDepth::ALL {
            let d = build(spec, depth);
            let tiles: usize = d
                .layers()
                .map(|l| l.pixels().tile_count() + l.mask().map_or(0, |m| m.tile_count()))
                .sum();
            let raw = raw_bytes(&d);
            let m = measure(&d, &path, Compression::default());
            println!(
                "| {} | {} | {tiles} | {:.0} | {:.1} | {:.2} | {:.2} | {:.0} | {:.2} | {:.0} | {:.1} | {:.1} |",
                spec.name,
                depth.bits(),
                raw as f64 / MIB,
                m.file as f64 / MIB,
                raw as f64 / m.file as f64,
                m.save,
                raw as f64 / 1e6 / m.save,
                m.load,
                raw as f64 / 1e6 / m.load,
                m.save_peak,
                m.load_peak,
            );
        }
    }
    println!("\ncompression levels (2048² × 6 layers, painted/strokes/flat, full):\n");
    println!("| bits | compression | file MiB | ratio | save s | save MB/s | load s | load MB/s |");
    println!("|---:|---|---:|---:|---:|---:|---:|---:|");
    let spec = Spec {
        name: "levels",
        side: 2048,
        layers: 6,
        coverage: 1.0,
        masks: false,
        clipping: false,
    };
    for depth in PixelDepth::ALL {
        let d = build(&spec, depth);
        let raw = raw_bytes(&d) as f64;
        for c in [
            Compression::Stored,
            Compression::Deflate { level: 1 },
            Compression::Deflate { level: 2 },
            Compression::Deflate { level: 3 },
            Compression::Deflate { level: 6 },
            Compression::Deflate { level: 9 },
        ] {
            let m = measure(&d, &path, c);
            println!(
                "| {} | {c:?} | {:.1} | {:.2} | {:.2} | {:.0} | {:.2} | {:.0} |",
                depth.bits(),
                m.file as f64 / MIB,
                raw / m.file as f64,
                m.save,
                raw / 1e6 / m.save,
                m.load,
                raw / 1e6 / m.load,
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

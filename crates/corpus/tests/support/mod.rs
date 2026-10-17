//! Shared support for the end-to-end tests (phase 4G): real brush
//! strokes committed the way the app commits them, a content digest
//! through the public API, structural checks, and save/load round trips.
#![allow(dead_code)]

use std::io::Cursor;

use glam::Vec2;
use tela_brush::dynamics::InputCaps;
use tela_brush::preset::builtin::{builtin_preset, ids};
use tela_brush::{BrushEngine, BrushInput, BrushPreset, PixelAlpha, StrokeBuffer};
use tela_canvas::{DirtyTiles, TILE_SIZE, Tile, TileCoord};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{ComposeScratch, Document, LayerPart, Overlay, OverlayPaint};
use tela_io::{Limits, SaveOptions, read_document, write_document};

/// A built-in preset by id.
pub fn preset(id: &str) -> BrushPreset {
    builtin_preset(id).expect("built-in preset")
}

/// A round brush of `size` px at `opacity`.
pub fn round(size: f32, opacity: f32) -> BrushPreset {
    let mut p = BrushPreset::plain(size);
    p.properties.opacity.base = opacity;
    p
}

pub fn eraser(size: f32) -> BrushPreset {
    let mut p = preset(ids::ERASER);
    p.properties.size.base = size;
    p
}

pub fn airbrush() -> BrushPreset {
    preset(ids::AIRBRUSH)
}

/// A straight path from `a` to `b` in `n` samples with pressure rising
/// from 0.3 to 1 (pen samples, 200 Hz).
pub fn line(a: (f32, f32), b: (f32, f32), n: usize) -> Vec<(f32, f32, f32)> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1).max(1) as f32;
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0.3 + 0.7 * t)
        })
        .collect()
}

fn input(x: f32, y: f32, pressure: f32, t: f64) -> BrushInput {
    let mut i = BrushInput::from_position(Vec2::new(x, y), t);
    i.pressure = pressure;
    i.caps = InputCaps {
        pressure: true,
        tilt: false,
        rotation: false,
    };
    i
}

/// Runs the brush engine over `path` (samples 5 ms apart) and returns the
/// finished stroke.
pub fn run_stroke(
    preset: &BrushPreset,
    color: Color,
    path: &[(f32, f32, f32)],
    seed: u64,
) -> StrokeBuffer {
    let mut engine = BrushEngine::new(preset.clone());
    engine.color = color;
    let (x, y, p) = path[0];
    engine.begin_stroke(input(x, y, p, 0.0), 1.0, seed);
    for (i, &(x, y, p)) in path.iter().enumerate().skip(1) {
        let t = i as f64 * 0.005;
        engine.add_input_at(input(x, y, p, t), t);
        if engine.is_time_based() {
            engine.advance_time(t);
        }
    }
    engine.end_stroke().expect("a stroke was begun")
}

/// What one committed stroke did.
pub struct Committed {
    pub dirty: DirtyTiles,
    /// Largest channel difference between the composite with the stroke
    /// as a preview (overlay) and after its commit, in 1/255.
    pub preview_vs_commit: u8,
}

/// Paints `path` on the document's current edit target exactly as the
/// app does: the target (layer, pixels or mask, alpha lock) is fixed when
/// the stroke begins, the preview is the overlay, the commit goes through
/// `modify_tiles` / `modify_mask`. `None` if the target refuses paint
/// (hidden, locked, disabled mask).
pub fn stroke(
    d: &mut Document,
    preset: &BrushPreset,
    color: Color,
    path: &[(f32, f32, f32)],
    seed: u64,
) -> Option<Committed> {
    let layer = d.active_layer();
    let part = d.active_part();
    d.paint_target(layer, part).ok()?;
    let locked = d.layer(layer).is_some_and(|l| l.alpha_locked());
    let alpha = if part == LayerPart::Pixels && locked {
        PixelAlpha::Preserved
    } else {
        PixelAlpha::Free
    };
    let buf = run_stroke(preset, color, path, seed);
    let touched: Vec<TileCoord> = buf.touched().iter().collect();
    // The last frame before the pen lifts: the stroke as an overlay.
    let pixels = |c: TileCoord, px: &mut [PremulRgba]| buf.paint_over_tile(c, px, alpha);
    let mask = |c: TileCoord, v: &mut [f32]| buf.paint_alpha_tile(c, v);
    let covers = |c: TileCoord| buf.touched().contains(c);
    let overlay = Overlay {
        target: layer,
        paint: match part {
            LayerPart::Pixels => OverlayPaint::Pixels(&pixels),
            LayerPart::Mask => OverlayPaint::Mask(&mask),
        },
        covers: &covers,
    };
    let preview: Vec<Option<Vec<[u8; 4]>>> = touched
        .iter()
        .map(|&c| compose(d, c, Some(&overlay)))
        .collect();
    let tiles = buf.touched().clone();
    let dirty = match part {
        LayerPart::Pixels => with_pixel_type!(d.depth(), P => {
            d.modify_tiles(layer, &tiles, |c, t: &mut Tile<P>| buf.commit_tile(c, t, alpha))
        }),
        LayerPart::Mask => d.modify_mask(layer, &tiles, |c, t| buf.commit_alpha_tile(c, t)),
    }
    .expect("the target accepted paint");
    // Compared as shown: the composite over the paper. A tile can be
    // composed over the opaque paper during the preview (a non-Normal
    // layer may draw there) and be empty after the commit (an alpha-locked
    // stroke over nothing stores nothing; the display adds the paper):
    // different tiles, the same picture.
    let paper = d.background().premultiply();
    let shown = |px: Option<Vec<[u8; 4]>>| -> Vec<[u8; 3]> {
        let n = (TILE_SIZE * TILE_SIZE) as usize;
        px.unwrap_or_else(|| vec![[0; 4]; n])
            .into_iter()
            .map(|p| {
                let o = PremulRgba::from_rgba8(p).over(paper);
                [o.r, o.g, o.b].map(tela_color::unorm8)
            })
            .collect()
    };
    let mut worst = 0;
    for (c, before) in touched.iter().zip(preview) {
        let after = compose(d, *c, None);
        match (before, after) {
            // Both composed: the composites themselves (P3).
            (Some(a), Some(b)) => {
                for (p, q) in a.iter().zip(&b) {
                    for k in 0..4 {
                        worst = worst.max(p[k].abs_diff(q[k]));
                    }
                }
            }
            // One side empty: what the display shows.
            (a, b) => {
                for (p, q) in shown(a).iter().zip(&shown(b)) {
                    for k in 0..3 {
                        worst = worst.max(p[k].abs_diff(q[k]));
                    }
                }
            }
        }
    }
    Some(Committed {
        dirty,
        preview_vs_commit: worst,
    })
}

/// FNV-1a 64.
pub struct Hash(u64);

impl Hash {
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    pub fn eat(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    pub fn finish(&self) -> u64 {
        self.0
    }
}

fn sorted(c: impl Iterator<Item = TileCoord>) -> Vec<TileCoord> {
    let mut v: Vec<_> = c.collect();
    v.sort_unstable_by_key(|c| (c.y, c.x));
    v
}

/// The stored values of a tile (the slice of `[u8; 4]` or `[u16; 4]`),
/// hashed with the standard (deterministic, fast) hasher.
fn eat_pixels<P: StoragePixel + std::hash::Hash>(h: &mut Hash, t: &Tile<P>) {
    use std::hash::Hasher;
    let mut s = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(t.pixels(), &mut s);
    h.eat(&s.finish().to_le_bytes());
}

/// Everything persistent (what a file keeps) plus the selection, through
/// the public API. Equal digests mean equal documents.
pub fn digest(d: &Document) -> u64 {
    digest_with(d, true, true)
}

/// [`digest`] without the selection (undo keeps the current selection).
pub fn content_digest(d: &Document) -> u64 {
    digest_with(d, false, true)
}

/// [`digest`] without selection and visibility (visibility is not a
/// history step, so undo does not restore it).
pub fn history_digest(d: &Document) -> u64 {
    digest_with(d, false, false)
}

fn digest_with(d: &Document, selection: bool, visibility: bool) -> u64 {
    let mut h = Hash::new();
    h.eat(&d.width().to_le_bytes());
    h.eat(&d.height().to_le_bytes());
    let p = d.background();
    for v in [p.r, p.g, p.b, p.a] {
        h.eat(&v.to_bits().to_le_bytes());
    }
    h.eat(&d.depth().bits().to_le_bytes());
    if selection {
        // Ids are never reused (L2): undoing an added layer keeps the
        // counter, so only the full digest includes it.
        h.eat(&d.next_layer_id().unwrap_or(0).to_le_bytes());
        h.eat(&d.active_layer().get().to_le_bytes());
        h.eat(&[u8::from(d.active_part() == LayerPart::Mask)]);
    }
    for l in d.layers() {
        h.eat(&l.id().get().to_le_bytes());
        h.eat(l.name().as_bytes());
        h.eat(&[
            u8::from(visibility && l.visible()),
            u8::from(l.locked()),
            u8::from(l.alpha_locked()),
            u8::from(l.clipped()),
        ]);
        h.eat(&l.opacity().to_bits().to_le_bytes());
        h.eat(l.blend_mode().name().as_bytes());
        with_pixel_type!(d.depth(), P => {
            let s = l.pixels().surface::<P>().expect("document depth");
            for c in sorted(s.tile_coords()) {
                h.eat(&c.x.to_le_bytes());
                h.eat(&c.y.to_le_bytes());
                eat_pixels(&mut h, s.tile(c).expect("listed"));
            }
        });
        if let Some(m) = l.mask() {
            h.eat(&[2, u8::from(m.enabled())]);
            for c in sorted(m.tile_coords()) {
                h.eat(&c.x.to_le_bytes());
                h.eat(&c.y.to_le_bytes());
                let v: Vec<u8> = m.reveal_tile(c).expect("listed").collect();
                h.eat(&v);
            }
        }
    }
    h.finish()
}

/// The tiles that show something or might: the canvas plus every tile any
/// layer stores.
pub fn tiles_of_interest(d: &Document) -> Vec<TileCoord> {
    let mut t = DirtyTiles::new();
    t.mark_rect(d.bounds());
    t.extend(&d.content_tiles());
    for l in d.layers() {
        for c in l.pixels().tile_coords() {
            t.mark(c);
        }
    }
    t.iter().collect()
}

/// Hash of the 8-bit composite of every tile of interest.
pub fn composite_digest(d: &Document) -> u64 {
    let mut h = Hash::new();
    for c in tiles_of_interest(d) {
        h.eat(&c.x.to_le_bytes());
        h.eat(&c.y.to_le_bytes());
        match compose(d, c, None) {
            Some(px) => px.iter().for_each(|p| h.eat(p)),
            None => h.eat(&[0xff]),
        }
    }
    h.finish()
}

/// Structural invariants and premultiplied validity, checked through the
/// public API (the document's own `check_invariants` is crate-private).
pub fn check(d: &Document) {
    let ids: Vec<u32> = d.layers().map(|l| l.id().get()).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "L1: unique ids");
    assert!(!ids.is_empty(), "L3: a layer");
    assert!(d.layer(d.active_layer()).is_some(), "L4: active exists");
    if let Some(next) = d.next_layer_id() {
        assert!(ids.iter().all(|&i| i < next), "L2: next id is fresh");
    }
    if d.active_part() == LayerPart::Mask {
        assert!(d.layer(d.active_layer()).unwrap().mask().is_some());
    }
    for l in d.layers() {
        assert!((0.0..=1.0).contains(&l.opacity()), "L5");
        assert_eq!(l.pixels().depth(), d.depth(), "DP1");
        with_pixel_type!(d.depth(), P => {
            let s = l.pixels().surface::<P>().unwrap();
            for c in s.tile_coords() {
                for p in s.tile(c).unwrap().pixels() {
                    let q = p.to_premul();
                    assert!(q.r <= q.a && q.g <= q.a && q.b <= q.a, "premultiplied: {q:?}");
                }
            }
        });
        if let Some(m) = l.mask() {
            for c in m.tile_coords() {
                assert!(m.reveal_tile(c).unwrap().any(|v| v != 255), "M3");
            }
        }
    }
    for c in tiles_of_interest(d) {
        if let Some(px) = compose(d, c, None) {
            for p in px {
                assert!(
                    p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3],
                    "composite premultiplied"
                );
            }
        }
    }
}

pub fn save_bytes(d: &Document) -> Vec<u8> {
    write_document(d, Cursor::new(Vec::new()), &SaveOptions::default())
        .expect("saves")
        .into_inner()
}

/// Saves and loads `d`, checking that nothing changed (content, composite,
/// and the bytes of a second save); returns the loaded document.
pub fn round_trip(d: &Document) -> Document {
    let bytes = save_bytes(d);
    let loaded = read_document(Cursor::new(&bytes), &Limits::default()).expect("loads");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let back = loaded.document;
    assert_eq!(digest(&back), digest(d), "save/load keeps the content");
    assert_eq!(
        composite_digest(&back),
        composite_digest(d),
        "and the picture"
    );
    assert_eq!(save_bytes(&back), bytes, "save ∘ load ∘ save = save");
    assert!(!back.can_undo(), "no history in files");
    back
}

/// SplitMix64: a reproducible stream from a seed.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// An opaque or translucent colour from a small palette.
pub fn colour(i: usize) -> Color {
    const C: [[f32; 3]; 6] = [
        [0.85, 0.2, 0.15],
        [0.1, 0.45, 0.9],
        [0.95, 0.8, 0.2],
        [0.15, 0.6, 0.3],
        [0.05, 0.05, 0.08],
        [0.9, 0.9, 0.95],
    ];
    let c = C[i % C.len()];
    Color::new(c[0], c[1], c[2])
}

pub fn depth_name(d: PixelDepth) -> u32 {
    d.bits()
}

thread_local! {
    static SCRATCH: std::cell::RefCell<ComposeScratch> = std::cell::RefCell::new(ComposeScratch::new());
}

/// `Document::compose_tile` with a reused scratch (it allocates 3.6 MB
/// per call otherwise).
pub fn compose(d: &Document, c: TileCoord, overlay: Option<&Overlay>) -> Option<Vec<[u8; 4]>> {
    SCRATCH.with(|s| {
        let mut s = s.borrow_mut();
        d.compose_tile_into(c, overlay, &mut s)
            .then(|| s.out.clone())
    })
}

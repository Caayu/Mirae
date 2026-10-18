//! Composing the live gradient preview fast enough to follow the pointer
//! (the 5F quality gate found whole-canvas previews 0.3–0.8 s behind, and
//! the far tiles never refreshed while the pointer kept moving).
//!
//! Two changes from composing tiles one by one on the UI thread, neither
//! touching what a tile contains:
//!
//! - **In parallel.** Tiles are composed on scoped threads, each with its
//!   own compose scratch and its own [`GradientPreview`] (the preview keeps
//!   a per-tile buffer and is not shared). Every tile is composed by the
//!   same function as before from the same inputs, so the result is bit for
//!   bit the sequential one (invariant GR5: the preview is the commit).
//! - **Fairly.** Tiles on screen go first, and among them the ones composed
//!   longest ago, so a pointer that keeps moving cannot starve the far
//!   tiles: every stale tile is refreshed in turn.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use tela_canvas::{DirtyTiles, TileCoord};
use tela_color::PremulRgba;
use tela_document::{
    ComposeScratch, Document, FillTarget, Gradient, GradientPaint, LayerPart, Overlay, OverlayPaint,
};

/// Tiles each worker composes between two uploads.
const PER_WORKER: usize = 2;

/// One worker's buffers, reused from frame to frame.
struct Slot {
    scratch: ComposeScratch,
    /// The tiles composed in the last batch: coordinate, whether it has
    /// content, and the index of its pixels in `outs`.
    done: Vec<(TileCoord, bool)>,
    outs: Vec<Vec<[u8; 4]>>,
}

/// The preview's workers and the age of every tile they composed.
pub(super) struct PreviewWorkers {
    slots: Vec<Slot>,
    /// The batch in which each tile was last composed.
    composed: BTreeMap<TileCoord, u64>,
    batch: u64,
}

impl PreviewWorkers {
    pub fn new() -> Self {
        let workers = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(1, 16);
        Self {
            slots: (0..workers)
                .map(|_| Slot {
                    scratch: ComposeScratch::new(),
                    done: Vec::with_capacity(PER_WORKER),
                    outs: Vec::new(),
                })
                .collect(),
            composed: BTreeMap::new(),
            batch: 0,
        }
    }

    /// Forgets the tiles' ages (a new drag starts).
    pub fn reset(&mut self) {
        self.composed.clear();
    }

    /// Orders `tiles` for composing: those on screen first, then, in each
    /// group, the ones composed longest ago (never composed first).
    pub fn order(&self, tiles: &mut [TileCoord], visible: &DirtyTiles) {
        tiles.sort_by_key(|c| {
            (
                !visible.contains(*c),
                self.composed.get(c).copied().unwrap_or(0),
            )
        });
    }

    /// Composes `tiles` in order with the gradient previewed on `target`,
    /// in batches, until `budget` has passed (at least one batch); hands
    /// every composed tile to `upload` on this thread. Returns how many of
    /// `tiles` were composed (a prefix).
    #[allow(clippy::too_many_arguments)]
    pub fn compose(
        &mut self,
        doc: &Document,
        gradient: &Gradient,
        target: FillTarget,
        paint: GradientPaint,
        tiles: &[TileCoord],
        budget: Duration,
        mut upload: impl FnMut(TileCoord, Option<&[[u8; 4]]>),
    ) -> usize {
        let started = Instant::now();
        let workers = self.slots.len();
        let mut done = 0;
        while done < tiles.len() && (done == 0 || started.elapsed() < budget) {
            let batch = &tiles[done..tiles.len().min(done + workers * PER_WORKER)];
            self.batch += 1;
            std::thread::scope(|scope| {
                for (w, slot) in self.slots.iter_mut().enumerate() {
                    let mine: Vec<TileCoord> =
                        batch.iter().skip(w).step_by(workers).copied().collect();
                    slot.done.clear();
                    if mine.is_empty() {
                        continue;
                    }
                    scope.spawn(move || compose_slot(slot, doc, gradient, target, paint, &mine));
                }
            });
            for slot in &self.slots {
                for (k, &(c, has)) in slot.done.iter().enumerate() {
                    upload(c, has.then_some(slot.outs[k].as_slice()));
                    self.composed.insert(c, self.batch);
                }
            }
            done += batch.len();
        }
        done
    }
}

/// One worker's share of a batch: its own preview (it keeps a tile buffer
/// of its own), the document's own composition, the result copied out.
fn compose_slot(
    slot: &mut Slot,
    doc: &Document,
    gradient: &Gradient,
    target: FillTarget,
    paint: GradientPaint,
    tiles: &[TileCoord],
) {
    let preview = doc.gradient_preview(target, gradient, paint);
    let pixels = |c: TileCoord, px: &mut [PremulRgba]| {
        if let Ok(p) = &preview {
            p.pixels(c, px);
        }
    };
    let mask = |c: TileCoord, r: &mut [f32]| {
        if let Ok(p) = &preview {
            p.mask(c, r);
        }
    };
    let covers = |c: TileCoord| preview.as_ref().is_ok_and(|p| p.covers(c));
    let overlay = Overlay {
        target: target.layer,
        paint: match target.part {
            LayerPart::Pixels => OverlayPaint::Pixels(&pixels),
            LayerPart::Mask => OverlayPaint::Mask(&mask),
        },
        covers: &covers,
    };
    for (k, &c) in tiles.iter().enumerate() {
        let has = doc.compose_tile_into(c, Some(&overlay), &mut slot.scratch);
        if slot.outs.len() <= k {
            slot.outs.push(Vec::new());
        }
        let out = &mut slot.outs[k];
        out.clear();
        if has {
            out.extend_from_slice(&slot.scratch.out);
        }
        slot.done.push((c, has));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tela_color::Color;
    use tela_document::{
        GradientExtend, GradientGeometry, GradientShape, GradientStop, GradientStops, Mixing,
    };

    fn gradient() -> Gradient {
        let stops = GradientStops::new([
            GradientStop::new(0.0, Color::new(1.0, 0.2, 0.0), 1.0),
            GradientStop::new(1.0, Color::new(0.0, 0.3, 1.0), 0.4),
        ])
        .unwrap();
        Gradient::new(
            GradientGeometry {
                shape: GradientShape::Radial,
                start: [300.5, 200.25],
                end: [700.0, 500.0],
            },
            &stops,
            Mixing::Perceptual,
            GradientExtend::Reflect,
            true,
        )
    }

    /// The parallel preview composes every tile exactly as one tile at a
    /// time on one thread does (GR5 is kept: same function, same inputs).
    #[test]
    fn parallel_tiles_are_the_sequential_tiles_bit_for_bit() {
        let doc = Document::new(1100, 900);
        let target = FillTarget {
            layer: doc.active_layer(),
            part: LayerPart::Pixels,
        };
        let g = gradient();
        let tiles: Vec<TileCoord> = doc
            .gradient_preview(target, &g, GradientPaint::Draw)
            .unwrap()
            .tiles()
            .iter()
            .collect();
        assert!(tiles.len() > 8);
        let mut parallel = BTreeMap::new();
        let mut workers = PreviewWorkers::new();
        let n = workers.compose(
            &doc,
            &g,
            target,
            GradientPaint::Draw,
            &tiles,
            Duration::from_secs(60),
            |c, px| {
                parallel.insert(c, px.map(<[[u8; 4]]>::to_vec));
            },
        );
        assert_eq!(n, tiles.len());
        let mut slot = Slot {
            scratch: ComposeScratch::new(),
            done: Vec::new(),
            outs: Vec::new(),
        };
        for &c in &tiles {
            slot.done.clear();
            compose_slot(&mut slot, &doc, &g, target, GradientPaint::Draw, &[c]);
            let expected = slot.done[0].1.then(|| slot.outs[0].clone());
            assert_eq!(parallel[&c], expected, "tile {c:?}");
        }
    }

    #[test]
    fn tiles_on_screen_come_first_then_the_oldest() {
        let mut workers = PreviewWorkers::new();
        let a = TileCoord { x: 0, y: 0 };
        let b = TileCoord { x: 1, y: 0 };
        let c = TileCoord { x: 2, y: 0 };
        let mut visible = DirtyTiles::new();
        visible.mark(b);
        visible.mark(c);
        workers.composed.insert(b, 5);
        let mut tiles = vec![a, b, c];
        workers.order(&mut tiles, &visible);
        assert_eq!(
            tiles,
            [c, b, a],
            "visible first; never composed before older"
        );
    }
}

//! Reordering layers (phase 5B: drag and drop, ADR 0023).
//!
//! The panel turns a drop into `Document::move_layer` with a relative
//! placement; everything else is the document's: ids and content travel
//! with the layer (LR1, LR2) and clipping groups are recomputed from the
//! new positions (LR3, the reference's rule from 4D C2 and 5B DR4–DR5).
//! Each case compares the reordered document with the same stack built
//! directly in its final order.

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, PremulRgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerId, Placement};

/// A layer: its colour (one opaque or translucent band) and whether it is
/// clipped.
#[derive(Clone, Copy)]
struct Spec {
    color: PremulRgba,
    /// Rows the band covers (`y0..y1` of an 8 × 8 area).
    rows: (u32, u32),
    clipped: bool,
    mode: BlendMode,
}

const fn spec(r: f32, g: f32, b: f32, a: f32, rows: (u32, u32), clipped: bool) -> Spec {
    Spec {
        color: PremulRgba::new(r * a, g * a, b * a, a),
        rows,
        clipped,
        mode: BlendMode::Normal,
    }
}

fn build(specs: &[Spec], depth: PixelDepth) -> (Document, Vec<LayerId>) {
    let mut d = Document::with_depth(8, 8, depth);
    let mut ids = Vec::new();
    for (k, s) in specs.iter().enumerate() {
        let id = if k == 0 {
            d.active_layer()
        } else {
            d.add_layer(Placement::Top).unwrap()
        };
        let mut t = DirtyTiles::new();
        t.mark(TileCoord { x: 0, y: 0 });
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, &t, |_, tile: &mut Tile<P>| {
            for y in s.rows.0..s.rows.1 {
                for x in 0..8 {
                    *tile.get_mut(x, y) = P::from_premul(s.color);
                }
            }
        }))
        .unwrap();
        d.set_clipped(id, s.clipped).unwrap();
        d.set_blend_mode(id, s.mode).unwrap();
        ids.push(id);
    }
    (d, ids)
}

fn composite(d: &Document) -> Vec<[u8; 4]> {
    d.compose_tile(TileCoord { x: 0, y: 0 }, None)
        .map(|t| (0..64).map(|i| t[(i / 8) * 256 + i % 8]).collect())
        .unwrap_or_default()
}

/// Layer properties and stored pixels, in stack order, by id.
fn snapshot(d: &Document) -> Vec<(LayerId, bool, BlendMode, Vec<[u8; 4]>)> {
    d.layers()
        .map(|l| {
            let px = (0..64)
                .map(|i| l.pixels().pixel_rgba8(i % 8, i / 8))
                .collect();
            (l.id(), l.clipped(), l.blend_mode(), px)
        })
        .collect()
}

const BASE: Spec = spec(0.9, 0.2, 0.1, 1.0, (0, 4), false);
const OTHER: Spec = spec(0.1, 0.3, 0.9, 0.6, (2, 8), false);
const CLIP: Spec = spec(0.1, 0.8, 0.2, 1.0, (0, 8), true);

/// A case: name, initial stack, which layer moves, where to, and the
/// final order as indices into the initial stack.
type Case = (&'static str, Vec<Spec>, usize, Place, Vec<usize>);

/// The clipping cases the reference showed.
fn cases() -> Vec<Case> {
    vec![
        // A layer dropped between a base and its clipped layer becomes
        // the clipped layer's new base (the group re-forms around it).
        (
            "new base inside a group",
            vec![BASE, CLIP, OTHER],
            2,
            Place::Below(1),
            vec![0, 2, 1],
        ),
        // The clipped layer dragged to the bottom has no base: ordinary.
        (
            "clipped to the bottom",
            vec![BASE, CLIP],
            1,
            Place::Bottom,
            vec![1, 0],
        ),
        // The clipped layer moved onto another base clips to it.
        (
            "clipped onto another base",
            vec![BASE, CLIP, OTHER],
            1,
            Place::Above(2),
            vec![0, 2, 1],
        ),
        // The base dragged above its clipped layer (4D C2): the clipped
        // layer is left without a base below, the base is ordinary.
        (
            "base above its clip",
            vec![BASE, CLIP],
            0,
            Place::Top,
            vec![1, 0],
        ),
        // Dragging a layer onto its own neighbour's far side and back.
        (
            "down and up again",
            vec![BASE, OTHER, CLIP],
            1,
            Place::Below(0),
            vec![1, 0, 2],
        ),
    ]
}

#[derive(Clone, Copy)]
enum Place {
    Above(usize),
    Below(usize),
    Top,
    Bottom,
}

#[test]
fn reordering_keeps_ids_and_content_and_regroups_clipping_by_position() {
    for depth in PixelDepth::ALL {
        for (name, stack, moved, to, order) in cases() {
            let (mut d, ids) = build(&stack, depth);
            let before = snapshot(&d);
            let placement = match to {
                Place::Above(i) => Placement::Above(ids[i]),
                Place::Below(i) => Placement::Below(ids[i]),
                Place::Top => Placement::Top,
                Place::Bottom => Placement::Bottom,
            };
            d.move_layer(ids[moved], placement).unwrap();
            // LR1, LR2: the same layers (ids, flags, pixels), in the new
            // order.
            let after = snapshot(&d);
            let want: Vec<_> = order.iter().map(|&i| before[i].clone()).collect();
            assert_eq!(after, want, "{name}: layers travel whole");
            // LR3: the picture is the stack built in that order.
            let (direct, _) = build(&order.iter().map(|&i| stack[i]).collect::<Vec<_>>(), depth);
            assert_eq!(composite(&d), composite(&direct), "{name} at {depth:?}");
            // One undo restores the original order and picture.
            let (original, _) = build(&stack, depth);
            d.undo().unwrap();
            assert_eq!(snapshot(&d), before, "{name}: undo");
            assert_eq!(composite(&d), composite(&original), "{name}: undo picture");
        }
    }
}

#[test]
fn moving_a_locked_layer_is_allowed() {
    // Reference DR7 (and 4A): locked layers can be dragged.
    let (mut d, ids) = build(&[BASE, OTHER], PixelDepth::Rgba8);
    d.set_locked(ids[0], true).unwrap();
    d.move_layer(ids[0], Placement::Top).unwrap();
    assert_eq!(
        d.layers().map(|l| l.id()).collect::<Vec<_>>(),
        vec![ids[1], ids[0]]
    );
}

#[test]
fn dropping_a_layer_where_it_already_is_records_nothing() {
    let (mut d, ids) = build(&[BASE, OTHER, CLIP], PixelDepth::Rgba8);
    let steps = d.history_len();
    // Above the layer below it, below the layer above it, onto itself.
    for p in [
        Placement::Above(ids[0]),
        Placement::Below(ids[2]),
        Placement::Above(ids[1]),
    ] {
        assert!(d.move_layer(ids[1], p).unwrap().is_empty());
    }
    assert_eq!(d.history_len(), steps);
}

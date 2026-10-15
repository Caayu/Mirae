//! Property tests of 16-bit storage and depth conversion (phase 4E,
//! ADR 0018), over random stacks built through the public API: every
//! mode, opacity, clipping, visibility and mask content.

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, Tile, TileCoord, TiledSurface};
use tela_color::{PixelDepth, Rgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerError, LayerId, Placement};

/// Side of the painted square; the rest of the tile stays empty.
const SIDE: u32 = 20;

#[derive(Clone, Debug)]
struct Spec {
    /// Straight RGBA per pixel of the square, row-major, 16-bit.
    pixels: Vec<[u16; 4]>,
    mode: usize,
    opacity: u8,
    clipped: bool,
    visible: bool,
    /// Mask values (255 shows) per pixel, if the layer has a mask.
    mask: Option<Vec<u8>>,
}

fn spec() -> impl Strategy<Value = Spec> {
    let n = (SIDE * SIDE) as usize;
    (
        prop::collection::vec(any::<[u16; 4]>(), n),
        0..BlendMode::ALL.len(),
        any::<u8>(),
        any::<bool>(),
        prop::bool::weighted(0.85),
        prop::option::of(prop::collection::vec(any::<u8>(), n)),
    )
        .prop_map(|(pixels, mode, opacity, clipped, visible, mask)| Spec {
            pixels,
            mode,
            opacity,
            clipped,
            visible,
            mask,
        })
}

fn tile() -> DirtyTiles {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    t
}

/// Premultiplies a straight 16-bit colour and stores it as `P`.
fn store<P: StoragePixel>(p: [u16; 4]) -> P {
    let [r, g, b, a] = p.map(|v| f32::from(v) / 65535.0);
    P::from_premul(Rgba::new(r, g, b, a).premultiply())
}

/// Builds the stack at `depth`; the colours are stored at that depth's
/// precision (so an 8-bit build holds 8-bit content).
fn build(specs: &[Spec], depth: PixelDepth) -> (Document, Vec<LayerId>) {
    let mut d = Document::with_depth(48, 48, depth);
    let mut ids = vec![d.active_layer()];
    for _ in 1..specs.len() {
        ids.push(d.add_layer(Placement::Top).unwrap());
    }
    for (s, &id) in specs.iter().zip(&ids) {
        with_pixel_type!(depth, P => d.modify_tiles(id, &tile(), |_, t: &mut Tile<P>| {
            for (i, p) in s.pixels.iter().enumerate() {
                *t.get_mut(i as u32 % SIDE, i as u32 / SIDE) = store::<P>(*p);
            }
        }))
        .unwrap();
        if let Some(values) = &s.mask {
            d.add_mask(id).unwrap();
            d.modify_mask(id, &tile(), |_, t| {
                for (i, v) in values.iter().enumerate() {
                    *t.get_mut(i as u32 % SIDE, i as u32 / SIDE) = *v;
                }
            })
            .unwrap();
        }
        d.set_blend_mode(id, BlendMode::ALL[s.mode]).unwrap();
        d.set_opacity(id, f32::from(s.opacity) / 255.0).unwrap();
        d.set_clipped(id, s.clipped).unwrap();
        d.set_visible(id, s.visible).unwrap();
    }
    (d, ids)
}

fn compose(d: &Document) -> Option<Vec<[u8; 4]>> {
    d.compose_tile(TileCoord { x: 0, y: 0 }, None)
}

/// One layer's id, name, visibility, opacity bits, mode, clip flag and
/// mask values.
type LayerShape = (u32, String, bool, u32, BlendMode, bool, Vec<u8>);

/// Everything public about the stack except pixels: ids, order, names,
/// properties and mask values.
fn structure(d: &Document) -> Vec<LayerShape> {
    d.layers()
        .map(|l| {
            let mask = l.mask().map_or_else(Vec::new, |m| {
                (0..SIDE as i32)
                    .flat_map(|y| (0..SIDE as i32).map(move |x| (x, y)))
                    .map(|(x, y)| m.value(x, y))
                    .collect()
            });
            (
                l.id().get(),
                l.name().to_string(),
                l.visible(),
                l.opacity().to_bits(),
                l.blend_mode(),
                l.clipped(),
                mask,
            )
        })
        .collect()
}

fn surfaces8(d: &Document) -> Vec<TiledSurface<[u8; 4]>> {
    d.layers()
        .map(|l| l.pixels().rgba8().unwrap().clone())
        .collect()
}

fn same_tiles<P: StoragePixel>(a: &[TiledSurface<P>], b: &[TiledSurface<P>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.tile_count() == b.tile_count() && a.tile_coords().all(|c| a.tile(c) == b.tile(c))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Depth changes precision, never appearance: 8-bit content widened
    /// to 16 bits composes to exactly the same display pixels, in every
    /// mode, with masks, clipping and opacity.
    #[test]
    fn widened_8_bit_stacks_compose_identically(specs in prop::collection::vec(spec(), 1..5)) {
        let (mut d, _) = build(&specs, PixelDepth::Rgba8);
        let eight = compose(&d);
        d.convert_depth(PixelDepth::Rgba16);
        prop_assert_eq!(compose(&d), eight);
    }

    /// Conversion keeps dimensions, ids, order, properties and masks; undo
    /// restores the 8-bit tiles exactly and redo the 16-bit ones; 8 → 16 →
    /// 8 is the identity.
    #[test]
    fn conversion_keeps_structure_and_undoes_exactly(specs in prop::collection::vec(spec(), 1..4)) {
        let (mut d, _) = build(&specs, PixelDepth::Rgba8);
        let (shape, eight) = (structure(&d), surfaces8(&d));
        let size = (d.width(), d.height(), d.active_layer());
        d.convert_depth(PixelDepth::Rgba16);
        prop_assert_eq!(structure(&d), shape.clone());
        prop_assert_eq!((d.width(), d.height(), d.active_layer()), size);
        let sixteen: Vec<_> = d.layers().map(|l| l.pixels().rgba16().unwrap().clone()).collect();
        d.undo().unwrap();
        prop_assert_eq!(d.depth(), PixelDepth::Rgba8);
        prop_assert!(same_tiles(&surfaces8(&d), &eight), "undo restores the 8-bit tiles");
        d.redo().unwrap();
        let again: Vec<_> = d.layers().map(|l| l.pixels().rgba16().unwrap().clone()).collect();
        prop_assert!(same_tiles(&again, &sixteen), "redo restores the 16-bit tiles");
        d.convert_depth(PixelDepth::Rgba8);
        prop_assert!(same_tiles(&surfaces8(&d), &eight), "8 -> 16 -> 8");
        prop_assert_eq!(structure(&d), shape);
    }

    /// 16 → 8 moves every stored channel by at most half an 8-bit level,
    /// and undo brings the 16-bit values back exactly.
    #[test]
    fn narrowing_is_the_nearest_level_and_undoable(specs in prop::collection::vec(spec(), 1..3)) {
        let (mut d, _) = build(&specs, PixelDepth::Rgba16);
        let sixteen: Vec<_> = d.layers().map(|l| l.pixels().rgba16().unwrap().clone()).collect();
        d.convert_depth(PixelDepth::Rgba8);
        for (l, s) in d.layers().zip(&sixteen) {
            let n = l.pixels().rgba8().unwrap();
            prop_assert_eq!(n.tile_count(), s.tile_count());
            for c in s.tile_coords() {
                for (a, b) in n.tile(c).unwrap().pixels().iter().zip(s.tile(c).unwrap().pixels()) {
                    for k in 0..4 {
                        let err = (f64::from(b[k]) / 257.0 - f64::from(a[k])).abs();
                        prop_assert!(err <= 0.5, "{:?} -> {:?}", b, a);
                    }
                    prop_assert!(a[0] <= a[3] && a[1] <= a[3] && a[2] <= a[3]);
                }
            }
        }
        d.undo().unwrap();
        let back: Vec<_> = d.layers().map(|l| l.pixels().rgba16().unwrap().clone()).collect();
        prop_assert!(same_tiles(&back, &sixteen));
    }

    /// C3 at 16 bits: composites of 16-bit stacks are valid premultiplied
    /// pixels, masks and clipping included.
    #[test]
    fn sixteen_bit_composites_stay_valid_premultiplied(specs in prop::collection::vec(spec(), 1..5)) {
        let (d, _) = build(&specs, PixelDepth::Rgba16);
        for p in compose(&d).unwrap_or_default() {
            prop_assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{:?}", p);
        }
    }

    /// L6 at both depths: an edit written for the other depth is refused
    /// and changes nothing, not even the history.
    #[test]
    fn edits_for_the_other_depth_change_nothing(specs in prop::collection::vec(spec(), 1..3), sixteen in any::<bool>()) {
        let depth = if sixteen { PixelDepth::Rgba16 } else { PixelDepth::Rgba8 };
        let (mut d, ids) = build(&specs, depth);
        let (shown, steps, shape) = (compose(&d), d.history_len(), structure(&d));
        let r = if sixteen {
            d.modify_tiles(ids[0], &tile(), |_, t: &mut Tile<[u8; 4]>| t.pixels_mut().fill([255; 4]))
        } else {
            d.modify_tiles(ids[0], &tile(), |_, t: &mut Tile<[u16; 4]>| t.pixels_mut().fill([65535; 4]))
        };
        let other = if sixteen { PixelDepth::Rgba8 } else { PixelDepth::Rgba16 };
        prop_assert_eq!(r.err(), Some(LayerError::WrongDepth { document: depth, edit: other }));
        prop_assert_eq!(compose(&d), shown);
        prop_assert_eq!(d.history_len(), steps);
        prop_assert_eq!(structure(&d), shape);
    }
}

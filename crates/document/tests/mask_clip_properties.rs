//! Property tests of composition with masks and clipping (phase 4D,
//! ADR 0017), over random stacks built through the public API: every mode,
//! opacity, clipping, visibility and mask content.

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, TileCoord};
use tela_color::Rgba;
use tela_document::{BlendMode, Document, LayerId, Placement};

/// Side of the painted square; the rest of the tile stays empty.
const SIDE: u32 = 24;

#[derive(Clone, Debug)]
struct Spec {
    /// Straight RGBA per pixel of the square, row-major.
    pixels: Vec<[u8; 4]>,
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
        prop::collection::vec(any::<[u8; 4]>(), n),
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

/// Builds the stack; `masks` false leaves every mask out, `enabled` false
/// creates them disabled.
fn build(specs: &[Spec], masks: bool, enabled: bool) -> (Document, Vec<LayerId>) {
    let mut d = Document::new(64, 64);
    let mut ids = vec![d.active_layer()];
    for _ in 1..specs.len() {
        ids.push(d.add_layer(Placement::Top).unwrap());
    }
    for (s, &id) in specs.iter().zip(&ids) {
        d.modify_tiles(id, &tile(), |_, t| {
            for (i, p) in s.pixels.iter().enumerate() {
                let [r, g, b, a] = p.map(|v| f32::from(v) / 255.0);
                let (x, y) = (i as u32 % SIDE, i as u32 / SIDE);
                *t.get_mut(x, y) = Rgba::new(r, g, b, a).premultiply().to_rgba8();
            }
        })
        .unwrap();
        if let (true, Some(values)) = (masks, &s.mask) {
            d.add_mask(id).unwrap();
            d.modify_mask(id, &tile(), |_, t| {
                for (i, v) in values.iter().enumerate() {
                    *t.get_mut(i as u32 % SIDE, i as u32 / SIDE) = *v;
                }
            })
            .unwrap();
            d.set_mask_enabled(id, enabled).unwrap();
        }
        d.set_blend_mode(id, BlendMode::ALL[s.mode]).unwrap();
        d.set_opacity(id, f32::from(s.opacity) / 255.0).unwrap();
        d.set_clipped(id, s.clipped).unwrap();
        d.set_visible(id, s.visible).unwrap();
    }
    (d, ids)
}

fn compose(d: &Document) -> Vec<[u8; 4]> {
    d.compose_tile(TileCoord { x: 0, y: 0 }, None)
        .unwrap_or_else(|| vec![[0; 4]; 256 * 256])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// M2: a disabled mask is exactly no mask, in every mode, with and
    /// without clipping.
    #[test]
    fn disabled_masks_equal_absent_masks(specs in prop::collection::vec(spec(), 1..5)) {
        let (without, _) = build(&specs, false, true);
        let (disabled, _) = build(&specs, true, false);
        prop_assert_eq!(compose(&without), compose(&disabled));
    }

    /// C3 extended: every composite is a valid premultiplied pixel
    /// (colour never above alpha), masks and clipping included.
    #[test]
    fn composites_stay_valid_premultiplied(specs in prop::collection::vec(spec(), 1..5)) {
        let (d, _) = build(&specs, true, true);
        for p in compose(&d) {
            prop_assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{:?}", p);
        }
    }

    /// CL1: clipped layers never add coverage: hiding every clipped layer
    /// that has a base leaves the composite's alpha unchanged (± 1/255).
    #[test]
    fn clipped_layers_never_change_coverage(specs in prop::collection::vec(spec(), 1..5)) {
        let (mut d, ids) = build(&specs, true, true);
        let shown = compose(&d);
        let mut based = false;
        for (s, id) in specs.iter().zip(&ids) {
            if s.clipped && based {
                d.set_visible(*id, false).unwrap();
            }
            based |= !s.clipped;
        }
        let hidden = compose(&d);
        for (a, b) in shown.iter().zip(&hidden) {
            prop_assert!(a[3].abs_diff(b[3]) <= 1, "{:?} vs {:?}", a, b);
        }
    }

    /// M5 / CL4 at the composite level: undoing every step of building the
    /// stack and redoing them gives the same pixels.
    #[test]
    fn undo_redo_restores_the_composite(specs in prop::collection::vec(spec(), 1..4)) {
        let (mut d, _) = build(&specs, true, true);
        let before = compose(&d);
        let steps = d.history_len();
        for _ in 0..steps {
            d.undo().unwrap();
        }
        for _ in 0..steps {
            d.redo().unwrap();
        }
        prop_assert_eq!(compose(&d), before);
    }
}

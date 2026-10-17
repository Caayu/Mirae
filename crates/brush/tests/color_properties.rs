//! Property tests of coloured paint (phase 5A): the eraser and mask
//! strokes ignore the colour (COL3, COL4), coloured paint keeps pixels
//! valid premultiplied (COL7). Alpha lock with random colours (COL5) is
//! `properties.rs::preserved_alpha_never_changes_alpha*`.

use glam::Vec2;
use proptest::prelude::*;
use tela_brush::preset::PaintMode;
use tela_brush::{Dab, DabSink, PixelAlpha, StampContext, StrokeBuffer};
use tela_canvas::{Tile, TileCoord};
use tela_color::{Color, StoragePixel};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 300,
        ..ProptestConfig::default()
    }
}

/// One stroke of two dabs over the corner of tile (0, 0).
fn stroke(color: Color, mode: PaintMode, opacity: f32, flow: f32, hardness: f32) -> StrokeBuffer {
    let mut s = StrokeBuffer::new(
        color,
        StampContext {
            anti_aliasing: true,
            mode,
            ..Default::default()
        },
    );
    for center in [Vec2::new(5.0, 5.0), Vec2::new(8.5, 6.0)] {
        s.dab(&Dab {
            center,
            radius: 4.0,
            opacity,
            flow,
            hardness,
            ..Dab::default()
        });
    }
    s
}

/// A tile whose 16 × 16 corner holds `pixels` (made valid premultiplied).
fn tile<P: StoragePixel>(pixels: &[[u8; 4]]) -> Tile<P> {
    let mut t: Tile<P> = Tile::new();
    for (i, &[r, g, b, a]) in pixels.iter().enumerate() {
        *t.get_mut(i as u32 % 16, i as u32 / 16) = P::from_rgba8([r.min(a), g.min(a), b.min(a), a]);
    }
    t
}

fn color() -> impl Strategy<Value = Color> {
    any::<[u8; 3]>().prop_map(Color::from_rgb8)
}

proptest! {
    #![proptest_config(config())]

    /// COL3: erasing gives the same pixels whatever the paint colour, at
    /// both depths and with alpha free or preserved.
    #[test]
    fn erasing_ignores_the_colour(
        pixels in prop::collection::vec(any::<[u8; 4]>(), 256),
        a in color(),
        b in color(),
        opacity in 0.05f32..=1.0,
        flow in 0.05f32..=1.0,
        hardness in 0.0f32..=1.0,
        lock in any::<bool>(),
    ) {
        let alpha = if lock { PixelAlpha::Preserved } else { PixelAlpha::Free };
        let c = TileCoord { x: 0, y: 0 };
        let (sa, sb) = (
            stroke(a, PaintMode::Erase, opacity, flow, hardness),
            stroke(b, PaintMode::Erase, opacity, flow, hardness),
        );
        let (mut t8a, mut t8b) = (tile::<[u8; 4]>(&pixels), tile::<[u8; 4]>(&pixels));
        sa.commit_tile(c, &mut t8a, alpha);
        sb.commit_tile(c, &mut t8b, alpha);
        prop_assert!(t8a == t8b);
        let (mut t16a, mut t16b) = (tile::<[u16; 4]>(&pixels), tile::<[u16; 4]>(&pixels));
        sa.commit_tile(c, &mut t16a, alpha);
        sb.commit_tile(c, &mut t16b, alpha);
        prop_assert!(t16a == t16b);
    }

    /// COL4: a stroke on a mask changes it by its alpha only: red, green,
    /// blue or any colour with the same coverage give the same mask.
    #[test]
    fn mask_strokes_ignore_the_colour(
        values in prop::collection::vec(any::<u8>(), 256),
        a in color(),
        opacity in 0.05f32..=1.0,
        flow in 0.05f32..=1.0,
        erase in any::<bool>(),
    ) {
        let mode = if erase { PaintMode::Erase } else { PaintMode::Paint };
        let c = TileCoord { x: 0, y: 0 };
        let mut results = Vec::new();
        for color in [a, Color::new(1.0, 0.0, 0.0), Color::new(0.0, 1.0, 0.0), Color::new(0.0, 0.0, 1.0)] {
            let mut t: Tile<u8> = Tile::new();
            for (i, v) in values.iter().enumerate() {
                *t.get_mut(i as u32 % 16, i as u32 / 16) = *v;
            }
            stroke(color, mode, opacity, flow, 0.5).commit_alpha_tile(c, &mut t);
            results.push(t);
        }
        prop_assert!(results.windows(2).all(|w| w[0] == w[1]));
    }

    /// COL7: coloured paint keeps every stored pixel valid premultiplied
    /// (colour never above alpha) at both depths.
    #[test]
    fn colored_paint_stays_valid_premultiplied(
        pixels in prop::collection::vec(any::<[u8; 4]>(), 256),
        color in color(),
        opacity in 0.0f32..=1.0,
        flow in 0.01f32..=1.0,
        hardness in 0.0f32..=1.0,
    ) {
        let c = TileCoord { x: 0, y: 0 };
        let s = stroke(color, PaintMode::Paint, opacity, flow, hardness);
        let mut t8 = tile::<[u8; 4]>(&pixels);
        s.commit_tile(c, &mut t8, PixelAlpha::Free);
        for p in t8.pixels() {
            prop_assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
        }
        let mut t16 = tile::<[u16; 4]>(&pixels);
        s.commit_tile(c, &mut t16, PixelAlpha::Free);
        for p in t16.pixels() {
            prop_assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3], "{p:?}");
        }
    }
}

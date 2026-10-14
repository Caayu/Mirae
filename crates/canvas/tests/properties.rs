//! Property tests for tile addressing and the view transform.

use glam::Vec2;
use proptest::prelude::*;
use tela_canvas::{IRect, TILE_SIZE, TileCoord, ViewTransform};

/// Pixel coordinates far from i32 overflow (documents are < 2^24 px).
fn coord() -> impl Strategy<Value = i32> {
    -(1 << 24)..(1 << 24)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every pixel belongs to exactly the tile `containing` returns,
    /// including negative coordinates.
    #[test]
    fn containing_tile_holds_the_pixel(x in coord(), y in coord()) {
        let c = TileCoord::containing(x, y);
        let r = c.rect();
        prop_assert!(x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1, "{c:?} {r:?}");
        prop_assert_eq!(r.width(), TILE_SIZE as i32);
    }

    /// `IRect::tiles` lists exactly the tiles a rectangle overlaps: each one
    /// intersects it, and every corner pixel's tile is listed.
    #[test]
    fn rect_tiles_cover_exactly(
        x0 in coord(), y0 in coord(), w in 0i32..2000, h in 0i32..2000,
    ) {
        let r = IRect::new(x0, y0, x0 + w, y0 + h);
        let tiles: Vec<TileCoord> = r.tiles().collect();
        if r.is_empty() {
            prop_assert!(tiles.is_empty());
        } else {
            prop_assert!(tiles.iter().all(|t| !t.rect().intersect(&r).is_empty()));
            for (px, py) in [(r.x0, r.y0), (r.x1 - 1, r.y0), (r.x0, r.y1 - 1), (r.x1 - 1, r.y1 - 1)] {
                prop_assert!(tiles.contains(&TileCoord::containing(px, py)));
            }
        }
    }

    /// Screen -> canvas -> screen is the identity for any view, and zooming
    /// or rotating at an anchor keeps the canvas point under it fixed.
    #[test]
    fn view_round_trips_and_pins_the_anchor(
        pan in (-5000.0f32..5000.0, -5000.0f32..5000.0),
        zoom in ViewTransform::MIN_ZOOM..ViewTransform::MAX_ZOOM,
        rotation in 0.0f32..std::f32::consts::TAU,
        p in (0.0f32..4000.0, 0.0f32..3000.0),
        factor in 0.5f32..2.0,
        delta in -1.0f32..1.0,
    ) {
        let mut v = ViewTransform { pan: Vec2::from(pan), zoom, rotation };
        let p = Vec2::from(p);
        let back = v.to_screen(v.to_canvas(p));
        // f32 pipeline: tolerance relative to the magnitudes involved.
        let tol = 1e-3 * (1.0 + p.length() + v.pan.length());
        prop_assert!(back.distance(p) < tol, "{back} vs {p}");
        let under = v.to_canvas(p);
        v.zoom_at(p, factor);
        v.rotate_at(p, delta);
        let moved = v.to_screen(under);
        prop_assert!(moved.distance(p) < tol, "anchor drifted to {moved}");
    }
}

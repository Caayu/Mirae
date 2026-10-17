//! Property tests of gradients: the painted pixels against an independent
//! model written out directly in `f64` (geometry, the reference's easing,
//! straight colour and alpha, transparent stops), and the invariants
//! GR1–GR12 on random documents, selections, alpha locks, targets,
//! depths, shapes, stops and paints, across a tile border.

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, IRect, TILE_SIZE, Tile, TileCoord};
use tela_color::gradient::{GradientStop, GradientStops, Mixing};
use tela_color::{Color, PixelDepth, PremulRgba, StoragePixel, with_pixel_type};

use crate::{
    Document, FillTarget, Gradient, GradientExtend, GradientGeometry, GradientPaint, GradientShape,
    LayerId, LayerPart, SelectionOp, SelectionShape,
};

/// The document straddles the border between tiles 0 and 1.
const W: i32 = 300;
const H: i32 = 20;

const SHAPES: [GradientShape; 5] = [
    GradientShape::Linear,
    GradientShape::Radial,
    GradientShape::Elliptical,
    GradientShape::Angular,
    GradientShape::Reflected,
];
const EXTENDS: [GradientExtend; 4] = [
    GradientExtend::Clamp,
    GradientExtend::Repeat,
    GradientExtend::Reflect,
    GradientExtend::Empty,
];
const MIXINGS: [Mixing; 3] = [Mixing::Standard, Mixing::Linear, Mixing::Perceptual];

#[derive(Clone, Debug)]
struct Spec {
    shape: GradientShape,
    extend: GradientExtend,
    mixing: Mixing,
    start: [f64; 2],
    end: [f64; 2],
    /// Position, RGB, opacity (some exactly 0).
    stops: Vec<(f32, [f32; 3], f32)>,
    dither: bool,
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        0..SHAPES.len(),
        0..EXTENDS.len(),
        0..MIXINGS.len(),
        (-60.0f64..360.0, -30.0f64..50.0),
        (-60.0f64..360.0, -30.0f64..50.0),
        prop::collection::vec(
            (
                0.0f32..=1.0,
                prop::array::uniform3(0.0f32..=1.0),
                prop_oneof![Just(0.0f32), Just(1.0f32), 0.0f32..=1.0],
            ),
            1..5,
        ),
        any::<bool>(),
    )
        .prop_map(|(s, e, m, a, b, stops, dither)| Spec {
            shape: SHAPES[s],
            extend: EXTENDS[e],
            mixing: MIXINGS[m],
            start: [a.0, a.1],
            end: [b.0, b.1],
            stops,
            dither,
        })
}

impl Spec {
    fn stops(&self) -> GradientStops {
        GradientStops::new(
            self.stops
                .iter()
                .map(|&(p, c, o)| GradientStop::new(p, Color::new(c[0], c[1], c[2]), o)),
        )
        .expect("at least one stop")
    }

    fn gradient(&self) -> Gradient {
        Gradient::new(
            GradientGeometry {
                shape: self.shape,
                start: self.start,
                end: self.end,
            },
            &self.stops(),
            self.mixing,
            self.extend,
            self.dither,
        )
    }
}

/// Random content: up to four rectangles of random premultiplied colours.
fn content() -> impl Strategy<Value = Vec<(i32, i32, i32, i32, [u8; 4])>> {
    prop::collection::vec(
        (0..W, 0..H, 1..80i32, 1..H, any::<[u8; 3]>(), any::<u8>()).prop_map(
            |(x, y, w, h, c, a)| {
                let p = c.map(|v| (u16::from(v) * u16::from(a) / 255) as u8);
                (x, y, w, h, [p[0], p[1], p[2], a])
            },
        ),
        0..4,
    )
}

fn document(rects: &[(i32, i32, i32, i32, [u8; 4])], depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(W as u32, H as u32, depth);
    let id = d.active_layer();
    for &(x, y, w, h, p) in rects {
        let r = IRect::new(x, y, (x + w).min(W), (y + h).min(H));
        let mut tiles = DirtyTiles::new();
        tiles.mark_rect(r);
        with_pixel_type!(depth, P => {
            d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
                let (ox, oy) = c.origin();
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        if (ox..ox + 256).contains(&x) {
                            *t.get_mut((x - ox) as u32, (y - oy) as u32) = P::from_rgba8(p);
                        }
                    }
                }
            })
            .unwrap();
        });
    }
    (d, id)
}

fn select(d: &mut Document, sel: Option<(u8, u8, u8, u8, bool)>) {
    if let Some((x, y, w, h, feather)) = sel {
        let (x, y) = (f32::from(x) * 2.0, f32::from(y));
        d.select(
            &SelectionShape::Rect([x, y], [x + f32::from(w) * 2.0, y + f32::from(h)]),
            false,
            SelectionOp::Replace,
        );
        if feather {
            d.feather_selection(3.0);
        }
    }
}

/// Every stored pixel of `id` (decoded) and, when it has one, its mask.
fn snapshot(d: &Document, id: LayerId) -> (Vec<PremulRgba>, Vec<u8>) {
    let l = d.layer(id).unwrap();
    let px = (0..W * H).map(|i| l.pixels().pixel(i % W, i / W)).collect();
    let mask = (0..W * H)
        .map(|i| {
            let (x, y) = (i % W, i / W);
            l.mask().map_or(255, |m| {
                let c = TileCoord::containing(x, y);
                let (ox, oy) = c.origin();
                m.reveal_tile(c).map_or(255, |mut t| {
                    t.nth(((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize)
                        .unwrap()
                })
            })
        })
        .collect();
    (px, mask)
}

// --- The independent model ---------------------------------------------------

fn model_ease(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    let q = if u < 0.5 {
        2.0 * u * u
    } else {
        1.0 - 2.0 * (1.0 - u) * (1.0 - u)
    };
    0.38 * u + 0.62 * q
}

/// Straight colour and alpha of the stops at `t`, Standard mixing.
fn model_ramp(stops: &[(f32, [f32; 3], f32)], t: f64) -> ([f64; 3], f64) {
    let mut s: Vec<(f64, [f64; 3], f64)> = stops
        .iter()
        .map(|&(p, c, o)| (f64::from(p), c.map(f64::from), f64::from(o)))
        .collect();
    s.sort_by(|a, b| a.0.total_cmp(&b.0));
    let t = t.clamp(0.0, 1.0);
    let next = s.iter().position(|v| v.0 > t);
    match next {
        None => (s[s.len() - 1].1, s[s.len() - 1].2),
        Some(0) => (s[0].1, s[0].2),
        Some(i) => {
            let (a, b) = (s[i - 1], s[i]);
            let e = model_ease((t - a.0) / (b.0 - a.0));
            let alpha = a.2 + (b.2 - a.2) * e;
            let c = if b.2 == 0.0 {
                a.1
            } else if a.2 == 0.0 {
                b.1
            } else {
                [0, 1, 2].map(|k| a.1[k] + (b.1[k] - a.1[k]) * e)
            };
            (c, alpha)
        }
    }
}

/// The position at the centre of pixel `(x, y)`, or `None` where the
/// model does not decide (anti-aliased seams and edges).
fn model_t(s: &Spec, x: i32, y: i32) -> Option<f64> {
    let (dx, dy) = (s.end[0] - s.start[0], s.end[1] - s.start[1]);
    let len = dx.hypot(dy);
    let raw = |cx: f64, cy: f64| -> f64 {
        let (px, py) = (cx - s.start[0], cy - s.start[1]);
        match s.shape {
            GradientShape::Linear => (px * dx + py * dy) / (len * len),
            GradientShape::Reflected => ((px * dx + py * dy) / (len * len)).abs(),
            GradientShape::Radial => px.hypot(py) / len,
            GradientShape::Elliptical => (px / dx).hypot(py / dy),
            GradientShape::Angular => {
                let a = py.atan2(px) - dy.atan2(dx);
                a.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU
            }
        }
    };
    let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
    let t = raw(cx, cy);
    // How much `t` changes over a pixel: the model decides only where a
    // wrap, a seam or an edge (all anti-aliased) is more than that away.
    let w = (raw(cx + 1.0, cy) - t)
        .abs()
        .max((raw(cx, cy + 1.0) - t).abs())
        .max((raw(cx - 1.0, cy) - t).abs())
        .max((raw(cx, cy - 1.0) - t).abs());
    let near = |v: f64| v.rem_euclid(1.0) < w || v.rem_euclid(1.0) > 1.0 - w;
    if s.shape == GradientShape::Angular {
        let r = (cx - s.start[0]).hypot(cy - s.start[1]);
        if near(t) || w > 0.25 || r < 4.0 {
            return None;
        }
        return Some(if s.extend == GradientExtend::Reflect {
            1.0 - (2.0 * t - 1.0).abs()
        } else {
            t
        });
    }
    match s.extend {
        GradientExtend::Clamp => Some(t),
        GradientExtend::Reflect => {
            let r = t.rem_euclid(2.0);
            Some(if r > 1.0 { 2.0 - r } else { r })
        }
        GradientExtend::Repeat => (!near(t) && w < 0.25).then(|| t.rem_euclid(1.0)),
        GradientExtend::Empty => (t > w && t < 1.0 - w).then_some(t),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn the_paint_matches_an_independent_model(s in spec()) {
        // Standard mixing on an empty layer without selection: the stored
        // premultiplied colour is the model's within a level (dither or
        // rounding).
        let s = Spec { mixing: Mixing::Standard, ..s };
        let g = s.gradient();
        prop_assume!(!g.is_empty());
        // Too short a drag makes `t` change faster than a level per pixel;
        // the model and Mirae only differ in rounding there, as everywhere.
        let (mut d, id) = document(&[], PixelDepth::Rgba8);
        let target = FillTarget { layer: id, part: LayerPart::Pixels };
        d.apply_gradient(target, &g, GradientPaint::Draw).unwrap();
        let p = d.layer(id).unwrap().pixels();
        for y in 0..H {
            for x in (0..W).step_by(3) {
                let Some(t) = model_t(&s, x, y) else { continue };
                let (c, a) = model_ramp(&s.stops, t);
                let want = [c[0] * a, c[1] * a, c[2] * a, a].map(|v| v * 255.0);
                let got = p.pixel_rgba8(x, y);
                for k in 0..4 {
                    let err = (f64::from(got[k]) - want[k]).abs();
                    prop_assert!(err <= 1.0 + 1e-3, "{s:?} ({x}, {y}) channel {k}: {} vs {}", got[k], want[k]);
                }
            }
        }
    }

    #[test]
    fn gradients_keep_every_invariant(
        s in spec(),
        rects in content(),
        sel in prop::option::of((0u8..140, 0u8..20, 1u8..60, 1u8..20, any::<bool>())),
        locked in any::<bool>(),
        mask in any::<bool>(),
        erase in any::<bool>(),
        sixteen in any::<bool>(),
    ) {
        let depth = if sixteen { PixelDepth::Rgba16 } else { PixelDepth::Rgba8 };
        let (mut d, id) = document(&rects, depth);
        if mask {
            d.add_mask(id).unwrap();
        }
        d.set_alpha_locked(id, locked).unwrap();
        select(&mut d, sel);
        let part = if mask { LayerPart::Mask } else { LayerPart::Pixels };
        let target = FillTarget { layer: id, part };
        let paint = if erase { GradientPaint::Erase } else { GradientPaint::Draw };
        let g = s.gradient();
        let twin = d.clone();
        let before = d.digest();
        let revision = d.revision();
        let steps = d.history_len();
        let (old_px, old_mask) = snapshot(&d, id);

        // GR2, GR3: previewing (and dropping the preview: a cancel)
        // changes nothing. GR5: the preview is what the commit stores.
        let previewed: Vec<(TileCoord, Vec<PremulRgba>, Vec<f32>)> = {
            let p = d.gradient_preview(target, &g, paint).unwrap();
            p.tiles().iter().map(|c| {
                let (ox, oy) = c.origin();
                let mut px: Vec<PremulRgba> = (0..(TILE_SIZE * TILE_SIZE) as i32)
                    .map(|i| d.layer(id).unwrap().pixels().pixel(ox + i % 256, oy + i / 256))
                    .collect();
                let mut reveal: Vec<f32> = (0..(TILE_SIZE * TILE_SIZE) as usize)
                    .map(|i| {
                        d.layer(id).unwrap().mask().map_or(1.0, |m| {
                            m.reveal_tile(c).map_or(1.0, |mut t| f32::from(t.nth(i).unwrap()) / 255.0)
                        })
                    })
                    .collect();
                if mask { p.mask(c, &mut reveal) } else { p.pixels(c, &mut px) }
                (c, px, reveal)
            }).collect()
        };
        prop_assert_eq!(&d.digest(), &before);
        prop_assert_eq!(d.revision(), revision);

        let dirty = d.apply_gradient(target, &g, paint).unwrap();
        let after = d.digest();
        // GR4: one step, or none when nothing changed.
        if after == before {
            prop_assert_eq!(d.history_len(), steps);
            prop_assert!(dirty.is_empty());
        } else {
            prop_assert_eq!(d.history_len(), steps + 1);
        }
        d.check_invariants();
        for (c, px, reveal) in &previewed {
            let (ox, oy) = c.origin();
            for i in 0..(TILE_SIZE * TILE_SIZE) as i32 {
                let (x, y) = (ox + i % 256, oy + i / 256);
                if mask {
                    let m = d.layer(id).unwrap().mask().unwrap();
                    let v = m.reveal_tile(*c).map_or(1.0, |mut t| f32::from(t.nth(i as usize).unwrap()) / 255.0);
                    prop_assert_eq!(reveal[i as usize], v, "mask ({}, {})", x, y);
                } else {
                    prop_assert_eq!(px[i as usize], d.layer(id).unwrap().pixels().pixel(x, y), "({}, {})", x, y);
                }
            }
        }
        let (new_px, new_mask) = snapshot(&d, id);
        for i in 0..(W * H) as usize {
            let (x, y) = (i as i32 % W, i as i32 / W);
            // GR6: no change where the selection covers nothing.
            if d.selection_coverage(x, y) == 0 && d.has_selection() {
                prop_assert_eq!(new_px[i], old_px[i]);
                prop_assert_eq!(new_mask[i], old_mask[i]);
            }
            // GR7: drawing with the alpha lock keeps every alpha.
            if locked && !mask && !erase {
                prop_assert_eq!(new_px[i].a, old_px[i].a, "alpha at ({}, {})", x, y);
            }
            // GR8, GR9 (the part not targeted never changes).
            if mask {
                prop_assert_eq!(new_px[i], old_px[i]);
            } else {
                prop_assert_eq!(new_mask[i], old_mask[i]);
            }
            // GR11: every stored pixel is premultiplied-valid.
            let p = new_px[i];
            prop_assert!(p.r <= p.a && p.g <= p.a && p.b <= p.a && p.a <= 1.0, "{p:?}");
        }
        // GR1: the same input gives the same document.
        let mut again = twin.clone();
        again.apply_gradient(target, &g, paint).unwrap();
        prop_assert_eq!(&again.digest(), &after);
        // GR12: undo and redo are exact.
        if after != before {
            d.undo().unwrap();
            prop_assert_eq!(&d.digest(), &before);
            d.redo().unwrap();
            prop_assert_eq!(&d.digest(), &after);
        }
        // GR10: a refused gradient changes nothing (a hidden layer).
        let mut hidden = twin;
        hidden.set_visible(id, false).unwrap();
        let h = hidden.digest();
        let n = hidden.history_len();
        prop_assert!(hidden.apply_gradient(target, &g, paint).is_err());
        prop_assert_eq!(hidden.digest(), h);
        prop_assert_eq!(hidden.history_len(), n);
    }

    #[test]
    fn mask_gradients_ignore_the_colours(s in spec(), recolour in prop::array::uniform3(0.0f32..=1.0)) {
        // GR8: two gradients that differ only in their colours reveal the
        // same mask values.
        let mut d = Document::new(W as u32, H as u32);
        let id = d.active_layer();
        d.add_mask(id).unwrap();
        let target = FillTarget { layer: id, part: LayerPart::Mask };
        let mut other = d.clone();
        d.apply_gradient(target, &s.gradient(), GradientPaint::Draw).unwrap();
        let mut t = s.clone();
        for stop in &mut t.stops {
            stop.1 = recolour;
        }
        other.apply_gradient(target, &t.gradient(), GradientPaint::Draw).unwrap();
        prop_assert_eq!(snapshot(&d, id).1, snapshot(&other, id).1);
    }

    #[test]
    fn eight_and_sixteen_bits_agree(s in spec(), rects in content()) {
        // GR9: the same gradient at both depths agrees within an 8-bit
        // level (the 16-bit one is never rounded through 8 bits; dithering
        // happens at each depth's own level).
        let s = Spec { dither: false, ..s };
        let g = s.gradient();
        let (mut a, ia) = document(&rects, PixelDepth::Rgba8);
        let (mut b, ib) = document(&rects, PixelDepth::Rgba16);
        let t = |id| FillTarget { layer: id, part: LayerPart::Pixels };
        a.apply_gradient(t(ia), &g, GradientPaint::Draw).unwrap();
        b.apply_gradient(t(ib), &g, GradientPaint::Draw).unwrap();
        for y in 0..H {
            for x in 0..W {
                let p8 = a.layer(ia).unwrap().pixels().pixel_rgba8(x, y);
                let p16 = b.layer(ib).unwrap().pixels().pixel(x, y);
                let q = [p16.r, p16.g, p16.b, p16.a];
                for k in 0..4 {
                    prop_assert!((f32::from(p8[k]) - q[k] * 255.0).abs() <= 1.0 + 1e-3, "({x}, {y}) {p8:?} {p16:?}");
                }
            }
        }
    }
}

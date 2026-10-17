//! Property tests of fill: the span flood against a plain breadth-first
//! flood over a pixel array (an independent implementation of the region
//! rule), and the invariants FL1–FL10 on random images, selections, seeds,
//! tolerances, paints and alpha locks, across a tile border.

use std::collections::VecDeque;

use proptest::prelude::*;
use tela_canvas::{DirtyTiles, IRect, Tile};
use tela_color::{Color, PixelDepth, PremulRgba, Rgba, StoragePixel, with_pixel_type};

use crate::fill::{distance, quantize, seed_color};
use crate::{
    ConnectedFill, Document, FillPaint, FillReference, FillTarget, LayerId, LayerPart, SelectionOp,
    SelectionShape, Tolerance,
};

/// The image area straddles the border between tiles 0 and 1.
const W: i32 = 300;
const H: i32 = 20;
const X0: i32 = 240;
const IW: i32 = 32;

#[derive(Clone, Debug)]
struct Image {
    palette: Vec<[u8; 4]>,
    pixels: Vec<u8>,
}

fn image() -> impl Strategy<Value = Image> {
    (
        prop::collection::vec(
            (
                any::<[u8; 3]>(),
                prop_oneof![Just(0u8), Just(255u8), any::<u8>()],
            )
                .prop_map(|(c, a)| [c[0], c[1], c[2], a]),
            1..5,
        ),
        prop::collection::vec(any::<u8>(), (IW * H) as usize),
    )
        .prop_map(|(palette, pixels)| Image { palette, pixels })
}

/// A selection: none, a rectangle, or a feathered rectangle.
fn selection() -> impl Strategy<Value = Option<(u8, u8, u8, u8, bool)>> {
    prop::option::of((0u8..60, 0u8..20, 1u8..60, 1u8..20, any::<bool>()))
}

/// A document holding `img` (straight colours, premultiplied into the
/// storage) at `(X0, 0)`, at `depth`.
fn document(img: &Image, depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(W as u32, H as u32, depth);
    let id = d.active_layer();
    let mut tiles = DirtyTiles::new();
    tiles.mark_rect(IRect::new(X0, 0, X0 + IW, H));
    with_pixel_type!(depth, P => {
        d.modify_tiles(id, &tiles, |c, t: &mut Tile<P>| {
            let (ox, _) = c.origin();
            for (i, &k) in img.pixels.iter().enumerate() {
                let (x, y) = (X0 + i as i32 % IW, i as i32 / IW);
                if (ox..ox + 256).contains(&x) {
                    let p = img.palette[usize::from(k) % img.palette.len()];
                    let s = Rgba::new(
                        f32::from(p[0]) / 255.0,
                        f32::from(p[1]) / 255.0,
                        f32::from(p[2]) / 255.0,
                        f32::from(p[3]) / 255.0,
                    );
                    *t.get_mut((x - ox) as u32, y as u32) = P::from_premul(s.premultiply());
                }
            }
        })
        .unwrap();
    });
    (d, id)
}

fn select(d: &mut Document, sel: Option<(u8, u8, u8, u8, bool)>) {
    if let Some((x, y, w, h, feather)) = sel {
        let (x, y) = (f32::from(x) + 220.0, f32::from(y));
        d.select(
            &SelectionShape::Rect([x, y], [x + f32::from(w), y + f32::from(h)]),
            false,
            SelectionOp::Replace,
        );
        if feather {
            d.feather_selection(2.0);
        }
    }
}

/// The region and edge by breadth-first search over a pixel array: the
/// rule written out directly (BK5–BK16), sharing only the colour distance.
fn naive(d: &Document, id: LayerId, fill: &ConnectedFill) -> Vec<f32> {
    let mut out = vec![0.0f32; (W * H) as usize];
    let (sx, sy) = fill.seed;
    if !(0..W).contains(&sx) || !(0..H).contains(&sy) || d.selection_coverage(sx, sy) == 0 {
        return out;
    }
    let px = |x: i32, y: i32| d.layer(id).unwrap().pixels().pixel(x, y);
    let seed = seed_color(px(sx, sy));
    let t = u32::from(fill.tolerance.threshold8()) * 257;
    let dist = |x: i32, y: i32| -> u32 {
        if d.selection_coverage(x, y) == 0 {
            65535
        } else {
            u32::from(quantize(distance(seed, px(x, y))))
        }
    };
    let mut inside = vec![false; (W * H) as usize];
    let mut queue = VecDeque::from([(sx, sy)]);
    inside[(sy * W + sx) as usize] = true;
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if (0..W).contains(&nx) && (0..H).contains(&ny) {
                let i = (ny * W + nx) as usize;
                if !inside[i] && dist(nx, ny) <= t {
                    inside[i] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    for y in 0..H {
        for x in 0..W {
            let i = (y * W + x) as usize;
            if inside[i] {
                out[i] = 1.0;
            } else if fill.anti_alias {
                let touches =
                    [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                        .iter()
                        .any(|&(nx, ny)| {
                            (0..W).contains(&nx)
                                && (0..H).contains(&ny)
                                && inside[(ny * W + nx) as usize]
                        });
                if touches {
                    out[i] = (65535 - dist(x, y)) as f32 / (65535 - t) as f32;
                }
            }
        }
    }
    out
}

fn layer_pixels(d: &Document, id: LayerId) -> Vec<PremulRgba> {
    (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .map(|(x, y)| d.layer(id).unwrap().pixels().pixel(x, y))
        .collect()
}

fn fill(seed: (u8, u8), tenths: u16, anti_alias: bool) -> ConnectedFill {
    ConnectedFill {
        seed: (X0 - 4 + i32::from(seed.0) % (IW + 8), i32::from(seed.1) % H),
        reference: FillReference::Target,
        tolerance: Tolerance::from_tenths(tenths),
        anti_alias,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn the_flood_matches_a_breadth_first_search(
        img in image(),
        sel in selection(),
        seed in any::<(u8, u8)>(),
        tenths in prop_oneof![Just(0u16), Just(1000u16), 0u16..=1000],
        aa in any::<bool>(),
    ) {
        let (mut d, id) = document(&img, PixelDepth::Rgba8);
        select(&mut d, sel);
        let f = fill(seed, tenths, aa);
        let target = FillTarget { layer: id, part: LayerPart::Pixels };
        let region = d.fill_region(target, &f).unwrap();
        let expected = naive(&d, id, &f);
        for y in 0..H {
            for x in 0..W {
                let (ours, theirs) = (region.coverage(x, y), expected[(y * W + x) as usize]);
                prop_assert!((ours - theirs).abs() < 1e-6, "({x}, {y}): {ours} vs {theirs}");
            }
        }
        // The seed belongs to the region whenever it is in the domain.
        let (sx, sy) = f.seed;
        prop_assert_eq!(region.contains(sx, sy), expected[(sy * W + sx) as usize] >= 1.0);
        // Tolerance 0 takes only the seed's exact colour.
        if tenths == 0 && region.contains(sx, sy) {
            let p = |x, y| d.layer(id).unwrap().pixels().pixel_rgba8(x, y);
            for y in 0..H {
                for x in 0..W {
                    if region.contains(x, y) {
                        let (a, b) = (p(x, y), p(sx, sy));
                        prop_assert!(a == b || (a[3] == 0 && b[3] == 0));
                    }
                }
            }
        }
    }

    #[test]
    fn fills_keep_every_invariant(
        img in image(),
        sel in selection(),
        seed in any::<(u8, u8)>(),
        tenths in 0u16..=1000,
        aa in any::<bool>(),
        colour in prop::option::of(any::<[u8; 3]>()),
        locked in any::<bool>(),
        sixteen in any::<bool>(),
    ) {
        let depth = if sixteen { PixelDepth::Rgba16 } else { PixelDepth::Rgba8 };
        let (mut d, id) = document(&img, depth);
        select(&mut d, sel);
        d.set_alpha_locked(id, locked).unwrap();
        let target = FillTarget { layer: id, part: LayerPart::Pixels };
        let paint = colour.map_or(FillPaint::Erase, |c| FillPaint::Color(Color::from_rgb8(c)));
        let f = fill(seed, tenths, aa);
        let region = d.fill_region(target, &f).unwrap();
        let before = layer_pixels(&d, id);
        let digest = d.digest();
        let steps = d.history_len();
        // Deterministic: the same fill on a copy gives the same document.
        let mut twin = d.clone();
        let dirty = d.fill_connected(target, &f, paint).unwrap();
        twin.fill_connected(target, &f, paint).unwrap();
        prop_assert_eq!(d.digest(), twin.digest());
        let after = layer_pixels(&d, id);
        for y in 0..H {
            for x in 0..W {
                let i = (y * W + x) as usize;
                let (a, b) = (before[i], after[i]);
                if a != b {
                    // FL1, FL3: only the region, its edge, and selected pixels.
                    prop_assert!(region.coverage(x, y) > 0.0, "({x}, {y}) outside the region");
                    prop_assert!(d.selection_coverage(x, y) > 0, "({x}, {y}) unselected");
                    prop_assert!(dirty.contains(tela_canvas::TileCoord::containing(x, y)));
                }
                // FL4: alpha lock keeps alpha (erase is not stopped, as the eraser).
                if locked && colour.is_some() {
                    prop_assert_eq!(a.a, b.a);
                }
                // Valid premultiplied.
                prop_assert!(b.r <= b.a + 1e-6 && b.g <= b.a + 1e-6 && b.b <= b.a + 1e-6);
            }
        }
        d.check_invariants();
        // FL7: one step (or none for a no-op), undone and redone exactly.
        if d.history_len() > steps {
            prop_assert_eq!(d.history_len(), steps + 1);
            let filled = d.digest();
            d.undo().unwrap();
            prop_assert_eq!(d.digest(), digest.clone());
            d.redo().unwrap();
            prop_assert_eq!(d.digest(), filled);
        } else {
            prop_assert_eq!(d.digest(), digest.clone());
            prop_assert!(dirty.is_empty());
        }
        // FL6: a refused fill changes nothing.
        let mut hidden = d.clone();
        hidden.set_visible(id, false).unwrap();
        let (hd, hs) = (hidden.digest(), hidden.history_len());
        prop_assert!(hidden.fill_connected(target, &f, paint).is_err());
        prop_assert_eq!((hidden.digest(), hidden.history_len()), (hd, hs));
    }

    #[test]
    fn a_full_selection_fills_like_no_selection(
        img in image(),
        seed in any::<(u8, u8)>(),
        tenths in 0u16..=1000,
        aa in any::<bool>(),
    ) {
        let (mut free, id) = document(&img, PixelDepth::Rgba8);
        let mut all = free.clone();
        all.select_all();
        let target = FillTarget { layer: id, part: LayerPart::Pixels };
        let f = fill(seed, tenths, aa);
        let paint = FillPaint::Color(Color::from_rgb8([10, 200, 90]));
        free.fill_connected(target, &f, paint).unwrap();
        all.fill_connected(target, &f, paint).unwrap();
        prop_assert_eq!(layer_pixels(&free, id), layer_pixels(&all, id));
    }

    #[test]
    fn mask_fills_ignore_the_colour(
        img in image(),
        seed in any::<(u8, u8)>(),
        tenths in 0u16..=1000,
        a in any::<[u8; 3]>(),
        b in any::<[u8; 3]>(),
    ) {
        let (mut d, id) = document(&img, PixelDepth::Rgba8);
        d.add_mask(id).unwrap();
        let mask = FillTarget { layer: id, part: LayerPart::Mask };
        // Give the mask some structure from the image: hide by alpha.
        let mut f = fill(seed, 300, false);
        f.reference = FillReference::Composite;
        d.fill_connected(mask, &f, FillPaint::Erase).unwrap();
        let f = fill(seed, tenths, true);
        let mut x = d.clone();
        let mut y = d.clone();
        x.fill_connected(mask, &f, FillPaint::Color(Color::from_rgb8(a))).unwrap();
        y.fill_connected(mask, &f, FillPaint::Color(Color::from_rgb8(b))).unwrap();
        prop_assert_eq!(x.digest(), y.digest());
    }

    #[test]
    fn eight_and_sixteen_bits_agree(
        img in image(),
        seed in any::<(u8, u8)>(),
        tenths in 0u16..=1000,
        aa in any::<bool>(),
        colour in any::<[u8; 3]>(),
    ) {
        // Opaque or transparent content: translucent colours on the
        // threshold may classify differently at 8 bits (D-5E-1).
        let mut img = img;
        for p in &mut img.palette {
            p[3] = if p[3] < 128 { 0 } else { 255 };
        }
        let target = |id| FillTarget { layer: id, part: LayerPart::Pixels };
        let f = fill(seed, tenths, aa);
        let paint = FillPaint::Color(Color::from_rgb8(colour));
        let mut out = Vec::new();
        for depth in PixelDepth::ALL {
            let (mut d, id) = document(&img, depth);
            d.fill_connected(target(id), &f, paint).unwrap();
            out.push((0..H).flat_map(|y| (0..W).map(move |x| (x, y)))
                .map(|(x, y)| d.layer(id).unwrap().pixels().pixel_rgba8(x, y))
                .collect::<Vec<_>>());
        }
        for (a, b) in out[0].iter().zip(&out[1]) {
            for c in 0..4 {
                prop_assert!(a[c].abs_diff(b[c]) <= 1, "{:?} vs {:?}", a, b);
            }
        }
    }
}

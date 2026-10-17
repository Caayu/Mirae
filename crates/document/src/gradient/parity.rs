//! The reference's measured gradients (parity record 5F), reproduced by
//! the document. Data: `tests/reference/gradient/` (see its README):
//! decoded captures (premultiplied RGBA8) of gradients the reference drew
//! on our probe documents.
//!
//! The reference's results are dithered (its own pattern, GD26) and its
//! dither is biased by about half a level (GD27), so most comparisons use
//! means: per column for linear gradients, per whole distance for radial
//! ones, which average its dither out; Mirae draws without dithering
//! here. A press at document pixel `p` is the point `p + ½` in the
//! reference (GD14), which evaluates colours at a pixel's corner (GD16):
//! for the colour, its gradients start at `p + 1` in Mirae's frame
//! (pixel centres), and that is where these tests place them (D-5F-2).

use tela_canvas::{IRect, TILE_SIZE, Tile, TiledSurface};
use tela_color::gradient::{GradientStop, GradientStops, Mixing};
use tela_color::{Color, PixelDepth, StoragePixel, with_pixel_type};

use super::*;
use crate::{Document, FillTarget, LayerId, LayerPart, LayerPixels, SelectionOp, SelectionShape};

const W: u32 = 400;
const H: u32 = 300;

fn png(name: &str) -> Vec<[u8; 4]> {
    let path = format!(
        "{}/tests/reference/gradient/{name}.png",
        env!("CARGO_MANIFEST_DIR")
    );
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut buf).unwrap();
    assert_eq!((frame.width, frame.height), (W, H));
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    buf[..frame.buffer_size()].as_chunks::<4>().0.to_vec()
}

/// The position `u` whose eased value is `e` (bisection).
fn unease(e: f64) -> f64 {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..50 {
        let m = (lo + hi) / 2.0;
        if f64::from(tela_color::gradient::ease(m as f32)) < e {
            lo = m;
        } else {
            hi = m;
        }
    }
    lo
}

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0)
}

fn blue() -> Color {
    Color::new(0.0, 0.0, 1.0)
}

/// A 400 × 300 document whose layer holds `before` (decoded premultiplied
/// RGBA8, as the captures store it) or nothing.
fn document(before: Option<&str>, depth: PixelDepth) -> (Document, LayerId) {
    let mut d = Document::with_depth(W, H, depth);
    if let Some(name) = before {
        let px = png(name);
        let pixels = with_pixel_type!(depth, P => {
            let mut s = TiledSurface::<P>::new();
            for (i, p) in px.into_iter().enumerate() {
                if p[3] > 0 {
                    s.set_pixel((i as u32 % W) as i32, (i as u32 / W) as i32, P::from_rgba8(p));
                }
            }
            LayerPixels::from_surface(s)
        });
        let (id, _) = d
            .add_layer_with_pixels(crate::Placement::Top, "before", pixels)
            .unwrap();
        return (d, id);
    }
    let id = d.active_layer();
    (d, id)
}

fn target(id: LayerId) -> FillTarget {
    FillTarget {
        layer: id,
        part: LayerPart::Pixels,
    }
}

fn draw(d: &mut Document, id: LayerId, g: &Gradient) -> Vec<[u8; 4]> {
    d.apply_gradient(target(id), g, GradientPaint::Draw)
        .unwrap();
    read(d, id)
}

fn read(d: &Document, id: LayerId) -> Vec<[u8; 4]> {
    let p = d.layer(id).unwrap().pixels();
    (0..W * H)
        .map(|i| p.pixel_rgba8((i % W) as i32, (i / W) as i32))
        .collect()
}

/// A gradient from the reference's press `p0` to its release `p1`
/// (document pixels), in Mirae's frame (see the module docs).
fn from_press(
    shape: GradientShape,
    p0: (f64, f64),
    p1: (f64, f64),
    stops: &GradientStops,
    mixing: Mixing,
    extend: GradientExtend,
) -> Gradient {
    Gradient::new(
        GradientGeometry {
            shape,
            start: [p0.0 + 1.0, p0.1 + 1.0],
            end: [p1.0 + 1.0, p1.1 + 1.0],
        },
        stops,
        mixing,
        extend,
        false,
    )
}

/// Per column: the mean of channel `ch` over rows `y0..y1`.
fn columns(px: &[[u8; 4]], ch: usize, y0: u32, y1: u32) -> Vec<f64> {
    (0..W)
        .map(|x| {
            (y0..y1)
                .map(|y| f64::from(px[(y * W + x) as usize][ch]))
                .sum::<f64>()
                / f64::from(y1 - y0)
        })
        .collect()
}

/// Per whole distance from `(cx, cy)` (pixel centres): the mean of
/// channel `ch`.
fn rings(px: &[[u8; 4]], ch: usize, cx: f64, cy: f64) -> Vec<f64> {
    let mut sum = vec![0.0; 600];
    let mut n = vec![0.0; 600];
    for y in 0..H {
        for x in 0..W {
            let r = (f64::from(x) + 0.5 - cx).hypot(f64::from(y) + 0.5 - cy) as usize;
            sum[r] += f64::from(px[(y * W + x) as usize][ch]);
            n[r] += 1.0;
        }
    }
    sum.iter()
        .zip(&n)
        .map(|(s, n)| if *n > 0.0 { s / n } else { 0.0 })
        .collect()
}

/// Prints a comparison's result when `TELA_PARITY_REPORT` is set (the
/// numbers quoted in the parity record).
fn check(what: &str, err: f64, tolerance: f64) {
    if std::env::var_os("TELA_PARITY_REPORT").is_some() {
        eprintln!("parity 5F: {what}: {err:.2} (tolerance {tolerance})");
    }
    assert!(err <= tolerance, "{what}: {err:.2} > {tolerance}");
}

/// The largest difference over `range` (and where).
fn worst(a: &[f64], b: &[f64], range: std::ops::Range<usize>) -> (f64, usize) {
    range
        .map(|i| ((a[i] - b[i]).abs(), i))
        .fold((0.0, 0), |m, v| if v.0 > m.0 { v } else { m })
}

#[test]
fn standard_linear_and_oklab_mixing_match_the_reference() {
    // GD6–GD8: black → white and red → blue, drag 100 → 300, in the three
    // modes Mirae shares with the reference. Column means of every row.
    // Tolerance: the reference's dither bias (≈ ½ level, GD27) plus
    // Mirae's rounding (½ level) plus the easing fit.
    let cases = [
        ("std_bw", Mixing::Standard, Color::BLACK, Color::WHITE),
        ("lps_bw", Mixing::Linear, Color::BLACK, Color::WHITE),
        ("pps_bw", Mixing::Perceptual, Color::BLACK, Color::WHITE),
        ("std_rb", Mixing::Standard, red(), blue()),
        ("lps_rb", Mixing::Linear, red(), blue()),
        ("pps_rb", Mixing::Perceptual, red(), blue()),
    ];
    for depth in PixelDepth::ALL {
        for (name, mixing, a, b) in cases {
            let reference = png(name);
            let g = from_press(
                GradientShape::Linear,
                (100.0, 150.0),
                (300.0, 150.0),
                &GradientStops::two(a, b),
                mixing,
                GradientExtend::Clamp,
            );
            let (mut d, id) = document(None, depth);
            let mirae = draw(&mut d, id, &g);
            for ch in 0..4 {
                let (err, x) = worst(
                    &columns(&reference, ch, 0, H),
                    &columns(&mirae, ch, 0, H),
                    0..W as usize,
                );
                check(&format!("{depth:?} {name} channel {ch} at x {x}"), err, 1.6);
            }
        }
    }
}

#[test]
fn three_undithered_stops_match_within_two_levels() {
    // GD10, GD27: red, (255, 128, 128) at 50, white; Padrão; no dither.
    // The reference's integers against Mirae's, pixel by pixel: within two
    // levels (the colour phase of D-5F-2 on the steepest part), equal on
    // two pixels of three.
    let stops = GradientStops::new([
        GradientStop::new(0.0, red(), 1.0),
        GradientStop::new(0.5, Color::from_rgb8([255, 128, 128]), 1.0),
        GradientStop::new(1.0, Color::WHITE, 1.0),
    ])
    .unwrap();
    let reference = png("ms_rbw_std_nodither");
    for depth in PixelDepth::ALL {
        let g = from_press(
            GradientShape::Linear,
            (100.0, 150.0),
            (300.0, 150.0),
            &stops,
            Mixing::Standard,
            GradientExtend::Clamp,
        );
        let (mut d, id) = document(None, depth);
        let mirae = draw(&mut d, id, &g);
        let mut exact = 0;
        let mut worst_px = 0;
        for (i, (r, m)) in reference.iter().zip(&mirae).enumerate() {
            for ch in 0..4 {
                worst_px = worst_px.max(r[ch].abs_diff(m[ch]));
            }
            exact += usize::from(r == m);
            let _ = i;
        }
        check(
            &format!("{depth:?} three stops, per pixel"),
            f64::from(worst_px),
            2.0,
        );
        let share = exact as f64 / reference.len() as f64;
        check(
            &format!("{depth:?} three stops, share of pixels not equal"),
            1.0 - share,
            0.5,
        );
    }
}

#[test]
fn edges_repeat_and_reverse_match_the_reference() {
    let bw = GradientStops::two(Color::BLACK, Color::WHITE);
    // GD14: the region of "nothing beyond" is the press's pixel centres:
    // the start at 100.5 in Mirae's frame. Alpha exactly; colour within
    // the half pixel of D-5F-2.
    let reference = png("g_nodraw");
    let g = Gradient::new(
        GradientGeometry {
            shape: GradientShape::Linear,
            start: [100.5, 150.5],
            end: [300.5, 150.5],
        },
        &bw,
        Mixing::Standard,
        GradientExtend::Empty,
        false,
    );
    let (mut d, id) = document(None, PixelDepth::Rgba8);
    let mirae = draw(&mut d, id, &g);
    let (err, x) = worst(
        &columns(&reference, 3, 0, H),
        &columns(&mirae, 3, 0, H),
        0..W as usize,
    );
    check(&format!("empty extend alpha at x {x}"), err, 1.6);
    let (err, x) = worst(
        &columns(&reference, 0, 0, H),
        &columns(&mirae, 0, 0, H),
        0..W as usize,
    );
    // The colour sits ≈ 0.7 px later in the reference (GD16): at most
    // 0.75 px × 2.05 levels/px (the steepest slope) + 1 level of dither
    // bias and rounding.
    check(&format!("empty extend colour at x {x}"), err, 2.6);
    // GD15: at 400 % the region is [181.73, 221.73].
    let reference = png("z400_nodraw");
    let g = Gradient::new(
        GradientGeometry {
            shape: GradientShape::Linear,
            start: [181.73, 150.5],
            end: [221.73, 150.5],
        },
        &bw,
        Mixing::Standard,
        GradientExtend::Empty,
        false,
    );
    let (mut d, id) = document(None, PixelDepth::Rgba8);
    let mirae = draw(&mut d, id, &g);
    let (err, x) = worst(
        &columns(&reference, 3, 0, H),
        &columns(&mirae, 3, 0, H),
        0..W as usize,
    );
    check(&format!("empty extend alpha at x {x}"), err, 1.6);
    // GD20, GD21: repeat and reverse of a 150 → 250 drag. The reverse is
    // symmetric about the presses' centres in the reference (mirrored
    // ramps cancel its colour phase), so it is compared with Mirae's
    // gradient at those centres.
    for (name, extend, shift) in [
        ("g_repeat", GradientExtend::Repeat, 1.0),
        ("g_reverse", GradientExtend::Reflect, 0.5),
    ] {
        let reference = png(name);
        let g = Gradient::new(
            GradientGeometry {
                shape: GradientShape::Linear,
                start: [150.0 + shift, 150.5],
                end: [250.0 + shift, 150.5],
            },
            &bw,
            Mixing::Standard,
            extend,
            false,
        );
        let (mut d, id) = document(None, PixelDepth::Rgba8);
        let mirae = draw(&mut d, id, &g);
        let (r, m) = (columns(&reference, 0, 0, H), columns(&mirae, 0, 0, H));
        // The wrap pixels (x ≡ 50 mod 100) blend both sides in both
        // (the reference 125.5, Mirae by area); compared apart.
        // A 100-pixel ramp is steep (up to 4.1 levels/px), so the claim is
        // in pixels: each column mean turned back into the position it
        // encodes (as the circles).
        let ramp = (0..W as usize).filter(|x| extend != GradientExtend::Repeat || x % 100 != 50);
        let err = ramp
            .filter(|&x| (8.0..247.0).contains(&r[x]) && (8.0..247.0).contains(&m[x]))
            .map(|x| (unease(r[x] / 255.0) - unease(m[x] / 255.0)).abs() * 100.0)
            .fold(0.0, f64::max);
        // Reverse mirrors the reference's colour phase (GD16) on every
        // other segment: up to twice the phase there.
        let tol = if extend == GradientExtend::Reflect {
            1.25
        } else {
            1.0
        };
        check(&format!("{name}, position, pixels"), err, tol);
        if extend == GradientExtend::Repeat {
            // Both wrap within one pixel of x = 50 + 100k: the reference's
            // wrap pixel is blended (its boundary at the press's centre),
            // Mirae's boundary falls between two pixels (the colour phase
            // of D-5F-2).
            for x in [50, 150, 250, 350] {
                for (who, v) in [("reference", &r), ("Mirae", &m)] {
                    assert!(
                        v[x - 1] - v[x + 1] > 200.0,
                        "{name}: {who} does not wrap at {x}"
                    );
                }
            }
        }
    }
}

#[test]
fn diagonal_circle_and_ellipse_match_the_reference() {
    let bw = GradientStops::two(Color::BLACK, Color::WHITE);
    // GD17: the diagonal drag, compared along its isolines' means
    // (pixels grouped by x + y).
    let reference = png("g_diag");
    let g = from_press(
        GradientShape::Linear,
        (100.0, 50.0),
        (300.0, 250.0),
        &bw,
        Mixing::Standard,
        GradientExtend::Clamp,
    );
    let (mut d, id) = document(None, PixelDepth::Rgba8);
    let mirae = draw(&mut d, id, &g);
    let diag = |px: &[[u8; 4]]| -> Vec<f64> {
        let mut s = vec![0.0; (W + H) as usize];
        let mut n = vec![0.0; (W + H) as usize];
        for y in 0..H {
            for x in 0..W {
                s[(x + y) as usize] += f64::from(px[(y * W + x) as usize][0]);
                n[(x + y) as usize] += 1.0;
            }
        }
        s.iter().zip(&n).map(|(s, n)| s / n).collect()
    };
    let (err, k) = worst(&diag(&reference), &diag(&mirae), 0..(W + H) as usize);
    check(&format!("diagonal at x + y = {k}"), err, 1.6);
    // GD18: the circle's centre is the press (the pixel's centre), its
    // radius min(|dx|, |dy|) in the reference (Mirae's radial gradient
    // takes the radius from the drag's length: the same circle is drawn
    // here). Compared as positions: each ring's mean is turned back into
    // the distance it encodes (the inverse of the easing), so the claim
    // is in pixels whatever the ramp's steepness; the reference's ramp
    // runs ≈ 0.7 px late (GD16, D-5F-2).
    for (name, centre, radius) in [
        ("g_circle", (150.0, 100.0), 100.0),
        ("g_circle2", (200.0, 150.0), 40.0),
    ] {
        let reference = png(name);
        let g = Gradient::new(
            GradientGeometry {
                shape: GradientShape::Radial,
                start: [centre.0 + 0.5, centre.1 + 0.5],
                end: [centre.0 + 0.5 + radius, centre.1 + 0.5],
            },
            &bw,
            Mixing::Standard,
            GradientExtend::Clamp,
            false,
        );
        let (mut d, id) = document(None, PixelDepth::Rgba8);
        let mirae = draw(&mut d, id, &g);
        let (cx, cy) = (centre.0 + 0.5, centre.1 + 0.5);
        let (a, b) = (rings(&reference, 0, cx, cy), rings(&mirae, 0, cx, cy));
        let mut err: f64 = 0.0;
        for r in 1..radius as usize {
            if (8.0..247.0).contains(&a[r]) && (8.0..247.0).contains(&b[r]) {
                let shift = (unease(a[r] / 255.0) - unease(b[r] / 255.0)) * radius;
                err = err.max(shift.abs());
            }
        }
        check(&format!("{name}: ring position, pixels"), err, 1.0);
    }
    // GD19: the ellipse with radii 100 × 50, unrotated: per row and column
    // through its centre.
    // As positions too (see the circles), centred on the press.
    let reference = png("g_ellipse");
    let g = Gradient::new(
        GradientGeometry {
            shape: GradientShape::Elliptical,
            start: [200.5, 150.5],
            end: [300.5, 200.5],
        },
        &bw,
        Mixing::Standard,
        GradientExtend::Clamp,
        false,
    );
    let (mut d, id) = document(None, PixelDepth::Rgba8);
    let mirae = draw(&mut d, id, &g);
    let position = |v: u8, radius: f64| unease(f64::from(v) / 255.0) * radius;
    let mut err: f64 = 0.0;
    let row = (100..300)
        .map(|x| (150 * W + x) as usize)
        .map(|i| (i, 100.0));
    let column = (100..200)
        .map(|y| (y * W + 200) as usize)
        .map(|i| (i, 50.0));
    for (i, radius) in row.chain(column) {
        let (r, m) = (reference[i][0], mirae[i][0]);
        if (8..247).contains(&r) && (8..247).contains(&m) {
            err = err.max((position(r, radius) - position(m, radius)).abs());
        }
    }
    // Single dithered pixels here (no means): a level of dither on top.
    check("ellipse: position along its axes, pixels", err, 1.5);
}

#[test]
fn alpha_is_straight_and_transparent_stops_have_no_colour() {
    // GD11: main red → white at opacity 0: straight red, alpha along the
    // easing (the alpha does not depend on the mixing mode).
    let to = |opacity: f32| {
        GradientStops::new([
            GradientStop::new(0.0, red(), 1.0),
            GradientStop::new(1.0, Color::WHITE, opacity),
        ])
        .unwrap()
    };
    let cases = [
        ("ft_red", 0.0, Mixing::Perceptual),
        ("ft_w50_std", 0.5, Mixing::Standard),
        ("ft_w1_std", 0.01, Mixing::Standard),
    ];
    for (name, opacity, mixing) in cases {
        let reference = png(name);
        let g = from_press(
            GradientShape::Linear,
            (100.0, 150.0),
            (300.0, 150.0),
            &to(opacity),
            mixing,
            GradientExtend::Clamp,
        );
        let (mut d, id) = document(None, PixelDepth::Rgba16);
        let mirae = draw(&mut d, id, &g);
        // Premultiplied channels: GD12's straight interpolation shows in
        // G and B (premultiplied would be far lower).
        for ch in 0..4 {
            let (err, x) = worst(
                &columns(&reference, ch, 0, H),
                &columns(&mirae, ch, 0, H),
                0..W as usize,
            );
            check(&format!("{name} channel {ch} at x {x}"), err, 1.6);
        }
    }
}

/// `before`, then black → transparent from (0, 280) to (400, 280),
/// Perceptual (only the alpha varies: the mode does not matter).
fn over(locked: bool) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
    let (mut d, id) = document(Some("over_before"), PixelDepth::Rgba16);
    d.set_alpha_locked(id, locked).unwrap();
    let g = from_press(
        GradientShape::Linear,
        (0.0, 280.0),
        (400.0, 280.0),
        &GradientStops::to_transparent(Color::BLACK),
        Mixing::Perceptual,
        GradientExtend::Clamp,
    );
    let mirae = draw(&mut d, id, &g);
    let name = if locked { "over_ft_alock" } else { "over_ft" };
    (png(name), mirae)
}

#[test]
fn over_existing_pixels_and_with_alpha_lock() {
    // GD29 (source-over) and GD30 (source-atop, alpha kept), per band of
    // the probe: the opaque red and translucent green blocks, the blue
    // alpha ramp, the empty band.
    for locked in [false, true] {
        let (reference, mirae) = over(locked);
        for (y0, y1) in [(30, 130), (180, 250), (265, 300)] {
            for ch in 0..4 {
                let (err, x) = worst(
                    &columns(&reference, ch, y0, y1),
                    &columns(&mirae, ch, y0, y1),
                    0..W as usize,
                );
                check(
                    &format!("over, locked {locked}, rows {y0}..{y1}, channel {ch} at x {x}"),
                    err,
                    2.0,
                );
            }
        }
    }
}

#[test]
fn selections_limit_and_scale_the_gradient() {
    // GD31, GD32: black → transparent from (100, 30) to (300, 30) through
    // a rectangle selection (150, 100)–(250, 200), hard and feathered by
    // 10 px (Mirae's feather, 5C).
    for (name, feather) in [("sel_hard", 0.0), ("sel_feather", 10.0)] {
        let reference = png(name);
        let (mut d, id) = document(None, PixelDepth::Rgba8);
        assert!(d.select(
            &SelectionShape::Rect([150.0, 100.0], [250.0, 200.0]),
            false,
            SelectionOp::Replace
        ));
        if feather > 0.0 {
            assert!(d.feather_selection(feather));
        }
        let g = from_press(
            GradientShape::Linear,
            (100.0, 30.0),
            (300.0, 30.0),
            &GradientStops::to_transparent(Color::BLACK),
            Mixing::Perceptual,
            GradientExtend::Clamp,
        );
        let mirae = draw(&mut d, id, &g);
        let mut worst_px = 0;
        for (r, m) in reference.iter().zip(&mirae) {
            worst_px = worst_px.max(r[3].abs_diff(m[3]));
        }
        // Per pixel against a dithered reference: its dither (one level),
        // its bias, Mirae's rounding, and with the feather Mirae's own
        // feather (within 2/255 of the reference's, 5C).
        let tol = if feather > 0.0 { 4.0 } else { 2.0 };
        check(
            &format!("{name} alpha, per pixel"),
            f64::from(worst_px),
            tol,
        );
        let (err, x) = worst(
            &columns(&reference, 3, 100, 200),
            &columns(&mirae, 3, 100, 200),
            0..W as usize,
        );
        // Plus, with the feather, the feathers' own difference (within
        // 2/255, 5C).
        let tol = if feather > 0.0 { 3.6 } else { 1.6 };
        check(&format!("{name} alpha, column means at x {x}"), err, tol);
        if feather == 0.0 {
            // Exactly the selection: nothing outside, everything inside.
            for (i, (r, m)) in reference.iter().zip(&mirae).enumerate() {
                assert_eq!(r[3] == 0, m[3] == 0, "pixel {i}");
            }
        }
    }
}

#[test]
fn a_mask_is_revealed_by_the_gradients_alpha() {
    // GD33: the ink layer (opaque grey 128) with a mask hiding everything;
    // black → transparent reveals by its alpha, opaque black → white
    // reveals everything.
    let cases = [
        ("mask_ft", GradientStops::to_transparent(Color::BLACK)),
        ("mask_bw", GradientStops::two(Color::BLACK, Color::WHITE)),
    ];
    for (name, stops) in cases {
        let reference = png(name);
        let mut d = Document::new(W, H);
        let id = d.active_layer();
        let mut tiles = tela_canvas::DirtyTiles::new();
        tiles.mark_rect(IRect::new(0, 0, W as i32, H as i32));
        d.modify_tiles(id, &tiles, |_, t: &mut Tile<[u8; 4]>| {
            t.pixels_mut().fill([128, 128, 128, 255]);
        })
        .unwrap();
        d.add_mask(id).unwrap();
        // Hide all (a fully opaque erase of the mask).
        let hide = Gradient::new(
            GradientGeometry {
                shape: GradientShape::Linear,
                start: [0.0, 0.0],
                end: [1.0, 0.0],
            },
            &GradientStops::two(Color::BLACK, Color::BLACK),
            Mixing::Standard,
            GradientExtend::Clamp,
            false,
        );
        let mask = FillTarget {
            layer: id,
            part: LayerPart::Mask,
        };
        d.apply_gradient(mask, &hide, GradientPaint::Erase).unwrap();
        let g = from_press(
            GradientShape::Linear,
            (100.0, 150.0),
            (300.0, 150.0),
            &stops,
            Mixing::Perceptual,
            GradientExtend::Clamp,
        );
        d.apply_gradient(mask, &g, GradientPaint::Draw).unwrap();
        // The composite of the layer alone: grey × reveal.
        let m = d.layer(id).unwrap().mask().unwrap();
        let reveal: Vec<[u8; 4]> = (0..W * H)
            .map(|i| {
                let (x, y) = ((i % W) as i32, (i / W) as i32);
                let c = tela_canvas::TileCoord::containing(x, y);
                let (ox, oy) = c.origin();
                let v = m.reveal_tile(c).map_or(255, |mut t| {
                    t.nth(((y - oy) as u32 * TILE_SIZE + (x - ox) as u32) as usize)
                        .unwrap()
                });
                [0, 0, 0, v]
            })
            .collect();
        let (err, x) = worst(
            &columns(&reference, 3, 0, H),
            &columns(&reveal, 3, 0, H),
            0..W as usize,
        );
        check(&format!("{name} reveal at x {x}"), err, 1.6);
    }
}

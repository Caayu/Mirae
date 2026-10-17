//! Parity probe for fill (phase 5E, docs/parity/5E-fill.md).
//!
//! `psd <kind> <path>` writes a 400 × 300 test document of our own. Every
//! kind has an opaque black layer "black" at the bottom and an opaque white
//! layer "white" above it (hiding "white" shows a layer over black, which
//! gives its premultiplied pixels, as in 5D); then:
//!
//! - `conn`: layer "ink" with black line art on transparency: a closed
//!   square, a diamond of diagonal-only pixels (closed for 4-connected
//!   regions, open for 8-connected ones), boxes joined by 1 and 3 px
//!   corridors, a checkerboard inside a box, a box holding a solid square
//!   with a hole, a region touching the canvas border, an anti-aliased
//!   ring and two boxes with 1 and 5 px gaps.
//! - `tol`: layer "ink" opaque grey 128 with 4 × 4 islands of nearby
//!   colours (single channels, alpha, combinations, random) and a chain of
//!   stripes stepping by one level, reachable only through stripe 128.
//! - `alpha`: layer "ink" with three zones: transparent (islands with a
//!   hidden colour under alpha 0, and low alphas), opaque black and opaque
//!   white (islands differing in alpha only, straight colour kept).
//! - `comp`: line art spread over several layers (Normal, 30 % opacity,
//!   Screen, clipped to an empty base, hidden, white on white) and an
//!   empty top layer "fill" for fills that refer to other layers.
//! - `lock`: layer "ink" with a horizontal alpha ramp of grey, an opaque
//!   block and transparency; layer "masked" (hidden by default in the
//!   probe) with a mask hiding everything but a box outline.
//!
//! `islands <kind> <over-white.png> [r g b]` lists, for the `tol` or
//! `alpha` document, which islands the fill reached (their centre is the
//! fill colour, magenta by default). `fit <dir>` reads `tol_<t>.png`
//! captures (tolerance `t` in tenths of a percent) and scores colour
//! distance models: for each model, the largest distance included, the
//! smallest excluded, and how many islands no threshold explains.
//! `chain <over-white.png>` reports how far the fill went along the chain.
//! `decode` turns captures over white and over black into premultiplied
//! RGBA (as `transform_probe decode`); `block` prints one channel.

use std::fs::File;
use std::path::Path;

use tela_corpus::psd::{PsdLayer, PsdMask, write_psd};

const W: u32 = 400;
const H: u32 = 300;
/// Island side and pitch.
const SIDE: u32 = 4;
const PITCH: u32 = 8;
/// The fill colour used in the probes.
const MAGENTA: [u8; 3] = [255, 0, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

type Px = Vec<[u8; 4]>;

fn canvas(p: [u8; 4]) -> Px {
    vec![p; (W * H) as usize]
}

fn set(px: &mut Px, x: i32, y: i32, p: [u8; 4]) {
    if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
        px[(y as u32 * W + x as u32) as usize] = p;
    }
}

fn rect(px: &mut Px, x0: i32, y0: i32, x1: i32, y1: i32, p: [u8; 4]) {
    for y in y0..y1 {
        for x in x0..x1 {
            set(px, x, y, p);
        }
    }
}

/// One-pixel outline of `[x0, x1) × [y0, y1)`.
fn outline(px: &mut Px, x0: i32, y0: i32, x1: i32, y1: i32, p: [u8; 4]) {
    rect(px, x0, y0, x1, y0 + 1, p);
    rect(px, x0, y1 - 1, x1, y1, p);
    rect(px, x0, y0, x0 + 1, y1, p);
    rect(px, x1 - 1, y0, x1, y1, p);
}

/// Anti-aliased ring (8 × 8 supersampled coverage) of colour `c`.
fn ring(px: &mut Px, cx: f64, cy: f64, r: f64, width: f64, c: [u8; 3]) {
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            let mut n = 0;
            for j in 0..8 {
                for i in 0..8 {
                    let dx = f64::from(x) + (f64::from(i) + 0.5) / 8.0 - cx;
                    let dy = f64::from(y) + (f64::from(j) + 0.5) / 8.0 - cy;
                    if ((dx * dx + dy * dy).sqrt() - r).abs() <= width / 2.0 {
                        n += 1;
                    }
                }
            }
            if n > 0 {
                let a = (f64::from(n) / 64.0 * 255.0).round() as u8;
                set(px, x, y, [c[0], c[1], c[2], a]);
            }
        }
    }
}

fn layer(name: &str, rgba: Px) -> PsdLayer {
    PsdLayer {
        name: name.into(),
        mode: *b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        rect: (0, 0, W, H),
        rgba,
        mask: None,
    }
}

fn backdrop() -> Vec<PsdLayer> {
    vec![
        layer("black", canvas(BLACK)),
        layer("white", canvas([255; 4])),
    ]
}

/// Line art of the `conn` document.
fn conn_ink() -> Px {
    let mut px = canvas(CLEAR);
    // (a) closed square.
    outline(&mut px, 20, 20, 80, 80, BLACK);
    // (b) diamond |dx| + |dy| = 30 around (150, 50): pixels touch only
    // diagonally.
    for d in -30..=30i32 {
        let r = 30 - d.abs();
        set(&mut px, 150 + d, 50 - r, BLACK);
        set(&mut px, 150 + d, 50 + r, BLACK);
    }
    // (c) boxes joined by a 1 px and a 3 px corridor.
    outline(&mut px, 200, 20, 250, 80, BLACK);
    outline(&mut px, 270, 20, 320, 80, BLACK);
    outline(&mut px, 340, 20, 390, 80, BLACK);
    rect(&mut px, 249, 50, 271, 51, CLEAR);
    rect(&mut px, 249, 49, 271, 50, BLACK);
    rect(&mut px, 249, 51, 271, 52, BLACK);
    rect(&mut px, 249, 50, 271, 51, CLEAR);
    rect(&mut px, 319, 49, 341, 52, CLEAR);
    rect(&mut px, 319, 48, 341, 49, BLACK);
    rect(&mut px, 319, 52, 341, 53, BLACK);
    rect(&mut px, 319, 49, 341, 52, CLEAR);
    // (d) a box with a 1 px checkerboard inside, 4 px from its walls.
    outline(&mut px, 16, 106, 88, 178, BLACK);
    for y in 110..174 {
        for x in 20..84 {
            if (x + y) % 2 == 0 {
                set(&mut px, x, y, BLACK);
            }
        }
    }
    // (e) a box holding a solid square with a hole.
    outline(&mut px, 110, 106, 190, 186, BLACK);
    rect(&mut px, 136, 132, 164, 160, BLACK);
    rect(&mut px, 146, 142, 154, 150, CLEAR);
    // (f) a region touching the right and bottom canvas edges.
    rect(&mut px, 300, 200, 400, 201, BLACK);
    rect(&mut px, 300, 200, 301, 300, BLACK);
    // (g) an anti-aliased ring.
    ring(&mut px, 240.0, 150.0, 30.0, 3.0, [0, 0, 0]);
    // (h) boxes with a 1 px and a 5 px gap at the top.
    outline(&mut px, 20, 200, 80, 260, BLACK);
    rect(&mut px, 50, 200, 51, 201, CLEAR);
    outline(&mut px, 100, 200, 160, 260, BLACK);
    rect(&mut px, 128, 200, 133, 201, CLEAR);
    px
}

/// An island: top-left corner and straight RGBA.
#[derive(Clone, Copy, Debug)]
struct Island {
    x: u32,
    y: u32,
    rgba: [u8; 4],
}

/// Deterministic xorshift for the random islands.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
}

/// Deltas for single-channel series: every level to 40, then every third.
fn deltas(max: i32) -> Vec<i32> {
    (1..=40).chain((43..=max).step_by(3)).collect()
}

const SEED: i32 = 128;

fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// The `tol` islands (straight RGBA), in slot order.
fn tol_colors() -> Vec<[u8; 4]> {
    let mut c = Vec::new();
    for ch in 0..3 {
        for sign in [1, -1] {
            for d in deltas(127) {
                let mut p = [SEED as u8, SEED as u8, SEED as u8, 255];
                p[ch] = clamp8(SEED + sign * d);
                c.push(p);
            }
        }
    }
    for d in deltas(255) {
        c.push([SEED as u8, SEED as u8, SEED as u8, clamp8(255 - d)]);
    }
    let patterns: [[i32; 3]; 6] = [
        [1, 1, 0],
        [0, 1, 1],
        [1, 0, 1],
        [1, 1, 1],
        [1, -1, 0],
        [-1, -1, 1],
    ];
    for p in patterns {
        for d in (2..=80).step_by(2) {
            c.push([
                clamp8(SEED + p[0] * d),
                clamp8(SEED + p[1] * d),
                clamp8(SEED + p[2] * d),
                255,
            ]);
        }
    }
    let mut rng = Rng(0x5e5e_f111_0bad_cafe);
    while c.len() < tol_slots().len() {
        let a = if rng.range(0, 3) == 0 {
            clamp8(255 - rng.range(0, 120))
        } else {
            255
        };
        c.push([
            clamp8(SEED + rng.range(-90, 90)),
            clamp8(SEED + rng.range(-90, 90)),
            clamp8(SEED + rng.range(-90, 90)),
            a,
        ]);
    }
    c
}

/// Island corners of the `tol` document: rows above the chain.
fn tol_slots() -> Vec<(u32, u32)> {
    let mut s = Vec::new();
    for y in (4..260).step_by(PITCH as usize) {
        for x in (4..W - SIDE).step_by(PITCH as usize) {
            s.push((x, y));
        }
    }
    s
}

fn tol_islands() -> Vec<Island> {
    tol_slots()
        .into_iter()
        .zip(tol_colors())
        .map(|((x, y), rgba)| Island { x, y, rgba })
        .collect()
}

/// The chain: stripe `k` (4 px wide, from x = 4 + 4k) has grey `128 + k`.
const CHAIN_Y: (i32, i32) = (272, 290);

fn tol_ink() -> Px {
    let grey = [SEED as u8, SEED as u8, SEED as u8, 255];
    let mut px = canvas(grey);
    for i in tol_islands() {
        rect(
            &mut px,
            i.x as i32,
            i.y as i32,
            (i.x + SIDE) as i32,
            (i.y + SIDE) as i32,
            i.rgba,
        );
    }
    // The chain sits in a transparent band, reachable from the grey only
    // through a grey passage at the left edge into stripe 0 (grey 128).
    rect(&mut px, 0, 264, W as i32, H as i32, CLEAR);
    for k in 0..98 {
        let v = clamp8(SEED + k);
        rect(
            &mut px,
            4 + 4 * k,
            CHAIN_Y.0,
            8 + 4 * k,
            CHAIN_Y.1,
            [v, v, v, 255],
        );
    }
    rect(&mut px, 0, 260, 4, 280, grey);
    px
}

/// Zones of the `alpha` document: x ranges and seed colours.
const ZONES: [(u32, u32, [u8; 4]); 3] = [
    (0, 132, [0, 0, 0, 0]),
    (136, 266, [0, 0, 0, 255]),
    (266, 400, [255, 255, 255, 255]),
];

fn alpha_islands() -> Vec<Island> {
    let mut out = Vec::new();
    for (zone, &(x0, x1, _)) in ZONES.iter().enumerate() {
        let mut colors: Vec<[u8; 4]> = Vec::new();
        match zone {
            0 => {
                // A hidden colour under alpha 0, then low alphas.
                for h in [
                    [255, 0, 0],
                    [255, 255, 255],
                    [128, 128, 128],
                    [0, 255, 0],
                    [0, 0, 255],
                    [1, 1, 1],
                ] {
                    colors.push([h[0], h[1], h[2], 0]);
                }
                for c in [[0, 0, 0], [255, 255, 255], [255, 0, 0]] {
                    for d in deltas(255) {
                        colors.push([c[0], c[1], c[2], clamp8(d)]);
                    }
                }
            }
            1 => {
                for d in deltas(255) {
                    colors.push([0, 0, 0, clamp8(255 - d)]);
                }
                for d in deltas(255) {
                    let v = clamp8(d);
                    colors.push([v, v, v, 255]);
                }
                for d in deltas(255) {
                    colors.push([255, 255, 255, clamp8(255 - d)]);
                }
            }
            _ => {
                for d in deltas(255) {
                    colors.push([255, 255, 255, clamp8(255 - d)]);
                }
                for d in deltas(255) {
                    let v = clamp8(255 - d);
                    colors.push([v, v, v, 255]);
                }
                for d in deltas(255) {
                    colors.push([0, 0, 0, clamp8(255 - d)]);
                }
            }
        }
        let mut slots = Vec::new();
        for y in (4..H - SIDE).step_by(PITCH as usize) {
            for x in (x0 + 4..x1 - SIDE - 2).step_by(PITCH as usize) {
                slots.push((x, y));
            }
        }
        assert!(slots.len() >= colors.len(), "zone {zone}: too many islands");
        out.extend(
            slots
                .into_iter()
                .zip(colors)
                .map(|((x, y), rgba)| Island { x, y, rgba }),
        );
    }
    out
}

fn alpha_ink() -> Px {
    let mut px = canvas(CLEAR);
    for &(x0, x1, seed) in &ZONES {
        rect(&mut px, x0 as i32, 0, x1 as i32, H as i32, seed);
    }
    // A white wall between the transparent and the black zone.
    rect(&mut px, 132, 0, 136, H as i32, [255; 4]);
    for i in alpha_islands() {
        rect(
            &mut px,
            i.x as i32,
            i.y as i32,
            (i.x + SIDE) as i32,
            (i.y + SIDE) as i32,
            i.rgba,
        );
    }
    px
}

fn islands_of(kind: &str) -> Vec<Island> {
    match kind {
        "tol" => tol_islands(),
        "alpha" => alpha_islands(),
        other => panic!("no islands in {other}"),
    }
}

/// The `comp` document: one closed box per layer, side by side.
fn comp_layers() -> Vec<PsdLayer> {
    let mut layers = backdrop();
    let boxed = |x0: i32, c: [u8; 4]| {
        let mut px = canvas(CLEAR);
        outline(&mut px, x0, 20, x0 + 60, 80, c);
        px
    };
    // Box A (Normal, black) and an anti-aliased ring.
    let mut line = boxed(10, BLACK);
    ring(&mut line, 200.0, 200.0, 40.0, 3.0, [0, 0, 0]);
    layers.push(layer("line", line));
    // Box B: grey 200 at 30 % opacity (shown as ~238).
    let mut faint = layer("faint", boxed(80, [200, 200, 200, 255]));
    faint.opacity = 77;
    layers.push(faint);
    // Box C: black in Screen (invisible over white).
    let mut screen = layer("screen", boxed(150, BLACK));
    screen.mode = *b"scrn";
    layers.push(screen);
    // Box D: black, clipped to a base that is empty there.
    let mut base = canvas(CLEAR);
    rect(&mut base, 380, 280, 396, 296, [128, 128, 128, 255]);
    layers.push(layer("base", base));
    let mut clip = layer("clip", boxed(220, BLACK));
    clip.clipped = true;
    layers.push(clip);
    // Box E: black, hidden layer.
    let mut hidden = layer("hidden", boxed(290, BLACK));
    hidden.hidden = true;
    layers.push(hidden);
    // Box F: white (invisible over the white layer).
    let mut white = canvas(CLEAR);
    outline(&mut white, 10, 120, 70, 180, [255; 4]);
    layers.push(layer("whitebox", white));
    layers.push(layer("fill", canvas(CLEAR)));
    layers
}

/// The `lock` document.
fn lock_layers() -> Vec<PsdLayer> {
    let mut layers = backdrop();
    let mut ink = canvas(CLEAR);
    for y in 40..140 {
        for x in 20..276 {
            set(&mut ink, x, y, [64, 64, 64, (x - 20) as u8]);
        }
    }
    rect(&mut ink, 300, 40, 380, 140, [64, 64, 64, 255]);
    layers.push(layer("ink", ink));
    let mut masked = layer("masked", canvas([128, 128, 128, 255]));
    masked.hidden = true;
    // Mask: hides everything except a revealed box outline (60..140 ×
    // 170..250) and a revealed square (300..340 × 190..230).
    let mut values = vec![0u8; (W * H) as usize];
    for y in 170..250u32 {
        for x in 60..140u32 {
            if y == 170 || y == 249 || x == 60 || x == 139 {
                values[(y * W + x) as usize] = 255;
            }
        }
    }
    for y in 190..230u32 {
        for x in 300..340u32 {
            values[(y * W + x) as usize] = 255;
        }
    }
    masked.mask = Some(PsdMask {
        rect: (0, 0, W, H),
        values,
        default: 0,
        disabled: false,
    });
    layers.push(masked);
    layers
}

fn read_png(path: &str) -> std::io::Result<(u32, u32, Px)> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().expect("png size fits")];
    let frame = reader.next_frame(&mut buf)?;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported PNG color type {other:?}"),
    };
    let px = buf[..frame.buffer_size()]
        .chunks_exact(channels)
        .map(|p| [p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }])
        .collect();
    Ok((frame.width, frame.height, px))
}

fn write_png(path: &str, w: u32, h: u32, px: &[[u8; 4]]) -> std::io::Result<()> {
    let mut enc = png::Encoder::new(std::io::BufWriter::new(File::create(path)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(px.as_flattened())?;
    Ok(())
}

/// Whether the island's two centre rows are the fill colour.
fn filled(px: &Px, w: u32, i: &Island, fill: [u8; 3]) -> bool {
    (1..3).all(|dy| {
        (1..3).all(|dx| {
            let p = px[((i.y + dy) * w + i.x + dx) as usize];
            (0..3).all(|c| p[c].abs_diff(fill[c]) <= 2)
        })
    })
}

/// A colour distance model: straight 8-bit RGBA of the seed and a pixel.
struct Model {
    name: &'static str,
    f: fn([f64; 4], [f64; 4]) -> f64,
}

fn premul(p: [f64; 4]) -> [f64; 4] {
    let a = p[3] / 255.0;
    [p[0] * a, p[1] * a, p[2] * a, p[3]]
}

fn max_abs(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

fn euclid(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn sum_abs(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

fn hsv(p: [f64; 4]) -> [f64; 3] {
    let (r, g, b) = (p[0] / 255.0, p[1] / 255.0, p[2] / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s, max]
}

fn lab(p: [f64; 4]) -> [f64; 3] {
    let lin = |c: f64| {
        let c = c / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(p[0]), lin(p[1]), lin(p[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn models() -> Vec<Model> {
    vec![
        Model {
            name: "max |dRGBA| straight",
            f: |a, b| max_abs(&a, &b),
        },
        Model {
            name: "max |dRGBA| premultiplied",
            f: |a, b| max_abs(&premul(a), &premul(b)),
        },
        Model {
            name: "max |dRGB| straight (alpha ignored)",
            f: |a, b| max_abs(&a[..3], &b[..3]),
        },
        Model {
            name: "euclid RGBA straight",
            f: |a, b| euclid(&a, &b),
        },
        Model {
            name: "euclid RGBA premultiplied",
            f: |a, b| euclid(&premul(a), &premul(b)),
        },
        Model {
            name: "euclid RGB straight",
            f: |a, b| euclid(&a[..3], &b[..3]),
        },
        Model {
            name: "sum |dRGBA| straight",
            f: |a, b| sum_abs(&a, &b),
        },
        Model {
            name: "sum |dRGB| straight",
            f: |a, b| sum_abs(&a[..3], &b[..3]),
        },
        Model {
            name: "luma 601 |dY| + alpha max",
            f: |a, b| {
                let y = |p: [f64; 4]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                (y(a) - y(b)).abs().max((a[3] - b[3]).abs())
            },
        },
        Model {
            name: "weighted euclid (redmean)",
            f: |a, b| {
                let rm = (a[0] + b[0]) / 2.0;
                let (dr, dg, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
                ((2.0 + rm / 256.0) * dr * dr
                    + 4.0 * dg * dg
                    + (2.0 + (255.0 - rm) / 256.0) * db * db)
                    .sqrt()
            },
        },
        Model {
            name: "HSV max(dH/360, dS, dV)·255",
            f: |a, b| {
                let (p, q) = (hsv(a), hsv(b));
                let dh = (p[0] - q[0]).abs().min(360.0 - (p[0] - q[0]).abs()) / 360.0;
                255.0 * dh.max((p[1] - q[1]).abs()).max((p[2] - q[2]).abs())
            },
        },
        Model {
            name: "CIE76 dE",
            f: |a, b| euclid(&lab(a), &lab(b)),
        },
        Model {
            name: "max |dRGBA| straight, RGB ignored at alpha 0",
            f: |a, b| {
                if a[3] == 0.0 || b[3] == 0.0 {
                    (a[3] - b[3]).abs()
                } else {
                    max_abs(&a, &b)
                }
            },
        },
        Model {
            name: "max(max|dRGB| straight, |dA|)·(premul for A<255)",
            f: |a, b| max_abs(&premul(a)[..3], &premul(b)[..3]).max((a[3] - b[3]).abs()),
        },
    ]
}

fn to_f(p: [u8; 4]) -> [f64; 4] {
    p.map(f64::from)
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let fill = |from: usize| -> [u8; 3] {
        if args.len() >= from + 3 {
            [0, 1, 2].map(|k| args[from + k].parse().expect("a channel 0..255"))
        } else {
            MAGENTA
        }
    };
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 4 => {
            let path = Path::new(&args[3]);
            let layers = match args[2].as_str() {
                "conn" => {
                    let mut l = backdrop();
                    l.push(layer("ink", conn_ink()));
                    l
                }
                "tol" => {
                    let mut l = backdrop();
                    l.push(layer("ink", tol_ink()));
                    l
                }
                "alpha" => {
                    let mut l = backdrop();
                    l.push(layer("ink", alpha_ink()));
                    l
                }
                "comp" => comp_layers(),
                "lock" => lock_layers(),
                other => panic!("unknown kind {other}"),
            };
            write_psd(path, W, H, &layers)
        }
        Some("png") if args.len() == 4 => {
            // The ink layer of a kind, straight RGBA (what the PSD holds).
            let px = match args[2].as_str() {
                "conn" => conn_ink(),
                "tol" => tol_ink(),
                "alpha" => alpha_ink(),
                "lock" => lock_layers().swap_remove(2).rgba,
                other => panic!("no ink layer in {other}"),
            };
            write_png(&args[3], W, H, &px)
        }
        Some("corpus") if args.len() >= 4 => {
            // One line per capture: which islands the fill reached, as a
            // hex bit string (island k = bit k), and the last chain
            // stripe reached (tol only). Arguments: out.csv, then
            // `kind:zone:dir` triples whose files are <kind>_<tenths>.png.
            use std::fmt::Write as _;
            let mut out = String::from("kind,zone,tenths,seed_x,seed_y,chain_last,islands\n");
            for spec in &args[3..] {
                let parts: Vec<&str> = spec.splitn(3, ':').collect();
                let (kind, zone, dir) =
                    (parts[0], parts[1].parse::<usize>().expect("zone"), parts[2]);
                let islands = islands_of(kind);
                let seed = if kind == "tol" {
                    (2, 2)
                } else {
                    [(1, 1), (137, 1), (268, 1)][zone]
                };
                let mut files: Vec<(u32, String)> = std::fs::read_dir(dir)?
                    .filter_map(|e| {
                        let name = e.ok()?.file_name().into_string().ok()?;
                        let t = name
                            .strip_prefix(&format!("{kind}_"))?
                            .strip_suffix(".png")?;
                        Some((t.parse().ok()?, name))
                    })
                    .collect();
                files.sort();
                for (t, name) in files {
                    let (w, _, px) = read_png(&format!("{dir}/{name}"))?;
                    let in_zone =
                        |i: &Island| kind == "tol" || (ZONES[zone].0..ZONES[zone].1).contains(&i.x);
                    let mut bits = vec![0u8; islands.len().div_ceil(8)];
                    for (k, i) in islands.iter().enumerate() {
                        if in_zone(i) && filled(&px, w, i, MAGENTA) {
                            bits[k / 8] |= 1 << (k % 8);
                        }
                    }
                    let chain = if kind == "tol" {
                        let y = ((CHAIN_Y.0 + CHAIN_Y.1) / 2) as u32;
                        (0..98u32)
                            .rev()
                            .find(|k| {
                                let p = px[(y * w + 6 + 4 * k) as usize];
                                (0..3).all(|c| p[c].abs_diff(MAGENTA[c]) <= 2)
                            })
                            .map_or(-1, |k| k as i32)
                    } else {
                        -1
                    };
                    let mut hex = String::new();
                    for b in &bits {
                        write!(hex, "{b:02x}").expect("string");
                    }
                    writeln!(out, "{kind},{zone},{t},{},{},{chain},{hex}", seed.0, seed.1)
                        .expect("string");
                }
            }
            std::fs::write(&args[2], out)
        }
        Some("islands") if args.len() >= 4 => {
            let (w, _, px) = read_png(&args[3])?;
            let fill = fill(4);
            for (k, i) in islands_of(&args[2]).iter().enumerate() {
                println!(
                    "{k}\t{},{}\t{:?}\t{}",
                    i.x,
                    i.y,
                    i.rgba,
                    u8::from(filled(&px, w, i, fill))
                );
            }
            Ok(())
        }
        Some("fit") if args.len() >= 3 => {
            let kind = args.get(3).map_or("tol", String::as_str);
            let islands = islands_of(kind);
            let seed = to_f(if kind == "tol" {
                [SEED as u8, SEED as u8, SEED as u8, 255]
            } else {
                ZONES[args.get(4).map_or(0, |z| z.parse().expect("zone"))].2
            });
            let mut files: Vec<(u32, String)> = std::fs::read_dir(&args[2])?
                .filter_map(|e| {
                    let name = e.ok()?.file_name().into_string().ok()?;
                    let t = name
                        .strip_prefix(&format!("{kind}_"))?
                        .strip_suffix(".png")?;
                    Some((t.parse().ok()?, name))
                })
                .collect();
            files.sort();
            for (t, name) in files {
                let (w, _, px) = read_png(&format!("{}/{name}", args[2]))?;
                println!("## tolerance {:.1} % ({name})", f64::from(t) / 10.0);
                let zone = |i: &Island| {
                    kind == "tol"
                        || ZONES
                            .iter()
                            .any(|z| z.2 == seed.map(|v| v as u8) && (z.0..z.1).contains(&i.x))
                };
                let marks: Vec<(&Island, bool)> = islands
                    .iter()
                    .filter(|i| zone(i))
                    .map(|i| (i, filled(&px, w, i, MAGENTA)))
                    .collect();
                println!(
                    "{} of {} islands filled",
                    marks.iter().filter(|m| m.1).count(),
                    marks.len()
                );
                for m in models() {
                    let d: Vec<(f64, bool)> = marks
                        .iter()
                        .map(|(i, f)| ((m.f)(seed, to_f(i.rgba)), *f))
                        .collect();
                    let inc = d
                        .iter()
                        .filter(|x| x.1)
                        .map(|x| x.0)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let exc = d
                        .iter()
                        .filter(|x| !x.1)
                        .map(|x| x.0)
                        .fold(f64::INFINITY, f64::min);
                    // Fewest islands misclassified by any single threshold.
                    let mut cuts: Vec<f64> = d.iter().map(|x| x.0).collect();
                    cuts.push(-1.0);
                    let wrong = cuts
                        .iter()
                        .map(|&c| d.iter().filter(|x| (x.0 <= c) != x.1).count())
                        .min()
                        .unwrap_or(0);
                    println!(
                        "{:<48} max incl {:>8.3}  min excl {:>8.3}  misfit {}",
                        m.name, inc, exc, wrong
                    );
                }
            }
            Ok(())
        }
        Some("aa") if args.len() >= 4 => {
            // How much fill colour landed on each pixel of each island
            // (decoded premultiplied capture), assuming `fill·c` composed
            // over the island: least squares over R, G, B, A.
            let (w, _, px) = read_png(&args[2])?;
            let kind = args.get(4).map_or("tol", String::as_str);
            let t: f64 = args[3].parse().expect("tolerance in %");
            let t8 = (t * 2.55 + 0.5).floor();
            let seed = to_f([SEED as u8, SEED as u8, SEED as u8, 255]);
            let m = [255.0, 0.0, 255.0, 255.0];
            println!("island\td\t(255-d)/(255-T)\tcorner\tedge\tcentre\trgba");
            for (k, i) in islands_of(kind).iter().enumerate() {
                let d = max_abs(&seed, &to_f(i.rgba));
                let p = premul(to_f(i.rgba));
                let c = |dx: u32, dy: u32| {
                    let q = to_f(px[((i.y + dy) * w + i.x + dx) as usize]);
                    let (mut num, mut den) = (0.0, 0.0);
                    for ch in 0..4 {
                        num += (q[ch] - p[ch]) * (m[ch] - p[ch]);
                        den += (m[ch] - p[ch]).powi(2);
                    }
                    if den == 0.0 { f64::NAN } else { num / den }
                };
                println!(
                    "{k}\t{d}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:?}",
                    ((255.0 - d) / (255.0 - t8)).clamp(0.0, 1.0),
                    c(0, 0),
                    c(1, 0),
                    c(1, 1),
                    i.rgba
                );
            }
            Ok(())
        }
        Some("chain") if args.len() >= 3 => {
            let (w, _, px) = read_png(&args[2])?;
            let y = ((CHAIN_Y.0 + CHAIN_Y.1) / 2) as u32;
            let filled: Vec<u8> = (0..98u32)
                .map(|k| {
                    let p = px[(y * w + 6 + 4 * k) as usize];
                    u8::from((0..3).all(|c| p[c].abs_diff(MAGENTA[c]) <= 2))
                })
                .collect();
            let last = filled.iter().rposition(|&f| f == 1);
            println!(
                "stripes filled: {} (last: {:?}, grey {:?}); pattern {:?}",
                filled.iter().filter(|&&f| f == 1).count(),
                last,
                last.map(|k| 128 + k),
                filled
            );
            Ok(())
        }
        Some("decode") if args.len() == 5 => {
            let (w, h, white) = read_png(&args[2])?;
            let (_, _, black) = read_png(&args[3])?;
            let out: Px = white
                .iter()
                .zip(&black)
                .map(|(wp, bp)| {
                    let d: i32 = (0..3).map(|c| i32::from(wp[c]) - i32::from(bp[c])).sum();
                    let a = (255 - (d as f64 / 3.0).round() as i32).clamp(0, 255) as u8;
                    [bp[0].min(a), bp[1].min(a), bp[2].min(a), a]
                })
                .collect();
            write_png(&args[4], w, h, &out)
        }
        Some("block") if args.len() >= 7 => {
            let (w, _, px) = read_png(&args[2])?;
            let n = |k: usize| -> u32 { args[k].parse().expect("a number") };
            let ch = args.get(7).map_or(3, |c| c.parse().expect("channel"));
            for y in n(4)..n(4) + n(6) {
                let row: Vec<String> = (n(3)..n(3) + n(5))
                    .map(|x| format!("{:3}", px[(y * w + x) as usize][ch]))
                    .collect();
                println!("{y:3}: {}", row.join(" "));
            }
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: fill_probe psd <conn|tol|alpha|comp|lock> <path> | islands <tol|alpha> <png> [r g b] | fit <dir> [tol|alpha zone] | chain <png> | decode <white> <black> <out> | block <png> x y w h [ch]"
            );
            std::process::exit(2);
        }
    }
}

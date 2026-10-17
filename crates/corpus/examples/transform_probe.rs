//! Parity probe for transforms (phase 5D, docs/parity/5D-transform.md).
//!
//! `psd <path> [pattern]` writes a 400 × 300 test document of our own:
//! an opaque black layer "black", an opaque white layer "white" above it
//! and a layer "ink" holding a 64 × 64 test pattern at (168, 118)
//! (centre (200, 150)): a one-pixel black/white checker, coloured 4 × 4
//! blocks, a red alpha ramp, a soft blue blob, an isolated opaque red
//! square and an anti-aliased green line. Transparent pixels carry a
//! hidden colour (pure green) so that interpolation of straight colour
//! would show it. In the reference the ink layer is transformed and the
//! canvas is read at 100 % zoom twice, over the white layer and with the
//! white layer hidden (over black).
//!
//! `decode <over-white.png> <over-black.png> <out.png>` turns the two
//! captures into the ink layer's premultiplied RGBA (colour = the capture
//! over black, alpha = 255 − (white − black)), stored as RGBA PNG bytes.
//!
//! `fit <decoded.png> <sx> <sy> <degrees> <tx> <ty> [px py]` renders the
//! pattern under the affine transform `x' = p + t + R(θ)·S·(x − p)` (pivot
//! `p`, default (200, 150); θ clockwise on screen, y down) with every
//! candidate filter, in premultiplied and in straight colour, and prints
//! each candidate's error against the decoded image.
//!
//! `block <decoded.png> <x0> <y0> <w> <h> [channel]` prints a block of
//! one channel (0–2 premultiplied colour, 3 alpha; default 3); `bbox`,
//! `diff`, `render` and `move` (a move through a rectangle selection,
//! scored against Mirae's selection model) help reading the results.
//!
//! The candidate models are switched by environment variables, so every
//! alternative that was scored stays reproducible: `EDGE` (`transparent`,
//! `clamp`, `mirror`, `renorm`, `zero`), `CLAMPUV`, `OUT=unpremul`,
//! `PYRAMID`, `COVER` (`1` area, `scan`, `dist`), `SUPER`, `HIDDEN=black`,
//! `NOFF`. The reference's bicubic default is
//! `OUT=unpremul CLAMPUV=1 EDGE=zero PYRAMID=1 COVER=dist` with
//! `Cubic(-1.0)`; nearest neighbour is `NOFF=-0.000001` without `COVER`
//! (docs/parity/5D-transform.md, T16–T24).

use std::fs::File;
use std::path::Path;

use tela_corpus::psd::{PsdLayer, write_psd};

const W: u32 = 400;
const H: u32 = 300;
/// The pattern's box.
const X0: i32 = 168;
const Y0: i32 = 118;
const SIDE: i32 = 64;

thread_local! {
    /// Side of the box being sampled (the pattern, or a reduced level).
    static SAMPLED_SIDE: std::cell::Cell<i32> = const { std::cell::Cell::new(SIDE) };
}

fn side() -> i32 {
    SAMPLED_SIDE.with(std::cell::Cell::get)
}
/// Colour stored under zero alpha in the PSD (never shown).
const HIDDEN: [u8; 3] = [0, 255, 0];

/// The ink layer, straight RGBA over the whole canvas.
fn pattern() -> Vec<[u8; 4]> {
    let mut px = vec![[HIDDEN[0], HIDDEN[1], HIDDEN[2], 0]; (W * H) as usize];
    let blocks: [[u8; 3]; 8] = [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [0, 255, 255],
        [255, 0, 255],
        [255, 255, 255],
        [37, 149, 213],
    ];
    for y in Y0..Y0 + SIDE {
        for x in X0..X0 + SIDE {
            let (lx, ly) = (x - X0, y - Y0);
            let p = match (lx < 32, ly < 32) {
                // One-pixel checker.
                (true, true) => {
                    let v = if (lx + ly) % 2 == 0 { 0 } else { 255 };
                    Some([v, v, v, 255])
                }
                // 4 × 4 coloured blocks.
                (false, true) => {
                    let c = blocks[(((lx - 32) / 4 + ly / 4 * 3) % 8) as usize];
                    Some([c[0], c[1], c[2], 255])
                }
                // Red alpha ramp, then a soft blue blob.
                (true, false) if ly < 48 => Some([255, 0, 0, (lx * 8 + 4) as u8]),
                (true, false) => {
                    let (dx, dy) = (f64::from(lx) + 0.5 - 16.0, f64::from(ly) + 0.5 - 56.0);
                    let a = (255.0 * (-(dx * dx + dy * dy) / (2.0 * 16.0)).exp()).round();
                    (a > 0.0).then_some([0, 0, 255, a as u8])
                }
                // An isolated red square and an anti-aliased green line.
                (false, false) => {
                    if (44..52).contains(&lx) && (44..52).contains(&ly) {
                        Some([255, 0, 0, 255])
                    } else {
                        let a = line_coverage(f64::from(lx), f64::from(ly));
                        (a > 0.0).then_some([0, 160, 0, (a * 255.0).round() as u8])
                    }
                }
            };
            if let Some(p) = p {
                px[(y as u32 * W + x as u32) as usize] = p;
            }
        }
    }
    px
}

/// Coverage of a 2 px wide line from (34, 62) to (62, 34) in pattern
/// coordinates, by 8 × 8 supersampling.
fn line_coverage(lx: f64, ly: f64) -> f64 {
    let mut n = 0;
    for j in 0..8 {
        for i in 0..8 {
            let (x, y) = (
                lx + (f64::from(i) + 0.5) / 8.0,
                ly + (f64::from(j) + 0.5) / 8.0,
            );
            // Distance to the line x + y = 96, between the end points.
            let d = (x + y - 96.0).abs() / 2f64.sqrt();
            if d <= 1.0 && (34.0..=62.0).contains(&x) {
                n += 1;
            }
        }
    }
    f64::from(n) / 64.0
}

fn layer(name: &str, rgba: Vec<[u8; 4]>) -> PsdLayer {
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

fn read_png(path: &str) -> std::io::Result<(u32, u32, Vec<[u8; 4]>)> {
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

/// The ink layer as `f64` RGBA, premultiplied or straight (transparent
/// pixels keep the hidden colour when straight).
fn source(premultiplied: bool) -> Vec<[f64; 4]> {
    pattern()
        .into_iter()
        .map(|p| {
            let a = f64::from(p[3]) / 255.0;
            // Straight colour under zero alpha: the PSD's hidden colour, or
            // black with `HIDDEN=black` (if the reference drops it).
            let hidden_black = std::env::var("HIDDEN").is_ok_and(|v| v == "black");
            let k = if premultiplied || (hidden_black && p[3] == 0) {
                a
            } else {
                1.0
            };
            [
                f64::from(p[0]) / 255.0 * k,
                f64::from(p[1]) / 255.0 * k,
                f64::from(p[2]) / 255.0 * k,
                a,
            ]
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum Filter {
    Nearest,
    Bilinear,
    /// Keys cubic with parameter `a`.
    Cubic(f64),
    /// Mitchell–Netravali (B, C).
    Bc(f64, f64),
    /// Lanczos with `n` lobes.
    Lanczos(i32),
    /// Bilinear averaged over an `n × n` grid of sub-samples of the output
    /// pixel (an area filter when reducing).
    Area(u32),
    /// Keys cubic `a`, each channel clamped to the range of the `n × n`
    /// nearest source pixels (2: the bilinear neighbours, 4: the whole
    /// kernel).
    CubicClamp(f64, i64),
}

fn keys(x: f64, a: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 {
        (a + 2.0) * x.powi(3) - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x.powi(3) - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn bc(x: f64, b: f64, c: f64) -> f64 {
    let x = x.abs();
    let v = if x < 1.0 {
        (12.0 - 9.0 * b - 6.0 * c) * x.powi(3) + (-18.0 + 12.0 * b + 6.0 * c) * x * x + 6.0
            - 2.0 * b
    } else if x < 2.0 {
        (-b - 6.0 * c) * x.powi(3)
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + 8.0 * b
            + 24.0 * c
    } else {
        0.0
    };
    v / 6.0
}

fn lanczos(x: f64, n: i32) -> f64 {
    let n = f64::from(n);
    if x == 0.0 {
        1.0
    } else if x.abs() < n {
        let p = std::f64::consts::PI * x;
        n * p.sin() * (p / n).sin() / (p * p)
    } else {
        0.0
    }
}

/// Edge model (environment `EDGE`): `transparent` (default) reads
/// transparency outside the canvas. The others treat the pattern box as
/// the whole source image and keep only output pixels whose centre maps
/// inside the box (a hard edge): `clamp` replicates its border, `mirror`
/// reflects it, `renorm` leaves pixels outside the box out of the kernel
/// and renormalizes the weights of the rest.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Transparent,
    Clamp,
    Mirror,
    Renorm,
    /// Transparent outside the box (the layer is), with the hard edge.
    Zero,
}

fn edge() -> Edge {
    static EDGE: std::sync::OnceLock<Edge> = std::sync::OnceLock::new();
    *EDGE.get_or_init(|| match std::env::var("EDGE").as_deref() {
        Ok("clamp") => Edge::Clamp,
        Ok("mirror") => Edge::Mirror,
        Ok("renorm") => Edge::Renorm,
        Ok("zero") => Edge::Zero,
        _ => Edge::Transparent,
    })
}

fn clamp_edges() -> bool {
    edge() != Edge::Transparent
}

/// Whether source pixel `(x, y)` is in the pattern box.
fn in_box(x: i64, y: i64) -> bool {
    (i64::from(X0)..i64::from(X0 + side())).contains(&x)
        && (i64::from(Y0)..i64::from(Y0 + side())).contains(&y)
}

fn texel(src: &[[f64; 4]], x: i64, y: i64) -> [f64; 4] {
    let reflect = |v: i64, lo: i64, n: i64| {
        let p = (v - lo).rem_euclid(2 * n);
        lo + if p < n { p } else { 2 * n - 1 - p }
    };
    let (x, y) = match edge() {
        Edge::Clamp | Edge::Renorm => (
            x.clamp(i64::from(X0), i64::from(X0 + side() - 1)),
            y.clamp(i64::from(Y0), i64::from(Y0 + side() - 1)),
        ),
        Edge::Mirror => (
            reflect(x, i64::from(X0), i64::from(side())),
            reflect(y, i64::from(Y0), i64::from(side())),
        ),
        Edge::Transparent | Edge::Zero => (x, y),
    };
    if x < 0 || y < 0 || x >= i64::from(W) || y >= i64::from(H) {
        return [0.0; 4];
    }
    src[(y as u32 * W + x as u32) as usize]
}

/// Samples at continuous source position `(u, v)` (pixel centres at
/// `i + 0.5`).
fn sample(src: &[[f64; 4]], f: Filter, u: f64, v: f64) -> [f64; 4] {
    let kernel = |f: Filter, d: f64| match f {
        Filter::Cubic(a) => keys(d, a),
        Filter::Bc(b, c) => bc(d, b, c),
        Filter::Lanczos(n) => lanczos(d, n),
        _ => unreachable!(),
    };
    match f {
        Filter::Nearest => {
            let o: f64 = std::env::var("NOFF")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            texel(src, (u + o).floor() as i64, (v + o).floor() as i64)
        }
        Filter::Bilinear | Filter::Area(_) => {
            let (u, v) = clamp_uv(u, v);
            let (x, y) = (u - 0.5, v - 0.5);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            let mut out = [0.0; 4];
            for (dx, dy, w) in [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let t = texel(src, x0 as i64 + dx, y0 as i64 + dy);
                for c in 0..4 {
                    out[c] += w * t[c];
                }
            }
            out
        }
        Filter::Cubic(_) | Filter::Bc(..) | Filter::Lanczos(_) => {
            let r = match f {
                Filter::Lanczos(n) => i64::from(n),
                _ => 2,
            };
            let (u, v) = if std::env::var("CLAMPUV").is_ok() {
                (
                    u.clamp(f64::from(X0) + 0.5, f64::from(X0 + side()) - 0.5),
                    v.clamp(f64::from(Y0) + 0.5, f64::from(Y0 + side()) - 0.5),
                )
            } else {
                (u, v)
            };
            let (x, y) = (u - 0.5, v - 0.5);
            let (x0, y0) = (x.floor() as i64, y.floor() as i64);
            let mut out = [0.0; 4];
            let mut total = 0.0;
            for j in y0 - r + 1..=y0 + r {
                let wy = kernel(f, y - j as f64);
                for i in x0 - r + 1..=x0 + r {
                    if edge() == Edge::Renorm && !in_box(i, j) {
                        continue;
                    }
                    let w = wy * kernel(f, x - i as f64);
                    total += w;
                    let t = texel(src, i, j);
                    for c in 0..4 {
                        out[c] += w * t[c];
                    }
                }
            }
            out.map(|v| v / total)
        }
        Filter::CubicClamp(a, n) => {
            let mut out = sample(src, Filter::Cubic(a), u, v);
            let (x0, y0) = ((u - 0.5).floor() as i64, (v - 0.5).floor() as i64);
            let r = n / 2;
            let (mut lo, mut hi) = ([f64::MAX; 4], [f64::MIN; 4]);
            for j in y0 - r + 1..=y0 + r {
                for i in x0 - r + 1..=x0 + r {
                    let t = texel(src, i, j);
                    for c in 0..4 {
                        lo[c] = lo[c].min(t[c]);
                        hi[c] = hi[c].max(t[c]);
                    }
                }
            }
            for c in 0..4 {
                out[c] = out[c].clamp(lo[c], hi[c]);
            }
            out
        }
    }
}

/// Renders the transformed pattern as premultiplied 8-bit RGBA.
fn render(m: &Mapping, f: Filter, premultiplied: bool) -> Vec<[u8; 4]> {
    let mut src = source(premultiplied);
    // With `PYRAMID=1`: halve the box (2 × 2 averages, anchored at its
    // corner) while the scale is at most ½ on both axes, then sample the
    // reduced level.
    let mut levels = 0;
    if std::env::var("PYRAMID").is_ok() {
        let mut s = m.s[0].abs().max(m.s[1].abs());
        let mut side = SIDE;
        while s <= 0.5 && side > 1 {
            let half = side / 2;
            let mut next = vec![[0.0; 4]; (W * H) as usize];
            for j in 0..half {
                for i in 0..half {
                    let mut acc = [0.0; 4];
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (x, y) = (X0 + 2 * i + dx, Y0 + 2 * j + dy);
                        let t = src[(y as u32 * W + x as u32) as usize];
                        for c in 0..4 {
                            acc[c] += t[c] / 4.0;
                        }
                    }
                    next[((Y0 + j) as u32 * W + (X0 + i) as u32) as usize] = acc;
                }
            }
            src = next;
            side = half;
            s *= 2.0;
            levels += 1;
        }
        SAMPLED_SIDE.with(|c| c.set(side));
    }
    let k = f64::from(1u32 << levels);
    let n = match f {
        Filter::Area(n) => n,
        _ => 1,
    };
    let mut out = vec![[0u8; 4]; (W * H) as usize];
    // `COVER=1`: the edge is anti-aliased by the exact area of the pixel
    // inside the transformed box instead of cut at pixel centres.
    let cover = std::env::var("COVER").is_ok();
    // `SUPER=<n>`: pixels the box edge crosses are averaged over n × n
    // sub-samples (those outside the box transparent); the others take
    // one sample. `SUPERCLAMP=1` clamps each sub-sample's alpha first.
    let sup: Option<u32> = std::env::var("SUPER").ok().and_then(|v| v.parse().ok());
    let sup_clamp = std::env::var("SUPERCLAMP").is_ok();
    for y in 0..H {
        for x in 0..W {
            let mut acc = [0.0; 4];
            let partial = sup.is_some() && {
                let c = coverage(m, x, y);
                c > 0.0 && c < 1.0
            };
            let n = if partial { sup.unwrap_or(1) } else { n };
            for j in 0..n {
                for i in 0..n {
                    let (sx, sy) = (
                        f64::from(x) + (f64::from(i) + 0.5) / f64::from(n),
                        f64::from(y) + (f64::from(j) + 0.5) / f64::from(n),
                    );
                    let (u, v) = m.inverse(sx, sy);
                    let inside = (f64::from(X0)..f64::from(X0 + SIDE)).contains(&u)
                        && (f64::from(Y0)..f64::from(Y0 + SIDE)).contains(&v);
                    if clamp_edges() && !inside && !cover {
                        continue;
                    }
                    let (u, v) = (
                        f64::from(X0) + (u - f64::from(X0)) / k,
                        f64::from(Y0) + (v - f64::from(Y0)) / k,
                    );
                    let mut s = sample(&src, f, u, v);
                    if partial && sup_clamp && s[3] > 1.0 {
                        s = s.map(|c| c / s[3]);
                    }
                    for c in 0..4 {
                        acc[c] += s[c] / f64::from(n * n);
                    }
                }
            }
            let cov = match std::env::var("COVER").as_deref() {
                Ok("scan") => scan_coverage(m, x, y),
                Ok("dist") => distance_coverage(m, x, y),
                Ok(_) => coverage(m, x, y),
                Err(_) => 1.0,
            };
            let a = acc[3].clamp(0.0, 1.0) * cov;
            if premultiplied && std::env::var("OUT").is_ok_and(|v| v == "unpremul") {
                // Unpremultiplied by the raw (unclamped) alpha, clamped and
                // stored straight at 8 bits; shown as straight × alpha.
                let a8 = (a * 255.0).round();
                let straight = |c: f64| {
                    if acc[3] > 0.0 {
                        ((c / acc[3]).clamp(0.0, 1.0) * 255.0).round()
                    } else {
                        0.0
                    }
                };
                let p = |c: f64| (straight(c) * a8 / 255.0).round() as u8;
                out[(y * W + x) as usize] = [p(acc[0]), p(acc[1]), p(acc[2]), a8 as u8];
                continue;
            }
            let k = if premultiplied { 1.0 } else { a };
            out[(y * W + x) as usize] = [
                ((acc[0] * k).clamp(0.0, a) * 255.0).round() as u8,
                ((acc[1] * k).clamp(0.0, a) * 255.0).round() as u8,
                ((acc[2] * k).clamp(0.0, a) * 255.0).round() as u8,
                (a * 255.0).round() as u8,
            ];
        }
    }
    out
}

/// `x' = p + t + R(θ)·S·(x − p)`.
struct Mapping {
    s: [f64; 2],
    theta: f64,
    t: [f64; 2],
    p: [f64; 2],
}

impl Mapping {
    fn forward(&self, u: f64, v: f64) -> (f64, f64) {
        let (dx, dy) = ((u - self.p[0]) * self.s[0], (v - self.p[1]) * self.s[1]);
        let (c, s) = (self.theta.cos(), self.theta.sin());
        (
            self.p[0] + self.t[0] + c * dx - s * dy,
            self.p[1] + self.t[1] + s * dx + c * dy,
        )
    }

    fn inverse(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.p[0] - self.t[0], y - self.p[1] - self.t[1]);
        let (c, s) = (self.theta.cos(), self.theta.sin());
        // R(−θ) with y down: clockwise on screen is positive θ.
        let (rx, ry) = (c * dx + s * dy, -s * dx + c * dy);
        (rx / self.s[0] + self.p[0], ry / self.s[1] + self.p[1])
    }
}

/// `nearest`, `bilinear`, `cubic:<a>`, `bc:<b>:<c>`, `lanczos:<n>`,
/// `area:<n>`.
fn parse_filter(s: &str) -> Filter {
    let mut parts = s.split(':');
    let name = parts.next().unwrap_or_default();
    let mut num = || -> f64 {
        parts
            .next()
            .expect("a parameter")
            .parse()
            .expect("a number")
    };
    match name {
        "nearest" => Filter::Nearest,
        "bilinear" => Filter::Bilinear,
        "cubic" => Filter::Cubic(num()),
        "bc" => Filter::Bc(num(), num()),
        "lanczos" => Filter::Lanczos(num() as i32),
        "area" => Filter::Area(num() as u32),
        "cclamp" => Filter::CubicClamp(num(), num() as i64),
        other => panic!("unknown filter {other}"),
    }
}

fn arg(args: &[String], i: usize) -> f64 {
    args[i].parse().expect("a number")
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 => write_psd(
            Path::new(&args[2]),
            W,
            H,
            &[
                layer("black", vec![[0, 0, 0, 255]; (W * H) as usize]),
                layer("white", vec![[255; 4]; (W * H) as usize]),
                layer("ink", pattern()),
            ],
        ),
        Some("decode") if args.len() == 5 => {
            let (w, h, white) = read_png(&args[2])?;
            let (_, _, black) = read_png(&args[3])?;
            let out: Vec<[u8; 4]> = white
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
        Some("fit") if args.len() == 8 || args.len() == 10 => {
            let (_, _, measured) = read_png(&args[2])?;
            let p = if args.len() == 10 {
                [arg(&args, 8), arg(&args, 9)]
            } else {
                [200.0, 150.0]
            };
            let m = Mapping {
                s: [arg(&args, 3), arg(&args, 4)],
                theta: arg(&args, 5).to_radians(),
                t: [arg(&args, 6), arg(&args, 7)],
                p,
            };
            let filters = [
                Filter::Nearest,
                Filter::Bilinear,
                Filter::Cubic(-0.5),
                Filter::Cubic(-0.75),
                Filter::Cubic(-1.0),
                Filter::Bc(1.0 / 3.0, 1.0 / 3.0),
                Filter::Bc(1.0, 0.0),
                Filter::Lanczos(2),
                Filter::Lanczos(3),
                Filter::Area(4),
                Filter::CubicClamp(-1.0, 2),
                Filter::CubicClamp(-1.0, 4),
                Filter::CubicClamp(-0.75, 2),
                Filter::CubicClamp(-0.5, 2),
            ];
            let mut rows = Vec::new();
            for f in filters {
                for premultiplied in [true, false] {
                    let r = render(&m, f, premultiplied);
                    let (mut max, mut sum, mut n, mut over2) = (0, 0u64, 0u64, 0);
                    for (a, b) in r.iter().zip(&measured) {
                        if a[3] == 0 && b[3] == 0 {
                            continue;
                        }
                        let e = (0..4)
                            .map(|c| (i32::from(a[c]) - i32::from(b[c])).abs())
                            .max()
                            .unwrap_or(0);
                        max = max.max(e);
                        sum += e as u64;
                        n += 1;
                        if e > 2 {
                            over2 += 1;
                        }
                    }
                    rows.push((
                        sum as f64 / n.max(1) as f64,
                        max,
                        over2,
                        n,
                        f,
                        premultiplied,
                    ));
                }
            }
            rows.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (mean, max, over2, n, f, pm) in rows {
                println!(
                    "{:<28} {:<8} mean {mean:7.3} max {max:3} >2: {over2:5} of {n}",
                    format!("{f:?}"),
                    if pm { "premul" } else { "straight" }
                );
            }
            Ok(())
        }
        Some("render") if args.len() == 10 => {
            let m = Mapping {
                s: [arg(&args, 3), arg(&args, 4)],
                theta: arg(&args, 5).to_radians(),
                t: [arg(&args, 6), arg(&args, 7)],
                p: [200.0, 150.0],
            };
            write_png(
                &args[2],
                W,
                H,
                &render(&m, parse_filter(&args[8]), args[9] == "premul"),
            )
        }
        Some("move") if args.len() >= 9 => {
            // move <decoded> <x0> <y0> <x1> <y1> <dx> <dy> [feather] [rule]:
            // the pattern moved through a rectangle selection (Mirae's
            // selection model, the reference's measured feather). `rule`:
            // `split` (default) leaves orig·(1 − s) and moves orig·s over
            // it; `cut` leaves orig where s < 1 (hole only at s = 255).
            let (_, _, measured) = read_png(&args[2])?;
            let r = [arg(&args, 3), arg(&args, 4), arg(&args, 5), arg(&args, 6)];
            let (dx, dy) = (arg(&args, 7) as i64, arg(&args, 8) as i64);
            let mut doc = tela_document::Document::new(W, H);
            doc.select(
                &tela_document::SelectionShape::Rect(
                    [r[0] as f32, r[1] as f32],
                    [r[2] as f32, r[3] as f32],
                ),
                false,
                tela_document::SelectionOp::Replace,
            );
            if let Some(f) = args.get(9).and_then(|v| v.parse::<f32>().ok()) {
                doc.feather_selection(f);
            }
            let cut = args.get(10).is_some_and(|v| v == "cut");
            let src = source(true);
            let s = |x: i64, y: i64| {
                if x < 0 || y < 0 || x >= i64::from(W) || y >= i64::from(H) {
                    0.0
                } else {
                    f64::from(doc.selection_coverage(x as i32, y as i32)) / 255.0
                }
            };
            let (mut max, mut sum, mut n) = (0, 0u64, 0u64);
            let mut worst = (0, 0);
            for y in 0..i64::from(H) {
                for x in 0..i64::from(W) {
                    let o = texel(&src, x, y);
                    let keep = if cut && s(x, y) < 1.0 {
                        1.0
                    } else {
                        1.0 - s(x, y)
                    };
                    let rest = o.map(|c| c * keep);
                    let (ux, uy) = (x - dx, y - dy);
                    let mv = texel(&src, ux, uy).map(|c| c * s(ux, uy));
                    let out: Vec<u8> = (0..4)
                        .map(|c| ((mv[c] + rest[c] * (1.0 - mv[3])) * 255.0).round() as u8)
                        .collect();
                    let m = measured[(y as u32 * W + x as u32) as usize];
                    if out[3] == 0 && m[3] == 0 {
                        continue;
                    }
                    let e = (0..4)
                        .map(|c| (i32::from(out[c]) - i32::from(m[c])).abs())
                        .max()
                        .unwrap_or(0);
                    if e > max {
                        max = e;
                        worst = (x, y);
                    }
                    sum += e as u64;
                    n += 1;
                }
            }
            println!(
                "mean {:.3} max {max} at {worst:?} over {n} pixels",
                sum as f64 / n.max(1) as f64
            );
            Ok(())
        }
        Some("bbox") if args.len() == 3 => {
            let (w, h, px) = read_png(&args[2])?;
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            for y in 0..h {
                for x in 0..w {
                    if px[(y * w + x) as usize][3] > 0 {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                    }
                }
            }
            println!("x {x0}..{x1} ({}) y {y0}..{y1} ({})", x1 - x0, y1 - y0);
            Ok(())
        }
        Some("diff") if args.len() == 4 => {
            // Error per 8 × 8 block between two decoded images, as a map
            // of the largest channel difference.
            let (w, h, a) = read_png(&args[2])?;
            let (_, _, b) = read_png(&args[3])?;
            for by in 0..h / 8 {
                let mut line = String::new();
                for bx in 0..w / 8 {
                    let mut e = 0;
                    for y in by * 8..by * 8 + 8 {
                        for x in bx * 8..bx * 8 + 8 {
                            let (p, q) = (a[(y * w + x) as usize], b[(y * w + x) as usize]);
                            for c in 0..4 {
                                e = e.max((i32::from(p[c]) - i32::from(q[c])).abs());
                            }
                        }
                    }
                    line.push(match e {
                        0 => '.',
                        1..=2 => ',',
                        3..=8 => 'o',
                        9..=32 => 'O',
                        _ => '#',
                    });
                }
                if line.chars().any(|c| c != '.') {
                    println!("{:3} {line}", by * 8);
                }
            }
            Ok(())
        }
        Some("block") if args.len() == 7 || args.len() == 8 => {
            let (w, _, px) = read_png(&args[2])?;
            let ch = if args.len() == 8 {
                arg(&args, 7) as usize
            } else {
                3
            };
            let (x0, y0) = (arg(&args, 3) as u32, arg(&args, 4) as u32);
            for y in y0..y0 + arg(&args, 6) as u32 {
                let line: Vec<String> = (x0..x0 + arg(&args, 5) as u32)
                    .map(|x| format!("{:3}", px[(y * w + x) as usize][ch]))
                    .collect();
                println!("{}", line.join(" "));
            }
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: transform_probe psd <path> | decode <white.png> <black.png> <out.png> | \
                 fit <decoded.png> <sx> <sy> <deg> <tx> <ty> [px py] | \
                 render <out.png> <sx> <sy> <deg> <tx> <ty> <nearest|bilinear|cubic> <premul|straight> | \
                 block <decoded.png> <x0> <y0> <w> <h> [channel]"
            );
            std::process::exit(2);
        }
    }
}

/// Fraction of output pixel `(x, y)` inside the transformed pattern box
/// (16 × 16 sub-samples).
fn coverage(m: &Mapping, x: u32, y: u32) -> f64 {
    let mut n = 0;
    for j in 0..16 {
        for i in 0..16 {
            let (u, v) = m.inverse(
                f64::from(x) + (f64::from(i) + 0.5) / 16.0,
                f64::from(y) + (f64::from(j) + 0.5) / 16.0,
            );
            if (f64::from(X0)..f64::from(X0 + SIDE)).contains(&u)
                && (f64::from(Y0)..f64::from(Y0 + SIDE)).contains(&v)
            {
                n += 1;
            }
        }
    }
    f64::from(n) / 256.0
}

/// Like [`coverage`], on 16 sub-scanlines with exact horizontal coverage
/// (`COVER=scan`).
fn scan_coverage(m: &Mapping, x: u32, y: u32) -> f64 {
    let corners = [
        (f64::from(X0), f64::from(Y0)),
        (f64::from(X0 + SIDE), f64::from(Y0)),
        (f64::from(X0 + SIDE), f64::from(Y0 + SIDE)),
        (f64::from(X0), f64::from(Y0 + SIDE)),
    ]
    .map(|(u, v)| m.forward(u, v));
    let mut total = 0.0;
    for j in 0..16 {
        let sy = f64::from(y) + (f64::from(j) + 0.5) / 16.0;
        let mut xs = Vec::new();
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            if (a.1 <= sy) != (b.1 <= sy) {
                xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        if xs.len() == 2 {
            let (lo, hi) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
            total += (hi.min(f64::from(x) + 1.0) - lo.max(f64::from(x))).max(0.0);
        }
    }
    total / 16.0
}

/// Edge anti-aliasing by distance (`COVER=dist`): for each side of the
/// transformed box, `clamp(0.5 + d)` with `d` the signed distance (output
/// pixels, positive inside) from the pixel centre to that side; the
/// product over the sides.
fn distance_coverage(m: &Mapping, x: u32, y: u32) -> f64 {
    let corners = [
        (f64::from(X0), f64::from(Y0)),
        (f64::from(X0 + SIDE), f64::from(Y0)),
        (f64::from(X0 + SIDE), f64::from(Y0 + SIDE)),
        (f64::from(X0), f64::from(Y0 + SIDE)),
    ]
    .map(|(u, v)| m.forward(u, v));
    let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
    let centre = (
        corners.iter().map(|c| c.0).sum::<f64>() / 4.0,
        corners.iter().map(|c| c.1).sum::<f64>() / 4.0,
    );
    let mut cov = 1.0;
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = (ex * ex + ey * ey).sqrt();
        let side = |p: (f64, f64)| (ex * (p.1 - a.1) - ey * (p.0 - a.0)) / len;
        let sign = side(centre).signum();
        cov *= (0.5 + sign * side((px, py))).clamp(0.0, 1.0);
    }
    cov
}

/// With `CLAMPUV`: the sample position limited to the centres of the
/// box's edge pixels.
fn clamp_uv(u: f64, v: f64) -> (f64, f64) {
    if std::env::var("CLAMPUV").is_ok() {
        (
            u.clamp(f64::from(X0) + 0.5, f64::from(X0 + side()) - 0.5),
            v.clamp(f64::from(Y0) + 0.5, f64::from(Y0 + side()) - 0.5),
        )
    } else {
        (u, v)
    }
}

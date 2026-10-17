//! Parity probe for gradients (phase 5F, docs/parity/5F-gradients.md).
//!
//! `psd <kind> <path>` writes a 400 × 300 test document of our own. Every
//! kind has an opaque black layer "black" at the bottom and an opaque white
//! layer "white" above it (hiding "white" shows a layer over black, which
//! gives its premultiplied pixels, as in 5D and 5E); then:
//!
//! - `plain`: an empty layer "ink" to drag gradients on.
//! - `over`: layer "ink" with an opaque red block, a translucent (alpha
//!   128) green block and an alpha ramp of blue, transparency elsewhere,
//!   for gradients over existing pixels and the alpha lock.
//! - `mask`: layer "ink" opaque grey 128 whose mask hides everything, and
//!   a hidden layer "shown" opaque grey 128 whose mask reveals everything,
//!   for gradients on masks.
//! - `clip`: layer "base" (an opaque disc) and an empty layer "ink"
//!   clipped to it.
//! - `mode`: layer "under" (vertical colour bars) and an empty layer "ink"
//!   in Multiply at 50 % opacity.
//!
//! Analysis of captures (decoded with `decode`, as `fill_probe decode`):
//! `cols <png> y0 y1` and `rows <png> x0 x1` print the mean and standard
//! deviation of each channel per column (row) over a band; `ring <png> cx
//! cy` per whole distance from a centre; `angle <png> cx cy r0 r1` per
//! degree around it; `noise <png> x0 y0 x1 y1` the residual of every
//! pixel from its column's mean (histogram, neighbour correlation, levels
//! per column); `diff <a> <b>` compares two captures; `block` prints one
//! channel.

use std::fs::File;
use std::path::Path;

use tela_corpus::psd::{PsdLayer, PsdMask, write_psd};

const W: u32 = 400;
const H: u32 = 300;
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

fn mask(default: u8) -> PsdMask {
    PsdMask {
        rect: (0, 0, W, H),
        values: vec![default; (W * H) as usize],
        default,
        disabled: false,
    }
}

fn layers(kind: &str) -> Option<Vec<PsdLayer>> {
    let mut l = vec![
        layer("black", canvas(BLACK)),
        layer("white", canvas([255; 4])),
    ];
    match kind {
        "plain" => l.push(layer("ink", canvas(CLEAR))),
        "over" => {
            let mut ink = canvas(CLEAR);
            rect(&mut ink, 20, 20, 180, 140, [255, 0, 0, 255]);
            rect(&mut ink, 220, 20, 380, 140, [0, 255, 0, 128]);
            for x in 20..276 {
                rect(&mut ink, x, 170, x + 1, 260, [0, 0, 255, (x - 20) as u8]);
            }
            l.push(layer("ink", ink));
        }
        "mask" => {
            let mut shown = layer("shown", canvas([128, 128, 128, 255]));
            shown.hidden = true;
            shown.mask = Some(mask(255));
            l.push(shown);
            let mut ink = layer("ink", canvas([128, 128, 128, 255]));
            ink.mask = Some(mask(0));
            l.push(ink);
        }
        "clip" => {
            let mut base = canvas(CLEAR);
            for y in 0..H as i32 {
                for x in 0..W as i32 {
                    let (dx, dy) = (x as f64 + 0.5 - 200.0, y as f64 + 0.5 - 150.0);
                    if dx * dx + dy * dy <= 100.0 * 100.0 {
                        set(&mut base, x, y, [40, 40, 40, 255]);
                    }
                }
            }
            l.push(layer("base", base));
            let mut ink = layer("ink", canvas(CLEAR));
            ink.clipped = true;
            l.push(ink);
        }
        "mode" => {
            let mut under = canvas(CLEAR);
            let bars = [
                [255, 255, 255, 255],
                [255, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [128, 128, 128, 255],
            ];
            for (i, c) in bars.iter().enumerate() {
                rect(
                    &mut under,
                    0,
                    60 * i as i32,
                    W as i32,
                    60 * (i as i32 + 1),
                    *c,
                );
            }
            l.push(layer("under", under));
            let mut ink = layer("ink", canvas(CLEAR));
            ink.mode = *b"mul ";
            ink.opacity = 128;
            l.push(ink);
        }
        _ => return None,
    }
    Some(l)
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

/// Mean and standard deviation of each channel of `pixels`.
fn stats<'a>(pixels: impl Iterator<Item = &'a [u8; 4]>) -> ([f64; 4], [f64; 4], usize) {
    let mut sum = [0.0f64; 4];
    let mut sq = [0.0f64; 4];
    let mut n = 0usize;
    for p in pixels {
        for c in 0..4 {
            let v = f64::from(p[c]);
            sum[c] += v;
            sq[c] += v * v;
        }
        n += 1;
    }
    let k = n.max(1) as f64;
    let mean = sum.map(|s| s / k);
    let mut sd = [0.0; 4];
    for c in 0..4 {
        sd[c] = (sq[c] / k - mean[c] * mean[c]).max(0.0).sqrt();
    }
    (mean, sd, n)
}

fn print_stats(key: String, (m, s, n): ([f64; 4], [f64; 4], usize)) {
    println!(
        "{key}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{n}",
        m[0], m[1], m[2], m[3], s[0], s[1], s[2], s[3]
    );
}

const HEADER: &str = "mean_r\tmean_g\tmean_b\tmean_a\tsd_r\tsd_g\tsd_b\tsd_a\tn";

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let num = |k: usize| -> f64 { args[k].parse().expect("a number") };
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 4 => {
            let Some(l) = layers(&args[2]) else {
                eprintln!("unknown kind {}", args[2]);
                std::process::exit(2);
            };
            write_psd(Path::new(&args[3]), W, H, &l)
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
        Some("cols") if args.len() == 5 => {
            let (w, _, px) = read_png(&args[2])?;
            let (y0, y1) = (num(3) as u32, num(4) as u32);
            println!("x\t{HEADER}");
            for x in 0..w {
                let it = (y0..y1).map(|y| &px[(y * w + x) as usize]);
                print_stats(x.to_string(), stats(it));
            }
            Ok(())
        }
        Some("rows") if args.len() == 5 => {
            let (w, h, px) = read_png(&args[2])?;
            let (x0, x1) = (num(3) as u32, num(4) as u32);
            println!("y\t{HEADER}");
            for y in 0..h {
                let it = (x0..x1).map(|x| &px[(y * w + x) as usize]);
                print_stats(y.to_string(), stats(it));
            }
            Ok(())
        }
        Some("ring") if args.len() == 5 => {
            let (w, h, px) = read_png(&args[2])?;
            let (cx, cy) = (num(3), num(4));
            let mut buckets: Vec<Vec<[u8; 4]>> = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let d = (f64::from(x) + 0.5 - cx).hypot(f64::from(y) + 0.5 - cy);
                    let k = d.floor() as usize;
                    if buckets.len() <= k {
                        buckets.resize(k + 1, Vec::new());
                    }
                    buckets[k].push(px[(y * w + x) as usize]);
                }
            }
            println!("r\t{HEADER}");
            for (k, b) in buckets.iter().enumerate() {
                if !b.is_empty() {
                    print_stats(k.to_string(), stats(b.iter()));
                }
            }
            Ok(())
        }
        Some("angle") if args.len() == 7 => {
            let (w, h, px) = read_png(&args[2])?;
            let (cx, cy, r0, r1) = (num(3), num(4), num(5), num(6));
            let mut buckets: Vec<Vec<[u8; 4]>> = vec![Vec::new(); 360];
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
                    let d = dx.hypot(dy);
                    if d < r0 || d >= r1 {
                        continue;
                    }
                    let a = dy.atan2(dx).to_degrees().rem_euclid(360.0);
                    buckets[(a.floor() as usize).min(359)].push(px[(y * w + x) as usize]);
                }
            }
            println!("deg\t{HEADER}");
            for (k, b) in buckets.iter().enumerate() {
                print_stats(k.to_string(), stats(b.iter()));
            }
            Ok(())
        }
        Some("noise") if args.len() == 7 => {
            let (w, _, px) = read_png(&args[2])?;
            let (x0, y0, x1, y1) = (num(3) as u32, num(4) as u32, num(5) as u32, num(6) as u32);
            // Residual of each pixel's red channel from its column mean.
            let mut res = vec![0.0f64; ((x1 - x0) * (y1 - y0)) as usize];
            let rw = (x1 - x0) as usize;
            let mut levels_per_col = Vec::new();
            for x in x0..x1 {
                let col: Vec<f64> = (y0..y1)
                    .map(|y| f64::from(px[(y * w + x) as usize][0]))
                    .collect();
                let m = col.iter().sum::<f64>() / col.len() as f64;
                let mut lv: Vec<u8> = (y0..y1).map(|y| px[(y * w + x) as usize][0]).collect();
                lv.sort_unstable();
                lv.dedup();
                levels_per_col.push(lv.len());
                for (j, v) in col.iter().enumerate() {
                    res[j * rw + (x - x0) as usize] = v - m;
                }
            }
            let mut hist = std::collections::BTreeMap::new();
            for r in &res {
                *hist.entry((r.round()) as i64).or_insert(0u64) += 1;
            }
            let var = res.iter().map(|r| r * r).sum::<f64>() / res.len() as f64;
            let corr = |dx: usize, dy: usize| {
                let mut s = 0.0;
                let mut n = 0.0;
                for j in 0..(y1 - y0) as usize - dy {
                    for i in 0..rw - dx {
                        s += res[j * rw + i] * res[(j + dy) * rw + i + dx];
                        n += 1.0;
                    }
                }
                s / n / var
            };
            println!("residual sd {:.4}", var.sqrt());
            println!("residual histogram (rounded): {hist:?}");
            println!(
                "correlation right {:.4}, down {:.4}, right 2 {:.4}, down 2 {:.4}",
                corr(1, 0),
                corr(0, 1),
                corr(2, 0),
                corr(0, 2)
            );
            let mut lc = std::collections::BTreeMap::new();
            for l in &levels_per_col {
                *lc.entry(*l).or_insert(0u32) += 1;
            }
            println!("distinct levels per column -> columns: {lc:?}");
            Ok(())
        }
        Some("bayer") if args.len() >= 7 => {
            // For columns holding two levels L and L + 1 (fraction f of
            // pixels at L + 1), each (x mod 4, y mod 4) cell's threshold
            // lies between the largest f where it stayed at L and the
            // smallest f where it rose.
            let (w, _, px) = read_png(&args[2])?;
            let (x0, x1, y0, y1) = (num(3) as u32, num(4) as u32, num(5) as u32, num(6) as u32);
            let n = args.get(7).map_or(4, |v| v.parse::<u32>().expect("period"));
            let mut lo = vec![vec![0.0f64; n as usize]; n as usize];
            let mut hi = vec![vec![1.0f64; n as usize]; n as usize];
            for x in x0..x1 {
                let col: Vec<u8> = (y0..y1).map(|y| px[(y * w + x) as usize][0]).collect();
                let min = *col.iter().min().unwrap_or(&0);
                if col.iter().any(|&v| v > min + 1) {
                    continue;
                }
                let f = col.iter().filter(|&&v| v > min).count() as f64 / col.len() as f64;
                for (k, &v) in col.iter().enumerate() {
                    let cell = [(x % n) as usize, ((y0 + k as u32) % n) as usize];
                    let (i, j) = (cell[0], cell[1]);
                    if v > min {
                        hi[j][i] = hi[j][i].min(f);
                    } else {
                        lo[j][i] = lo[j][i].max(f);
                    }
                }
            }
            for j in 0..n as usize {
                let row: Vec<String> = (0..n as usize)
                    .map(|i| format!("{:.2}-{:.2}", lo[j][i], hi[j][i]))
                    .collect();
                println!("y%{n}={j}: {}", row.join(" "));
            }
            Ok(())
        }
        Some("diff") if args.len() == 4 => {
            let (_, _, a) = read_png(&args[2])?;
            let (_, _, b) = read_png(&args[3])?;
            let mut differ = 0usize;
            let mut max = 0u8;
            for (p, q) in a.iter().zip(&b) {
                let d = (0..4).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
                if d > 0 {
                    differ += 1;
                }
                max = max.max(d);
            }
            println!("pixels {} differ {differ} max {max}", a.len());
            Ok(())
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
                "usage: gradient_probe psd <plain|over|mask|clip|mode> <path> | decode <white> <black> <out> | cols <png> y0 y1 | rows <png> x0 x1 | ring <png> cx cy | angle <png> cx cy r0 r1 | noise <png> x0 y0 x1 y1 | diff <a> <b> | block <png> x y w h [ch]"
            );
            std::process::exit(2);
        }
    }
}

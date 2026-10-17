//! Parity probe for selections (phase 5C, docs/parity/5C-selection.md).
//!
//! `psd <path> [width height]` writes a test document of our own: an
//! opaque white layer "paper" under an empty layer "ink". In the
//! reference a selection is made, the ink layer is filled with black
//! through it (or painted), and the document is exported to PNG; over the
//! white layer every pixel's coverage is `255 − red`.
//!
//! `row <png> <y> <x0> <x1>` and `col <png> <x> <y0> <y1>` print the
//! coverage along a line; `block <png> <x0> <y0> <w> <h>` prints a block;
//! `stats <png>` prints the covered bounds, the number of partial pixels
//! and a histogram of coverage values.

use std::fs::File;
use std::path::Path;

use tela_corpus::psd::{PsdLayer, write_psd};

struct Image {
    width: u32,
    height: u32,
    /// Coverage `255 − red` (red of the composite over the white layer),
    /// or the alpha when the export is transparent and has no paper.
    values: Vec<u8>,
}

fn load(path: &str) -> std::io::Result<Image> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().expect("png size fits")];
    let frame = reader.next_frame(&mut buf)?;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported PNG color type {other:?}"),
    };
    let values = buf[..frame.buffer_size()]
        .chunks_exact(channels)
        .map(|p| {
            if channels == 4 && p[3] < 255 {
                p[3]
            } else {
                255 - p[0]
            }
        })
        .collect();
    Ok(Image {
        width: frame.width,
        height: frame.height,
        values,
    })
}

impl Image {
    fn at(&self, x: u32, y: u32) -> u8 {
        self.values[(y * self.width + x) as usize]
    }
}

fn arg(args: &[String], i: usize) -> u32 {
    args[i].parse().expect("a number")
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 || args.len() == 5 => {
            let (w, h) = if args.len() == 5 {
                (arg(&args, 3), arg(&args, 4))
            } else {
                (400, 300)
            };
            let layer = |name: &str, px: [u8; 4]| PsdLayer {
                name: name.into(),
                mode: *b"norm",
                opacity: 255,
                clipped: false,
                hidden: false,
                rect: (0, 0, w, h),
                rgba: vec![px; (w * h) as usize],
                mask: None,
            };
            write_psd(
                Path::new(&args[2]),
                w,
                h,
                &[layer("paper", [255; 4]), layer("ink", [0; 4])],
            )
        }
        Some("row") if args.len() == 6 => {
            let img = load(&args[2])?;
            let y = arg(&args, 3);
            let line: Vec<String> = (arg(&args, 4)..arg(&args, 5))
                .map(|x| img.at(x, y).to_string())
                .collect();
            println!("{}", line.join(" "));
            Ok(())
        }
        Some("col") if args.len() == 6 => {
            let img = load(&args[2])?;
            let x = arg(&args, 3);
            let line: Vec<String> = (arg(&args, 4)..arg(&args, 5))
                .map(|y| img.at(x, y).to_string())
                .collect();
            println!("{}", line.join(" "));
            Ok(())
        }
        Some("block") if args.len() == 7 => {
            let img = load(&args[2])?;
            let (x0, y0) = (arg(&args, 3), arg(&args, 4));
            for y in y0..y0 + arg(&args, 6) {
                let line: Vec<String> = (x0..x0 + arg(&args, 5))
                    .map(|x| format!("{:3}", img.at(x, y)))
                    .collect();
                println!("{}", line.join(" "));
            }
            Ok(())
        }
        Some("stats") if args.len() == 3 => {
            let img = load(&args[2])?;
            let mut hist = [0u32; 256];
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
            for y in 0..img.height {
                for x in 0..img.width {
                    let v = img.at(x, y);
                    hist[v as usize] += 1;
                    if v > 0 {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                    }
                }
            }
            println!("size {}x{}", img.width, img.height);
            let area: f64 = img.values.iter().map(|&v| f64::from(v) / 255.0).sum();
            println!("coverage sum {area:.1} px");
            println!("bounds x {x0}..{x1} y {y0}..{y1}");
            println!(
                "zero {} full {} partial {}",
                hist[0],
                hist[255],
                hist[1..255].iter().sum::<u32>()
            );
            let partial: Vec<String> = (1..255)
                .filter(|&v| hist[v] > 0)
                .map(|v| format!("{v}:{}", hist[v]))
                .collect();
            println!("partial values {}", partial.join(" "));
            Ok(())
        }
        Some("fit-ellipse") if args.len() == 7 => {
            let img = load(&args[2])?;
            let (x0, y0, x1, y1) = (
                arg(&args, 3) as f64,
                arg(&args, 4) as f64,
                arg(&args, 5) as f64,
                arg(&args, 6) as f64,
            );
            let (cx, cy, rx, ry) = (
                (x0 + x1) / 2.0,
                (y0 + y1) / 2.0,
                (x1 - x0) / 2.0,
                (y1 - y0) / 2.0,
            );
            let inside = |x: f64, y: f64| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
            let grids = [
                (1u32, 1u32),
                (2, 2),
                (4, 4),
                (8, 8),
                (16, 16),
                (64, 64),
                (256, 2),
                (256, 3),
                (256, 4),
                (256, 5),
                (256, 8),
                (2, 256),
                (4, 256),
                (8, 4),
                (16, 4),
                (4, 8),
                (16, 2),
            ];
            for (nx, ny) in grids {
                let (mut max, mut sum, mut count, mut over) = (0i32, 0i64, 0, 0);
                for y in 0..img.height {
                    for x in 0..img.width {
                        let mut hits = 0;
                        for sy in 0..ny {
                            for sx in 0..nx {
                                let px = x as f64 + (sx as f64 + 0.5) / nx as f64;
                                let py = y as f64 + (sy as f64 + 0.5) / ny as f64;
                                hits += u32::from(inside(px, py));
                            }
                        }
                        let model = (255.0 * hits as f64 / (nx * ny) as f64).round() as i32;
                        let d = i32::from(img.at(x, y)) - model;
                        if d != 0 || img.at(x, y) != 0 {
                            max = max.max(d.abs());
                            sum += i64::from(d.abs());
                            count += 1;
                            over += i32::from(d.abs() > 2);
                        }
                    }
                }
                println!(
                    "{nx:3}x{ny:<3} samples: max {max}, mean {:.2}, pixels > 2 off: {over}",
                    sum as f64 / count.max(1) as f64
                );
            }
            Ok(())
        }
        Some("fit-inset") if args.len() == 7 => {
            // Anti-aliased ellipse: radii inset by `s`, 16×16 samples.
            let img = load(&args[2])?;
            let b = [3, 4, 5, 6].map(|i| arg(&args, i) as f64);
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            for k in 0..=20 {
                let s = k as f64 * 0.05;
                let (rx, ry) = ((b[2] - b[0]) / 2.0 - s, (b[3] - b[1]) / 2.0 - s);
                let (mut sum, mut count, mut max) = (0i64, 0, 0i32);
                for y in 0..img.height {
                    for x in 0..img.width {
                        let mut hits = 0;
                        for sy in 0..16 {
                            for sx in 0..16 {
                                let px = x as f64 + (sx as f64 + 0.5) / 16.0;
                                let py = y as f64 + (sy as f64 + 0.5) / 16.0;
                                hits += u32::from(
                                    ((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2) <= 1.0,
                                );
                            }
                        }
                        let model = (255.0 * f64::from(hits) / 256.0).round() as i32;
                        let v = i32::from(img.at(x, y));
                        if v != 0 || model != 0 {
                            let d = (v - model).abs();
                            sum += i64::from(d);
                            count += 1;
                            max = max.max(d);
                        }
                    }
                }
                println!(
                    "inset {s:.2}: mean {:.2} max {max}",
                    sum as f64 / f64::from(count.max(1))
                );
            }
            Ok(())
        }
        Some("fit-feather") if args.len() == 7 || args.len() == 8 => {
            // A blurred hard rectangle against a separable Gaussian of
            // standard deviation σ (searched), evaluated per pixel centre.
            let img = load(&args[2])?;
            let b = [3, 4, 5, 6].map(|i| arg(&args, i) as f64);
            let fit = |sigma: f64| {
                let step = |d: f64| 0.5 * (1.0 + erf(d / (sigma * std::f64::consts::SQRT_2)));
                let (mut max, mut sum, mut n) = (0i32, 0i64, 0);
                for y in 0..img.height {
                    for x in 0..img.width {
                        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                        let cov = (step(px - b[0]) - step(px - b[2]))
                            * (step(py - b[1]) - step(py - b[3]));
                        let model = (255.0 * cov).round() as i32;
                        let v = i32::from(img.at(x, y));
                        if v != 0 || model != 0 {
                            let d = (v - model).abs();
                            max = max.max(d);
                            sum += i64::from(d);
                            n += 1;
                        }
                    }
                }
                (max, sum as f64 / f64::from(n.max(1)))
            };
            let mut best = (f64::MAX, 0.0);
            let mut sigma = 0.05;
            while sigma < 40.0 {
                let (_, mean) = fit(sigma);
                if mean < best.0 {
                    best = (mean, sigma);
                }
                sigma += 0.01;
            }
            let (max, mean) = fit(best.1);
            println!("sigma {:.2}: mean {mean:.3}, max {max}", best.1);
            if let Some(s) = args.get(7) {
                let s: f64 = s.parse().expect("a number");
                let (max, mean) = fit(s);
                println!("sigma {s:.2} (given): mean {mean:.3}, max {max}");
            }
            Ok(())
        }
        Some("fit-kernel") if args.len() == 8 => {
            // A hard rectangle convolved with a sampled, normalized discrete
            // Gaussian (σ given; radius ⌈3σ⌉), separably.
            let img = load(&args[2])?;
            let b = [3, 4, 5, 6].map(|i| arg(&args, i) as i64);
            let sigma: f64 = args[7].parse().expect("a number");
            for radius_factor in [2.0, 2.5, 3.0, 3.5, 4.0] {
                let r = (radius_factor * sigma).ceil().max(1.0) as i64;
                let w: Vec<f64> = (-r..=r)
                    .map(|k| (-((k * k) as f64) / (2.0 * sigma * sigma)).exp())
                    .collect();
                let total: f64 = w.iter().sum();
                let line = |p: i64, lo: i64, hi: i64| -> f64 {
                    (-r..=r)
                        .filter(|k| (lo..hi).contains(&(p + k)))
                        .map(|k| w[(k + r) as usize])
                        .sum::<f64>()
                        / total
                };
                let (mut max, mut sum, mut n) = (0i32, 0i64, 0);
                for y in 0..img.height as i64 {
                    for x in 0..img.width as i64 {
                        let cov = line(x, b[0], b[2]) * line(y, b[1], b[3]);
                        let model = (255.0 * cov).round() as i32;
                        let v = i32::from(img.at(x as u32, y as u32));
                        if v != 0 || model != 0 {
                            let d = (v - model).abs();
                            max = max.max(d);
                            sum += i64::from(d);
                            n += 1;
                        }
                    }
                }
                println!(
                    "radius {r} ({radius_factor}σ): mean {:.3}, max {max}",
                    sum as f64 / f64::from(n.max(1))
                );
            }
            Ok(())
        }
        Some("fit-op") if args.len() == 6 => {
            // `fit-op <a.png> <b.png> <result.png> add|sub|int`: scores
            // candidate rules for combining partial coverages.
            let (a, b, r) = (load(&args[2])?, load(&args[3])?, load(&args[4])?);
            type Rule = (&'static str, fn(f64, f64) -> f64);
            let rules: &[Rule] = match args[5].as_str() {
                "add" => &[
                    ("max(a, b)", |a, b| a.max(b)),
                    ("a + b − ab", |a, b| a + b - a * b),
                    ("min(1, a + b)", |a, b| (a + b).min(1.0)),
                ],
                "sub" => &[
                    ("a(1 − b)", |a, b| a * (1.0 - b)),
                    ("max(0, a − b)", |a, b| (a - b).max(0.0)),
                    ("min(a, 1 − b)", |a, b| a.min(1.0 - b)),
                ],
                _ => &[
                    ("ab", |a, b| a * b),
                    ("min(a, b)", |a, b| a.min(b)),
                    ("max(0, a + b − 1)", |a, b| (a + b - 1.0).max(0.0)),
                    ("floor(ab)", |a, b| (a * b * 255.0 + 1e-9).floor() / 255.0),
                    ("(AB + 255) >> 8", |a, b| {
                        ((a * b * 65025.0).round() + 255.0).div_euclid(256.0) / 255.0
                    }),
                    ("ceil(ab)", |a, b| (a * b * 255.0 - 1e-9).ceil() / 255.0),
                ],
            };
            for (name, rule) in rules {
                let (mut max, mut sum, mut n, mut all_max) = (0i32, 0i64, 0, 0i32);
                for y in 0..a.height {
                    for x in 0..a.width {
                        let (va, vb) = (a.at(x, y), b.at(x, y));
                        let model = (255.0 * rule(f64::from(va) / 255.0, f64::from(vb) / 255.0))
                            .round() as i32;
                        let d = (i32::from(r.at(x, y)) - model).abs();
                        all_max = all_max.max(d);
                        if (1..255).contains(&va) && (1..255).contains(&vb) {
                            max = max.max(d);
                            sum += i64::from(d);
                            n += 1;
                        }
                    }
                }
                println!(
                    "{name:18} both partial ({n} px): max {max}, mean {:.2}; every pixel: max {all_max}",
                    sum as f64 / f64::from(n.max(1))
                );
            }
            Ok(())
        }
        Some("rule") if args.len() == 9 => {
            // `rule <s.png> <result.png> x0 y0 w h paint|paint2|erase`:
            // the result against a rule of the selection coverage `s`.
            let (s, r) = (load(&args[2])?, load(&args[3])?);
            let (x0, y0) = (arg(&args, 4), arg(&args, 5));
            let (mut max, mut sum, mut n) = (0i32, 0i64, 0);
            for y in y0..y0 + arg(&args, 7) {
                for x in x0..x0 + arg(&args, 6) {
                    let v = f64::from(s.at(x, y)) / 255.0;
                    let model = match args[8].as_str() {
                        "paint" => v,
                        "paint2" => v + v * (1.0 - v),
                        _ => 1.0 - v,
                    };
                    let d = (i32::from(r.at(x, y)) - (255.0 * model).round() as i32).abs();
                    max = max.max(d);
                    sum += i64::from(d);
                    n += 1;
                }
            }
            println!(
                "{} px: max {max}, mean {:.2}",
                n,
                sum as f64 / f64::from(n.max(1))
            );
            Ok(())
        }
        Some("fit-box") if args.len() == 7 => {
            // Hard-edged ellipse: which box (edges moved by quarter pixels)
            // does centre sampling reproduce best?
            let img = load(&args[2])?;
            let base = [3, 4, 5, 6].map(|i| arg(&args, i) as f64);
            let steps = [-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0];
            let mut best = (u32::MAX, [0.0; 4]);
            for a in steps {
                for b in steps {
                    for c in steps {
                        for d in steps {
                            let (x0, y0, x1, y1) =
                                (base[0] + a, base[1] + b, base[2] + c, base[3] + d);
                            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                            let (rx, ry) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
                            let mut wrong = 0;
                            for y in (base[1] as u32).saturating_sub(3)
                                ..(base[3] as u32 + 3).min(img.height)
                            {
                                for x in (base[0] as u32).saturating_sub(3)
                                    ..(base[2] as u32 + 3).min(img.width)
                                {
                                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                                    let inside =
                                        ((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2) <= 1.0;
                                    wrong += u32::from(inside != (img.at(x, y) > 127));
                                }
                            }
                            if wrong < best.0 {
                                best = (wrong, [x0, y0, x1, y1]);
                            }
                        }
                    }
                }
            }
            println!("best box {:?}: {} pixels differ", best.1, best.0);
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: selection_probe psd <path> [w h] | row <png> <y> <x0> <x1> | \
                 col <png> <x> <y0> <y1> | block <png> <x0> <y0> <w> <h> | stats <png>"
            );
            std::process::exit(2);
        }
    }
}

/// Error function (Abramowitz and Stegun 7.1.26, |error| < 1.5e-7).
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    y.copysign(x)
}

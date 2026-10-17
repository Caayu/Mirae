//! Which rule does the reference use over a translucent backdrop? (phase
//! 5B, D-4C-1)
//!
//! `backdrop_fit <measured.csv> [fit]` reads measured blend results (the
//! columns of `blend_probe` / `backdrop_probe`: mode, opacity, bottom and
//! top straight RGBA, measured straight RGBA), keeps the rows whose bottom
//! is translucent (0 < α < 255) and whose top is visible, and scores each
//! candidate model per mode: the largest and mean channel error (1/255) of
//! the colour where the measured alpha is not zero, and the largest alpha
//! error. With `fit`, it also solves, per mode and alpha pair, the least
//! squares weights of `C = wS·Cs + wB·B(Cb, Cs) + wC·Cb` and prints them
//! beside the weights each model predicts, to show where a model's
//! structure departs from the data.
//!
//! Every model is computed in `f64` on straight, sRGB-encoded values
//! (ADR 0016). The models are public formulas or plain algebraic
//! variations of them, never anything taken from the reference.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
struct Row {
    opacity: f64,
    bottom: [f64; 4],
    top: [f64; 4],
    measured: [u8; 4],
}

fn parse(path: &str) -> BTreeMap<String, Vec<Row>> {
    let text = std::fs::read_to_string(path).expect("measured CSV");
    let mut out: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for line in text.lines().skip(1) {
        let v: Vec<&str> = line.split(',').collect();
        let n = |i: usize| f64::from(v[i].parse::<u8>().expect("byte")) / 255.0;
        let row = Row {
            opacity: n(1),
            bottom: [n(4), n(5), n(6), n(7)],
            top: [n(8), n(9), n(10), n(11)],
            measured: [12, 13, 14, 15].map(|i| v[i].parse::<u8>().expect("byte")),
        };
        out.entry(v[0].to_string()).or_default().push(row);
    }
    out
}

// Blend functions, W3C Compositing and Blending Level 1 §9–10, in f64.

fn screen(b: f64, s: f64) -> f64 {
    b + s - b * s
}

fn hard_light(b: f64, s: f64) -> f64 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn soft_light(b: f64, s: f64) -> f64 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f64; 3]) -> [f64; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut c = c;
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}

fn set_lum(c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f64; 3]) -> f64 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f64; 3], s: f64) -> [f64; 3] {
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&a, &b| c[a].total_cmp(&c[b]));
    let (min, mid, max) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0; 3];
    if c[max] > c[min] {
        out[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        out[max] = s;
    }
    out
}

fn blend(mode: &str, b: [f64; 3], s: [f64; 3]) -> [f64; 3] {
    let sep = |f: fn(f64, f64) -> f64| [f(b[0], s[0]), f(b[1], s[1]), f(b[2], s[2])];
    match mode {
        "normal" => s,
        "multiply" => sep(|b, s| b * s),
        "screen" => sep(screen),
        "overlay" => sep(|b, s| hard_light(s, b)),
        "darken" => sep(f64::min),
        "lighten" => sep(f64::max),
        "color-dodge" => sep(|b, s| {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }),
        "color-burn" => sep(|b, s| {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }),
        "hard-light" => sep(hard_light),
        "soft-light" => sep(soft_light),
        "difference" => sep(|b, s| (b - s).abs()),
        "exclusion" => sep(|b, s| b + s - 2.0 * b * s),
        "add" => sep(|b, s| (b + s).min(1.0)),
        "hue" => set_lum(set_sat(s, sat(b)), lum(b)),
        "saturation" => set_lum(set_sat(b, sat(s)), lum(b)),
        "color" => set_lum(s, lum(b)),
        "luminosity" => set_lum(b, lum(s)),
        other => panic!("unknown mode {other}"),
    }
}

/// The inputs of one combination, straight and un-premultiplied.
struct Inputs {
    cs: [f64; 3],
    cb: [f64; 3],
    a_s: f64,
    a_b: f64,
    b: [f64; 3],
    /// The blend function applied to premultiplied values.
    b_premul: [f64; 3],
}

fn inputs(mode: &str, r: &Row) -> Inputs {
    let cs = [r.top[0], r.top[1], r.top[2]];
    let cb = [r.bottom[0], r.bottom[1], r.bottom[2]];
    Inputs {
        cs,
        cb,
        a_s: r.top[3] * r.opacity,
        a_b: r.bottom[3],
        b: blend(mode, cb, cs),
        b_premul: blend(
            mode,
            cb.map(|c| c * r.bottom[3]),
            cs.map(|c| c * r.top[3] * r.opacity),
        ),
    }
}

/// A model: straight colour and alpha from the inputs.
type Model = fn(&Inputs) -> ([f64; 3], f64);

fn union(i: &Inputs) -> f64 {
    i.a_s + i.a_b - i.a_s * i.a_b
}

fn additive(i: &Inputs) -> f64 {
    (i.a_s + i.a_b).min(1.0)
}

/// Straight colour from weights on Cs, B and Cb.
fn mix(i: &Inputs, w: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|k| w[0] * i.cs[k] + w[1] * i.b[k] + w[2] * i.cb[k])
}

/// W3C general formula: Cs' = (1 − αb)Cs + αb·B, source-over.
fn w3c_weights(i: &Inputs) -> [f64; 3] {
    let ao = union(i);
    [
        i.a_s * (1.0 - i.a_b) / ao,
        i.a_s * i.a_b / ao,
        (1.0 - i.a_s) * i.a_b / ao,
    ]
}

/// Blend as if the backdrop were opaque, then source-over of the blended
/// colour (Cs' = B).
fn opaque_blend_weights(i: &Inputs) -> [f64; 3] {
    let ao = union(i);
    [0.0, i.a_s / ao, (1.0 - i.a_s) * i.a_b / ao]
}

/// W3C with the backdrop term itself composed over the backdrop:
/// C = [αs(1 − αb)Cs + αb·X] / αo with X = [αs·B + (1 − αs)αb·Cb] / αo.
fn nested_weights(i: &Inputs) -> [f64; 3] {
    let ao = union(i);
    [
        i.a_s * (1.0 - i.a_b) / ao,
        i.a_b * i.a_s / (ao * ao),
        i.a_b * (1.0 - i.a_s) * i.a_b / (ao * ao),
    ]
}

const MODELS: [(&str, Model); 7] = [
    ("w3c", |i| (mix(i, w3c_weights(i)), union(i))),
    ("w3c+additive-alpha", |i| {
        (mix(i, w3c_weights(i)), additive(i))
    }),
    ("opaque-blend-over", |i| {
        (mix(i, opaque_blend_weights(i)), union(i))
    }),
    ("nested", |i| (mix(i, nested_weights(i)), union(i))),
    ("nested+additive-alpha", |i| {
        (mix(i, nested_weights(i)), additive(i))
    }),
    ("premultiplied-blend", |i| {
        let ao = union(i);
        (i.b_premul.map(|c| c / ao), ao)
    }),
    ("isolated", |i| {
        let ao = union(i);
        let w = [i.a_s / ao, 0.0, (1.0 - i.a_s) * i.a_b / ao];
        (mix(i, w), ao)
    }),
];

fn quantize(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn translucent(r: &Row) -> bool {
    r.bottom[3] > 0.0 && r.bottom[3] < 1.0 && r.top[3] > 0.0
}

fn score(rows: &BTreeMap<String, Vec<Row>>) {
    print!("| mode |");
    for (name, _) in MODELS {
        print!(" {name} colour max / mean | α max |");
    }
    println!();
    print!("|---|");
    for _ in MODELS {
        print!("---:|---:|");
    }
    println!();
    for (mode, list) in rows {
        print!("| {mode} |");
        for (_, model) in MODELS {
            let (mut max, mut sum, mut n, mut amax) = (0u8, 0.0, 0usize, 0u8);
            for r in list.iter().filter(|r| translucent(r)) {
                let (c, a) = model(&inputs(mode, r));
                amax = amax.max(quantize(a).abs_diff(r.measured[3]));
                if r.measured[3] > 0 {
                    for (v, m) in c.iter().zip(r.measured) {
                        let e = quantize(*v).abs_diff(m);
                        max = max.max(e);
                        sum += f64::from(e);
                        n += 1;
                    }
                }
            }
            print!(" {max} / {:.2} | {amax} |", sum / n.max(1) as f64);
        }
        println!();
    }
}

/// Least squares for `C = wS·Cs + wB·B + wC·Cb` (3×3 normal equations).
fn solve(eqs: &[([f64; 3], f64)]) -> Option<[f64; 3]> {
    let mut m = [[0.0f64; 4]; 3];
    for (x, y) in eqs {
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += x[i] * x[j];
            }
            m[i][3] += x[i] * y;
        }
    }
    for c in 0..3 {
        let p = (c..3).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs()))?;
        if m[p][c].abs() < 1e-9 {
            return None;
        }
        m.swap(c, p);
        for r in 0..3 {
            if r != c {
                let f = m[r][c] / m[c][c];
                let pivot = m[c];
                for (v, p) in m[r].iter_mut().zip(pivot).skip(c) {
                    *v -= f * p;
                }
            }
        }
    }
    Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
}

fn fit(rows: &BTreeMap<String, Vec<Row>>) {
    println!();
    println!("| mode | αs | αb | n | fitted wS wB wC | resid max | w3c | nested |");
    println!("|---|---:|---:|---:|---|---:|---|---|");
    for (mode, list) in rows {
        let mut groups: BTreeMap<(u32, u32), Vec<&Row>> = BTreeMap::new();
        for r in list.iter().filter(|r| translucent(r) && r.measured[3] > 0) {
            let i = inputs(mode, r);
            let key = ((i.a_s * 1e4).round() as u32, (i.a_b * 1e4).round() as u32);
            groups.entry(key).or_default().push(r);
        }
        for ((s, b), group) in groups {
            let mut eqs = Vec::new();
            for r in &group {
                let i = inputs(mode, r);
                for k in 0..3 {
                    eqs.push(([i.cs[k], i.b[k], i.cb[k]], f64::from(r.measured[k]) / 255.0));
                }
            }
            let Some(w) = solve(&eqs) else {
                continue;
            };
            let resid = eqs
                .iter()
                .map(|(x, y)| ((x[0] * w[0] + x[1] * w[1] + x[2] * w[2] - y) * 255.0).abs())
                .fold(0.0, f64::max);
            let i = inputs(mode, group[0]);
            let f = |w: [f64; 3]| format!("{:.3} {:.3} {:.3}", w[0], w[1], w[2]);
            println!(
                "| {mode} | {:.3} | {:.3} | {} | {} | {resid:.1} | {} | {} |",
                f64::from(s) / 1e4,
                f64::from(b) / 1e4,
                group.len(),
                f(w),
                f(w3c_weights(&i)),
                f(nested_weights(&i)),
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: backdrop_fit <measured.csv> [fit]");
        std::process::exit(2);
    };
    let rows = parse(path);
    score(&rows);
    if args.get(2).map(String::as_str) == Some("fit") {
        fit(&rows);
    }
}

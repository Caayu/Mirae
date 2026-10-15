//! Black-box probe for layer masks and clipping (phase 4D parity gate).
//!
//! `clip_probe psd <out.psd>` writes a layered test document in the public
//! PSD format: one square region per [`SCENARIOS`] entry, each an 8 × 8
//! grid of swatches. Rows vary the **base alpha** ([`BASE_ALPHA`]);
//! columns vary either the **clipped colour** ([`CLIP_COLORS`]) or, where
//! a scenario says so, a **mask value** ([`MASK_VALUES`]). Each scenario
//! is a short layer stack (layers of other scenarios are empty in its
//! region). Opened in the reference application and exported to PNG
//! through its normal UI, it gives the reference's result for every cell.
//!
//! `clip_probe read <in.png> <out.csv>` samples the centre of every swatch
//! of such an export (straight-alpha RGBA as stored in the PNG).
//!
//! The scenario table is the single source of truth for both commands and
//! for the model comparison in `crates/document/tests/clip_parity.rs`
//! (which repeats the stacks in Tela's own terms).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use tela_color::{PremulRgba, Rgba};
use tela_corpus::psd::{PsdLayer, PsdMask, write_psd};
use tela_document::BlendMode;

const CELL: u32 = 12;
const N: u32 = 8;
const REGION: u32 = CELL * N;
const PITCH: u32 = REGION + 8;
const COLUMNS: u32 = 6;

/// Base colour (straight) and its alpha per row.
const BASE_RGB: [u8; 3] = [200, 160, 60];
const BASE_ALPHA: [u8; 8] = [255, 224, 160, 128, 96, 64, 32, 0];
/// Clipped-layer colours per column (straight RGBA).
const CLIP_COLORS: [[u8; 4]; 8] = [
    [40, 90, 220, 255],
    [255, 255, 255, 255],
    [0, 0, 0, 255],
    [128, 128, 128, 255],
    [30, 200, 60, 255],
    [210, 40, 180, 128],
    [240, 150, 40, 64],
    [0, 0, 0, 0],
];
/// Mask values per column where a scenario masks by column.
const MASK_VALUES: [u8; 8] = [255, 224, 160, 128, 96, 64, 32, 0];
/// Opaque layer put under some stacks.
const BELOW: [u8; 4] = [40, 180, 120, 255];
const RED_HALF: [u8; 4] = [230, 30, 30, 128];
const BLUE: [u8; 4] = [40, 90, 220, 255];

#[derive(Clone, Copy)]
enum Content {
    /// `BASE_RGB` at the row's `BASE_ALPHA`.
    Base,
    /// `CLIP_COLORS` by column.
    Cols,
    Fill([u8; 4]),
}

#[derive(Clone, Copy)]
enum Mask {
    /// `MASK_VALUES` by column.
    Cols { disabled: bool },
    /// Mask rectangle over the left half only, `inside` there, `default`
    /// elsewhere.
    LeftHalf { inside: u8, default: u8 },
}

#[derive(Clone, Copy)]
struct L {
    content: Content,
    mode: &'static [u8; 4],
    opacity: u8,
    clipped: bool,
    hidden: bool,
    mask: Option<Mask>,
}

const fn layer(content: Content) -> L {
    L {
        content,
        mode: b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        mask: None,
    }
}
const fn clip(content: Content) -> L {
    let mut l = layer(content);
    l.clipped = true;
    l
}
const fn op(mut l: L, opacity: u8) -> L {
    l.opacity = opacity;
    l
}
const fn mode(mut l: L, key: &'static [u8; 4]) -> L {
    l.mode = key;
    l
}
const fn hide(mut l: L) -> L {
    l.hidden = true;
    l
}
const fn masked(mut l: L, m: Mask) -> L {
    l.mask = Some(m);
    l
}

const BASE: L = layer(Content::Base);
const CLIP: L = clip(Content::Cols);
const UNDER: L = layer(Content::Fill(BELOW));
const MUL: &[u8; 4] = b"mul ";
const SCR: &[u8; 4] = b"scrn";
const MASK_COLS: Mask = Mask::Cols { disabled: false };

/// Scenario name and layer stack (bottom first). The first scenario's
/// clipped layer is the bottom layer of the whole document (a clipped
/// layer with no base).
const SCENARIOS: &[(&str, &[L])] = &[
    ("orphan_clip", &[CLIP]),
    ("clip_normal", &[BASE, CLIP]),
    ("clip_op60", &[BASE, op(CLIP, 153)]),
    ("base_op50", &[op(BASE, 128), CLIP]),
    ("base_op50_clip_op60", &[op(BASE, 128), op(CLIP, 153)]),
    ("clip_multiply", &[BASE, mode(CLIP, MUL)]),
    ("clip_screen", &[BASE, mode(CLIP, SCR)]),
    ("clip_difference", &[BASE, mode(CLIP, b"diff")]),
    ("clip_add", &[BASE, mode(CLIP, b"lddg")]),
    ("below_clip_normal", &[UNDER, BASE, CLIP]),
    ("below_clip_multiply", &[UNDER, BASE, mode(CLIP, MUL)]),
    ("below_clip_screen", &[UNDER, BASE, mode(CLIP, SCR)]),
    ("below_base_multiply", &[UNDER, mode(BASE, MUL), CLIP]),
    (
        "below_base_screen_clip_multiply",
        &[UNDER, mode(BASE, SCR), mode(CLIP, MUL)],
    ),
    (
        "below_base_op50_clip_multiply",
        &[UNDER, op(BASE, 128), mode(CLIP, MUL)],
    ),
    ("two_clips", &[BASE, CLIP, clip(Content::Fill(RED_HALF))]),
    (
        "three_clips",
        &[
            BASE,
            CLIP,
            mode(clip(Content::Fill(RED_HALF)), MUL),
            mode(clip(Content::Fill([255, 255, 255, 96])), SCR),
        ],
    ),
    (
        "clips_swapped",
        &[BASE, clip(Content::Fill(RED_HALF)), CLIP],
    ),
    (
        "base_mask",
        &[masked(BASE, MASK_COLS), clip(Content::Fill(BLUE))],
    ),
    (
        "clip_mask",
        &[BASE, masked(clip(Content::Fill(BLUE)), MASK_COLS)],
    ),
    (
        "base_mask_disabled",
        &[
            masked(BASE, Mask::Cols { disabled: true }),
            clip(Content::Fill(BLUE)),
        ],
    ),
    (
        "base_mask_op50",
        &[op(masked(BASE, MASK_COLS), 128), clip(Content::Fill(BLUE))],
    ),
    ("base_hidden", &[UNDER, hide(BASE), CLIP]),
    (
        "clip_hidden",
        &[BASE, hide(CLIP), clip(Content::Fill(RED_HALF))],
    ),
    (
        "group_boundary",
        &[
            BASE,
            CLIP,
            layer(Content::Fill([220, 230, 240, 128])),
            clip(Content::Fill([230, 30, 30, 255])),
        ],
    ),
    ("mask_normal", &[masked(BASE, MASK_COLS)]),
    ("mask_op60", &[op(masked(BASE, MASK_COLS), 153)]),
    (
        "mask_multiply_below",
        &[UNDER, mode(masked(BASE, MASK_COLS), MUL)],
    ),
    (
        "mask_screen_below",
        &[UNDER, mode(masked(BASE, MASK_COLS), SCR)],
    ),
    (
        "mask_disabled",
        &[masked(BASE, Mask::Cols { disabled: true })],
    ),
    (
        "mask_default_black",
        &[masked(
            BASE,
            Mask::LeftHalf {
                inside: 255,
                default: 0,
            },
        )],
    ),
    (
        "mask_default_white",
        &[masked(
            BASE,
            Mask::LeftHalf {
                inside: 0,
                default: 255,
            },
        )],
    ),
    (
        "clip_multiply_masked_base_op50",
        &[
            UNDER,
            op(masked(BASE, MASK_COLS), 128),
            mode(clip(Content::Fill(BLUE)), MUL),
        ],
    ),
];

fn region_origin(index: usize) -> (u32, u32) {
    let i = index as u32;
    (8 + (i % COLUMNS) * PITCH, 8 + (i / COLUMNS) * PITCH)
}

fn size() -> (u32, u32) {
    let rows = (SCENARIOS.len() as u32).div_ceil(COLUMNS);
    (8 + COLUMNS * PITCH, 8 + rows * PITCH)
}

fn content_at(c: Content, row: usize, col: usize) -> [u8; 4] {
    match c {
        Content::Base => [BASE_RGB[0], BASE_RGB[1], BASE_RGB[2], BASE_ALPHA[row]],
        Content::Cols => CLIP_COLORS[col],
        Content::Fill(p) => p,
    }
}

fn psd_layers() -> Vec<PsdLayer> {
    let mut out = Vec::new();
    for (s, (name, stack)) in SCENARIOS.iter().enumerate() {
        let (ox, oy) = region_origin(s);
        for (k, l) in stack.iter().enumerate() {
            let mut rgba = Vec::with_capacity((REGION * REGION) as usize);
            for y in 0..REGION {
                for x in 0..REGION {
                    rgba.push(content_at(
                        l.content,
                        (y / CELL) as usize,
                        (x / CELL) as usize,
                    ));
                }
            }
            let mask = l.mask.map(|m| match m {
                Mask::Cols { disabled } => PsdMask {
                    rect: (ox, oy, REGION, REGION),
                    values: (0..REGION * REGION)
                        .map(|i| MASK_VALUES[((i % REGION) / CELL) as usize])
                        .collect(),
                    default: 255,
                    disabled,
                },
                Mask::LeftHalf { inside, default } => PsdMask {
                    rect: (ox, oy, REGION / 2, REGION),
                    values: vec![inside; (REGION / 2 * REGION) as usize],
                    default,
                    disabled: false,
                },
            });
            out.push(PsdLayer {
                name: format!("{name} {k}"),
                mode: *l.mode,
                opacity: l.opacity,
                clipped: l.clipped,
                hidden: l.hidden,
                rect: (ox, oy, REGION, REGION),
                rgba,
                mask,
            });
        }
    }
    out
}

fn read_png(path: &str, csv: &str) -> std::io::Result<()> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().expect("png size fits")];
    let frame = reader.next_frame(&mut buf)?;
    let (w, h) = size();
    assert_eq!(
        (frame.width, frame.height),
        (w, h),
        "unexpected export size"
    );
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported PNG color type {other:?}"),
    };
    assert_eq!(
        frame.bit_depth,
        png::BitDepth::Eight,
        "8-bit export expected"
    );
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) as usize) * channels;
        let a = if channels == 4 { buf[i + 3] } else { 255 };
        [buf[i], buf[i + 1], buf[i + 2], a]
    };
    let mut out = BufWriter::new(File::create(csv)?);
    writeln!(out, "scenario,row,col,r,g,b,a,spread")?;
    for (s, (name, _)) in SCENARIOS.iter().enumerate() {
        let (ox, oy) = region_origin(s);
        for row in 0..N {
            for col in 0..N {
                let (x0, y0) = (ox + col * CELL, oy + row * CELL);
                let c = px(x0 + CELL / 2, y0 + CELL / 2);
                let mut spread = 0u8;
                for dy in 2..CELL - 2 {
                    for dx in 2..CELL - 2 {
                        let p = px(x0 + dx, y0 + dy);
                        for k in 0..4 {
                            spread = spread.max(p[k].abs_diff(c[k]));
                        }
                    }
                }
                writeln!(
                    out,
                    "{name},{row},{col},{},{},{},{},{spread}",
                    c[0], c[1], c[2], c[3]
                )?;
            }
        }
    }
    out.flush()
}

fn blend_mode(key: &[u8; 4]) -> BlendMode {
    match key {
        b"norm" => BlendMode::Normal,
        b"mul " => BlendMode::Multiply,
        b"scrn" => BlendMode::Screen,
        b"diff" => BlendMode::Difference,
        b"lddg" => BlendMode::Add,
        other => panic!("unmapped key {other:?}"),
    }
}

fn mask_at(m: Mask, col: usize, left: bool) -> f32 {
    match m {
        Mask::Cols { disabled: true } => 1.0,
        Mask::Cols { .. } => f32::from(MASK_VALUES[col]) / 255.0,
        Mask::LeftHalf { inside, default } => {
            f32::from(if left { inside } else { default }) / 255.0
        }
    }
}

/// Candidate compositing models for a clipping group.
#[derive(Clone, Copy, Debug)]
enum Model {
    /// Base and clipped layers composed in their own buffer, clipped ones
    /// with the W3C general formula and Porter-Duff source-atop (alpha
    /// stays the base's); the buffer is then blended onto the backdrop
    /// with the base's mode and opacity.
    IsolatedW3cAtop,
    /// Like `IsolatedW3cAtop`, but the blend function's result replaces
    /// the source colour fully (`Cs' = B(Cb, Cs)`), as if the base were
    /// opaque.
    IsolatedOpaqueBlend,
    /// Each layer blended onto the running backdrop; a clipped layer's
    /// alpha multiplied by its base's (masked) alpha.
    NonIsolated,
}

const MODELS: [Model; 3] = [
    Model::IsolatedW3cAtop,
    Model::IsolatedOpaqueBlend,
    Model::NonIsolated,
];

fn one(mode: BlendMode, src: PremulRgba, opacity: f32, dst: PremulRgba) -> PremulRgba {
    let mut d = [dst];
    mode.composite(&[src], opacity, &mut d);
    d[0]
}

fn straight(p: PremulRgba) -> [f32; 3] {
    if p.a <= 0.0 {
        return [0.0; 3];
    }
    [p.r / p.a, p.g / p.a, p.b / p.a].map(|c| c.min(1.0))
}

/// Source-atop with a blend function: the result keeps the backdrop's
/// alpha. `opaque` selects `Cs' = B(Cb, Cs)` over the W3C mix.
fn atop(
    mode: BlendMode,
    src: PremulRgba,
    opacity: f32,
    dst: PremulRgba,
    opaque: bool,
) -> PremulRgba {
    let (a_s, a_b) = (src.a * opacity, dst.a);
    if a_s <= 0.0 || a_b <= 0.0 {
        return dst;
    }
    let (cs, cb) = (straight(src), straight(dst));
    // B(Cb, Cs): an opaque source blended onto an opaque backdrop.
    let b = one(
        mode,
        Rgba::new(cs[0], cs[1], cs[2], 1.0).premultiply(),
        1.0,
        Rgba::new(cb[0], cb[1], cb[2], 1.0).premultiply(),
    );
    let mix = |cs: f32, b: f32, cb_premul: f32| {
        let cs_prime = if opaque {
            b
        } else {
            (1.0 - a_b) * cs + a_b * b
        };
        a_s * a_b * cs_prime + (1.0 - a_s) * cb_premul
    };
    PremulRgba::new(
        mix(cs[0], b.r, dst.r),
        mix(cs[1], b.g, dst.g),
        mix(cs[2], b.b, dst.b),
        a_b,
    )
}

/// One cell of a scenario under `model`, premultiplied RGBA8.
fn eval(stack: &[L], row: usize, col: usize, paper: bool, model: Model) -> [u8; 4] {
    let left = (col as u32) < N / 2;
    let px = |l: &L| {
        let [r, g, b, a] = content_at(l.content, row, col).map(|v| f32::from(v) / 255.0);
        // Stored as Tela stores layers: 8-bit premultiplied.
        let stored = PremulRgba::from_rgba8(Rgba::new(r, g, b, a).premultiply().to_rgba8());
        stored.scale(l.mask.map_or(1.0, |m| mask_at(m, col, left)))
    };
    let mut acc = if paper {
        Rgba::WHITE.premultiply()
    } else {
        PremulRgba::TRANSPARENT
    };
    let mut i = 0;
    // A clipped layer with nothing below it starts its own group.
    while i < stack.len() {
        let base = &stack[i];
        let mut j = i + 1;
        while j < stack.len() && stack[j].clipped {
            j += 1;
        }
        let clipped = &stack[i + 1..j];
        i = j;
        if base.hidden {
            continue;
        }
        let (mode, opacity) = (blend_mode(base.mode), f32::from(base.opacity) / 255.0);
        let visible = clipped.iter().filter(|c| !c.hidden);
        match model {
            Model::IsolatedW3cAtop | Model::IsolatedOpaqueBlend => {
                let mut group = px(base);
                for c in visible {
                    let opaque = matches!(model, Model::IsolatedOpaqueBlend);
                    let c_opacity = f32::from(c.opacity) / 255.0;
                    group = atop(blend_mode(c.mode), px(c), c_opacity, group, opaque);
                }
                acc = one(mode, group, opacity, acc);
            }
            Model::NonIsolated => {
                let b = px(base);
                acc = one(mode, b, opacity, acc);
                for c in visible {
                    let c_opacity = f32::from(c.opacity) / 255.0;
                    acc = one(blend_mode(c.mode), px(c).scale(b.a), c_opacity, acc);
                }
            }
        }
    }
    acc.to_rgba8()
}

/// Largest premultiplied channel difference per scenario and model.
fn fit_one(csv: &str, paper: bool) -> std::io::Result<Vec<[u8; 3]>> {
    let text = std::fs::read_to_string(csv)?;
    let mut worst = vec![[0u8; 3]; SCENARIOS.len()];
    for line in text.lines().skip(1) {
        let v: Vec<&str> = line.split(',').collect();
        let s = SCENARIOS
            .iter()
            .position(|(name, _)| *name == v[0])
            .expect("known scenario");
        let (row, col): (usize, usize) = (v[1].parse().unwrap(), v[2].parse().unwrap());
        let m = [3, 4, 5, 6].map(|i| v[i].parse::<u8>().unwrap());
        let a = f32::from(m[3]) / 255.0;
        let measured = [
            (f32::from(m[0]) * a).round() as u8,
            (f32::from(m[1]) * a).round() as u8,
            (f32::from(m[2]) * a).round() as u8,
            m[3],
        ];
        for (k, model) in MODELS.iter().enumerate() {
            let got = eval(SCENARIOS[s].1, row, col, paper, *model);
            let e = (0..4).map(|i| got[i].abs_diff(measured[i])).max().unwrap();
            worst[s][k] = worst[s][k].max(e);
        }
    }
    Ok(worst)
}

fn fit(with_paper: &str, without_paper: &str) -> std::io::Result<()> {
    let p = fit_one(with_paper, true)?;
    let n = fit_one(without_paper, false)?;
    println!("Largest premultiplied channel error (1/255), with paper / without paper:\n");
    println!(
        "| scenario | {:?} | {:?} | {:?} |",
        MODELS[0], MODELS[1], MODELS[2]
    );
    println!("|---|---:|---:|---:|");
    for (s, (name, _)) in SCENARIOS.iter().enumerate() {
        println!(
            "| {name} | {} / {} | {} / {} | {} / {} |",
            p[s][0], n[s][0], p[s][1], n[s][1], p[s][2], n[s][2]
        );
    }
    Ok(())
}

/// Prepared content for the UI-driven probes (alpha lock, mask painting):
/// an 800 × 600 document. "alpha" holds `BASE_RGB` bands over the top half
/// with alpha 255, 128, 0 and a left-to-right ramp 0..255 (x 600..800);
/// "solid" holds opaque `BLUE` over the bottom half.
fn ui_layers() -> Vec<PsdLayer> {
    let alpha = |x: u32| match x {
        0..200 => 255,
        200..400 => 128,
        400..600 => 0,
        _ => ((x - 600) * 255 / 199) as u8,
    };
    let [r, g, b] = BASE_RGB;
    let top = (0..800 * 300).map(|i| [r, g, b, alpha(i % 800)]).collect();
    let layer = |name: &str, top: u32, rgba| PsdLayer {
        name: name.into(),
        mode: *b"norm",
        opacity: 255,
        clipped: false,
        hidden: false,
        rect: (0, top, 800, 300),
        rgba,
        mask: None,
    };
    vec![
        layer("solid", 300, vec![BLUE; 800 * 300]),
        layer("alpha", 0, top),
    ]
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("psd") if args.len() == 3 => {
            let (w, h) = size();
            write_psd(Path::new(&args[2]), w, h, &psd_layers())
        }
        Some("ui") if args.len() == 3 => write_psd(Path::new(&args[2]), 800, 600, &ui_layers()),
        Some("read") if args.len() == 4 => read_png(&args[2], &args[3]),
        Some("fit") if args.len() == 4 => fit(&args[2], &args[3]),
        _ => {
            eprintln!(
                "usage: clip_probe psd <out.psd> | ui <out.psd> | read <in.png> <out.csv> | fit <with_paper.csv> <without_paper.csv>"
            );
            std::process::exit(2);
        }
    }
}

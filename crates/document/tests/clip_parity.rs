//! Mask and clipping parity with the reference application (phase 4D
//! gate, ADR 0017).
//!
//! `tests/reference/clip_with_paper.csv` and `clip_without_paper.csv` are
//! black-box measurements of the reference for the 34 scenarios of
//! `cargo run -p tela-corpus --example clip_probe` (8 × 8 swatches each;
//! see the README there and docs/parity/4D-masks-clipping-alpha-lock.md).
//! These tests build the same stacks through Tela's public `Document` API
//! and require every swatch to match within 2/255 (premultiplied), the
//! 8-bit rounding of Multiply already accepted in 4C.
//!
//! The scenario table repeats the probe's in Tela's terms; the probe's
//! `SCENARIOS` is the source both follow.

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, Rgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerId, Placement};

const CELL: u32 = 12;
const N: u32 = 8;
const REGION: u32 = CELL * N;

const BASE_RGB: [u8; 3] = [200, 160, 60];
const BASE_ALPHA: [u8; 8] = [255, 224, 160, 128, 96, 64, 32, 0];
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
const MASK_VALUES: [u8; 8] = [255, 224, 160, 128, 96, 64, 32, 0];
const BELOW: [u8; 4] = [40, 180, 120, 255];
const RED_HALF: [u8; 4] = [230, 30, 30, 128];
const BLUE: [u8; 4] = [40, 90, 220, 255];

#[derive(Clone, Copy)]
enum Content {
    Base,
    Cols,
    Fill([u8; 4]),
}

#[derive(Clone, Copy)]
enum Mask {
    Cols { disabled: bool },
    LeftHalf { inside: u8, default: u8 },
}

#[derive(Clone, Copy)]
struct L {
    content: Content,
    mode: BlendMode,
    opacity: u8,
    clipped: bool,
    hidden: bool,
    mask: Option<Mask>,
}

const fn layer(content: Content) -> L {
    L {
        content,
        mode: BlendMode::Normal,
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
const fn mode(mut l: L, mode: BlendMode) -> L {
    l.mode = mode;
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
const MUL: BlendMode = BlendMode::Multiply;
const SCR: BlendMode = BlendMode::Screen;
const MASK_COLS: Mask = Mask::Cols { disabled: false };

const SCENARIOS: &[(&str, &[L])] = &[
    ("orphan_clip", &[CLIP]),
    ("clip_normal", &[BASE, CLIP]),
    ("clip_op60", &[BASE, op(CLIP, 153)]),
    ("base_op50", &[op(BASE, 128), CLIP]),
    ("base_op50_clip_op60", &[op(BASE, 128), op(CLIP, 153)]),
    ("clip_multiply", &[BASE, mode(CLIP, MUL)]),
    ("clip_screen", &[BASE, mode(CLIP, SCR)]),
    (
        "clip_difference",
        &[BASE, mode(CLIP, BlendMode::Difference)],
    ),
    ("clip_add", &[BASE, mode(CLIP, BlendMode::Add)]),
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

/// Premultiplied, at the storage's precision, as Tela stores layers.
fn stored<P: StoragePixel>(c: [u8; 4]) -> P {
    let [r, g, b, a] = c.map(|v| f32::from(v) / 255.0);
    P::from_premul(Rgba::new(r, g, b, a).premultiply())
}

fn content_at(c: Content, row: u32, col: u32) -> [u8; 4] {
    match c {
        Content::Base => [
            BASE_RGB[0],
            BASE_RGB[1],
            BASE_RGB[2],
            BASE_ALPHA[row as usize],
        ],
        Content::Cols => CLIP_COLORS[col as usize],
        Content::Fill(p) => p,
    }
}

fn mask_at(m: Mask, col: u32) -> u8 {
    match m {
        Mask::Cols { .. } => MASK_VALUES[col as usize],
        Mask::LeftHalf { inside, default } => {
            if col < N / 2 {
                inside
            } else {
                default
            }
        }
    }
}

fn all_tiles() -> DirtyTiles {
    let mut t = DirtyTiles::new();
    t.mark(TileCoord { x: 0, y: 0 });
    t
}

/// One scenario as a Tela document, through the public API only.
fn build(stack: &[L], depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(REGION, REGION, depth);
    let mut ids: Vec<LayerId> = vec![d.active_layer()];
    for _ in 1..stack.len() {
        ids.push(d.add_layer(Placement::Top).unwrap());
    }
    for (l, &id) in stack.iter().zip(&ids) {
        with_pixel_type!(depth, P => d.modify_tiles(id, &all_tiles(), |_, t: &mut Tile<P>| {
            for y in 0..REGION {
                for x in 0..REGION {
                    *t.get_mut(x, y) = stored::<P>(content_at(l.content, y / CELL, x / CELL));
                }
            }
        }))
        .unwrap();
        if let Some(m) = l.mask {
            d.add_mask(id).unwrap();
            d.modify_mask(id, &all_tiles(), |_, t| {
                for y in 0..REGION {
                    for x in 0..REGION {
                        *t.get_mut(x, y) = mask_at(m, x / CELL);
                    }
                }
            })
            .unwrap();
            if let Mask::Cols { disabled: true } = m {
                d.set_mask_enabled(id, false).unwrap();
            }
        }
        d.set_blend_mode(id, l.mode).unwrap();
        d.set_opacity(id, f32::from(l.opacity) / 255.0).unwrap();
        d.set_clipped(id, l.clipped).unwrap();
        d.set_visible(id, !l.hidden).unwrap();
    }
    d
}

struct Row {
    scenario: String,
    row: u32,
    col: u32,
    measured: [u8; 4],
}

fn rows(file: &str) -> Vec<Row> {
    let path = format!("{}/tests/reference/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("reference CSV");
    text.lines()
        .skip(1)
        .map(|line| {
            let v: Vec<&str> = line.split(',').collect();
            let n = |i: usize| v[i].parse::<u8>().unwrap();
            Row {
                scenario: v[0].to_string(),
                row: v[1].parse().unwrap(),
                col: v[2].parse().unwrap(),
                measured: [n(3), n(4), n(5), n(6)],
            }
        })
        .collect()
}

/// Largest premultiplied channel difference over every measured swatch.
/// `paper`: the reference's export had its white paper layer; Tela's
/// composite is put over white as the display does.
fn worst_difference(file: &str, paper: bool, depth: PixelDepth) -> (u8, String) {
    let rows = rows(file);
    assert_eq!(rows.len(), SCENARIOS.len() * 64, "every swatch measured");
    let mut worst = (0u8, String::new());
    for (name, stack) in SCENARIOS {
        let doc = build(stack, depth);
        let tile = doc
            .compose_tile(TileCoord { x: 0, y: 0 }, None)
            .unwrap_or_else(|| vec![[0; 4]; 256 * 256]);
        for r in rows.iter().filter(|r| r.scenario == *name) {
            let (x, y) = (r.col * CELL + CELL / 2, r.row * CELL + CELL / 2);
            let mut got = tile[(y * 256 + x) as usize];
            if paper {
                let keep = 255 - u16::from(got[3]);
                got = [got[0], got[1], got[2], 0].map(|c| (u16::from(c) + keep) as u8);
                got[3] = 255;
            }
            let a = f32::from(r.measured[3]) / 255.0;
            let m = r.measured;
            let expected = [
                (f32::from(m[0]) * a).round() as u8,
                (f32::from(m[1]) * a).round() as u8,
                (f32::from(m[2]) * a).round() as u8,
                m[3],
            ];
            let e = (0..4).map(|i| got[i].abs_diff(expected[i])).max().unwrap();
            if e > worst.0 {
                worst = (
                    e,
                    format!(
                        "{name} row {} col {}: Tela {got:?}, reference {expected:?}",
                        r.row, r.col
                    ),
                );
            }
        }
    }
    worst
}

#[test]
fn masks_and_clipping_match_the_reference_over_its_paper() {
    let (e, at) = worst_difference("clip_with_paper.csv", true, PixelDepth::Rgba8);
    assert!(e <= 2, "largest difference {e}/255 at {at}");
}

/// Phase 4E: the same stacks in 16-bit documents (colours at 16-bit
/// precision, masks 8-bit as always), against the reference's 8-bit
/// measurements (it has no 16-bit documents).
#[test]
fn masks_and_clipping_match_the_reference_in_16_bit_documents() {
    for (file, paper) in [
        ("clip_with_paper.csv", true),
        ("clip_without_paper.csv", false),
    ] {
        let (e, at) = worst_difference(file, paper, PixelDepth::Rgba16);
        assert!(e <= 2, "{file}: largest difference {e}/255 at {at}");
    }
}

#[test]
fn masks_and_clipping_match_the_reference_without_paper() {
    let (e, at) = worst_difference("clip_without_paper.csv", false, PixelDepth::Rgba8);
    assert!(e <= 2, "largest difference {e}/255 at {at}");
}

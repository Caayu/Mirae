//! Merge down against the reference (phase 5B gate, ADR 0023).
//!
//! `tests/reference/merge_down.csv` holds the reference's exports of the
//! `merge_probe` scenarios after "Merge with layer below" (and, for the
//! scenarios whose appearance changes, before it), 64 swatches each:
//! rows are the `BOTTOM` swatches, columns the `TOP` ones (the constants
//! below mirror `crates/corpus/examples/merge_probe.rs`; a mismatch would
//! fail against the measured data). Each stack is rebuilt through
//! `Document`, merged with `Document::merge_down` and composed; every
//! channel must match within 2/255, premultiplied.

use std::collections::BTreeMap;

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, Rgba, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, ComposeScratch, Document, LayerId, LowerMask, Placement};

const BOTTOM: [[u8; 4]; 8] = [
    [220, 40, 40, 255],
    [220, 40, 40, 128],
    [37, 149, 213, 191],
    [37, 149, 213, 64],
    [40, 190, 60, 255],
    [0, 0, 0, 128],
    [255, 255, 255, 64],
    [0, 0, 0, 0],
];
const TOP: [[u8; 4]; 8] = [
    [90, 220, 40, 255],
    [90, 220, 40, 128],
    [30, 40, 250, 160],
    [213, 149, 37, 64],
    [0, 0, 0, 255],
    [255, 255, 255, 128],
    [128, 128, 128, 200],
    [0, 0, 0, 0],
];
const MASK: [u8; 8] = [255, 160, 96, 0, 255, 160, 96, 0];
const UNDER: [u8; 4] = [213, 149, 37, 255];
const FILL_BLUE: [u8; 4] = [30, 40, 250, 200];

#[derive(Clone, Copy)]
enum Content {
    Bottom,
    Top,
    Fill([u8; 4]),
}

#[derive(Clone, Copy)]
struct L {
    content: Content,
    mode: BlendMode,
    opacity: u8,
    clipped: bool,
    /// `Some(enabled)`: a mask of `MASK` columns.
    mask: Option<bool>,
}

const fn l(content: Content) -> L {
    L {
        content,
        mode: BlendMode::Normal,
        opacity: 255,
        clipped: false,
        mask: None,
    }
}
const fn op(mut x: L, o: u8) -> L {
    x.opacity = o;
    x
}
const fn mode(mut x: L, m: BlendMode) -> L {
    x.mode = m;
    x
}
const fn clip(mut x: L) -> L {
    x.clipped = true;
    x
}
const fn masked(mut x: L, enabled: bool) -> L {
    x.mask = Some(enabled);
    x
}

const B: L = l(Content::Bottom);
const T: L = l(Content::Top);
const U: L = l(Content::Fill(UNDER));

/// A scenario: its stack (bottom first), the index of the layer merged
/// down (`None`: compare the stack as it is, before merging), what to do
/// with the lower mask, and whether the export had the paper.
struct S {
    name: &'static str,
    stack: Vec<L>,
    merge: Option<usize>,
    lower_mask: LowerMask,
    paper: bool,
}

fn s(name: &'static str, stack: &[L]) -> S {
    S {
        name,
        stack: stack.to_vec(),
        merge: Some(stack.len() - 1),
        lower_mask: LowerMask::Apply,
        paper: false,
    }
}

fn scenarios() -> Vec<S> {
    use BlendMode::*;
    let mut v = vec![
        s("md_normal", &[B, T]),
        s("md_top_op60", &[B, op(T, 153)]),
        s("md_bottom_op60", &[op(B, 153), T]),
        s("md_top_multiply", &[B, mode(T, Multiply)]),
        s("md_top_screen_op60", &[B, op(mode(T, Screen), 153)]),
        s("md_top_hue", &[B, mode(T, Hue)]),
        s("md_bottom_multiply", &[mode(B, Multiply), T]),
        s(
            "md_both_modes",
            &[op(mode(B, Multiply), 204), op(mode(T, Screen), 153)],
        ),
        s("md_top_mask", &[B, masked(T, true)]),
        s("md_bottom_mask_apply", &[masked(B, true), T]),
        s("md_top_mask_disabled", &[B, masked(T, false)]),
        s("md_clip_into_base", &[B, clip(T)]),
        s(
            "md_clip_multiply_into_base",
            &[B, clip(op(mode(T, Multiply), 204))],
        ),
        s(
            "md_clip_into_clip",
            &[U, clip(l(Content::Fill(FILL_BLUE))), clip(T)],
        ),
        s("md_under_bottom_multiply", &[U, mode(B, Multiply), T]),
        s("md_under_top_screen", &[U, B, op(mode(T, Screen), 153)]),
    ];
    let mut keep = s("md_bottom_mask_keep", &[masked(B, true), T]);
    keep.lower_mask = LowerMask::Keep;
    v.push(keep);
    // The base of a clipping group merged into the layer below it; the
    // clipped layer above now clips to the merged layer.
    let mut base = s("md_base_down", &[U, B, clip(T)]);
    base.merge = Some(1);
    v.push(base);
    let mut paper = s("md_top_screen_op60_paper", &[B, op(mode(T, Screen), 153)]);
    paper.paper = true;
    v.push(paper);
    // The stacks as they were before merging, where merging changed the
    // reference's result (so the comparison above is not trivially the
    // same picture).
    for (name, stack, paper) in [
        (
            "md_top_screen_op60_paper_before",
            vec![B, op(mode(T, Screen), 153)],
            true,
        ),
        (
            "md_under_bottom_multiply_before",
            vec![U, mode(B, Multiply), T],
            false,
        ),
        (
            "md_under_top_screen_before",
            vec![U, B, op(mode(T, Screen), 153)],
            false,
        ),
    ] {
        v.push(S {
            name,
            stack,
            merge: None,
            lower_mask: LowerMask::Apply,
            paper,
        });
    }
    v
}

fn reference() -> BTreeMap<String, Vec<[u8; 4]>> {
    let path = format!(
        "{}/tests/reference/merge_down.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("reference CSV");
    let mut out: BTreeMap<String, Vec<[u8; 4]>> = BTreeMap::new();
    for line in text.lines().skip(1) {
        let v: Vec<&str> = line.split(',').collect();
        let n = |i: usize| v[i].parse::<u8>().unwrap();
        let cells = out
            .entry(v[0].to_string())
            .or_insert_with(|| vec![[0; 4]; 64]);
        cells[usize::from(n(1)) * 8 + usize::from(n(2))] = [n(3), n(4), n(5), n(6)];
    }
    out
}

fn premultiplied(c: [u8; 4]) -> [u8; 4] {
    Rgba::new(
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
        f32::from(c[3]) / 255.0,
    )
    .premultiply()
    .to_rgba8()
}

fn pixel(c: Content, row: usize, col: usize) -> [u8; 4] {
    match c {
        Content::Bottom => BOTTOM[row],
        Content::Top => TOP[col],
        Content::Fill(p) => p,
    }
}

/// Builds the stack: swatch (row, col) is pixel (col, row) of tile (0, 0).
fn build(stack: &[L], depth: PixelDepth) -> (Document, Vec<LayerId>) {
    let mut d = Document::with_depth(8, 8, depth);
    let mut ids = Vec::new();
    for (k, spec) in stack.iter().enumerate() {
        let id = if k == 0 {
            d.active_layer()
        } else {
            d.add_layer(Placement::Top).unwrap()
        };
        let mut t = DirtyTiles::new();
        t.mark(TileCoord { x: 0, y: 0 });
        with_pixel_type!(d.depth(), P => d.modify_tiles(id, &t, |_, tile: &mut Tile<P>| {
            for row in 0..8 {
                for col in 0..8 {
                    let c = pixel(spec.content, row, col);
                    *tile.get_mut(col as u32, row as u32) = P::from_premul(
                        Rgba::new(
                            f32::from(c[0]) / 255.0,
                            f32::from(c[1]) / 255.0,
                            f32::from(c[2]) / 255.0,
                            f32::from(c[3]) / 255.0,
                        )
                        .premultiply(),
                    );
                }
            }
        }))
        .unwrap();
        d.set_opacity(id, f32::from(spec.opacity) / 255.0).unwrap();
        d.set_blend_mode(id, spec.mode).unwrap();
        d.set_clipped(id, spec.clipped).unwrap();
        if let Some(enabled) = spec.mask {
            d.add_mask(id).unwrap();
            d.modify_mask(id, &t, |_, m| {
                for y in 0..8 {
                    for (x, v) in MASK.iter().enumerate() {
                        *m.get_mut(x as u32, y) = *v;
                    }
                }
            })
            .unwrap();
            d.set_mask_enabled(id, enabled).unwrap();
        }
        ids.push(id);
    }
    (d, ids)
}

/// The composite as the reference exported it: over transparency, or
/// over the white paper (opaque).
fn composite(d: &Document, paper: bool) -> Vec<[u8; 4]> {
    let mut scratch = ComposeScratch::new();
    let c = TileCoord { x: 0, y: 0 };
    let drew = if paper {
        d.compose_tile_into(c, None, &mut scratch)
    } else {
        d.compose_tile_transparent_into(c, &mut scratch)
    };
    assert!(drew);
    let p = scratch.premultiplied();
    (0..64)
        .map(|i| {
            let px = p[(i / 8) * 256 + i % 8];
            if paper {
                px.over(Rgba::WHITE.premultiply()).to_rgba8()
            } else {
                px.to_rgba8()
            }
        })
        .collect()
}

fn check(depth: PixelDepth) {
    let reference = reference();
    let mut failures = Vec::new();
    let mut checked = 0;
    for sc in scenarios() {
        let want = &reference[sc.name];
        let (mut d, ids) = build(&sc.stack, depth);
        if let Some(k) = sc.merge {
            let count = d.layer_count();
            d.merge_down(ids[k], sc.lower_mask).unwrap();
            assert_eq!(d.layer_count(), count - 1, "{}", sc.name);
        }
        let got = composite(&d, sc.paper);
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            checked += 1;
            let w = if sc.paper { *w } else { premultiplied(*w) };
            let err = (0..4).map(|k| g[k].abs_diff(w[k])).max().unwrap();
            if err > 2 {
                failures.push(format!(
                    "{} swatch ({}, {}): mirae {g:?} reference {w:?}",
                    sc.name,
                    i / 8,
                    i % 8
                ));
            }
        }
    }
    assert_eq!(checked, 22 * 64);
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures[..failures.len().min(30)].join("\n")
    );
}

#[test]
fn merged_layers_match_the_reference() {
    check(PixelDepth::Rgba8);
}

/// The reference has no 16-bit documents: its 8-bit results are the
/// target, at the same tolerance.
#[test]
fn merged_layers_match_the_reference_in_16_bit_documents() {
    check(PixelDepth::Rgba16);
}

/// Where the reference's export did not change when merging (every
/// scenario except the ones with something under a non-Normal pair, the
/// paper, a kept lower mask and a clipping base merged down), Mirae's
/// composite does not change either (MD1, within 1/255).
#[test]
fn merging_keeps_the_picture_where_the_reference_does() {
    let preserved = [
        "md_normal",
        "md_top_op60",
        "md_bottom_op60",
        "md_top_multiply",
        "md_top_screen_op60",
        "md_top_hue",
        "md_bottom_multiply",
        "md_both_modes",
        "md_top_mask",
        "md_bottom_mask_apply",
        "md_top_mask_disabled",
        "md_clip_into_base",
        "md_clip_multiply_into_base",
        "md_clip_into_clip",
    ];
    for depth in [PixelDepth::Rgba8, PixelDepth::Rgba16] {
        for sc in scenarios()
            .into_iter()
            .filter(|s| preserved.contains(&s.name))
        {
            let (mut d, ids) = build(&sc.stack, depth);
            let before = composite(&d, false);
            d.merge_down(ids[sc.merge.unwrap()], sc.lower_mask).unwrap();
            let after = composite(&d, false);
            for (b, a) in before.iter().zip(&after) {
                for k in 0..4 {
                    assert!(
                        b[k].abs_diff(a[k]) <= 1,
                        "{} at {depth:?}: {b:?} became {a:?}",
                        sc.name
                    );
                }
            }
        }
    }
}

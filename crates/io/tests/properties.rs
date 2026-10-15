//! Property tests: random documents round-trip exactly; random input and
//! random damage to real files never panic and never load as a different
//! document.

mod common;

use std::io::Cursor;

use common::{assert_same, load, paint, save};
use proptest::prelude::*;
use tela_color::PixelDepth;
use tela_document::{BlendMode, Document, LayerPart, Placement};
use tela_io::{IoError, Limits, read_png};

#[derive(Clone, Debug)]
enum Op {
    Add(bool),
    Paint(u8, i32, i32, u32),
    PaintMask(u8, i32, i32, u8),
    AddMask(u8),
    Opacity(u8, u32),
    Blend(u8, u8),
    Flags(u8, bool, bool, bool, bool),
    MaskEnabled(u8, bool),
    Remove(u8),
    Select(u8, bool),
    Convert,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<bool>().prop_map(Op::Add),
        (any::<u8>(), -3i32..5, -3i32..4, any::<u32>())
            .prop_map(|(l, x, y, k)| Op::Paint(l, x, y, k)),
        (any::<u8>(), -2i32..3, -2i32..3, any::<u8>())
            .prop_map(|(l, x, y, v)| Op::PaintMask(l, x, y, v)),
        any::<u8>().prop_map(Op::AddMask),
        (any::<u8>(), any::<u32>()).prop_map(|(l, bits)| Op::Opacity(l, bits)),
        (any::<u8>(), any::<u8>()).prop_map(|(l, m)| Op::Blend(l, m)),
        (
            any::<u8>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>()
        )
            .prop_map(|(l, a, b, c, d)| Op::Flags(l, a, b, c, d)),
        (any::<u8>(), any::<bool>()).prop_map(|(l, e)| Op::MaskEnabled(l, e)),
        any::<u8>().prop_map(Op::Remove),
        (any::<u8>(), any::<bool>()).prop_map(|(l, m)| Op::Select(l, m)),
        Just(Op::Convert),
    ]
}

fn pick(d: &Document, i: u8) -> tela_document::LayerId {
    d.layers()
        .nth(usize::from(i) % d.layer_count())
        .unwrap()
        .id()
}

/// Applies an operation; refusals (locked layers, …) are fine.
fn apply(d: &mut Document, op: &Op) {
    match *op {
        Op::Add(top) => {
            let at = if top {
                Placement::Top
            } else {
                Placement::Bottom
            };
            let _ = d.add_layer(at);
        }
        Op::Paint(l, x, y, k) => {
            let id = pick(d, l);
            if d.layer(id).is_some_and(|l| !l.locked()) {
                paint(d, id, &[(x, y)], k);
            }
        }
        Op::PaintMask(l, x, y, v) => {
            let id = pick(d, l);
            let _ = d.modify_mask(id, &common::tiles(&[(x, y)]), |_, t| {
                for (i, p) in t.pixels_mut().iter_mut().enumerate() {
                    if i % 3 == 0 {
                        *p = v.wrapping_add(i as u8);
                    }
                }
            });
        }
        Op::AddMask(l) => {
            let _ = d.add_mask(pick(d, l));
        }
        Op::Opacity(l, bits) => {
            // Any f32 in 0..=1, all digits significant.
            let o = f32::from_bits(bits) % 1.0;
            if o.is_finite() {
                let _ = d.set_opacity(pick(d, l), o.abs());
            }
        }
        Op::Blend(l, m) => {
            let mode = BlendMode::ALL[usize::from(m) % BlendMode::ALL.len()];
            let _ = d.set_blend_mode(pick(d, l), mode);
        }
        Op::Flags(l, visible, locked, alpha, clipped) => {
            let id = pick(d, l);
            let _ = d.set_clipped(id, clipped);
            let _ = d.set_alpha_locked(id, alpha);
            let _ = d.set_visible(id, visible);
            let _ = d.set_locked(id, locked);
        }
        Op::MaskEnabled(l, e) => {
            let _ = d.set_mask_enabled(pick(d, l), e);
        }
        Op::Remove(l) => {
            let _ = d.remove_layer(pick(d, l));
        }
        Op::Select(l, mask) => {
            let _ = d.set_active_layer(pick(d, l));
            if mask {
                let _ = d.set_active_part(LayerPart::Mask);
            }
        }
        Op::Convert => {
            let other = match d.depth() {
                PixelDepth::Rgba8 => PixelDepth::Rgba16,
                PixelDepth::Rgba16 => PixelDepth::Rgba8,
            };
            d.convert_depth(other);
        }
    }
}

fn not_internal<T>(r: Result<T, IoError>) -> Result<Option<T>, TestCaseError> {
    match r {
        Err(IoError::Internal(m)) => Err(TestCaseError::fail(format!("panic: {m}"))),
        Err(_) => Ok(None),
        Ok(v) => Ok(Some(v)),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_documents_round_trip_exactly(sixteen in any::<bool>(), ops in proptest::collection::vec(op(), 0..24)) {
        let depth = if sixteen { PixelDepth::Rgba16 } else { PixelDepth::Rgba8 };
        let mut d = Document::with_depth(700, 500, depth);
        for o in &ops {
            apply(&mut d, o);
        }
        let bytes = save(&d);
        let loaded = load(&bytes).unwrap();
        prop_assert!(loaded.warnings.is_empty());
        assert_same(&loaded.document, &d);
        prop_assert_eq!(save(&loaded.document), bytes);
    }

    #[test]
    fn random_bytes_are_errors(bytes in proptest::collection::vec(any::<u8>(), 0..4096), zip_magic in any::<bool>()) {
        let mut b = bytes;
        if zip_magic && b.len() >= 4 {
            b[..4].copy_from_slice(b"PK\x03\x04");
        }
        prop_assert!(not_internal(load(&b))?.is_none());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(&b);
        let imported = read_png(Cursor::new(&png), (100, 100), PixelDepth::Rgba8, &Limits::default());
        not_internal(imported)?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn damaged_documents_never_load_as_something_else(
        edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        cut in proptest::option::of(any::<prop::sample::Index>()),
    ) {
        let original = common::sample_document(PixelDepth::Rgba16);
        let mut b = save(&original);
        for (at, v) in &edits {
            let i = at.index(b.len());
            b[i] = *v;
        }
        if let Some(cut) = cut {
            b.truncate(cut.index(b.len()));
        }
        if let Some(loaded) = not_internal(load(&b))? {
            assert_same(&loaded.document, &original);
        }
    }

    #[test]
    fn damaged_images_never_panic(
        edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        sixteen in any::<bool>(),
    ) {
        let mut d = Document::new(300, 200);
        let l = d.active_layer();
        paint(&mut d, l, &[(0, 0)], 3);
        let mut png = Vec::new();
        let opts = tela_io::PngExport {
            background: tela_io::PngBackground::Transparent,
            bits: if sixteen { tela_io::PngBits::Sixteen } else { tela_io::PngBits::Eight },
        };
        tela_io::write_png(&d, &mut png, &opts).unwrap();
        for (at, v) in &edits {
            let i = at.index(png.len());
            png[i] = *v;
        }
        not_internal(read_png(Cursor::new(&png), (300, 200), PixelDepth::Rgba16, &Limits::default()))?;
    }
}

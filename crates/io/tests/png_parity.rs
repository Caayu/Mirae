//! PNG import and export against the reference's own exports
//! (docs/parity/4F-file-workflow.md, F14–F18).
//!
//! `tests/reference/*.png` are the test images we wrote with `png_probe`;
//! `docs/parity/4F/csp_*.png` are what the reference exported after
//! importing each into a new default document (1600 × 1200, the image
//! centred) with its paper hidden ("transparent") or shown ("paper").
//! Mirae repeats the same steps headless and the results are compared
//! pixel for pixel.

use std::io::Cursor;
use std::path::PathBuf;

use tela_color::PixelDepth;
use tela_document::{Document, Placement};
use tela_io::{Limits, PngBackground, PngExport, read_png, write_png};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// An 8-bit RGBA PNG's pixels.
fn pixels(bytes: &[u8]) -> Vec<[u8; 4]> {
    let mut r = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight)
    );
    assert_eq!((info.width, info.height), (1600, 1200));
    buf.as_chunks::<4>().0.to_vec()
}

fn reference(name: &str) -> Vec<[u8; 4]> {
    pixels(&std::fs::read(root().join("../../docs/parity/4F").join(name)).unwrap())
}

/// Mirae: new 1600 × 1200 document, import `image`, export.
fn mirae(image: &str, depth: PixelDepth, background: PngBackground) -> Vec<[u8; 4]> {
    let bytes = std::fs::read(root().join("tests/reference").join(image)).unwrap();
    let img = read_png(Cursor::new(bytes), (1600, 1200), depth, &Limits::default()).unwrap();
    let mut d = Document::with_depth(1600, 1200, depth);
    let below = d.active_layer();
    d.add_layer_with_pixels(Placement::Above(below), image, img.pixels)
        .unwrap();
    let mut out = Vec::new();
    let options = PngExport {
        background,
        ..PngExport::default()
    };
    write_png(&d, &mut out, &options).unwrap();
    pixels(&out)
}

/// `(pixels that differ, largest channel difference)`. Two fully
/// transparent pixels are equal whatever their colour: the reference
/// writes white under zero alpha, Mirae black (D-4F-6), and no viewer can
/// see it.
fn compare(a: &[[u8; 4]], b: &[[u8; 4]]) -> (usize, u8) {
    let mut n = 0;
    let mut worst = 0;
    for (p, q) in a.iter().zip(b) {
        if p[3] == 0 && q[3] == 0 {
            continue;
        }
        let d = (0..4).map(|c| p[c].abs_diff(q[c])).max().unwrap();
        n += usize::from(d > 0);
        worst = worst.max(d);
    }
    (n, worst)
}

#[test]
fn exports_with_the_paper_equal_the_reference_exactly() {
    // F14: composite over white; premultiplied storage loses nothing
    // here (rounding commutes with adding the integer 255 − a).
    for depth in PixelDepth::ALL {
        let got = mirae("rgba8.png", depth, PngBackground::Paper);
        assert_eq!(
            compare(&got, &reference("csp_rgba8_paper.png")),
            (0, 0),
            "{depth:?}"
        );
    }
}

#[test]
fn opaque_imports_export_transparent_exactly_as_the_reference() {
    for depth in PixelDepth::ALL {
        let got = mirae("rgb8.png", depth, PngBackground::Transparent);
        assert_eq!(
            compare(&got, &reference("csp_rgb8_transparent.png")),
            (0, 0),
            "{depth:?}"
        );
    }
}

#[test]
fn translucent_imports_match_the_reference_in_16_bit_documents() {
    let want = reference("csp_rgba8_transparent.png");
    // F16: the reference keeps straight colour exactly; so does a 16-bit
    // document.
    let sixteen = mirae("rgba8.png", PixelDepth::Rgba16, PngBackground::Transparent);
    assert_eq!(compare(&sixteen, &want), (0, 0));
    // An 8-bit document stores premultiplied bytes (ADR 0001): colour at
    // low alpha moves (D-4F-4); alpha and opaque pixels do not.
    let eight = mirae("rgba8.png", PixelDepth::Rgba8, PngBackground::Transparent);
    let (n, worst) = compare(&eight, &want);
    assert!(n > 0 && worst <= 128, "{n} pixels, worst {worst}");
    for (p, q) in eight.iter().zip(&want) {
        assert_eq!(p[3], q[3], "alpha is exact");
        if q[3] == 255 {
            assert_eq!(p, q, "opaque pixels are exact");
        }
    }
}

#[test]
fn sixteen_bit_imports_round_where_the_reference_truncates() {
    let want = reference("csp_rgba16_transparent.png");
    for depth in PixelDepth::ALL {
        let got = mirae("rgba16.png", depth, PngBackground::Transparent);
        let (n, worst) = compare(&got, &want);
        // D-4F-3: 22 of the 80 swatches (24 × 24 px each) differ by one
        // level, exactly where rounding and `v >> 8` disagree.
        assert_eq!((n, worst), (22 * 24 * 24, 1), "{depth:?}");
    }
}

#[test]
fn only_the_colour_under_zero_alpha_differs_from_the_reference() {
    let want = reference("csp_rgb8_transparent.png");
    let got = mirae("rgb8.png", PixelDepth::Rgba8, PngBackground::Transparent);
    let clear: Vec<_> = got.iter().zip(&want).filter(|(_, q)| q[3] == 0).collect();
    assert!(
        clear
            .iter()
            .all(|(p, q)| **p == [0, 0, 0, 0] && **q == [255, 255, 255, 0])
    );
    assert_eq!(clear.len(), 1600 * 1200 - 244 * 244);
}

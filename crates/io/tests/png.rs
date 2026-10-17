//! PNG export and import fidelity.

mod common;

use std::collections::HashMap;
use std::io::Cursor;

use common::paint;
use tela_canvas::TileCoord;
use tela_color::{PixelDepth, PremulRgba, unorm8, unorm16};
use tela_document::{BlendMode, ComposeScratch, Document, Placement};
use tela_io::{IoError, Limits, PngBackground, PngBits, PngExport, Warning, read_png, write_png};

/// A decoded PNG: width, height, channels per pixel, 16-bit, samples.
struct Decoded {
    w: u32,
    h: u32,
    sixteen: bool,
    data: Vec<u8>,
    srgb: bool,
}

impl Decoded {
    fn rgba(&self, x: u32, y: u32) -> [u16; 4] {
        let i = (y * self.w + x) as usize * 4;
        if self.sixteen {
            [0, 1, 2, 3]
                .map(|c| u16::from_be_bytes([self.data[2 * (i + c)], self.data[2 * (i + c) + 1]]))
        } else {
            [0, 1, 2, 3].map(|c| u16::from(self.data[i + c]))
        }
    }
}

fn decode(bytes: &[u8]) -> Decoded {
    let dec = png::Decoder::new(Cursor::new(bytes));
    let mut r = dec.read_info().unwrap();
    let mut data = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut data).unwrap();
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "always RGBA (F13, F14)"
    );
    let srgb = r.info().srgb.is_some();
    Decoded {
        w: info.width,
        h: info.height,
        sixteen: info.bit_depth == png::BitDepth::Sixteen,
        data,
        srgb,
    }
}

fn export(d: &Document, background: PngBackground, bits: PngBits) -> Result<Decoded, IoError> {
    let mut out = Vec::new();
    write_png(d, &mut out, &PngExport { background, bits })?;
    Ok(decode(&out))
}

/// A document with translucent paint over several tiles, a partial last
/// tile row and column (600 × 300), a mask and a clipped layer.
fn scene(depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(600, 300, depth);
    let base = d.active_layer();
    paint(&mut d, base, &[(0, 0), (1, 0), (2, 1)], 5);
    let top = d.add_layer(Placement::Top).unwrap();
    paint(&mut d, top, &[(0, 0), (1, 1)], 9);
    d.set_opacity(top, 0.63).unwrap();
    d.add_mask(top).unwrap();
    d.modify_mask(top, &common::tiles(&[(0, 0)]), |_, t| {
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = (i % 251) as u8;
        }
    })
    .unwrap();
    let clip = d.add_layer(Placement::Top).unwrap();
    paint(&mut d, clip, &[(1, 0)], 2);
    d.set_clipped(clip, true).unwrap();
    d.set_blend_mode(clip, BlendMode::Multiply).unwrap();
    d
}

/// Composed tiles, as the compositor produced them (f32 and RGBA8).
type Composed = HashMap<TileCoord, Option<(Vec<PremulRgba>, Vec<[u8; 4]>)>>;

/// The composite at one pixel, in f32 and as the 8-bit display tile.
fn composite(d: &Document, x: u32, y: u32, cache: &mut Composed) -> (PremulRgba, [u8; 4]) {
    let c = TileCoord::containing(x as i32, y as i32);
    let (ox, oy) = c.origin();
    let tile = cache.entry(c).or_insert_with(|| {
        let mut scratch = ComposeScratch::new();
        d.compose_tile_into(c, None, &mut scratch)
            .then(|| (scratch.premultiplied().to_vec(), scratch.out.clone()))
    });
    let i = ((y as i32 - oy) * 256 + (x as i32 - ox)) as usize;
    tile.as_ref()
        .map_or((PremulRgba::TRANSPARENT, [0; 4]), |(f, e)| (f[i], e[i]))
}

#[test]
fn export_with_paper_is_the_composite_over_the_paper() {
    for depth in PixelDepth::ALL {
        let d = scene(depth);
        let png = export(&d, PngBackground::Paper, PngBits::Eight).unwrap();
        assert_eq!(
            (png.w, png.h, png.sixteen, png.srgb),
            (600, 300, false, true)
        );
        let paper = d.background().premultiply();
        let mut scratch = Composed::new();
        let mut worst_display = 0;
        for y in (0..300).step_by(3) {
            for x in (0..600).step_by(3) {
                let (f, eight) = composite(&d, x, y, &mut scratch);
                let over = f.over(paper);
                let want = [over.r, over.g, over.b, 1.0].map(|v| u16::from(unorm8(v)));
                assert_eq!(png.rgba(x, y), want, "({x}, {y}) exactly the f32 composite");
                // The screen adds the paper to the 8-bit composite instead.
                let shown = PremulRgba::from_rgba8(eight).over(paper);
                let shown = [shown.r, shown.g, shown.b].map(|v| u16::from(unorm8(v)));
                for (got, s) in png.rgba(x, y).into_iter().zip(shown) {
                    worst_display = worst_display.max(got.abs_diff(s));
                }
            }
        }
        assert!(
            worst_display <= 1,
            "within 1/255 of the screen: {worst_display}"
        );
    }
}

#[test]
fn transparent_export_is_the_unpremultiplied_composite() {
    let d = scene(PixelDepth::Rgba8);
    let png = export(&d, PngBackground::Transparent, PngBits::Eight).unwrap();
    let mut scratch = Composed::new();
    let mut seen_translucent = false;
    for y in (0..300).step_by(2) {
        for x in (0..600).step_by(2) {
            let (f, eight) = composite(&d, x, y, &mut scratch);
            let s = f.unpremultiply();
            let want = [s.r, s.g, s.b, s.a].map(|v| u16::from(unorm8(v)));
            assert_eq!(png.rgba(x, y), want, "({x}, {y})");
            // Premultiplied again, it is the 8-bit composite within a level.
            let got = png.rgba(x, y);
            let a = f32::from(got[3]) / 255.0;
            for c in 0..3 {
                let p = unorm8(f32::from(got[c]) / 255.0 * a);
                assert!(p.abs_diff(eight[c]) <= 1, "({x}, {y}) channel {c}");
            }
            seen_translucent |= (1..255).contains(&got[3]);
        }
    }
    assert!(seen_translucent);
}

#[test]
fn sixteen_bit_export_keeps_a_16_bit_composite() {
    let d = scene(PixelDepth::Rgba16);
    let wide = export(&d, PngBackground::Paper, PngBits::Sixteen).unwrap();
    let narrow = export(&d, PngBackground::Paper, PngBits::Eight).unwrap();
    assert!(wide.sixteen && !narrow.sixteen);
    let paper = d.background().premultiply();
    let mut scratch = Composed::new();
    let mut levels = std::collections::HashSet::new();
    for y in (0..300).step_by(3) {
        for x in (0..600).step_by(3) {
            let (f, _) = composite(&d, x, y, &mut scratch);
            let over = f.over(paper);
            let want = [over.r, over.g, over.b, 1.0].map(unorm16);
            assert_eq!(wide.rgba(x, y), want);
            for c in 0..3 {
                let n = u16::from(tela_color::narrow16(wide.rgba(x, y)[c]));
                assert!(n.abs_diff(narrow.rgba(x, y)[c]) <= 1);
                levels.insert(wide.rgba(x, y)[c] % 257);
            }
        }
    }
    assert!(levels.len() > 1, "values between 8-bit levels are exported");
}

#[test]
fn transparent_export_of_blend_layers_is_the_composite_without_paper() {
    // Until 5B this was refused (D-4F-5): how blend layers combine with a
    // translucent stack was unknown (D-4C-1). It is measured now (ADR
    // 0022), so the export is the composite over transparency.
    for depth in PixelDepth::ALL {
        let mut d = scene(depth);
        let top = d.layers().nth(1).unwrap().id();
        d.set_blend_mode(top, BlendMode::Screen).unwrap();
        let png = export(&d, PngBackground::Transparent, PngBits::Eight).unwrap();
        let mut scratch = ComposeScratch::new();
        let mut differs_from_paper = false;
        for (x, y) in [(10, 10), (300, 20), (520, 280), (100, 200), (599, 299)] {
            let c = TileCoord::containing(x as i32, y as i32);
            let (ox, oy) = c.origin();
            let i = ((y as i32 - oy) * 256 + (x as i32 - ox)) as usize;
            let p = if d.compose_tile_transparent_into(c, &mut scratch) {
                scratch.premultiplied()[i]
            } else {
                PremulRgba::TRANSPARENT
            };
            let s = p.unpremultiply();
            let want = [s.r, s.g, s.b, s.a].map(|v| u16::from(unorm8(v)));
            assert_eq!(png.rgba(x, y), want, "{depth:?} at ({x}, {y})");
            // Over the paper, Screen would see the white paper instead.
            if d.paper_backdrop(c) && p.a < 1.0 && p.a > 0.0 {
                differs_from_paper = true;
            }
        }
        assert!(differs_from_paper, "the scene has translucent blend pixels");
    }
}

/// A PNG with every (colour, alpha) pair: pixel (c, a) is grey c at a.
fn every_pair_png(sixteen: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 256, 256);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(if sixteen {
        png::BitDepth::Sixteen
    } else {
        png::BitDepth::Eight
    });
    let mut w = enc.write_header().unwrap();
    let mut data = Vec::new();
    for a in 0..256u32 {
        for c in 0..256u32 {
            if sixteen {
                // Low bytes set: values between 8-bit levels.
                let v = (c * 257 + (c * 37 + a) % 257).min(65535) as u16;
                let al = (a * 257 + (a * 91) % 257).min(65535) as u16;
                for s in [v, v, v, al] {
                    data.extend_from_slice(&s.to_be_bytes());
                }
            } else {
                data.extend([c as u8, c as u8, c as u8, a as u8]);
            }
        }
    }
    w.write_image_data(&data).unwrap();
    drop(w);
    out
}

fn import(bytes: &[u8], canvas: (u32, u32), depth: PixelDepth) -> tela_io::ImportedImage {
    read_png(Cursor::new(bytes), canvas, depth, &Limits::default()).unwrap()
}

#[test]
fn import_premultiplies_once_at_the_document_depth() {
    let png = every_pair_png(false);
    let eight = import(&png, (256, 256), PixelDepth::Rgba8);
    let sixteen = import(&png, (256, 256), PixelDepth::Rgba16);
    assert_eq!(eight.origin, (0, 0));
    assert!(eight.warnings.is_empty(), "{:?}", eight.warnings);
    let s8 = eight.pixels.rgba8().unwrap();
    let s16 = sixteen.pixels.rgba16().unwrap();
    for a in 0..256u32 {
        for c in 0..256u32 {
            let (x, y) = (c as i32, a as i32);
            let p8 = ((c * a) as f64 / 255.0).round() as u8;
            assert_eq!(s8.pixel(x, y), [p8, p8, p8, a as u8], "8-bit ({c}, {a})");
            let p16 = (f64::from(c * a) * 65535.0 / 65025.0).round() as u16;
            assert_eq!(
                s16.pixel(x, y),
                [p16, p16, p16, (a * 257) as u16],
                "16-bit ({c}, {a})"
            );
        }
    }
    // Transparent pixels store nothing; this image has none fully clear
    // except row 0, so one tile.
    assert_eq!(s8.tile_count(), 1);
}

#[test]
fn eight_bit_images_survive_import_and_export_exactly_in_16_bit_documents() {
    let png = every_pair_png(false);
    let mut worst8 = 0;
    for depth in PixelDepth::ALL {
        let img = import(&png, (256, 256), depth);
        let mut d = Document::with_depth(256, 256, depth);
        let l = d.active_layer();
        d.add_layer_with_pixels(Placement::Above(l), "pairs", img.pixels)
            .unwrap();
        let out = export(&d, PngBackground::Transparent, PngBits::Eight).unwrap();
        for a in 1..256u32 {
            for c in 0..256u32 {
                let got = out.rgba(c, a);
                assert_eq!(got[3], a as u16, "alpha is always exact");
                let err = got[0].abs_diff(c as u16);
                match depth {
                    // F16: the reference keeps straight colour exactly; a
                    // 16-bit document does too.
                    PixelDepth::Rgba16 => assert_eq!(err, 0, "({c}, {a})"),
                    PixelDepth::Rgba8 => worst8 = worst8.max(err),
                }
            }
        }
    }
    // Premultiplied 8-bit storage loses colour at very low alpha (ADR
    // 0001): at alpha 1 only black or white survive.
    assert!(worst8 > 100 && worst8 <= 128, "{worst8}");
}

#[test]
fn sixteen_bit_images_keep_16_bits_or_round_to_the_nearest_level() {
    let png = every_pair_png(true);
    let sixteen = import(&png, (256, 256), PixelDepth::Rgba16);
    assert!(sixteen.warnings.is_empty());
    let eight = import(&png, (256, 256), PixelDepth::Rgba8);
    assert_eq!(eight.warnings, [Warning::ReducedTo8Bits]);
    let (s16, s8) = (
        sixteen.pixels.rgba16().unwrap(),
        eight.pixels.rgba8().unwrap(),
    );
    let mut truncation_differs = 0;
    for a in 0..256u32 {
        for c in 0..256u32 {
            let v = f64::from((c * 257 + (c * 37 + a) % 257).min(65535));
            let al = f64::from((a * 257 + (a * 91) % 257).min(65535));
            let p = v / 65535.0 * (al / 65535.0);
            let want16 = (p * 65535.0).round() as u16;
            let (x, y) = (c as i32, a as i32);
            assert_eq!(s16.pixel(x, y)[0], want16, "16-bit kept ({c}, {a})");
            assert_eq!(s16.pixel(x, y)[3], al as u16);
            let want8 = (p * 255.0).round() as u8;
            assert_eq!(s8.pixel(x, y)[0], want8, "nearest 8-bit level ({c}, {a})");
            truncation_differs += usize::from(((al as u16) >> 8) as u8 != s8.pixel(x, y)[3]);
        }
    }
    // The reference truncates (F18); Mirae rounds: they differ often.
    assert!(truncation_differs > 10_000, "{truncation_differs}");
}

#[test]
fn imports_are_centred_like_the_reference() {
    let png = every_pair_png(false);
    // F15: 244 px in 1600 × 1200 lands at (678, 478); here 256 px.
    assert_eq!(
        import(&png, (1600, 1200), PixelDepth::Rgba8).origin,
        (672, 472)
    );
    assert_eq!(
        import(&png, (257, 256), PixelDepth::Rgba8).origin,
        (0, 0),
        "rounded down"
    );
    // Larger than the canvas: negative origin, pixels outside kept.
    let big = import(&png, (100, 50), PixelDepth::Rgba8);
    assert_eq!(big.origin, (-78, -103));
    let s = big.pixels.rgba8().unwrap();
    assert_eq!(s.pixel(-78 + 255, -103 + 255), [255, 255, 255, 255]);
    assert!(s.tile(TileCoord { x: -1, y: -1 }).is_some());
}

fn encode(
    w: u32,
    h: u32,
    colour: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    setup: impl FnOnce(&mut png::Encoder<&mut Vec<u8>>),
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(colour);
    enc.set_depth(depth);
    setup(&mut enc);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(data).unwrap();
    drop(wr);
    out
}

#[test]
fn every_colour_type_is_read() {
    use png::{BitDepth::*, ColorType::*};
    let px = |b: &[u8], d| import(b, (2, 1), d).pixels.pixel_rgba8(0, 0);
    let grey = encode(2, 1, Grayscale, Eight, &[200, 10], |_| {});
    assert_eq!(px(&grey, PixelDepth::Rgba8), [200, 200, 200, 255]);
    let grey_alpha = encode(2, 1, GrayscaleAlpha, Eight, &[200, 51, 0, 0], |_| {});
    assert_eq!(px(&grey_alpha, PixelDepth::Rgba8), [40, 40, 40, 51]);
    let rgb = encode(2, 1, Rgb, Eight, &[1, 2, 3, 4, 5, 6], |_| {});
    assert_eq!(px(&rgb, PixelDepth::Rgba16), [1, 2, 3, 255]);
    let one_bit = encode(2, 1, Grayscale, One, &[0b0100_0000], |_| {});
    assert_eq!(px(&one_bit, PixelDepth::Rgba8), [0, 0, 0, 255]);
    let palette = encode(2, 1, Indexed, Eight, &[1, 0], |e| {
        e.set_palette(vec![10, 20, 30, 250, 100, 50]);
        e.set_trns(vec![255, 128]);
    });
    assert_eq!(px(&palette, PixelDepth::Rgba8), [125, 50, 25, 128]);
}

#[test]
fn colour_space_chunks_are_reported_not_applied() {
    let data = [100u8, 150, 200, 255, 0, 0, 0, 0];
    let plain = encode(
        2,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &data,
        |_| {},
    );
    assert!(
        import(&plain, (2, 1), PixelDepth::Rgba8)
            .warnings
            .is_empty()
    );
    let srgb = encode(
        2,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &data,
        |e| {
            e.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        },
    );
    assert!(import(&srgb, (2, 1), PixelDepth::Rgba8).warnings.is_empty());
    let gamma = encode(
        2,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &data,
        |e| {
            e.set_source_gamma(png::ScaledFloat::new(1.0));
        },
    );
    let img = import(&gamma, (2, 1), PixelDepth::Rgba8);
    assert_eq!(img.warnings, [Warning::ColourSpaceIgnored]);
    assert_eq!(
        img.pixels.pixel_rgba8(0, 0),
        [100, 150, 200, 255],
        "values as stored"
    );
    // Mirae's own export re-imports without warnings.
    let d = scene(PixelDepth::Rgba8);
    let mut out = Vec::new();
    write_png(&d, &mut out, &PngExport::default()).unwrap();
    assert!(
        import(&out, (600, 300), PixelDepth::Rgba8)
            .warnings
            .is_empty()
    );
}

/// CRC-32 (ISO 3309) for hand-made PNG chunks.
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

/// An Adam7-interlaced RGBA8 PNG of `w × h` with pixel `f(x, y)`, its
/// zlib stream made of stored blocks (no compressor needed).
fn interlaced(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    const PASSES: [(u32, u32, u32, u32); 7] = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let mut raw = Vec::new();
    for (x0, y0, dx, dy) in PASSES {
        for y in (y0..h).step_by(dy as usize) {
            let xs: Vec<u32> = (x0..w).step_by(dx as usize).collect();
            if xs.is_empty() {
                continue;
            }
            raw.push(0);
            for x in xs {
                raw.extend(f(x, y));
            }
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &raw {
        s1 = (s1 + u32::from(b)) % 65_521;
        s2 = (s2 + s1) % 65_521;
    }
    z.extend(((s2 << 16) | s1).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 1]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[test]
fn interlaced_images_are_read() {
    let f = |x: u32, y: u32| [(x * 7) as u8, (y * 5) as u8, 9, 255];
    let png = interlaced(37, 23, f);
    let img = import(&png, (37, 23), PixelDepth::Rgba8);
    let s = img.pixels.rgba8().unwrap();
    for y in 0..23 {
        for x in 0..37 {
            assert_eq!(s.pixel(x as i32, y as i32), f(x, y), "({x}, {y})");
        }
    }
}

#[test]
fn oversized_and_broken_images_are_refused() {
    let png = every_pair_png(false);
    let tight = Limits {
        max_image_pixels: 1000,
        ..Limits::default()
    };
    assert!(matches!(
        read_png(Cursor::new(&png), (10, 10), PixelDepth::Rgba8, &tight),
        Err(IoError::LimitExceeded {
            what: "image pixels",
            ..
        })
    ));
    let tight = Limits {
        max_image_side: 100,
        ..Limits::default()
    };
    assert!(matches!(
        read_png(Cursor::new(&png), (10, 10), PixelDepth::Rgba8, &tight),
        Err(IoError::LimitExceeded { .. })
    ));
    // A header claiming 2³¹−1 × 2³¹−1 pixels.
    let mut huge = png.clone();
    huge[16..20].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
    huge[20..24].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
    let crc = crc32(&huge[12..29]);
    huge[29..33].copy_from_slice(&crc.to_be_bytes());
    assert!(matches!(
        read_png(
            Cursor::new(&huge),
            (10, 10),
            PixelDepth::Rgba8,
            &Limits::default()
        ),
        Err(IoError::LimitExceeded { .. })
    ));
    // Cut short: an error, not a partial layer.
    for len in [8, 33, 100, png.len() / 2, png.len() - 13] {
        let r = read_png(
            Cursor::new(&png[..len]),
            (10, 10),
            PixelDepth::Rgba8,
            &Limits::default(),
        );
        assert!(matches!(r, Err(IoError::InvalidImage(_))), "{len}: {r:?}");
    }
}

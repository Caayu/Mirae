//! The v1 compatibility contract: the committed fixtures in
//! `tests/fixtures/v1/` must load, now and in every future version, as
//! exactly the documents described here. The fixtures are never
//! regenerated (`examples/make_fixture.rs` refuses to overwrite one); if
//! this test fails, the reader broke compatibility — fix the reader, not
//! the fixture. The digest is defined here, independent of the writer.

use std::path::PathBuf;

use tela_canvas::TileCoord;
use tela_color::PixelDepth;
use tela_document::{BlendMode, Document, LayerPart};
use tela_io::{Limits, load_document};

fn fixture(name: &str) -> Document {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v1")
        .join(name);
    let loaded = load_document(&path, &Limits::default()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    loaded.document
}

/// FNV-1a 64 over everything a document file keeps, in a canonical order.
fn digest(d: &Document) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let sorted = |coords: Vec<TileCoord>| {
        let mut c = coords;
        c.sort_unstable_by_key(|c| (c.y, c.x));
        c
    };
    eat(&d.width().to_le_bytes());
    eat(&d.height().to_le_bytes());
    let p = d.background();
    for v in [p.r, p.g, p.b, p.a] {
        eat(&v.to_bits().to_le_bytes());
    }
    eat(&d.depth().bits().to_le_bytes());
    eat(&d.next_layer_id().unwrap_or(0).to_le_bytes());
    eat(&d.active_layer().get().to_le_bytes());
    eat(&[u8::from(d.active_part() == LayerPart::Mask)]);
    for l in d.layers() {
        eat(&l.id().get().to_le_bytes());
        eat(l.name().as_bytes());
        eat(&[
            u8::from(l.visible()),
            u8::from(l.locked()),
            u8::from(l.alpha_locked()),
            u8::from(l.clipped()),
        ]);
        eat(&l.opacity().to_bits().to_le_bytes());
        eat(l.blend_mode().name().as_bytes());
        for c in sorted(l.pixels().tile_coords().collect()) {
            eat(&c.x.to_le_bytes());
            eat(&c.y.to_le_bytes());
            match d.depth() {
                PixelDepth::Rgba8 => {
                    let t = l.pixels().rgba8().unwrap().tile(c).unwrap();
                    eat(t.pixels().as_flattened());
                }
                PixelDepth::Rgba16 => {
                    let t = l.pixels().rgba16().unwrap().tile(c).unwrap();
                    for v in t.pixels().as_flattened() {
                        eat(&v.to_le_bytes());
                    }
                }
            }
        }
        if let Some(m) = l.mask() {
            eat(&[2, u8::from(m.enabled())]);
            for c in sorted(m.tile_coords().collect()) {
                eat(&c.x.to_le_bytes());
                eat(&c.y.to_le_bytes());
                let values: Vec<u8> = m.reveal_tile(c).unwrap().collect();
                eat(&values);
            }
        }
    }
    h
}

/// What both fixtures hold, apart from pixel precision.
fn check_structure(d: &Document, depth: PixelDepth) {
    assert_eq!((d.width(), d.height(), d.depth()), (600, 400, depth));
    assert_eq!(
        d.next_layer_id(),
        Some(7),
        "id 4 was used by a removed layer"
    );
    assert_eq!(d.active_layer().get(), 5);
    assert_eq!(d.active_part(), LayerPart::Mask);
    let layers: Vec<_> = d.layers().collect();
    let names: Vec<&str> = layers.iter().map(|l| l.name()).collect();
    assert_eq!(
        names,
        [
            "Hidden locked",
            "Background",
            "Ink",
            "Clipped shade",
            "Masked — ✓"
        ]
    );
    let ids: Vec<u32> = layers.iter().map(|l| l.id().get()).collect();
    assert_eq!(ids, [6, 1, 2, 3, 5]);
    let [hidden, bg, ink, shade, masked] = layers[..] else {
        unreachable!()
    };
    assert!(!hidden.visible() && hidden.locked());
    assert!(!hidden.mask().unwrap().enabled());
    assert_eq!(bg.pixels().tile_count(), 6);
    assert_eq!(
        (ink.blend_mode(), ink.opacity()),
        (BlendMode::Multiply, 0.75)
    );
    assert!(ink.alpha_locked() && !ink.clipped());
    assert!(
        ink.pixels().has_tile(TileCoord { x: -1, y: 0 }),
        "off-canvas tile"
    );
    assert_eq!(
        (shade.blend_mode(), shade.opacity()),
        (BlendMode::Screen, 0.5)
    );
    assert!(shade.clipped());
    let m = masked.mask().unwrap();
    assert!(m.enabled());
    assert_eq!(m.value(256 + 7, 3), 7);
    assert_eq!(m.value(256 + 255, 9), 255);
    assert_eq!(m.value(10, 10), 255, "no tile there: shows everything");
    assert!(
        !masked.pixels().has_tile(TileCoord { x: 0, y: 0 }),
        "absent stays absent"
    );
}

#[test]
fn the_8_bit_v1_fixture_loads_as_written() {
    let d = fixture("rgba8.mirae");
    check_structure(&d, PixelDepth::Rgba8);
    let bg = d.layers().nth(1).unwrap().pixels().rgba8().unwrap();
    assert_eq!(bg.pixel(300, 10), [128, 115, 89, 255]);
    assert_eq!(digest(&d), 16_149_644_824_482_673_829, "digest");
}

#[test]
fn the_16_bit_v1_fixture_loads_as_written() {
    let d = fixture("rgba16.mirae");
    check_structure(&d, PixelDepth::Rgba16);
    let bg = d.layers().nth(1).unwrap().pixels().rgba16().unwrap();
    assert_eq!(bg.pixel(300, 10), [32768, 29491, 22937, 65535]);
    assert_eq!(digest(&d), 12_746_941_364_386_863_180, "digest");
}

/// FNV-1a 64 of a file's bytes.
fn file_hash(name: &str) -> u64 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/v1")
        .join(name);
    std::fs::read(path)
        .unwrap()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[test]
fn the_fixtures_are_the_committed_files() {
    // A regenerated or re-encoded fixture is no longer the contract.
    assert_eq!(
        file_hash("rgba8.mirae"),
        10_489_748_692_115_602_618,
        "rgba8.mirae"
    );
    assert_eq!(
        file_hash("rgba16.mirae"),
        6_969_975_651_774_566_589,
        "rgba16.mirae"
    );
}

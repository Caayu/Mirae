//! Shared helpers for the tela-io integration tests.
#![allow(dead_code)]

use std::io::{Cursor, Write};

use tela_canvas::{DirtyTiles, Tile, TileCoord};
use tela_color::{PixelDepth, StoragePixel, with_pixel_type};
use tela_document::{BlendMode, Document, LayerPart, Placement};
use tela_io::{Limits, Loaded, SaveOptions, read_document, write_document};

pub fn tiles(coords: &[(i32, i32)]) -> DirtyTiles {
    let mut t = DirtyTiles::new();
    for &(x, y) in coords {
        t.mark(TileCoord { x, y });
    }
    t
}

/// Paints a deterministic pattern (seeded by `k`) into the given tiles.
pub fn paint(d: &mut Document, layer: tela_document::LayerId, at: &[(i32, i32)], k: u32) {
    let depth = d.depth();
    with_pixel_type!(depth, P => {
        d.modify_tiles(layer, &tiles(at), |c, t: &mut Tile<P>| {
            for (i, p) in t.pixels_mut().iter_mut().enumerate() {
                let i = i as u32;
                if !(i * 7 + k + c.x as u32).is_multiple_of(5) {
                    continue;
                }
                let a = (i * 13 + k * 31) % 256;
                let c = |s: u32| ((s * a) / 255) as u8;
                let mut px = P::from_rgba8([c(i % 256), c((i / 3) % 256), c(k * 17 % 256), a as u8]);
                if depth == PixelDepth::Rgba16 {
                    // Values between 8-bit levels, premultiplied order kept.
                    px = P::from_premul(tela_color::PremulRgba::new(
                        f32::from(c(i % 256)) / 255.0 * 0.999,
                        (0.0003 * (i % 7) as f32).min(a as f32 / 255.0),
                        0.0,
                        a as f32 / 255.0,
                    ));
                }
                *p = px;
            }
        })
        .unwrap();
    });
}

/// A document exercising everything a file must keep: several layers,
/// every flag, a blend mode, opacity with many digits, masks (one
/// disabled, one sparse with a negative tile), clipping, alpha lock,
/// hidden and locked layers, tiles far off the canvas and at negative
/// coordinates, a tile erased back to transparency (present but blank),
/// a removed layer (its id stays used), a unicode name, the mask as the
/// edit target.
pub fn sample_document(depth: PixelDepth) -> Document {
    let mut d = Document::with_depth(1000, 600, depth);
    let base = d.active_layer();
    paint(
        &mut d,
        base,
        &[(0, 0), (1, 0), (3, 2), (-2, -1), (40, -30)],
        1,
    );
    let clip = d.add_layer(Placement::Top).unwrap();
    paint(&mut d, clip, &[(0, 0), (1, 1)], 2);
    d.set_clipped(clip, true).unwrap();
    d.set_blend_mode(clip, BlendMode::Multiply).unwrap();
    d.set_opacity(clip, 0.437_119_3).unwrap();
    let masked = d.add_layer(Placement::Top).unwrap();
    d.set_clipped(masked, false).unwrap();
    paint(&mut d, masked, &[(2, 1)], 3);
    d.add_mask(masked).unwrap();
    d.modify_mask(masked, &tiles(&[(2, 1), (-1, 0)]), |c, t| {
        for (i, v) in t.pixels_mut().iter_mut().enumerate() {
            *v = ((i as i32 * 3 + c.x) % 256) as u8;
        }
    })
    .unwrap();
    d.set_alpha_locked(masked, true).unwrap();
    let hidden = d.add_layer(Placement::Bottom).unwrap();
    paint(&mut d, hidden, &[(0, 1)], 4);
    d.add_mask(hidden).unwrap();
    d.set_mask_enabled(hidden, false).unwrap();
    d.set_visible(hidden, false).unwrap();
    d.set_name(hidden, "Esboço — 草稿 ✏").unwrap();
    // A tile that exists but holds nothing (erased).
    with_pixel_type!(depth, P => {
        d.modify_tiles(base, &tiles(&[(1, 0)]), |_, t: &mut Tile<P>| t.pixels_mut().fill(P::default()))
            .unwrap();
    });
    let gone = d.add_layer(Placement::Top).unwrap();
    d.remove_layer(gone).unwrap();
    d.set_blend_mode(base, BlendMode::Screen).unwrap();
    d.set_locked(base, true).unwrap();
    d.set_active_layer(masked).unwrap();
    d.set_active_part(LayerPart::Mask).unwrap();
    d
}

pub fn save(d: &Document) -> Vec<u8> {
    write_document(d, Cursor::new(Vec::new()), &SaveOptions::default())
        .unwrap()
        .into_inner()
}

pub fn load(bytes: &[u8]) -> Result<Loaded, tela_io::IoError> {
    read_document(Cursor::new(bytes), &Limits::default())
}

fn same_surface<P: StoragePixel>(
    a: &tela_canvas::TiledSurface<P>,
    b: &tela_canvas::TiledSurface<P>,
    what: &str,
) {
    let mut ca: Vec<TileCoord> = a.tile_coords().collect();
    let mut cb: Vec<TileCoord> = b.tile_coords().collect();
    ca.sort_unstable();
    cb.sort_unstable();
    assert_eq!(ca, cb, "{what}: the same tiles exist");
    for c in ca {
        assert!(a.tile(c) == b.tile(c), "{what}: tile {c:?} bit-exact");
    }
}

/// Everything a file keeps, compared bit for bit (history excluded).
pub fn assert_same(a: &Document, b: &Document) {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let (pa, pb) = (a.background(), b.background());
    assert_eq!(
        [pa.r, pa.g, pa.b, pa.a].map(f32::to_bits),
        [pb.r, pb.g, pb.b, pb.a].map(f32::to_bits)
    );
    assert_eq!(a.depth(), b.depth());
    assert_eq!(a.next_layer_id(), b.next_layer_id());
    assert_eq!(a.active_layer(), b.active_layer());
    assert_eq!(a.active_part(), b.active_part());
    assert_eq!(a.layer_count(), b.layer_count());
    for (la, lb) in a.layers().zip(b.layers()) {
        let what = format!("layer {}", la.id());
        assert_eq!(la.id(), lb.id(), "order and ids");
        assert_eq!(la.name(), lb.name(), "{what}");
        assert_eq!(la.visible(), lb.visible(), "{what}");
        assert_eq!(la.opacity().to_bits(), lb.opacity().to_bits(), "{what}");
        assert_eq!(la.blend_mode(), lb.blend_mode(), "{what}");
        assert_eq!(la.locked(), lb.locked(), "{what}");
        assert_eq!(la.alpha_locked(), lb.alpha_locked(), "{what}");
        assert_eq!(la.clipped(), lb.clipped(), "{what}");
        with_pixel_type!(a.depth(), P => same_surface(
            la.pixels().surface::<P>().unwrap(),
            lb.pixels().surface::<P>().unwrap(),
            &what,
        ));
        match (la.mask(), lb.mask()) {
            (None, None) => {}
            (Some(ma), Some(mb)) => {
                assert_eq!(ma.enabled(), mb.enabled(), "{what} mask");
                let mut ca: Vec<TileCoord> = ma.tile_coords().collect();
                let mut cb: Vec<TileCoord> = mb.tile_coords().collect();
                ca.sort_unstable();
                cb.sort_unstable();
                assert_eq!(ca, cb, "{what}: the same mask tiles exist");
                for c in ca {
                    assert!(
                        ma.reveal_tile(c).unwrap().eq(mb.reveal_tile(c).unwrap()),
                        "{what}: mask tile {c:?}"
                    );
                }
            }
            _ => panic!("{what}: mask presence differs"),
        }
    }
}

/// How one entry of a hand-made archive is stored.
#[derive(Clone, Copy)]
pub enum Method {
    Stored,
    Deflated,
}

/// A ZIP archive with exactly these entries, in this order.
pub fn archive(entries: &[(&str, &[u8], Method)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data, method) in entries {
        let m = match method {
            Method::Stored => zip::CompressionMethod::Stored,
            Method::Deflated => zip::CompressionMethod::Deflated,
        };
        let o = zip::write::SimpleFileOptions::default().compression_method(m);
        w.start_file(*name, o).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// The entries of an archive, in order, with their contents.
pub fn entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut v = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut v).unwrap();
            (f.name().unwrap().into_owned(), v)
        })
        .collect()
}

/// Rebuilds a saved document's archive after `edit` changed its entries.
pub fn rebuild(bytes: &[u8], edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
    let mut e = entries(bytes);
    edit(&mut e);
    let list: Vec<(&str, &[u8], Method)> = e
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice(), Method::Deflated))
        .collect();
    archive(&list)
}

/// The manifest of a saved document as JSON, edited, written back.
pub fn with_manifest(bytes: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    rebuild(bytes, |e| {
        let m = e.iter_mut().find(|(n, _)| n == "manifest.json").unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&m.1).unwrap();
        edit(&mut v);
        m.1 = serde_json::to_vec(&v).unwrap();
    })
}

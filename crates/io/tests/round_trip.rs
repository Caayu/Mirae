//! Save → load is the identity, bit for bit, at both depths.

mod common;

use std::io::Cursor;

use common::{assert_same, load, sample_document, save};
use tela_canvas::TileCoord;
use tela_color::PixelDepth;
use tela_document::{Document, LayerPart, Placement};
use tela_io::{
    Compression, IoError, Limits, SaveOptions, load_document, save_document, write_document,
};

#[test]
fn save_then_load_is_bit_exact_at_both_depths() {
    for depth in PixelDepth::ALL {
        let d = sample_document(depth);
        let loaded = load(&save(&d)).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_same(&loaded.document, &d);
        let back = &loaded.document;
        // Sparse layout, negative and far tiles, the blank-but-present tile.
        let base = back.layers().nth(1).unwrap();
        let coords: Vec<TileCoord> = base.pixels().tile_coords().collect();
        assert!(coords.contains(&TileCoord { x: -2, y: -1 }));
        assert!(coords.contains(&TileCoord { x: 40, y: -30 }));
        assert!(
            coords.contains(&TileCoord { x: 1, y: 0 }),
            "erased tile kept"
        );
        assert!(
            !coords.contains(&TileCoord { x: 2, y: 0 }),
            "absent stays absent"
        );
        assert_eq!(back.active_part(), LayerPart::Mask);
        // No history is stored; the loaded document starts clean.
        assert!(!back.can_undo() && !back.can_redo());
    }
}

#[test]
fn the_composite_is_unchanged_by_a_round_trip() {
    for depth in PixelDepth::ALL {
        let d = sample_document(depth);
        let back = load(&save(&d)).unwrap().document;
        for y in -1..3 {
            for x in -2..4 {
                let c = TileCoord { x, y };
                assert_eq!(d.compose_tile(c, None), back.compose_tile(c, None), "{c:?}");
            }
        }
    }
}

#[test]
fn saving_is_deterministic_and_idempotent() {
    for depth in PixelDepth::ALL {
        let d = sample_document(depth);
        let once = save(&d);
        assert_eq!(save(&d), once, "same document, same bytes");
        let again = save(&load(&once).unwrap().document);
        assert_eq!(again, once, "save ∘ load ∘ save = save");
    }
}

#[test]
fn every_compression_reads_back_identically() {
    let d = sample_document(PixelDepth::Rgba16);
    for compression in [
        Compression::Stored,
        Compression::Deflate { level: 1 },
        Compression::Deflate { level: 9 },
    ] {
        let options = SaveOptions {
            compression,
            ..SaveOptions::default()
        };
        let bytes = write_document(&d, Cursor::new(Vec::new()), &options)
            .unwrap()
            .into_inner();
        assert_same(&load(&bytes).unwrap().document, &d);
    }
}

#[test]
fn minimal_and_extreme_documents_round_trip() {
    // One empty layer.
    let d = Document::new(1, 1);
    assert_same(&load(&save(&d)).unwrap().document, &d);
    // Exhausted ids would need 4 billion layers; ids far apart do not.
    let mut d = Document::with_depth(300, 200, PixelDepth::Rgba16);
    for _ in 0..30 {
        let l = d.add_layer(Placement::Top).unwrap();
        d.remove_layer(l).unwrap();
    }
    d.add_layer(Placement::Bottom).unwrap();
    let back = load(&save(&d)).unwrap().document;
    assert_same(&back, &d);
    assert_eq!(
        back.next_layer_id(),
        Some(33),
        "ids stay unique after loading"
    );
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tela-io-rt-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn files_on_disk_round_trip_and_failed_saves_keep_the_old_file() {
    let dir = scratch("disk");
    let path = dir.join("drawing.mirae");
    let d = sample_document(PixelDepth::Rgba8);
    save_document(&d, &path, &SaveOptions::default()).unwrap();
    assert_same(
        &load_document(&path, &Limits::default()).unwrap().document,
        &d,
    );
    let before = std::fs::read(&path).unwrap();
    // A document the limits refuse is not written at all.
    let tight = SaveOptions {
        limits: Limits {
            max_layers: 1,
            ..Limits::default()
        },
        ..SaveOptions::default()
    };
    assert!(matches!(
        save_document(&d, &path, &tight),
        Err(IoError::LimitExceeded { what: "layers", .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before, "old file untouched");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "no temporary files: {leftovers:?}");
    // A missing file is a file system error.
    assert!(matches!(
        load_document(&dir.join("nope.mirae"), &Limits::default()),
        Err(IoError::Filesystem(_))
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Writes the sample document for inspection when `MIRAE_SAMPLE` names a
/// path (`cargo run -p tela-io --example mirae_dump -- <path>`).
#[test]
fn write_the_sample_when_asked() {
    if let Some(path) = std::env::var_os("MIRAE_SAMPLE") {
        let d = sample_document(PixelDepth::Rgba8);
        save_document(&d, std::path::Path::new(&path), &SaveOptions::default()).unwrap();
    }
}

#[test]
fn files_identify_themselves_in_their_first_57_bytes() {
    // docs/formats/mirae-v1.md: `mimetype` first and stored.
    let bytes = save(&Document::new(10, 10));
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert_eq!(&bytes[8..10], &[0, 0], "stored");
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(&bytes[38..57], tela_io::MIMETYPE.as_bytes());
}

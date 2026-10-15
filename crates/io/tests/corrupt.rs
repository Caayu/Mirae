//! Malfored, truncated and hostile files: always a specific error, never
//! a panic (a panic would surface as `IoError::Internal` through the
//! guard, so every case also checks that it is not one).

mod common;

use std::io::Cursor;

use common::{Method, archive, entries, load, rebuild, sample_document, save, with_manifest};
use serde_json::{Value, json};
use tela_color::PixelDepth;
use tela_document::PartsError;
use tela_io::{IoError, Limits, MIMETYPE, Warning, read_document};

/// An edit of the manifest JSON.
type Edit = Box<dyn FnOnce(&mut Value)>;

fn sample() -> Vec<u8> {
    save(&sample_document(PixelDepth::Rgba8))
}

fn err(bytes: &[u8]) -> IoError {
    match load(bytes) {
        Ok(_) => panic!("loaded a corrupt file"),
        Err(IoError::Internal(m)) => panic!("panicked inside the reader: {m}"),
        Err(e) => e,
    }
}

fn err_with(bytes: &[u8], limits: &Limits) -> IoError {
    match read_document(Cursor::new(bytes), limits) {
        Ok(_) => panic!("loaded despite the limits"),
        Err(IoError::Internal(m)) => panic!("panicked inside the reader: {m}"),
        Err(e) => e,
    }
}

fn manifest_err(edit: impl FnOnce(&mut Value)) -> IoError {
    err(&with_manifest(&sample(), edit))
}

#[test]
fn files_that_are_not_documents() {
    for junk in [
        &b""[..],
        b"hello",
        &[0u8; 4096][..],
        b"\x89PNG\r\n\x1a\n not really",
        b"PK\x03\x04 truncated local header",
    ] {
        assert!(matches!(err(junk), IoError::NotADocument), "{junk:?}");
    }
    // ZIP archives that are not Mirae documents.
    let other = archive(&[("readme.txt", b"hi", Method::Deflated)]);
    assert!(matches!(err(&other), IoError::NotADocument));
    let wrong = archive(&[("mimetype", b"application/zip", Method::Stored)]);
    assert!(matches!(err(&wrong), IoError::NotADocument));
    let no_manifest = archive(&[("mimetype", MIMETYPE.as_bytes(), Method::Stored)]);
    assert!(matches!(err(&no_manifest), IoError::InvalidManifest(_)));
}

#[test]
fn truncated_files_are_errors_at_every_length() {
    let bytes = sample();
    let step = (bytes.len() / 300).max(1);
    for len in (0..bytes.len()).step_by(step).chain([bytes.len() - 1]) {
        let _ = err(&bytes[..len]);
    }
}

#[test]
fn flipped_bytes_never_panic_and_never_yield_a_wrong_document() {
    let bytes = sample();
    let original = sample_document(PixelDepth::Rgba8);
    let step = (bytes.len() / 400).max(1);
    for at in (0..bytes.len()).step_by(step) {
        let mut b = bytes.clone();
        b[at] ^= 0x5a;
        match load(&b) {
            Err(IoError::Internal(m)) => panic!("panic at byte {at}: {m}"),
            Err(_) => {}
            // A flip in something unchecked (a header's time, an unused
            // field) may load; it must then be the same document.
            Ok(loaded) => common::assert_same(&loaded.document, &original),
        }
    }
}

#[test]
fn manifest_problems_name_the_field() {
    let cases: Vec<(&str, Edit)> = vec![
        ("format", Box::new(|m| m["format"] = json!("krita"))),
        ("version", Box::new(|m| m["version"] = json!("1"))),
        ("version", Box::new(|m| m["version"] = json!(1.5))),
        ("version", Box::new(|m| m["version"] = json!(-1))),
        ("version 0", Box::new(|m| m["version"] = json!(0))),
        ("canvas", Box::new(|m| m["canvas"] = json!([1000, 600]))),
        (
            "canvas.width",
            Box::new(|m| m["canvas"]["width"] = json!(-5)),
        ),
        (
            "canvas.height",
            Box::new(|m| m["canvas"]["height"] = json!(5e9)),
        ),
        ("paper", Box::new(|m| m["paper"] = json!([1, 1, 1]))),
        (
            "next_layer_id",
            Box::new(|m| {
                m.as_object_mut().unwrap().remove("next_layer_id");
            }),
        ),
        (
            "active_part",
            Box::new(|m| m["active_part"] = json!("both")),
        ),
        (
            "layers[1].opacity",
            Box::new(|m| m["layers"][1]["opacity"] = json!("half")),
        ),
        (
            "layers[0].visible",
            Box::new(|m| m["layers"][0]["visible"] = json!(1)),
        ),
        (
            "layers[0].name",
            Box::new(|m| {
                m["layers"][0].as_object_mut().unwrap().remove("name");
            }),
        ),
        (
            "layers[1].pixels[0]",
            Box::new(|m| m["layers"][1]["pixels"][0] = json!([1])),
        ),
        (
            "layers[1].pixels[0]",
            Box::new(|m| m["layers"][1]["pixels"][0] = json!([9e9, 0])),
        ),
        (
            "layers[1].pixels[0]",
            Box::new(|m| m["layers"][1]["pixels"][0] = json!([8388608, 0])),
        ),
        (
            "twice",
            Box::new(|m| {
                let first = m["layers"][1]["pixels"][0].clone();
                m["layers"][1]["pixels"][1] = first;
            }),
        ),
        (
            "layers[3].mask",
            Box::new(|m| m["layers"][3]["mask"] = json!(true)),
        ),
        ("layers", Box::new(|m| m["layers"] = json!({}))),
    ];
    for (field, edit) in cases {
        match manifest_err(edit) {
            IoError::InvalidManifest(msg) => assert!(msg.contains(field), "{field}: {msg}"),
            other => panic!("{field}: {other:?}"),
        }
    }
    // Not JSON at all, or cut short.
    let not_json = rebuild(&sample(), |e| {
        e.iter_mut().find(|(n, _)| n == "manifest.json").unwrap().1 = b"{\"format\": \"mi".to_vec();
    });
    assert!(matches!(err(&not_json), IoError::InvalidManifest(m) if m.contains("JSON")));
    // Deeply nested JSON is refused by the parser, without a stack overflow.
    let deep = rebuild(&sample(), |e| {
        let m = "[".repeat(100_000);
        e.iter_mut().find(|(n, _)| n == "manifest.json").unwrap().1 = m.into_bytes();
    });
    assert!(matches!(err(&deep), IoError::InvalidManifest(_)));
}

#[test]
fn versions_newer_than_this_build_are_refused_whole() {
    for v in [json!(2), json!(u64::MAX)] {
        // Even a newer file that would otherwise parse is not half-loaded.
        assert!(matches!(
            manifest_err(|m| m["version"] = v.clone()),
            IoError::UnsupportedVersion { supported: 1, .. }
        ));
    }
    // A newer version whose structure changed entirely fails as such.
    let future = rebuild(&sample(), |e| {
        e.iter_mut().find(|(n, _)| n == "manifest.json").unwrap().1 =
            br#"{"format":"mirae","version":7,"scenes":[]}"#.to_vec();
    });
    assert!(matches!(
        err(&future),
        IoError::UnsupportedVersion {
            found: 7,
            supported: 1
        }
    ));
}

#[test]
fn values_that_would_change_the_picture_are_errors_not_fallbacks() {
    assert!(matches!(
        manifest_err(|m| m["layers"][1]["blend"] = json!("linear-burn")),
        IoError::Unsupported(msg) if msg.contains("linear-burn")
    ));
    assert!(matches!(
        manifest_err(|m| m["depth"] = json!("rgba32f")),
        IoError::Unsupported(_)
    ));
    assert!(matches!(
        manifest_err(|m| m["layers"][2]["opacity"] = json!(1.5)),
        IoError::InvalidDocument(PartsError::InvalidOpacity { layer: 2 })
    ));
    assert!(matches!(
        manifest_err(|m| m["paper"] = json!([1, 1, 1, 0.5])),
        IoError::InvalidDocument(PartsError::InvalidPaper)
    ));
    // JSON has no NaN; a number beyond f32 becomes infinite and is refused.
    assert!(matches!(
        manifest_err(|m| m["layers"][2]["opacity"] = json!(1e300)),
        IoError::InvalidDocument(PartsError::InvalidOpacity { .. })
    ));
}

#[test]
fn inconsistent_documents_are_refused() {
    let cases: Vec<(Edit, PartsError)> = vec![
        (
            Box::new(|m| m["layers"][2]["id"] = json!(1)),
            PartsError::DuplicateId(1),
        ),
        (
            Box::new(|m| m["layers"][0]["id"] = json!(0)),
            PartsError::ZeroId,
        ),
        (
            Box::new(|m| m["active_layer"] = json!(77)),
            PartsError::ActiveNotFound(77),
        ),
        (
            Box::new(|m| m["next_layer_id"] = json!(2)),
            PartsError::StaleNextId {
                next: 2,
                largest: 4,
            },
        ),
        (
            Box::new(|m| m["canvas"]["width"] = json!(0)),
            PartsError::InvalidSize {
                width: 0,
                height: 600,
            },
        ),
        (
            Box::new(|m| {
                m["active_layer"] = json!(1);
                m["active_part"] = json!("mask");
            }),
            PartsError::MaskTargetWithoutMask,
        ),
    ];
    for (edit, want) in cases {
        match manifest_err(edit) {
            IoError::InvalidDocument(e) => assert_eq!(e, want),
            other => panic!("{want:?}: {other:?}"),
        }
    }
    // Without layers (and so without tile entries).
    let empty = rebuild(
        &with_manifest(&sample(), |m| m["layers"] = json!([])),
        |e| {
            e.retain(|(n, _)| !n.starts_with("layers/"));
        },
    );
    assert!(matches!(
        err(&empty),
        IoError::InvalidDocument(PartsError::NoLayers)
    ));
}

#[test]
fn missing_extra_and_damaged_tiles_are_errors() {
    let bytes = sample();
    let tile = "layers/2/pixels/0_0";
    // Listed but absent.
    let missing = rebuild(&bytes, |e| e.retain(|(n, _)| n != tile));
    assert!(
        matches!(err(&missing), IoError::CorruptTile { entry, problem } if entry == tile && problem == "missing")
    );
    // Present but not listed: data that would silently vanish.
    let extra = rebuild(&bytes, |e| {
        e.push(("layers/2/pixels/9_9".to_owned(), vec![0; 262_144]));
    });
    assert!(matches!(err(&extra), IoError::MalformedArchive(m) if m.contains("9_9")));
    // A layer that does not exist.
    let stray = rebuild(&bytes, |e| {
        e.push(("layers/99/mask/0_0".to_owned(), vec![0; 65_536]))
    });
    assert!(matches!(err(&stray), IoError::MalformedArchive(_)));
    // Short, long, empty.
    for len in [0, 1, 262_143, 262_145, 524_288] {
        let wrong = rebuild(&bytes, |e| {
            e.iter_mut().find(|(n, _)| n == tile).unwrap().1 = vec![1; len];
        });
        assert!(matches!(err(&wrong), IoError::CorruptTile { .. }), "{len}");
    }
    // A mask tile that shows everything is not something Mirae writes.
    let blank_mask = rebuild(&bytes, |e| {
        e.iter_mut()
            .find(|(n, _)| n == "layers/3/mask/2_1")
            .unwrap()
            .1 = vec![255; 65_536];
    });
    assert!(matches!(
        err(&blank_mask),
        IoError::InvalidDocument(PartsError::MaskTileRevealsAll { layer: 3, .. })
    ));
}

/// Offset of the central directory header of `name` in a raw archive.
fn central_header(bytes: &[u8], name: &str) -> usize {
    (0..bytes.len() - 46)
        .find(|&i| {
            bytes[i..i + 4] == [0x50, 0x4b, 0x01, 0x02] && {
                let n = usize::from(u16::from_le_bytes([bytes[i + 28], bytes[i + 29]]));
                bytes.get(i + 46..i + 46 + n) == Some(name.as_bytes())
            }
        })
        .expect("entry in the central directory")
}

/// Offset of the local header of `name`.
fn local_header(bytes: &[u8], name: &str) -> usize {
    let cd = central_header(bytes, name);
    u32::from_le_bytes(bytes[cd + 42..cd + 46].try_into().unwrap()) as usize
}

fn stored_sample() -> Vec<u8> {
    let e = entries(&sample());
    let list: Vec<(&str, &[u8], Method)> = e
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice(), Method::Stored))
        .collect();
    archive(&list)
}

#[test]
fn checksums_sizes_and_methods_are_enforced() {
    let tile = "layers/1/pixels/0_0";
    // A flipped data byte of a stored tile: the CRC catches it.
    let mut b = stored_sample();
    let local = local_header(&b, tile);
    let name_len = usize::from(u16::from_le_bytes([b[local + 26], b[local + 27]]));
    let extra_len = usize::from(u16::from_le_bytes([b[local + 28], b[local + 29]]));
    b[local + 30 + name_len + extra_len + 1000] ^= 1;
    assert!(
        matches!(err(&b), IoError::CorruptTile { problem, .. } if problem.contains("checksum"))
    );
    // A declared size that lies: the reader reads at most one byte past
    // the expected size, whatever the stream holds.
    let bomb = rebuild(&sample(), |e| {
        e.iter_mut().find(|(n, _)| n == tile).unwrap().1 = vec![0; 8 << 20];
    });
    let mut b = bomb;
    let cd = central_header(&b, tile);
    b[cd + 24..cd + 28].copy_from_slice(&262_144u32.to_le_bytes());
    let local = local_header(&b, tile);
    b[local + 22..local + 26].copy_from_slice(&262_144u32.to_le_bytes());
    assert!(matches!(err(&b), IoError::CorruptTile { .. }));
    // An unknown compression method (12 = bzip2) and the encryption flag.
    for (offset_cd, offset_local, value) in [(10usize, 8usize, 12u16), (8, 6, 1)] {
        let mut b = stored_sample();
        let cd = central_header(&b, tile);
        b[cd + offset_cd..cd + offset_cd + 2].copy_from_slice(&value.to_le_bytes());
        let local = local_header(&b, tile);
        b[local + offset_local..local + offset_local + 2].copy_from_slice(&value.to_le_bytes());
        let e = err(&b);
        assert!(
            matches!(
                e,
                IoError::MalformedArchive(_) | IoError::CorruptTile { .. }
            ),
            "{e:?}"
        );
    }
}

#[test]
fn duplicate_and_hostile_entry_names_are_refused() {
    // Two entries with one name (the ZIP reader would keep only one).
    let a = "layers/1/pixels/0_0";
    let b = "layers/1/pixels/zzz";
    let mut bytes = rebuild(&sample(), |e| e.push((b.to_owned(), vec![0; 262_144])));
    let mut at = 0;
    while let Some(i) = bytes[at..].windows(b.len()).position(|w| w == b.as_bytes()) {
        bytes[at + i..at + i + b.len()].copy_from_slice(a.as_bytes());
        at += i + b.len();
    }
    assert!(matches!(err(&bytes), IoError::MalformedArchive(m) if m.contains("duplicate")));
    for hostile in [
        "../outside",
        "/etc/passwd",
        "layers/../../x",
        "C:\\x",
        "a\u{1}b",
    ] {
        let b = rebuild(&sample(), |e| e.push((hostile.to_owned(), b"x".to_vec())));
        assert!(
            matches!(err(&b), IoError::MalformedArchive(m) if m.contains("unsafe")),
            "{hostile:?}"
        );
    }
}

#[test]
fn unknown_fields_and_entries_load_with_warnings() {
    let bytes = with_manifest(&sample(), |m| {
        m["thumbnail_hint"] = json!({"size": 256});
        m["layers"][0]["colour_label"] = json!("red");
        m["layers"][3]["mask"]["feather"] = json!(2.5);
        m["canvas"]["dpi"] = json!(300);
    });
    let bytes = rebuild(&bytes, |e| {
        e.push(("thumbnail.png".to_owned(), b"\x89PNG".to_vec()));
        e.push(("layers/".to_owned(), Vec::new()));
    });
    let loaded = load(&bytes).unwrap();
    common::assert_same(&loaded.document, &sample_document(PixelDepth::Rgba8));
    assert_eq!(
        loaded.warnings,
        [
            Warning::UnknownField("canvas.dpi".to_owned()),
            Warning::UnknownField("layers[0].colour_label".to_owned()),
            Warning::UnknownField("layers[3].mask.feather".to_owned()),
            Warning::UnknownField("thumbnail_hint".to_owned()),
            Warning::UnknownEntry("thumbnail.png".to_owned()),
        ]
    );
}

#[test]
fn limits_are_checked_before_the_data_is_read() {
    let bytes = sample();
    let cases: [(Limits, &str); 8] = [
        (
            Limits {
                max_layers: 3,
                ..Limits::default()
            },
            "layers",
        ),
        (
            Limits {
                max_tiles: 10,
                ..Limits::default()
            },
            "tiles",
        ),
        (
            Limits {
                max_pixel_bytes: 1 << 20,
                ..Limits::default()
            },
            "bytes of pixel data",
        ),
        (
            Limits {
                max_canvas_side: 999,
                ..Limits::default()
            },
            "pixels of canvas side",
        ),
        (
            Limits {
                max_name_bytes: 7,
                ..Limits::default()
            },
            "bytes of layer name",
        ),
        (
            Limits {
                max_manifest_bytes: 100,
                ..Limits::default()
            },
            "bytes of manifest",
        ),
        (
            Limits {
                max_entries: 5,
                ..Limits::default()
            },
            "archive entries",
        ),
        (
            Limits {
                max_directory_bytes: 100,
                ..Limits::default()
            },
            "bytes of ZIP directory",
        ),
    ];
    for (limits, what) in cases {
        match err_with(&bytes, &limits) {
            IoError::LimitExceeded { what: w, .. } => assert_eq!(w, what),
            other => panic!("{what}: {other:?}"),
        }
    }
    // Declared sizes are checked before anything is decompressed: a
    // manifest claiming 4 GiB is refused without reading it.
    let mut b = bytes;
    let cd = central_header(&b, "manifest.json");
    b[cd + 24..cd + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        err(&b),
        IoError::LimitExceeded {
            what: "bytes of manifest",
            ..
        } | IoError::MalformedArchive(_)
    ));
}

#[test]
fn a_million_layers_or_absurd_sizes_are_refused_cheaply() {
    let bytes = with_manifest(&sample(), |m| {
        let layer = m["layers"][0].clone();
        m["layers"] = Value::Array(vec![layer; 20_000]);
    });
    assert!(matches!(
        err(&bytes),
        IoError::LimitExceeded { what: "layers", .. }
    ));
    let bytes = with_manifest(&sample(), |m| {
        m["canvas"] = json!({"width": 4_000_000_000u32, "height": 1});
    });
    assert!(matches!(
        err(&bytes),
        IoError::LimitExceeded {
            what: "pixels of canvas side",
            ..
        }
    ));
    let bytes = with_manifest(&sample(), |m| {
        let tiles: Vec<Value> = (0..1_100_000)
            .map(|i| json!([i % 4000, i / 4000]))
            .collect();
        m["layers"][1]["pixels"] = Value::Array(tiles);
    });
    assert!(matches!(err(&bytes), IoError::LimitExceeded { .. }));
}

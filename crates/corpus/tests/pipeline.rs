//! One headless session through the whole stack (phase 4G): the real
//! brush engine paints into a document that uses every phase-4 feature,
//! the history is walked, the depth converted, the document saved as
//! `.mirae`, loaded, recomposed and compared — no UI involved.

mod support;

use support::*;
use tela_brush::preset::builtin::ids;
use tela_canvas::TileCoord;
use tela_color::PixelDepth;
use tela_document::{BlendMode, Document, HistoryAction, LayerId, LayerPart, Placement};

fn paint(
    d: &mut Document,
    p: &tela_brush::BrushPreset,
    c: usize,
    path: &[(f32, f32, f32)],
    seed: u64,
) {
    let done = stroke(d, p, colour(c), path, seed).expect("the target takes paint");
    assert!(done.preview_vs_commit <= 1, "preview == commit");
}

/// Layer ids of the session document.
struct Ids {
    flats: LayerId,
    light: LayerId,
    grain: LayerId,
}

/// Steps 1–8 of the session: an 8-bit document built with real brushes
/// and every phase-4 layer feature.
fn drawing_session() -> (Document, Ids) {
    // 1. A document (8-bit, as new documents are), with paper.
    let mut d = Document::new(1100, 700);
    let sketch = d.active_layer();
    d.set_name(sketch, "Sketch").unwrap();
    // 2–3. Real brushes: pencil lines, an ink line with pressure.
    for (i, y) in [120.0, 260.0, 400.0, 540.0].into_iter().enumerate() {
        paint(
            &mut d,
            &preset(ids::PENCIL),
            4,
            &line((60.0, y), (1040.0, y + 40.0), 120),
            100 + i as u64,
        );
    }
    // 4. Layers: colour flats, shading, highlights, a texture.
    d.add_layer(Placement::Above(sketch)).unwrap();
    let flats = d.active_layer();
    paint(
        &mut d,
        &round(260.0, 1.0),
        1,
        &line((200.0, 350.0), (900.0, 350.0), 60),
        110,
    );
    d.add_layer(Placement::Top).unwrap();
    let shade = d.active_layer();
    paint(
        &mut d,
        &preset(ids::SOFT_ROUND),
        4,
        &line((250.0, 420.0), (850.0, 300.0), 60),
        111,
    );
    d.add_layer(Placement::Top).unwrap();
    let light = d.active_layer();
    paint(
        &mut d,
        &preset(ids::AIRBRUSH),
        5,
        &line((400.0, 300.0), (700.0, 330.0), 60),
        112,
    );
    d.add_layer(Placement::Top).unwrap();
    let grain = d.active_layer();
    paint(
        &mut d,
        &preset(ids::CHALK),
        2,
        &line((100.0, 650.0), (1000.0, 80.0), 120),
        113,
    );
    // 5. Properties.
    d.set_opacity(sketch, 0.4).unwrap();
    d.set_opacity(grain, 0.7).unwrap();
    d.move_layer(sketch, Placement::Top).unwrap();
    // 6. A mask on the highlights, erased in a band.
    d.set_active_layer(light).unwrap();
    d.add_mask(light).unwrap();
    paint(
        &mut d,
        &eraser(70.0),
        0,
        &line((550.0, 200.0), (560.0, 450.0), 40),
        114,
    );
    // 7. Clipping: shading and highlights clipped to the flats; alpha
    //    lock on the flats, recoloured at the edges.
    d.set_clipped(shade, true).unwrap();
    d.set_clipped(light, true).unwrap();
    d.set_alpha_locked(flats, true).unwrap();
    d.set_active_layer(flats).unwrap();
    paint(
        &mut d,
        &round(120.0, 0.8),
        3,
        &line((180.0, 300.0), (920.0, 400.0), 60),
        115,
    );
    // 8. Blend modes.
    d.set_blend_mode(shade, BlendMode::Multiply).unwrap();
    d.set_blend_mode(light, BlendMode::Screen).unwrap();
    d.set_blend_mode(grain, BlendMode::Overlay).unwrap();
    (
        d,
        Ids {
            flats,
            light,
            grain,
        },
    )
}

#[test]
fn a_whole_drawing_session_survives_history_conversion_and_files() {
    let (
        mut d,
        Ids {
            flats,
            light,
            grain,
        },
    ) = drawing_session();
    check(&d);
    let session = content_digest(&d);
    let picture = composite_digest(&d);
    assert!(
        d.paper_backdrop(TileCoord { x: 1, y: 1 }),
        "grain blends with the paper"
    );

    // 9. Undo everything, redo everything.
    let steps = d.history_len();
    assert!(steps >= 20, "{steps} steps");
    while d.undo().is_some() {}
    assert_eq!(d.layer_count(), 1);
    assert!(
        d.layers().all(|l| l.pixels().tile_count() == 0),
        "back to the empty document"
    );
    while d.redo().is_some() {}
    assert_eq!(content_digest(&d), session);
    assert_eq!(composite_digest(&d), picture);

    // 10. Convert to 16 bits (exact), keep painting there.
    d.convert_depth(PixelDepth::Rgba16);
    assert_eq!(composite_digest(&d), picture, "8 → 16 is exact (DP2)");
    d.set_active_layer(grain).unwrap();
    for i in 0..10 {
        paint(
            &mut d,
            &round(300.0, 0.02),
            1,
            &line((300.0, 200.0), (800.0, 500.0), 30),
            120 + i,
        );
    }
    assert_eq!(d.next_undo(), Some(HistoryAction::Paint));
    check(&d);

    // 11–14. Save, load, recompose, compare state and picture.
    let loaded = round_trip(&d);
    assert_eq!(loaded.depth(), PixelDepth::Rgba16);
    let ids: Vec<_> = d
        .layers()
        .map(|l| (l.id(), l.name().to_owned(), l.clipped()))
        .collect();
    let back: Vec<_> = loaded
        .layers()
        .map(|l| (l.id(), l.name().to_owned(), l.clipped()))
        .collect();
    assert_eq!(ids, back);
    assert_eq!(
        loaded.layer(light).unwrap().mask().map(|m| m.enabled()),
        Some(true)
    );
    assert!(loaded.layer(flats).unwrap().alpha_locked());
    // The loaded document is editable like the original and stays equal.
    let mut a = d.clone();
    let mut b = loaded;
    for doc in [&mut a, &mut b] {
        doc.set_active_layer(light).unwrap();
        doc.set_active_part(LayerPart::Mask).unwrap();
        paint(
            doc,
            &round(40.0, 1.0),
            0,
            &line((560.0, 250.0), (560.0, 400.0), 20),
            130,
        );
    }
    assert_eq!(content_digest(&a), content_digest(&b));
    assert_eq!(composite_digest(&a), composite_digest(&b));
}

/// The session's composite over its paper, as the screen shows it.
fn shown(d: &Document) -> tela_corpus::Image {
    let (w, h) = (d.width(), d.height());
    let mut img = tela_corpus::Image::new(w, h);
    let paper = d.background().premultiply();
    for c in tiles_of_interest(d) {
        let Some(px) = compose(d, c, None) else {
            continue;
        };
        let (ox, oy) = c.origin();
        for (i, p) in px.iter().enumerate() {
            let (x, y) = (ox + (i % 256) as i32, oy + (i / 256) as i32);
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                continue;
            }
            let o = tela_color::PremulRgba::from_rgba8(*p).over(paper);
            img.set(x as u32, y as u32, o.to_rgba8());
        }
    }
    // Tiles with nothing show the paper.
    for y in 0..h {
        for x in 0..w {
            if img.get(x, y)[3] == 0 {
                img.set(x, y, paper.to_rgba8());
            }
        }
    }
    img
}

/// An integrated golden: the session document as shown. It pins how the
/// features combine (brush goldens and the blend sheet pin them apart).
/// Update only deliberately: `TELA_UPDATE_GOLDEN=1` (docs/golden-tests.md).
#[test]
fn the_session_document_matches_its_golden_image() {
    let (d, _) = drawing_session();
    let got = shown(&d);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/document_session.png");
    if tela_corpus::update_golden_requested() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        got.save_png(&path).unwrap();
        return;
    }
    let want = tela_corpus::Image::load_png(&path)
        .expect("golden exists (TELA_UPDATE_GOLDEN=1 creates it)");
    let diff = got
        .diff(&want, tela_corpus::Thresholds::default().pixel_tolerance)
        .expect("same size");
    assert!(
        tela_corpus::Thresholds::default().accepts(&diff),
        "{diff:?}"
    );
}

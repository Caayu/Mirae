//! Real workflows end to end (the 5F quality gate, section 9): each
//! scenario leaves a report with screenshots in target/e2e/<scenario>/.

#![cfg(windows)]

mod common;

use std::time::Duration;

use tela_e2e::win::Button;
use tela_e2e::{Session, read_png, role, sel, vk};

/// A: a whole illustration — outline, flats with the bucket, a Multiply
/// shadow layer through a selection, a gradient background under a
/// mask, export.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn a_complete_illustration() {
    let mut s = Session::start("scenario-a-illustration");
    s.maximize();
    // Outline: a closed shape with the ink pen.
    s.type_into(&role("SpinButton").after("Size"), "24");
    s.drag_path_doc(
        &[
            [1200.0, 800.0],
            [2800.0, 900.0],
            [3000.0, 2100.0],
            [1500.0, 2300.0],
            [1000.0, 1500.0],
            [1200.0, 800.0],
        ],
        14,
    );
    s.key(&[vk::CONTROL, vk::SHIFT], vk::letter('n'));
    // Flats with the bucket, referring to the canvas.
    s.type_into(&role("TextInput").after("Hex"), "F2C29B");
    s.click(&sel("Fill").role("Button"));
    let p = s.doc_to_client(2000.0, 1500.0);
    s.click_at(p[0], p[1]);
    assert_eq!(
        s.pixel(2000, 1500).0,
        [0xF2, 0xC2, 0x9B, 255],
        "flat colour"
    );
    // Shadow: a Multiply layer, a lasso, a soft brush.
    s.click(&sel("New").after("Layers"));
    s.choose(&role("ComboBox").after("Blend"), "Multiply");
    s.click(&sel("Clip"));
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Lasso"));
    s.drag_path_doc(
        &[
            [2000.0, 900.0],
            [3100.0, 900.0],
            [3100.0, 2400.0],
            [2000.0, 2400.0],
            [2000.0, 900.0],
        ],
        8,
    );
    s.click(&sel("Brush").role("Button"));
    s.choose(&role("ComboBox").after("Preset"), "Soft Round");
    s.type_into(&role("SpinButton").after("Size"), "500");
    s.type_into(&role("TextInput").after("Hex"), "8A7FB0");
    s.drag_doc([2300.0, 1000.0], [2300.0, 2200.0], 16);
    s.key(&[vk::CONTROL], vk::letter('d'));
    let (shadow, _) = s.pixel(2350, 1500);
    let (lit, _) = s.pixel(1400, 1500);
    assert!(
        shadow[0] < lit[0],
        "the shadow darkens: {shadow:?} vs {lit:?}"
    );
    assert_eq!(
        s.pixel(3500, 2800).0,
        [255, 255, 255, 255],
        "clipping keeps it on the flat"
    );
    // A gradient background under everything, masked at the bottom.
    s.click(&sel("New").after("Layers"));
    for _ in 0..3 {
        s.click(&sel("Down"));
    }
    s.click(&sel("Gradient").role("Button"));
    s.type_into(&role("TextInput").after("Hex"), "2E5E8C");
    s.click(&sel("Main to clear"));
    s.drag_doc([2000.0, 0.0], [2000.0, 3000.0], 16);
    s.click(&sel("Add mask"));
    s.click(&sel("Eraser").role("Button"));
    s.type_into(&role("SpinButton").after("Size"), "400");
    s.drag_doc([200.0, 2800.0], [3800.0, 2800.0], 16);
    s.click(&sel("Pixels").after("Edit"));
    s.shot("illustration");
    let png = s.dir().join("illustration.png");
    s.click(&sel("Export PNG…"));
    s.file_dialog(&png);
    let (w, h, _, rgba) = read_png(&png);
    assert_eq!((w, h), (4000, 3000));
    let at = |x: usize, y: usize| {
        let i = (y * 4000 + x) * 4;
        [
            rgba[i] as u8,
            rgba[i + 1] as u8,
            rgba[i + 2] as u8,
            rgba[i + 3] as u8,
        ]
    };
    assert_eq!(
        at(2000, 1500),
        s.pixel(2000, 1500).0,
        "the export is the composite"
    );
    assert_eq!(at(300, 300), s.pixel(300, 300).0);
    assert_eq!(s.layers().len(), 4);
}

/// B: complex editing — import, feathered selection, transform, recolour
/// with alpha lock, reorder, merge, save.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn b_complex_editing() {
    let mut s = Session::start("scenario-b-editing");
    s.maximize();
    let png = s.dir().join("photo.png");
    let (w, h) = (800u32, 600u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, p) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        p.copy_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255]);
    }
    tela_e2e::write_png(&png, &rgba, w, h);
    s.click(&sel("Import PNG…"));
    s.file_dialog(&png);
    assert_eq!(s.layers().len(), 2);
    // A feathered selection, moved by a transform.
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Ellipse"));
    s.drag_doc([1700.0, 1300.0], [2300.0, 1700.0], 8);
    s.type_into(&role("SpinButton").after("Feather"), "25");
    s.click(&sel("Apply").after("Feather"));
    s.click(&sel("Transform").role("Button"));
    s.type_into(&role("SpinButton").after("Angle"), "30");
    s.click(&sel("Apply"));
    assert_eq!(s.settle()["document"]["undo"], "transform");
    s.key(&[vk::CONTROL], vk::letter('d'));
    // Recolour with alpha lock: the picture's alpha is kept.
    s.click(&sel("Lock alpha"));
    s.click(&sel("Brush").role("Button"));
    s.type_into(&role("SpinButton").after("Size"), "300");
    s.type_into(&role("TextInput").after("Hex"), "FF00FF");
    s.drag_doc([1500.0, 1500.0], [2600.0, 1500.0], 12);
    let (_, outside) = s.pixel(500, 500);
    assert_eq!(
        outside.unwrap()[3],
        0,
        "alpha lock painted nothing outside the picture"
    );
    s.click(&sel("Lock alpha"));
    // Reorder and merge down.
    s.click(&sel("Down"));
    s.click(&sel("Up"));
    s.click(&sel("Merge down"));
    if s.settle()["prompt"] == "merge" {
        s.click(&sel("Keep"));
    }
    assert_eq!(s.layers().len(), 1);
    let digest = s.digest();
    let file = s.dir().join("edited.mirae");
    s.key(&[vk::SHIFT, vk::ALT], vk::letter('s'));
    s.file_dialog(&file);
    s.new_document(8);
    s.key(&[vk::CONTROL], vk::letter('o'));
    s.file_dialog(&file);
    assert_eq!(s.digest(), digest, "saved and reopened identical");
    s.shot("edited");
}

/// C: a 16-bit document — colour painting, a gradient, clipping, a mask,
/// undo/redo, save/reopen, a 16-bit PNG.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn c_sixteen_bit_document() {
    let mut s = Session::start("scenario-c-16bit");
    s.maximize();
    s.new_document(16);
    assert_eq!(s.state()["document"]["bits"], 16);
    s.type_into(&role("TextInput").after("Hex"), "204060");
    s.choose(&role("ComboBox").after("Preset"), "Soft Round");
    s.type_into(&role("SpinButton").after("Size"), "900");
    s.drag_doc([1000.0, 1500.0], [3000.0, 1500.0], 20);
    s.click(&sel("New").after("Layers"));
    s.click(&sel("Clip"));
    s.click(&sel("Gradient").role("Button"));
    s.click(&sel("Linear light").after("Mix"));
    s.type_into(&role("TextInput").after("Hex"), "FFD080");
    s.drag_doc([1000.0, 1000.0], [3000.0, 2000.0], 16);
    s.click(&sel("Add mask"));
    s.click(&sel("Eraser").role("Button"));
    s.drag_doc([2000.0, 1000.0], [2000.0, 2000.0], 10);
    let before = s.digest();
    for _ in 0..3 {
        s.key(&[vk::CONTROL], vk::letter('z'));
    }
    for _ in 0..3 {
        s.key(&[vk::CONTROL], vk::letter('y'));
    }
    assert_eq!(s.digest(), before, "undo/redo round trip");
    let file = s.dir().join("deep.mirae");
    s.key(&[vk::SHIFT, vk::ALT], vk::letter('s'));
    s.file_dialog(&file);
    s.new_document(8);
    s.key(&[vk::CONTROL], vk::letter('o'));
    s.file_dialog(&file);
    assert_eq!(s.state()["document"]["bits"], 16);
    assert_eq!(s.digest(), before, "reopened identical");
    s.click(&sel("16-bit").role("CheckBox"));
    let png = s.dir().join("deep.png");
    s.click(&sel("Export PNG…"));
    s.file_dialog(&png);
    let (_, _, bits, rgba) = read_png(&png);
    assert_eq!(bits, 16);
    // A 16-bit export carries more than 8 bits somewhere in the ramp.
    let fine = rgba.iter().filter(|&&v| v % 257 != 0).count();
    assert!(
        fine > 1000,
        "16-bit precision in the export ({fine} samples)"
    );
    s.shot("16bit");
}

/// D: interruptions in the middle of every kind of drag and session.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn d_interruptions() {
    let mut s = Session::start("scenario-d-interruptions");
    s.maximize();
    s.drag_doc([1000.0, 1500.0], [3000.0, 1500.0], 16);
    let base = s.digest();
    // Escape during a selection drag and a gradient drag.
    for tool in ["Select", "Gradient"] {
        s.click(&sel(tool).role("Button"));
        let a = s.doc_to_client(1200.0, 1200.0);
        let b = s.doc_to_client(2600.0, 1900.0);
        s.move_to(a[0], a[1]);
        s.press(Button::Left);
        s.drag_moves(a, b, 10);
        s.key(&[], vk::ESCAPE);
        s.release(Button::Left);
        let st = s.settle();
        assert_eq!(st["drag"], "none", "{tool}");
        assert_eq!(s.digest(), base, "{tool}: Escape left nothing");
    }
    // Undo, tool and layer keys during a gradient drag do nothing.
    let a = s.doc_to_client(1200.0, 1200.0);
    let b = s.doc_to_client(2600.0, 1900.0);
    s.move_to(a[0], a[1]);
    s.press(Button::Left);
    s.drag_moves(a, b, 10);
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.key(&[], vk::letter('b'));
    s.key(&[vk::CONTROL, vk::SHIFT], vk::letter('n'));
    s.release(Button::Left);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "gradient", "the gradient was drawn");
    assert_eq!(
        st["document"]["layers"].as_array().unwrap().len(),
        1,
        "no layer added mid-drag"
    );
    s.key(&[vk::CONTROL], vk::letter('z'));
    // A transform session: another layer, a file command, a resize.
    s.click(&sel("Transform").role("Button"));
    s.click(&sel("New").after("Layers"));
    assert_eq!(s.settle()["prompt"], "apply_transform");
    s.click(&sel("Cancel"));
    s.key(&[vk::CONTROL], vk::letter('s'));
    std::thread::sleep(Duration::from_millis(300));
    assert!(s.dialogs().is_empty(), "no file dialog during a transform");
    s.place(100, 100, 1100, 750);
    assert_eq!(
        s.settle()["transform"],
        true,
        "the session survives a resize"
    );
    s.key(&[], vk::ESCAPE);
    s.maximize();
    assert_eq!(s.digest(), base);
    // A stroke and the focus lost mid-stroke: the stroke ends cleanly.
    s.click(&sel("Brush").role("Button"));
    let a = s.doc_to_client(1000.0, 500.0);
    let b = s.doc_to_client(3000.0, 500.0);
    s.move_to(a[0], a[1]);
    s.press(Button::Left);
    s.drag_moves(a, b, 10);
    {
        let _thief = tela_e2e::win::FocusThief::new();
        std::thread::sleep(Duration::from_millis(200));
        tela_e2e::win::button(Button::Left, false);
    }
    s.front();
    let st = s.settle();
    assert_eq!(st["drag"], "none");
    assert_eq!(st["document"]["undo"], "paint", "the stroke was committed");
}

/// E: a long session switching tools and repeating operations; nothing
/// residual (stuck drags, focus, stale tiles, desynchronized controls).
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn e_long_session() {
    let mut s = Session::start("scenario-e-long-session");
    s.maximize();
    let tools = [
        "Brush",
        "Select",
        "Fill",
        "Gradient",
        "Eraser",
        "Move",
        "Eyedropper",
        "Brush",
    ];
    // Strokes wide enough to stay several screen pixels wide when the
    // whole document is shown (thinner ones are averaged by the display).
    s.type_into(&role("SpinButton").after("Size"), "150");
    for round in 0..6 {
        for t in tools {
            s.click(&sel(t).role("Button"));
            let st = s.settle();
            assert_eq!(st["drag"], "none", "round {round}, {t}");
            assert_eq!(
                st["ui_focus"], false,
                "round {round}, {t}: no field holds the keys"
            );
        }
        let y = 400.0 + round as f32 * 400.0;
        s.click(&sel("Brush").role("Button"));
        s.drag_doc([500.0, y], [3500.0, y], 12);
        s.click(&sel("Gradient").role("Button"));
        s.click(&sel("Main to clear"));
        s.drag_doc([500.0, y], [1500.0, y + 200.0], 12);
        s.key(&[vk::CONTROL], vk::letter('z'));
        s.key(&[vk::CONTROL], vk::letter('y'));
        // Shortcuts keep working after a field was used.
        s.type_into(&role("TextInput").after("Hex"), "112233");
        s.key(&[], vk::letter('d'));
        assert_eq!(
            s.state()["colors"]["main"],
            "#000000",
            "round {round}: D after typing"
        );
        // The screen shows what the document holds.
        let (doc, _) = s.pixel(2000, y as i32);
        let screen = s.screen_rgb_doc(2000.0, y);
        assert!(
            common::diff([screen[0], screen[1], screen[2], 255], doc) <= 24,
            "round {round}: screen {screen:?} vs document {doc:?}"
        );
    }
    let st = s.settle();
    assert_eq!(st["pending_tiles"], 0);
    s.shot("after-rounds");
}

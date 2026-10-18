//! Files and history through the real interface: New, Open, Save, Save As
//! (native dialogs), PNG import and export at 8 and 16 bits, the
//! unsaved-changes question, a hostile file, and the undo history's
//! save point, redo invalidation and merged steps.

#![cfg(windows)]

mod common;

use serde_json::Value;
use tela_e2e::{Session, read_png, role, sel, vk};

fn paint_something(s: &mut Session) {
    s.click(&sel("Brush").role("Button"));
    s.drag_doc([1000.0, 1500.0], [3000.0, 1500.0], 20);
    s.drag_doc([2000.0, 800.0], [2000.0, 2200.0], 20);
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn save_reopen_and_export_keep_every_pixel_at_8_and_16_bits() {
    let mut s = Session::start("files-save-open-export");
    s.maximize();
    for bits in [8u32, 16] {
        s.new_document(bits);
        assert_eq!(s.state()["document"]["bits"], bits);
        paint_something(&mut s);
        // A second, translucent layer in Multiply.
        s.click(&sel("New").after("Layers"));
        s.choose(&role("ComboBox").after("Blend"), "Multiply");
        s.type_into(&role("SpinButton").after("Layers").after("Opacity"), "60");
        s.click(&sel("Gradient").role("Button"));
        s.drag_doc([500.0, 500.0], [3500.0, 2500.0], 20);
        let st = s.settle();
        assert_eq!(st["document"]["layers"][1]["blend"], "Multiply");
        assert_eq!(st["document"]["layers"][1]["opacity"], 0.6);
        let digest = s.digest();
        let probes: Vec<[u8; 4]> = [[2000, 1500], [600, 600], [3400, 2400], [100, 2900]]
            .iter()
            .map(|p| s.pixel(p[0], p[1]).0)
            .collect();
        // Save As (Shift+Alt+S: Ctrl+Shift+S is taken by another program on
        // the development machine), into the test's folder.
        let file = s.dir().join(format!("doc{bits}.mirae"));
        s.key(&[vk::SHIFT, vk::ALT], vk::letter('s'));
        s.file_dialog(&file);
        let st = s.settle();
        assert!(file.exists(), "saved");
        assert_eq!(st["document"]["modified"], false, "saved is clean");
        assert_eq!(st["document"]["name"], format!("doc{bits}"));
        // Export PNG at the document's depth.
        if bits == 16 {
            s.click(&sel("16-bit").role("CheckBox"));
        }
        let png = s.dir().join(format!("doc{bits}.png"));
        s.click(&sel("Export PNG…"));
        s.file_dialog(&png);
        let (w, h, depth, rgba) = read_png(&png);
        assert_eq!((w, h, depth), (4000, 3000, if bits == 16 { 16 } else { 8 }));
        for (p, expected) in [[2000, 1500], [600, 600], [3400, 2400], [100, 2900]]
            .iter()
            .zip(&probes)
        {
            let i = ((p[1] * 4000 + p[0]) * 4) as usize;
            let to8 = |v: u16| {
                if depth == 16 {
                    (f32::from(v) / 257.0).round() as u8
                } else {
                    v as u8
                }
            };
            let got = [
                to8(rgba[i]),
                to8(rgba[i + 1]),
                to8(rgba[i + 2]),
                to8(rgba[i + 3]),
            ];
            assert!(
                common::diff(got, *expected) <= 1,
                "exported {p:?}: {got:?} against the composite {expected:?}"
            );
        }
        // New, then Open the file again: the same document.
        s.new_document(8);
        assert_eq!(s.layers().len(), 1);
        s.key(&[vk::CONTROL], vk::letter('o'));
        s.file_dialog(&file);
        let st = s.settle();
        assert_eq!(st["document"]["bits"], bits);
        assert_eq!(st["document"]["modified"], false);
        assert_eq!(s.digest(), digest, "reopened identical ({bits}-bit)");
        s.shot(&format!("reopened-{bits}"));
    }
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn unsaved_changes_are_never_discarded_silently() {
    let mut s = Session::start("files-unsaved");
    paint_something(&mut s);
    let digest = s.digest();
    // New asks; Cancel keeps everything.
    s.click(&sel("New").role("Button"));
    s.click(&sel("New 8-bit document"));
    assert_eq!(s.settle()["prompt"], "unsaved");
    s.shot("asked");
    s.click(&sel("Cancel"));
    let st = s.settle();
    assert_eq!(st["prompt"], "");
    assert_eq!(s.digest(), digest, "Cancel kept the document");
    // Closing asks too; Cancel keeps the window.
    tela_e2e::win::close(s.window);
    std::thread::sleep(std::time::Duration::from_millis(400));
    assert_eq!(s.settle()["prompt"], "unsaved");
    s.click(&sel("Cancel"));
    assert!(!s.exited());
    // Save from the question writes the file and then proceeds.
    let file = s.dir().join("kept.mirae");
    s.click(&sel("New").role("Button"));
    s.click(&sel("New 16-bit document"));
    s.click(&sel("Save").after("Don't Save"));
    s.file_dialog(&file);
    let st = s.settle();
    assert!(file.exists());
    assert_eq!(st["document"]["bits"], 16, "the new document followed");
    assert_eq!(st["document"]["modified"], false);
    // Undo back to a save point is clean again.
    paint_something(&mut s);
    assert_eq!(s.settle()["document"]["modified"], true);
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.key(&[vk::CONTROL], vk::letter('z'));
    assert_eq!(s.settle()["document"]["modified"], false);
    s.close("Don't Save");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn a_hostile_file_is_refused_and_the_document_kept() {
    let mut s = Session::start("files-hostile");
    paint_something(&mut s);
    let digest = s.digest();
    let bad = s.dir().join("broken.mirae");
    std::fs::write(&bad, b"PK\x03\x04 this is not a Mirae document").unwrap();
    s.key(&[vk::CONTROL], vk::letter('o'));
    if s.state()["prompt"] == "unsaved" {
        s.click(&sel("Don't Save"));
    }
    s.file_dialog(&bad);
    let st = s.settle();
    s.shot("refused");
    assert_eq!(st["prompt"], "notice", "an error is shown");
    s.click(&sel("OK"));
    assert_eq!(s.digest(), digest, "the open document is untouched");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn import_png_adds_a_layer() {
    let mut s = Session::start("files-import");
    let png = s.dir().join("square.png");
    let (w, h) = (300u32, 200u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for p in rgba.as_chunks_mut::<4>().0 {
        p.copy_from_slice(&[200, 40, 20, 255]);
    }
    tela_e2e::write_png(&png, &rgba, w, h);
    s.click(&sel("Import PNG…"));
    s.file_dialog(&png);
    let st = s.settle();
    assert_eq!(st["document"]["layers"].as_array().unwrap().len(), 2);
    assert_eq!(st["document"]["undo"], "import image");
    let (c, _) = s.pixel(2000, 1500);
    assert_eq!(c, [200, 40, 20, 255], "centred");
    s.key(&[vk::CONTROL], vk::letter('z'));
    assert_eq!(s.layers().len(), 1);
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn history_save_point_redo_invalidation_and_merged_steps() {
    let mut s = Session::start("history");
    s.maximize();
    paint_something(&mut s);
    let after_two = s.digest();
    s.key(&[vk::CONTROL], vk::letter('z'));
    assert_eq!(s.settle()["document"]["redo"], "paint");
    // A new action drops the redo step.
    s.drag_doc([500.0, 500.0], [900.0, 900.0], 10);
    let st = s.settle();
    assert_eq!(st["document"]["redo"], Value::Null, "redo invalidated");
    assert_ne!(s.digest(), after_two);
    // Two opacity edits in a row are one step.
    s.type_into(&role("SpinButton").after("Layers").after("Opacity"), "70");
    s.type_into(&role("SpinButton").after("Layers").after("Opacity"), "40");
    assert_eq!(s.active_layer()["opacity"], 0.4);
    s.key(&[vk::CONTROL], vk::letter('z'));
    let st = s.settle();
    assert_eq!(
        s.active_layer()["opacity"],
        1.0,
        "both opacity edits undone at once"
    );
    assert_eq!(st["document"]["undo"], "paint");
    // The Undo and Redo buttons do what the keys do.
    s.click(&sel("Undo"));
    s.click(&sel("Redo"));
    s.click(&sel("Redo"));
    assert_eq!(s.active_layer()["opacity"], 0.4);
}

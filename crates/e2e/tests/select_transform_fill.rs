//! Selections, transforms and fill through the real interface (the E2E
//! matrix, docs/quality/E2E_MATRIX.md).

#![cfg(windows)]

mod common;

use tela_e2e::{Session, role, sel, vk};

fn black(p: [u8; 4]) -> bool {
    p[0] < 40 && p[3] == 255
}

fn white(p: [u8; 4]) -> bool {
    p == [255, 255, 255, 255]
}

/// Fills the whole (empty) layer through Edit › Fill with the main colour.
fn fill_layer(s: &mut Session) {
    s.key(&[vk::ALT], vk::DELETE);
    s.settle();
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn selections_shapes_operators_commands_and_painting_inside() {
    let mut s = Session::start("selection");
    s.maximize();
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Rectangle"));
    s.drag_doc([1000.0, 1000.0], [2000.0, 2000.0], 10);
    assert_eq!(s.settle()["document"]["selection"], true);
    assert_eq!(s.state()["document"]["undo"], "selection");
    // Add an ellipse (mode button), subtract a lasso (Alt at the press).
    s.click(&sel("Add"));
    s.click(&sel("Ellipse"));
    s.drag_doc([2200.0, 1000.0], [3000.0, 1800.0], 10);
    s.click(&sel("New").after("Mode"));
    s.click(&sel("Lasso"));
    s.front();
    tela_e2e::win::key(vk::ALT, true);
    s.drag_path_doc(
        &[
            [1100.0, 1100.0],
            [1400.0, 1100.0],
            [1400.0, 1400.0],
            [1100.0, 1400.0],
            [1100.0, 1100.0],
        ],
        6,
    );
    tela_e2e::win::key(vk::ALT, false);
    s.settle();
    // Fill the selection black: inside the rectangle and the ellipse, not
    // in the lasso hole, nowhere else.
    s.click(&sel("Fill").role("Button"));
    s.click(&sel("Fill selection"));
    let probe = |s: &mut Session, x, y| s.pixel(x, y).0;
    assert!(black(probe(&mut s, 1800, 1800)), "rectangle");
    assert!(black(probe(&mut s, 2600, 1400)), "ellipse");
    assert!(white(probe(&mut s, 1250, 1250)), "the lasso subtracted");
    assert!(white(probe(&mut s, 3500, 2500)), "outside");
    // Invert, deselect, select all.
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Invert"));
    assert_eq!(s.settle()["document"]["undo"], "invert selection");
    s.click(&sel("None").after("Select").after("Select"));
    assert_eq!(s.settle()["document"]["selection"], false);
    s.key(&[vk::CONTROL], vk::letter('a'));
    assert_eq!(s.settle()["document"]["selection"], true);
    s.key(&[vk::CONTROL], vk::letter('d'));
    // Intersect with Shift+Alt: two rectangles keep their overlap.
    s.click(&sel("Rectangle"));
    s.drag_doc([200.0, 2200.0], [800.0, 2800.0], 8);
    s.front();
    tela_e2e::win::key(vk::SHIFT, true);
    tela_e2e::win::key(vk::ALT, true);
    s.drag_doc([500.0, 2500.0], [1100.0, 2900.0], 8);
    tela_e2e::win::key(vk::ALT, false);
    tela_e2e::win::key(vk::SHIFT, false);
    // Feather it and paint across: soft edge.
    s.type_into(&role("SpinButton").after("Feather"), "20");
    s.click(&sel("Apply").after("Feather"));
    assert_eq!(s.settle()["document"]["undo"], "feather selection");
    s.click(&sel("Brush").role("Button"));
    s.type_into(&role("SpinButton").after("Size"), "800");
    s.drag_doc([300.0, 2650.0], [1000.0, 2650.0], 10);
    assert!(white(probe(&mut s, 300, 2650)), "outside the intersection");
    assert!(black(probe(&mut s, 650, 2650)), "inside it");
    let (edge, _) = s.pixel(500, 2650);
    assert!(edge[0] > 30 && edge[0] < 225, "a feathered edge: {edge:?}");
    s.shot("selection");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn move_flip_scale_rotate_apply_and_cancel() {
    let mut s = Session::start("transform");
    s.maximize();
    // A black square in the middle.
    s.click(&sel("Select").role("Button"));
    s.drag_doc([1800.0, 1300.0], [2200.0, 1700.0], 8);
    fill_layer(&mut s);
    s.key(&[vk::CONTROL], vk::letter('d'));
    let square = s.digest();
    // Move tool: drag it right by 400.
    s.click(&sel("Move").role("Button"));
    s.drag_doc([2000.0, 1500.0], [2400.0, 1500.0], 12);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "transform");
    assert!(
        white(s.pixel(1850, 1500).0) && black(s.pixel(2550, 1500).0),
        "moved"
    );
    s.key(&[vk::CONTROL], vk::letter('z'));
    assert_eq!(s.digest(), square);
    // Transform: width 50 % typed, then Apply.
    s.click(&sel("Transform").role("Button"));
    assert_eq!(s.settle()["transform"], true);
    s.type_into(&role("SpinButton").after("W"), "50");
    s.shot("transform-session");
    s.click(&sel("Apply"));
    let st = s.settle();
    assert_eq!(st["transform"], false);
    assert_eq!(st["document"]["undo"], "transform");
    assert!(white(s.pixel(1850, 1500).0), "narrower");
    assert!(black(s.pixel(2000, 1500).0));
    s.key(&[vk::CONTROL], vk::letter('z'));
    // Flip in a session, then Escape cancels it: nothing changed.
    s.click(&sel("Move").role("Button"));
    s.click(&sel("Horizontal"));
    assert_eq!(s.settle()["transform"], true);
    s.key(&[], vk::ESCAPE);
    assert_eq!(s.settle()["transform"], false);
    assert_eq!(s.digest(), square, "cancel changes nothing");
    // Angle 45 and Enter.
    s.click(&sel("Transform").role("Button"));
    s.type_into(&role("SpinButton").after("Angle"), "45");
    s.key(&[], vk::ENTER);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "transform");
    assert!(black(s.pixel(2000, 1500).0), "the centre stays");
    assert!(black(s.pixel(2000, 1250).0), "a rotated corner reaches up");
    // Another tool during a session asks first.
    s.click(&sel("Transform").role("Button"));
    s.click(&sel("Brush").role("Button"));
    assert_eq!(s.settle()["prompt"], "apply_transform");
    s.click(&sel("Cancel"));
    s.key(&[], vk::ESCAPE);
    s.shot("transform");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn fill_bucket_tolerance_reference_selection_lock_and_mask() {
    let mut s = Session::start("fill");
    s.maximize();
    // A closed black ring on layer 1.
    s.type_into(&role("SpinButton").after("Size"), "40");
    s.drag_path_doc(
        &[
            [1000.0, 1000.0],
            [3000.0, 1000.0],
            [3000.0, 2000.0],
            [1000.0, 2000.0],
            [1000.0, 1000.0],
        ],
        12,
    );
    // On a new layer, the bucket refers to the canvas: it fills inside the
    // ring only.
    s.click(&sel("New").after("Layers"));
    s.type_into(&role("TextInput").after("Hex"), "00AA00");
    s.click(&sel("Fill").role("Button"));
    let p = s.doc_to_client(2000.0, 1500.0);
    s.click_at(p[0], p[1]);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "fill");
    assert_eq!(
        s.pixel(2000, 1500).1.unwrap(),
        [0, 170, 0, 255],
        "inside, on the new layer"
    );
    assert_eq!(s.pixel(500, 500).1.unwrap()[3], 0, "not outside the ring");
    s.key(&[vk::CONTROL], vk::letter('z'));
    // Referring to the layer only, the empty layer fills everywhere.
    s.click(&sel("Layer").after("Refer to"));
    s.click_at(p[0], p[1]);
    assert_eq!(s.pixel(500, 500).1.unwrap(), [0, 170, 0, 255]);
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.click(&sel("Canvas").after("Refer to"));
    // A selection limits it.
    s.click(&sel("Select").role("Button"));
    s.drag_doc([1500.0, 1200.0], [2500.0, 1800.0], 8);
    s.click(&sel("Fill").role("Button"));
    s.click_at(p[0], p[1]);
    assert_eq!(s.pixel(2000, 1500).1.unwrap(), [0, 170, 0, 255]);
    assert_eq!(
        s.pixel(1200, 1500).1.unwrap()[3],
        0,
        "outside the selection"
    );
    s.key(&[vk::CONTROL], vk::letter('d'));
    // Alpha lock: a fill recolours only what exists.
    s.click(&sel("Lock alpha"));
    s.type_into(&role("TextInput").after("Hex"), "0000FF");
    s.click(&sel("Layer").after("Refer to"));
    s.click_at(p[0], p[1]);
    assert_eq!(s.pixel(2000, 1500).1.unwrap(), [0, 0, 255, 255]);
    assert_eq!(
        s.pixel(1200, 1500).1.unwrap()[3],
        0,
        "transparent stays transparent"
    );
    s.click(&sel("Lock alpha"));
    // On a mask, the bucket with the eraser end hides (here: E then fill).
    s.click(&sel("Add mask"));
    s.key(&[], vk::letter('e'));
    s.click(&sel("Fill").role("Button"));
    s.click_at(p[0], p[1]);
    let (shown, _) = s.pixel(2000, 1500);
    assert_ne!(
        shown,
        [0, 0, 255, 255],
        "the mask hides the blue: {shown:?}"
    );
    // Tolerance from the panel.
    s.type_into(&role("SpinButton").after("Tolerance"), "50");
    let st = s.settle();
    assert!(st["document"]["undo"] == "fill" || st["document"]["undo"] == "paint mask");
    s.shot("fill");
}

//! The gradient tool end to end (docs/quality/UI_AUDIT.md, section 5 of
//! the 5F quality gate): the real executable, the mouse and keyboard, the
//! panel, the view; the result compared with the engine applied to the
//! same geometry (the oracle), on the document and on the screen, during
//! the drag (the preview) and after it.

#![cfg(windows)]

mod common;

use std::time::{Duration, Instant};

use common::{Oracle, client_to_doc, diff, grid, press_point, settings};
use tela_e2e::win::{Button, FocusThief};
use tela_e2e::{Session, sel, vk};

/// What one gradient drag showed and left.
struct Drawn {
    /// How long the preview took to cover the canvas after the last move.
    preview_ms: u128,
    /// Grid points whose on-screen preview differed from the oracle.
    preview_wrong: usize,
    /// Grid points whose committed pixel differed from the oracle.
    document_wrong: usize,
    /// Grid points whose screen after the commit differed.
    screen_wrong: usize,
    points: usize,
}

/// Drags a gradient from document point `a` to `b` with the tool's current
/// settings and compares the preview (held before the release), the
/// document and the screen with the oracle on a grid of points.
fn drag_and_compare(s: &mut Session, label: &str, a: [f32; 2], b: [f32; 2]) -> Drawn {
    let state = s.settle();
    let pa = press_point(s, a);
    let pb = press_point(s, b);
    let da = client_to_doc(&state, pa);
    let db = client_to_doc(&state, pb);
    let mut oracle = Oracle::new(&state);
    oracle.draw(&settings(&state), da, db);
    let points = grid(&state, 6);
    let expected: Vec<[u8; 4]> = points
        .iter()
        .map(|p| oracle.composite(p[0], p[1]))
        .collect();

    s.move_to(pa[0], pa[1]);
    s.press(Button::Left);
    s.drag_moves(pa, pb, 24);
    // The preview, with the button still held.
    let moved = Instant::now();
    let deadline = moved + Duration::from_secs(5);
    let mut preview_ms = u128::MAX;
    while Instant::now() < deadline {
        let st = s.state();
        if st["pending_tiles"].as_u64() == Some(0) && st["gradient"]["session"] == true {
            preview_ms = moved.elapsed().as_millis();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(120));
    s.shot(&format!("{label}-preview"));
    let preview_wrong = points
        .iter()
        .zip(&expected)
        .filter(|(p, e)| {
            let (x, y) = (p[0] as f32 + 0.5, p[1] as f32 + 0.5);
            if !s.canvas_visible(x, y) {
                return false;
            }
            let c = s.screen_rgb_doc(x, y);
            diff([c[0], c[1], c[2], 255], **e) > 24
        })
        .count();
    s.release(Button::Left);
    let st = s.settle();
    assert_eq!(st["gradient"]["session"], false, "{label}: the drag ended");
    assert_eq!(
        st["document"]["undo"], "gradient",
        "{label}: one gradient step"
    );
    s.shot(&format!("{label}-committed"));
    let mut document_wrong = 0;
    let mut screen_wrong = 0;
    for (p, e) in points.iter().zip(&expected) {
        let (got, _) = s.pixel(p[0], p[1]);
        if diff(got, *e) > 2 {
            document_wrong += 1;
            s.step(&format!(
                "{label}: document {p:?} is {got:?}, expected {e:?}"
            ));
        }
        let (x, y) = (p[0] as f32 + 0.5, p[1] as f32 + 0.5);
        if !s.canvas_visible(x, y) {
            continue;
        }
        let c = s.screen_rgb_doc(x, y);
        if diff([c[0], c[1], c[2], 255], *e) > 24 {
            screen_wrong += 1;
        }
    }
    let drawn = Drawn {
        preview_ms,
        preview_wrong,
        document_wrong,
        screen_wrong,
        points: points.len(),
    };
    s.step(&format!(
        "{label}: preview complete after {} ms; wrong of {}: preview {}, document {}, screen {}",
        if drawn.preview_ms == u128::MAX {
            "never (5 s)".to_owned()
        } else {
            drawn.preview_ms.to_string()
        },
        drawn.points,
        drawn.preview_wrong,
        drawn.document_wrong,
        drawn.screen_wrong
    ));
    drawn
}

fn gradient_tool(s: &mut Session) {
    s.key(&[], vk::letter('g'));
    let st = s.settle();
    if st["tool"] != "gradient" {
        s.key(&[], vk::letter('g'));
    }
    assert_eq!(s.settle()["tool"], "gradient");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn g_cycles_fill_and_gradient_and_the_panel_shows_the_tool() {
    let mut s = Session::start("gradient-tool-keys");
    s.maximize();
    let tool_button = || sel("Gradient").role("Button");
    let bucket = || sel("Fill").role("Button");
    assert_eq!(s.expect(&tool_button()).toggled, Some(false));
    s.key(&[], vk::letter('g'));
    assert_eq!(
        s.settle()["tool"],
        "gradient",
        "the first G is the gradient"
    );
    assert_eq!(s.expect(&tool_button()).toggled, Some(true));
    s.key(&[], vk::letter('g'));
    assert_eq!(s.settle()["tool"], "fill");
    assert_eq!(s.expect(&bucket()).toggled, Some(true));
    assert_eq!(s.expect(&tool_button()).toggled, Some(false));
    s.key(&[], vk::letter('b'));
    assert_eq!(s.settle()["tool"], "brush");
    s.key(&[], vk::letter('g'));
    assert_eq!(
        s.settle()["tool"],
        "fill",
        "G returns to the last of the two"
    );
    s.click(&tool_button());
    assert_eq!(s.settle()["tool"], "gradient");
    s.shot("gradient-tool");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn every_shape_and_end_draws_what_the_engine_draws_preview_included() {
    let mut s = Session::start("gradient-shapes");
    s.maximize();
    gradient_tool(&mut s);
    let blank = s.digest();
    let cases: &[(&str, &str, [f32; 2], [f32; 2])] = &[
        ("Linear", "Extend", [800.0, 1500.0], [3200.0, 1500.0]),
        ("Linear", "Repeat", [1500.0, 1000.0], [2100.0, 1300.0]),
        ("Linear", "Mirror", [1500.0, 1000.0], [2100.0, 1300.0]),
        ("Linear", "None", [1000.0, 600.0], [3000.0, 2400.0]),
        ("Radial", "Extend", [2000.0, 1500.0], [2900.0, 1900.0]),
        ("Radial", "Repeat", [2000.0, 1500.0], [2500.0, 1500.0]),
        ("Ellipse", "Extend", [2000.0, 1500.0], [3200.0, 2100.0]),
        ("Angular", "Extend", [2000.0, 1500.0], [2800.0, 1100.0]),
        ("Reflected", "Extend", [2000.0, 1500.0], [2600.0, 1500.0]),
    ];
    let mut failures = Vec::new();
    for &(shape, end, a, b) in cases {
        s.click(&sel(shape).after("Shape"));
        s.click(&sel(end).after("Ends"));
        let label = format!("{shape}-{end}").to_lowercase();
        let d = drag_and_compare(&mut s, &label, a, b);
        if d.document_wrong > 0 {
            failures.push(format!(
                "{label}: {} document pixels differ",
                d.document_wrong
            ));
        }
        if d.preview_ms > 1500 {
            failures.push(format!(
                "{label}: preview incomplete after 1.5 s ({} ms)",
                d.preview_ms
            ));
        }
        if d.preview_wrong > d.points / 10 {
            failures.push(format!(
                "{label}: preview wrong at {} points",
                d.preview_wrong
            ));
        }
        if d.screen_wrong > d.points / 10 {
            failures.push(format!(
                "{label}: screen wrong at {} points",
                d.screen_wrong
            ));
        }
        s.key(&[vk::CONTROL], vk::letter('z'));
        s.settle();
        assert_eq!(s.digest(), blank, "{label}: undo restores the blank layer");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn escape_cancels_and_undo_redo_restore_exactly() {
    let mut s = Session::start("gradient-cancel-undo");
    s.maximize();
    gradient_tool(&mut s);
    let blank = s.digest();
    let a = press_point(&s, [1000.0, 1500.0]);
    let b = press_point(&s, [3000.0, 1500.0]);
    s.move_to(a[0], a[1]);
    s.press(Button::Left);
    s.drag_moves(a, b, 16);
    std::thread::sleep(Duration::from_millis(300));
    s.shot("before-escape");
    s.key(&[], vk::ESCAPE);
    s.release(Button::Left);
    let st = s.settle();
    s.shot("after-escape");
    assert_eq!(st["gradient"]["session"], false);
    assert_eq!(st["document"]["undo"], serde_json::Value::Null, "no step");
    assert_eq!(s.digest(), blank, "nothing drawn");
    // The screen shows the white paper again where the preview was.
    let c = s.screen_rgb_doc(1100.0, 1500.0);
    assert_eq!(c, [255, 255, 255], "the preview is gone from the screen");
    s.drag(a, b, 16);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "gradient");
    let drawn = s.digest();
    assert_ne!(drawn, blank);
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.settle();
    assert_eq!(s.digest(), blank, "undo");
    s.key(&[vk::CONTROL], vk::letter('y'));
    s.settle();
    assert_eq!(s.digest(), drawn, "redo");
    // Repeating: a second gradient over the first is a second step.
    s.drag(b, a, 16);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "gradient");
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.settle();
    assert_eq!(s.digest(), drawn, "undo of the second only");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn losing_the_focus_during_a_drag_leaves_no_ghost_preview() {
    let mut s = Session::start("gradient-focus-loss");
    s.maximize();
    gradient_tool(&mut s);
    let blank = s.digest();
    let a = press_point(&s, [1000.0, 1500.0]);
    let b = press_point(&s, [3000.0, 1500.0]);
    s.move_to(a[0], a[1]);
    s.press(Button::Left);
    s.drag_moves(a, b, 16);
    std::thread::sleep(Duration::from_millis(200));
    {
        // Another window takes the focus mid-drag (a notification, a
        // dialog, Alt+Tab).
        let _thief = FocusThief::new();
        std::thread::sleep(Duration::from_millis(300));
        tela_e2e::win::button(Button::Left, false);
        std::thread::sleep(Duration::from_millis(200));
    }
    s.front();
    let st = s.settle();
    s.shot("after-focus-loss");
    assert_eq!(st["drag"], "none");
    assert_eq!(
        st["gradient"]["session"], false,
        "the gradient session ends with the drag"
    );
    assert_eq!(s.digest(), blank);
    let c = s.screen_rgb_doc(1100.0, 1500.0);
    assert_eq!(c, [255, 255, 255], "no preview is left on the screen");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn stops_edited_in_the_panel_reach_the_drawing() {
    let mut s = Session::start("gradient-stops");
    s.maximize();
    gradient_tool(&mut s);
    s.click(&sel("Add stop"));
    let st = s.settle();
    let stops = st["gradient"]["stops"].as_array().unwrap().clone();
    assert_eq!(stops.len(), 3, "a stop was added");
    assert_eq!(st["gradient"]["selected"], 1);
    // The hex field: type red and apply it to the stop.
    let hex = s.expect(&tela_e2e::role("TextInput").after("Hex"));
    let c = hex.center();
    s.click_at(c[0], c[1]);
    s.key(&[vk::CONTROL], vk::letter('a'));
    s.type_text("FF0000");
    s.key(&[], vk::ENTER);
    let st = s.settle();
    assert_eq!(
        st["colors"]["main"], "#FF0000",
        "the hex field set the colour"
    );
    s.click(&sel("Fixed").after("Stop 2 of 3"));
    let st = s.settle();
    assert_eq!(st["gradient"]["stops"][1]["color"], "#FF0000");
    // Keyboard shortcuts still work after typing in the field.
    s.key(&[], vk::letter('d'));
    let st = s.settle();
    assert_eq!(
        st["colors"]["main"], "#000000",
        "D resets the colours after the hex field was used"
    );
    let d = drag_and_compare(&mut s, "three-stops", [800.0, 1500.0], [3200.0, 1500.0]);
    assert_eq!(d.document_wrong, 0);
    let (mid, _) = s.pixel(2000, 1500);
    assert!(mid[0] > 200 && mid[1] < 60, "red in the middle: {mid:?}");
    s.click(&sel("Reverse"));
    let st = s.settle();
    assert_eq!(st["gradient"]["stops"][0]["color"], "sub");
    s.click(&sel("Remove"));
    let st = s.settle();
    assert_eq!(st["gradient"]["stops"].as_array().unwrap().len(), 2);
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn zoomed_rotated_and_panned_views_map_the_drag() {
    let mut s = Session::start("gradient-views");
    s.maximize();
    gradient_tool(&mut s);
    // Home fits the document; then zoom, rotate and pan around its centre.
    s.key(&[], vk::HOME);
    s.settle();
    let centre = s.doc_to_client(2000.0, 1500.0);
    s.wheel(centre, 3);
    // Rotate 30° (Shift + wheel: 15° per wheel event).
    s.front();
    tela_e2e::win::key(vk::SHIFT, true);
    std::thread::sleep(Duration::from_millis(80));
    for _ in 0..2 {
        s.wheel(centre, 1);
        std::thread::sleep(Duration::from_millis(80));
    }
    tela_e2e::win::key(vk::SHIFT, false);
    // Pan with the space bar held.
    let before = s.settle()["view"]["to_screen"].clone();
    tela_e2e::win::key(vk::SPACE, true);
    std::thread::sleep(Duration::from_millis(80));
    s.drag(
        [centre[0], centre[1]],
        [centre[0] - 150.0, centre[1] + 80.0],
        10,
    );
    tela_e2e::win::key(vk::SPACE, false);
    let st = s.settle();
    assert!(st["view"]["rotation"].as_f64().unwrap().abs() > 0.4);
    assert_ne!(st["view"]["to_screen"], before, "the space drag panned");
    assert_eq!(
        st["document"]["undo"],
        serde_json::Value::Null,
        "and drew nothing"
    );
    s.click(&sel("Radial").after("Shape"));
    let d = drag_and_compare(&mut s, "rotated-radial", [1900.0, 1450.0], [2100.0, 1500.0]);
    assert_eq!(
        d.document_wrong, 0,
        "the drag maps through the rotated view"
    );
    s.click(&sel("Linear").after("Shape"));
    let d = drag_and_compare(&mut s, "rotated-linear", [1800.0, 1400.0], [2200.0, 1600.0]);
    assert_eq!(d.document_wrong, 0);
    assert_eq!(d.screen_wrong, 0, "the screen agrees with the document");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn sixteen_bit_documents_draw_the_same_gradient() {
    let mut s = Session::start("gradient-16bit");
    s.maximize();
    s.click(&sel("New").role("Button"));
    s.click(&sel("New 16-bit document"));
    let st = s.settle();
    assert_eq!(st["document"]["bits"], 16);
    gradient_tool(&mut s);
    let d = drag_and_compare(&mut s, "16bit-linear", [800.0, 1500.0], [3200.0, 1500.0]);
    assert_eq!(d.document_wrong, 0);
    s.click(&sel("Radial").after("Shape"));
    let d = drag_and_compare(&mut s, "16bit-radial", [2000.0, 1500.0], [2600.0, 1500.0]);
    assert_eq!(d.document_wrong, 0);
}

/// Regression (Q-05 in docs/quality/UI_AUDIT.md): after the eraser (E or
/// the strip), the gradient and the bucket used to erase without saying
/// so — a gradient "did nothing" on an empty layer and wiped a drawn one.
/// Now each has its own visible Draw / Erase mode, Draw by default.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn the_eraser_does_not_turn_gradients_and_fills_into_erasers() {
    let mut s = Session::start("gradient-after-eraser");
    s.maximize();
    s.click(&sel("Brush").role("Button"));
    s.drag_doc([1000.0, 1000.0], [3000.0, 1000.0], 20);
    let drawn = s.digest();
    // The eraser, then the gradient by its key (as a user would).
    s.key(&[], vk::letter('e'));
    assert_eq!(s.settle()["tool"], "eraser");
    gradient_tool(&mut s);
    let st = s.settle();
    assert_eq!(
        st["gradient"]["erase"], false,
        "the gradient draws by default"
    );
    assert_eq!(s.expect(&sel("Draw").after("Mode")).toggled, Some(true));
    let a = s.doc_to_client(500.0, 2000.0);
    let b = s.doc_to_client(3500.0, 2000.0);
    s.drag(a, b, 16);
    s.settle();
    let (p, _) = s.pixel(2000, 1000);
    assert_eq!(p[3], 255, "the drawing is still there");
    assert_ne!(s.digest(), drawn, "the gradient drew");
    s.key(&[vk::CONTROL], vk::letter('z'));
    // Erase mode is a visible choice and does erase.
    s.click(&sel("Erase").after("Mode"));
    assert_eq!(s.settle()["gradient"]["erase"], true);
    assert!(
        s.widget(&sel("Erasing: the gradient removes paint"))
            .is_ok(),
        "a warning shows"
    );
    s.shot("erase-mode");
    s.drag(a, b, 16);
    let (_, layer) = s.pixel(2000, 1000);
    assert_eq!(layer.unwrap()[3], 0, "Erase mode erased");
    s.key(&[vk::CONTROL], vk::letter('z'));
    // The bucket after the eraser fills, too.
    s.key(&[], vk::letter('e'));
    s.click(&sel("Fill").role("Button"));
    assert_eq!(s.settle()["fill"]["erase"], false);
    let c = s.doc_to_client(2000.0, 2500.0);
    s.click_at(c[0], c[1]);
    assert_eq!(s.settle()["document"]["undo"], "fill");
    assert_eq!(
        s.pixel(2000, 1000).0[3],
        255,
        "the bucket filled, it did not erase"
    );
}

/// Section 5 of the quality gate: the gradient with a selection (hard and
/// feathered), alpha lock, a mask, a translucent Multiply layer and a
/// clipped layer, through the interface.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn gradients_with_selection_lock_mask_opacity_blend_and_clipping() {
    let mut s = Session::start("gradient-document-rules");
    s.maximize();
    s.key(&[], vk::HOME);
    gradient_tool(&mut s);
    // A hard rectangle selection: only inside changes, with the values of
    // the unselected gradient (the geometry comes from the drag).
    s.click(&sel("Select").role("Button"));
    s.drag_doc([1500.0, 1000.0], [2500.0, 2000.0], 8);
    s.click(&sel("Gradient").role("Button"));
    let state = s.settle();
    let pa = press_point(&s, [800.0, 1500.0]);
    let pb = press_point(&s, [3200.0, 1500.0]);
    let mut oracle = Oracle::new(&state);
    oracle.draw(
        &settings(&state),
        client_to_doc(&state, pa),
        client_to_doc(&state, pb),
    );
    s.drag(pa, pb, 16);
    s.settle();
    for x in [1600, 2000, 2400] {
        let (_, l) = s.pixel(x, 1500);
        assert!(
            diff(l.unwrap(), oracle.layer(x, 1500)) <= 2,
            "inside at {x}"
        );
    }
    for (x, y) in [(1000, 1500), (3000, 1500), (2000, 500)] {
        assert_eq!(s.pixel(x, y).1.unwrap()[3], 0, "outside at ({x}, {y})");
    }
    s.key(&[vk::CONTROL], vk::letter('z'));
    // Feathered: the alpha falls off across the border.
    s.click(&sel("Select").role("Button"));
    s.type_into(&tela_e2e::role("SpinButton").after("Feather"), "40");
    s.click(&sel("Apply").after("Feather"));
    s.click(&sel("Gradient").role("Button"));
    s.drag(pa, pb, 16);
    let edge = s.pixel(1500, 1500).1.unwrap()[3];
    assert!(edge > 40 && edge < 220, "feathered edge alpha {edge}");
    s.key(&[vk::CONTROL], vk::letter('z'));
    s.key(&[vk::CONTROL], vk::letter('d'));
    // Alpha lock over a disc: only the disc is recoloured.
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Ellipse"));
    s.drag_doc([1700.0, 1200.0], [2300.0, 1800.0], 8);
    s.key(&[vk::ALT], vk::DELETE);
    s.key(&[vk::CONTROL], vk::letter('d'));
    s.click(&sel("Lock alpha"));
    s.click(&sel("Gradient").role("Button"));
    s.type_into(&tela_e2e::role("TextInput").after("Hex"), "FF0000");
    s.drag(pa, pb, 16);
    let (_, inside) = s.pixel(1800, 1500);
    assert_eq!(inside.unwrap()[3], 255);
    assert!(inside.unwrap()[0] > 100, "recoloured: {inside:?}");
    assert_eq!(
        s.pixel(1000, 1500).1.unwrap()[3],
        0,
        "transparent stays transparent"
    );
    s.click(&sel("Lock alpha"));
    // A mask in Erase mode: the mask hides by the gradient's alpha, the
    // pixels stay as they were.
    let before = s.digest();
    s.click(&sel("Add mask"));
    s.click(&sel("Erase").after("Mode"));
    s.drag(pa, pb, 16);
    let after = s.digest();
    assert_eq!(
        before["layers"][0]["pixels"], after["layers"][0]["pixels"],
        "the pixels did not change"
    );
    assert_ne!(
        after["layers"][0]["mask"], before["layers"][0]["mask"],
        "the mask did"
    );
    let (shown, _) = s.pixel(2200, 1500);
    assert!(shown[1] > 200, "the disc is hidden there: {shown:?}");
    s.click(&sel("Draw").after("Mode"));
    s.click(&sel("Pixels").after("Edit"));
    // A Multiply layer at 50 %: the stored pixels are the plain gradient.
    s.click(&sel("New").after("Layers"));
    s.choose(&tela_e2e::role("ComboBox").after("Blend"), "Multiply");
    s.type_into(
        &tela_e2e::role("SpinButton")
            .after("Layers")
            .after("Opacity"),
        "50",
    );
    s.click(&sel("Gradient").role("Button"));
    let state = s.settle();
    let mut oracle = Oracle::new(&state);
    oracle.draw(
        &settings(&state),
        client_to_doc(&state, pa),
        client_to_doc(&state, pb),
    );
    s.drag(pa, pb, 16);
    for x in [900, 2000, 3100] {
        let (_, l) = s.pixel(x, 2500);
        assert!(
            diff(l.unwrap(), oracle.layer(x, 2500)) <= 2,
            "not baked at {x}"
        );
    }
    // Clipped: the layer holds the gradient everywhere, the picture shows
    // it only over the layer below.
    s.click(&sel("Clip"));
    let (shown, layer) = s.pixel(500, 500);
    assert_eq!(layer.unwrap()[3], 255, "stored everywhere");
    assert_eq!(shown, [255, 255, 255, 255], "shown only where the base is");
    s.shot("rules");
}

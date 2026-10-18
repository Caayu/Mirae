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

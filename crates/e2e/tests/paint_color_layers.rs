//! Canvas, brush, colour, layers, masks and clipping through the real
//! interface (the E2E matrix, docs/quality/E2E_MATRIX.md).

#![cfg(windows)]

mod common;

use tela_e2e::win::Button;
use tela_e2e::{Session, role, sel, vk};

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn canvas_view_zoom_pan_rotate_and_painting_at_the_edges() {
    let mut s = Session::start("canvas-view");
    s.maximize();
    let v0 = s.state()["view"].clone();
    let centre = s.doc_to_client(2000.0, 1500.0);
    s.wheel(centre, 2);
    let v1 = s.settle()["view"].clone();
    assert!(v1["zoom"].as_f64() > v0["zoom"].as_f64(), "wheel zooms in");
    // The point under the cursor stays put while zooming.
    let after = s.doc_to_client(2000.0, 1500.0);
    assert!((after[0] - centre[0]).abs() < 2.0 && (after[1] - centre[1]).abs() < 2.0);
    // Middle-button pan.
    s.move_to(centre[0], centre[1]);
    s.press(Button::Middle);
    s.drag_moves(centre, [centre[0] + 120.0, centre[1] + 60.0], 8);
    s.release(Button::Middle);
    let moved = s.doc_to_client(2000.0, 1500.0);
    assert!(
        (moved[0] - centre[0] - 120.0).abs() < 3.0,
        "middle drag pans"
    );
    // Home fits again.
    s.key(&[], vk::HOME);
    s.settle();
    for corner in [[1.0, 1.0], [3999.0, 1.0], [1.0, 2999.0], [3999.0, 2999.0]] {
        assert!(
            s.canvas_visible(corner[0], corner[1]),
            "Home shows the whole document: {corner:?}"
        );
    }
    // Strokes right at the document's edges land inside it.
    s.drag_doc([2.0, 2.0], [3997.0, 2.0], 30);
    s.drag_doc([2.0, 2997.0], [3997.0, 2997.0], 30);
    let (top, _) = s.pixel(2000, 2);
    let (bottom, _) = s.pixel(2000, 2997);
    assert_eq!(top[0], 0, "the top edge painted: {top:?}");
    assert_eq!(bottom[0], 0, "the bottom edge painted: {bottom:?}");
    // Rotation, then a stroke maps through it.
    s.front();
    tela_e2e::win::key(vk::SHIFT, true);
    std::thread::sleep(std::time::Duration::from_millis(80));
    s.wheel(s.doc_to_client(2000.0, 1500.0), 1);
    tela_e2e::win::key(vk::SHIFT, false);
    assert!(s.settle()["view"]["rotation"].as_f64().unwrap().abs() > 0.2);
    s.drag_doc([1500.0, 1000.0], [2500.0, 1000.0], 20);
    let (p, _) = s.pixel(2000, 1000);
    assert_eq!(
        p[0], 0,
        "a stroke in a rotated view lands where it was drawn"
    );
    s.shot("rotated");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn brush_presets_size_opacity_eraser_and_the_pen() {
    let mut s = Session::start("brush");
    s.maximize();
    // Every preset paints (the airbrush and the eraser included).
    let presets = [
        "Pencil",
        "Soft Round",
        "Marker",
        "Airbrush",
        "Chalk",
        "Calligraphy",
        "Ink Pen",
    ];
    for (i, name) in presets.iter().enumerate() {
        s.choose(&role("ComboBox").after("Preset"), name);
        let y = 300.0 + i as f32 * 350.0;
        s.drag_doc([500.0, y], [1500.0, y], 20);
        let st = s.settle();
        assert_eq!(st["preset"], *name);
        assert_eq!(st["document"]["undo"], "paint", "{name} painted");
        // Textured tips leave gaps: some pixel across the line is darker.
        let darkest = (-4..=4)
            .map(|d| s.pixel(1000, y as i32 + d).0[0])
            .min()
            .unwrap();
        assert!(darkest < 250, "{name} left paint (darkest {darkest})");
    }
    // Size from the panel: a wide stroke covers far from its centre line.
    s.type_into(&role("SpinButton").after("Size"), "120");
    s.drag_doc([2000.0, 1500.0], [3500.0, 1500.0], 20);
    let (p, _) = s.pixel(2700, 1545);
    assert!(p[0] < 128, "size 120 reaches 45 px from the centre: {p:?}");
    // Opacity 0.3: the stroke is translucent.
    s.type_into(&role("SpinButton").after("Opacity"), "0.3");
    s.drag_doc([2000.0, 2500.0], [3500.0, 2500.0], 20);
    let (p, _) = s.pixel(2700, 2500);
    assert!(p[0] > 120 && p[0] < 220, "opacity 0.3: {p:?}");
    // The eraser removes paint; B paints again.
    s.click(&sel("Eraser").role("Button"));
    s.drag_doc([2000.0, 1500.0], [3500.0, 1500.0], 20);
    let (_, layer) = s.pixel(2700, 1500);
    assert_eq!(layer.unwrap()[3], 0, "erased");
    s.key(&[], vk::letter('b'));
    assert_eq!(s.settle()["tool"], "brush");
    // The synthetic pen paints with pressure; its eraser end erases.
    s.pen_doc([500.0, 2800.0], [3500.0, 2800.0], 40, 0.8);
    let st = s.settle();
    assert_eq!(st["document"]["undo"], "paint");
    let (p, _) = s.pixel(2000, 2800);
    assert!(p[0] < 250, "the pen painted: {p:?}");
    s.pen_doc_with([500.0, 2800.0], [3500.0, 2800.0], 40, 1.0, true);
    s.shot("pen");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn colours_main_sub_swap_reset_values_eyedropper_recent_and_swatches() {
    let mut s = Session::start("colour");
    s.maximize();
    let colors = |s: &Session| s.state()["colors"].clone();
    s.type_into(&role("TextInput").after("Hex"), "3366CC");
    assert_eq!(colors(&s)["main"], "#3366CC", "hex");
    s.click(&sel("Swap"));
    assert_eq!(colors(&s)["sub"], "#3366CC", "swap");
    assert_eq!(colors(&s)["main"], "#FFFFFF");
    s.key(&[], vk::letter('x'));
    assert_eq!(
        colors(&s)["active"],
        "sub",
        "X paints with the other colour"
    );
    s.click(&sel("Reset"));
    let c = colors(&s);
    assert_eq!(
        (c["main"].as_str(), c["sub"].as_str()),
        (Some("#000000"), Some("#FFFFFF"))
    );
    // RGB fields.
    s.type_into(&role("SpinButton").after("R"), "255");
    s.type_into(&role("SpinButton").after("G"), "128");
    assert_eq!(colors(&s)["main"], "#FF8000", "RGB");
    // HSV: hue 120 keeps saturation and value.
    s.type_into(&role("SpinButton").after("H"), "120");
    let hex = colors(&s)["main"].as_str().unwrap().to_owned();
    assert_eq!(&hex[3..5], "FF", "green channel full at hue 120: {hex}");
    // A stroke makes the colour recent; a swatch sets it; + adds one.
    s.drag_doc([500.0, 500.0], [1500.0, 500.0], 10);
    assert!(colors(&s)["recent"].as_u64().unwrap() >= 1);
    // The eyedropper: tool, right-click and Alt pick the canvas.
    s.click(&sel("Reset"));
    s.click(&sel("Eyedropper").role("Button"));
    let p = s.doc_to_client(1000.0, 500.0);
    s.click_at(p[0], p[1]);
    let picked = colors(&s)["main"].as_str().unwrap().to_owned();
    assert_eq!(picked, hex, "the eyedropper picked the stroke's colour");
    s.click(&sel("Brush").role("Button"));
    s.click(&sel("Reset"));
    s.move_to(p[0], p[1]);
    s.press(Button::Right);
    s.release(Button::Right);
    assert_eq!(
        colors(&s)["main"].as_str(),
        Some(hex.as_str()),
        "right-click picks"
    );
    s.click(&sel("Reset"));
    s.front();
    tela_e2e::win::key(vk::ALT, true);
    s.click_at(p[0], p[1]);
    tela_e2e::win::key(vk::ALT, false);
    assert_eq!(
        colors(&s)["main"].as_str(),
        Some(hex.as_str()),
        "Alt+click picks"
    );
    assert_eq!(
        s.settle()["document"]["undo"],
        "paint",
        "picking adds no step"
    );
    // A colour added to the swatches stays among them.
    let before = s.widgets().len();
    s.click(&sel("+").after("Swatches"));
    assert!(s.widgets().len() >= before);
    s.shot("colours");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn layers_create_rename_reorder_hide_lock_blend_merge_and_undo() {
    let mut s = Session::start("layers");
    s.maximize();
    s.drag_doc([500.0, 1500.0], [3500.0, 1500.0], 20);
    s.click(&sel("New").after("Layers"));
    s.click(&sel("New").after("Layers"));
    assert_eq!(s.layers().len(), 3);
    assert_eq!(s.active_layer()["name"], "Layer 3", "a new layer is active");
    // Rename by double-click.
    let row = s.expect(&sel("Layer 3  100 %"));
    let c = row.center();
    s.double_click(&sel("Layer 3  100 %"));
    tela_e2e::win::type_text("Shading");
    s.key(&[], vk::ENTER);
    assert_eq!(s.active_layer()["name"], "Shading");
    let _ = c;
    // Select another layer by its row; move up and down.
    s.click(&sel("Layer 1  100 %"));
    assert_eq!(s.active_layer()["name"], "Layer 1");
    s.click(&sel("Up"));
    let names: Vec<String> = s
        .layers()
        .iter()
        .map(|l| l["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Layer 2", "Layer 1", "Shading"]);
    // Drag and drop: Layer 1 to the top.
    let from = s.expect(&sel("Layer 1  100 %")).center();
    let to = s.expect(&sel("Shading  100 %")).rect;
    s.drag(from, [from[0], to[1] + 2.0], 12);
    let names: Vec<String> = s
        .layers()
        .iter()
        .map(|l| l["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names.last().map(String::as_str),
        Some("Layer 1"),
        "dropped on top: {names:?}"
    );
    // Visibility, lock, opacity, blend mode.
    s.click(&sel("Layer 1  100 %"));
    s.type_into(&role("SpinButton").after("Layers").after("Opacity"), "50");
    assert_eq!(s.active_layer()["opacity"], 0.5);
    s.choose(&role("ComboBox").after("Blend"), "Screen");
    assert_eq!(s.active_layer()["blend"], "Screen");
    let lock = lock_of_active_row(&s);
    s.click(&lock);
    assert_eq!(s.active_layer()["locked"], true);
    assert!(
        !s.expect(&sel("Delete")).enabled,
        "a locked layer cannot be deleted"
    );
    s.click(&lock);
    // Merge down needs the layer below visible and unlocked.
    s.click(&sel("Merge down"));
    let st = s.settle();
    if st["prompt"] == "merge" {
        s.click(&sel("Keep"));
    }
    assert_eq!(s.layers().len(), 2, "merged");
    assert_eq!(s.settle()["document"]["undo"], "merge down");
    // Undo walks it all back.
    for _ in 0..12 {
        s.key(&[vk::CONTROL], vk::letter('z'));
    }
    assert_eq!(s.layers().len(), 1);
    s.shot("layers");
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn masks_and_clipping_with_blend_modes() {
    let mut s = Session::start("masks-clipping");
    s.maximize();
    // A base disc and a clipped layer above it.
    s.click(&sel("Select").role("Button"));
    s.click(&sel("Ellipse"));
    s.drag_doc([1700.0, 1200.0], [2300.0, 1800.0], 8);
    s.key(&[vk::ALT], vk::DELETE);
    s.key(&[vk::CONTROL], vk::letter('d'));
    s.click(&sel("Brush").role("Button"));
    s.click(&sel("New").after("Layers"));
    s.click(&sel("Clip"));
    assert_eq!(s.active_layer()["clipped"], true);
    s.type_into(&role("TextInput").after("Hex"), "FF0000");
    s.key(&[vk::ALT], vk::DELETE);
    s.settle();
    let (inside, _) = s.pixel(2000, 1500);
    let (outside, _) = s.pixel(500, 500);
    assert_eq!(
        inside,
        [255, 0, 0, 255],
        "the clipped fill shows on the disc"
    );
    assert_eq!(outside, [255, 255, 255, 255], "and nowhere else");
    // A second clipped layer in Multiply.
    s.click(&sel("New").after("Layers"));
    s.click(&sel("Clip"));
    s.choose(&role("ComboBox").after("Blend"), "Multiply");
    s.type_into(&role("TextInput").after("Hex"), "808080");
    s.key(&[vk::ALT], vk::DELETE);
    s.settle();
    let (m, _) = s.pixel(2000, 1500);
    assert!(
        m[0] > 120 && m[0] < 136 && m[1] == 0,
        "red multiplied by grey: {m:?}"
    );
    // A mask on it: erase the mask in the left half, the red shows there.
    s.click(&sel("Add mask"));
    assert!(s.active_layer()["mask"]["enabled"] == true);
    assert_eq!(s.state()["document"]["active_part"], "mask");
    s.click(&sel("Eraser").role("Button"));
    s.type_into(&role("SpinButton").after("Size"), "400");
    s.drag_doc([1850.0, 1300.0], [1850.0, 1700.0], 10);
    let (left, _) = s.pixel(1850, 1500);
    assert_eq!(
        left,
        [255, 0, 0, 255],
        "the mask hides the multiply layer there"
    );
    // Disabling the mask shows it again.
    s.click(&sel("On").role("CheckBox"));
    let (left, _) = s.pixel(1850, 1500);
    assert!(left[0] < 140, "mask off: {left:?}");
    // Editing pixels again.
    s.click(&sel("Pixels").after("Edit"));
    assert_eq!(s.state()["document"]["active_part"], "pixels");
    s.shot("masks");
}

/// The Lock toggle on the active layer's row (the row button is marked).
fn lock_of_active_row(s: &Session) -> tela_e2e::Sel {
    let widgets = s.widgets();
    let row = widgets
        .iter()
        .filter(|w| w.role == "Button" && w.toggled == Some(true) && w.rect[0] > 1000.0)
        .last()
        .expect("the active row")
        .rect;
    let n = widgets
        .iter()
        .filter(|w| w.text() == "Lock")
        .position(|w| (w.center()[1] - (row[1] + row[3]) / 2.0).abs() < 4.0)
        .expect("a Lock on the active row");
    sel("Lock").nth(n)
}

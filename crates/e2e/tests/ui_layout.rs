//! The interface gate (docs/quality/QUALITY_GATE.md, "Interface gate"): at
//! reference window sizes every essential control can be reached inside
//! the window (scrolling its panel if need be), interactive widgets do not
//! overlap, the layout does not move under the cursor when the app's state
//! changes, and the active tool is visibly marked.

#![cfg(windows)]

use tela_e2e::{Sel, Session, Widget, role, sel, vk};

/// The tool strip's buttons, in order.
const TOOLS: [&str; 8] = [
    "Brush",
    "Eraser",
    "Eyedropper",
    "Select",
    "Move",
    "Fill",
    "Gradient",
    "Transform",
];

/// Controls a user needs whatever the tool, by what they say.
fn always() -> Vec<(&'static str, Sel)> {
    let mut v = vec![
        ("new document", sel("New").role("Button")),
        ("open", sel("Open…")),
        ("save", sel("Save")),
        ("save as", sel("Save As…")),
        ("import", sel("Import PNG…")),
        ("export", sel("Export PNG…")),
        ("undo", sel("Undo")),
        ("redo", sel("Redo")),
        ("depth conversion", sel("To 16-bit")),
        ("new layer", sel("New").after("Layers")),
        ("delete layer", sel("Delete")),
        ("merge down", sel("Merge down")),
        ("clear layer", sel("Clear")),
        ("blend mode", role("ComboBox").after("Blend")),
        ("layer opacity", role("Slider").after("Opacity")),
        ("add mask", sel("Add mask")),
        ("clip", sel("Clip")),
        ("lock alpha", sel("Lock alpha")),
        ("swap colours", sel("Swap")),
        ("hex", role("TextInput").after("Hex")),
    ];
    for t in TOOLS {
        v.push((t, sel(t).role("Button")));
    }
    v
}

/// Each tool's own options (shown when the tool is active).
fn per_tool() -> Vec<(&'static str, Vec<Sel>)> {
    vec![
        (
            "Brush",
            vec![role("ComboBox").after("Preset"), role("Slider").after("Size")],
        ),
        ("Eraser", vec![role("Slider").after("Size")]),
        ("Select", vec![sel("Rectangle"), sel("Lasso"), sel("All")]),
        ("Fill", vec![sel("Fill layer"), role("Slider").after("Tolerance")]),
        (
            "Gradient",
            vec![sel("Radial"), sel("Add stop"), sel("Reverse"), sel("Main to clear")],
        ),
        ("Move", vec![sel("Horizontal"), sel("Start transform")]),
    ]
}

const INTERACTIVE: &[&str] = &[
    "Button",
    "CheckBox",
    "RadioButton",
    "ComboBox",
    "Slider",
    "SpinButton",
    "TextInput",
    "Label",
];

/// Pairs of interactive widgets inside the window that overlap.
fn overlaps(widgets: &[Widget], size: [i32; 2]) -> Vec<String> {
    let list: Vec<&Widget> = widgets
        .iter()
        .filter(|w| INTERACTIVE.contains(&w.role.as_str()) && w.inside(size))
        .collect();
    let mut out = Vec::new();
    for (i, a) in list.iter().enumerate() {
        for b in &list[i + 1..] {
            let w = a.visible[2].min(b.visible[2]) - a.visible[0].max(b.visible[0]);
            let h = a.visible[3].min(b.visible[3]) - a.visible[1].max(b.visible[1]);
            if w > 2.0 && h > 2.0 {
                out.push(format!(
                    "{} {:?} overlaps {} {:?}",
                    a.role,
                    a.text(),
                    b.role,
                    b.text()
                ));
            }
        }
    }
    out
}

/// Checks one window size; returns the problems found.
fn check(s: &mut Session, label: &str) -> Vec<String> {
    let size = s.client_size();
    let mut problems = Vec::new();
    s.shot(&format!("{label}-{}x{}", size[0], size[1]));
    for (name, selector) in always() {
        if let Err(e) = s.reveal(&selector) {
            problems.push(format!("{label}: {name}: {e}"));
        }
    }
    for o in overlaps(&s.widgets(), size) {
        problems.push(format!("{label}: {o}"));
    }
    for (tool, options) in per_tool() {
        s.click(&sel(tool).role("Button"));
        for o in options {
            if let Err(e) = s.reveal(&o) {
                problems.push(format!("{label}: {tool} options: {e}"));
            }
        }
        // Looking through the interface never changes the document.
        let st = s.settle();
        if st["document"]["undo"] != serde_json::Value::Null {
            problems.push(format!(
                "{label}: after the {tool} options the document has a step {}",
                st["document"]["undo"]
            ));
        }
        for o in overlaps(&s.widgets(), s.client_size()) {
            problems.push(format!("{label}: {tool} options: {o}"));
        }
    }
    s.click(&sel("Brush").role("Button"));
    problems
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn essential_controls_are_reachable_at_reference_sizes() {
    let mut s = Session::start("ui-layout-sizes");
    let mut problems = check(&mut s, "default");
    s.place(0, 0, 1920, 1080);
    problems.extend(check(&mut s, "1920x1080"));
    s.place(100, 100, 1024, 700);
    problems.extend(check(&mut s, "small"));
    s.place(100, 100, 800, 600);
    problems.extend(check(&mut s, "smallest"));
    s.maximize();
    problems.extend(check(&mut s, "maximized"));
    for p in &problems {
        s.step(&format!("PROBLEM {p}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn the_layout_does_not_move_when_the_state_changes() {
    let mut s = Session::start("ui-layout-stability");
    s.maximize();
    let probe = || {
        [
            sel("Undo"),
            sel("Swap"),
            role("Slider").after("Size"),
            sel("New").after("Layers"),
            sel("Gradient").role("Button"),
        ]
    };
    let before: Vec<[f32; 4]> = probe().iter().map(|p| s.expect(p).rect).collect();
    // A stroke adds a recent colour and an undo step; a second layer adds
    // a row to the list.
    s.drag_doc([1000.0, 1500.0], [3000.0, 1500.0], 20);
    s.key(&[vk::CONTROL, vk::SHIFT], vk::letter('n'));
    s.drag_doc([1000.0, 1000.0], [3000.0, 1000.0], 20);
    s.settle();
    let after: Vec<[f32; 4]> = probe().iter().map(|p| s.expect(p).rect).collect();
    s.shot("after-changes");
    let moved: Vec<String> = probe()
        .iter()
        .zip(before.iter().zip(&after))
        .filter(|(_, (a, b))| (a[1] - b[1]).abs() > 0.5 || (a[0] - b[0]).abs() > 0.5)
        .map(|(p, (a, b))| format!("{p:?} moved from {a:?} to {b:?}"))
        .collect();
    assert!(moved.is_empty(), "{}", moved.join("\n"));
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn the_active_tool_is_marked_in_the_interface() {
    let mut s = Session::start("ui-active-tool");
    s.maximize();
    // Each tool by its key; then exactly one strip button reads as
    // selected, and it is the right one, and the options show its name.
    let keys: &[(&[u16], u16, &str, &str)] = &[
        (&[], vk::letter('b'), "brush", "Brush"),
        (&[], vk::letter('m'), "selection", "Select"),
        (&[], vk::letter('g'), "gradient", "Gradient"),
        (&[], vk::letter('g'), "fill", "Fill"),
        (&[], vk::letter('k'), "move", "Move"),
        (&[], vk::letter('e'), "eraser", "Eraser"),
        (&[], vk::letter('b'), "brush", "Brush"),
    ];
    let mut problems = Vec::new();
    for (mods, key, tool, button) in keys {
        s.key(mods, *key);
        let st = s.settle();
        if st["tool"] != *tool {
            problems.push(format!("the key for {tool} gave {}", st["tool"]));
            continue;
        }
        let widgets = s.widgets();
        let marked: Vec<&str> = TOOLS
            .iter()
            .copied()
            .filter(|t| {
                sel(t)
                    .role("Button")
                    .find(&widgets)
                    .is_some_and(|w| w.toggled == Some(true))
            })
            .collect();
        if marked != [*button] {
            problems.push(format!("{tool}: the strip marks {marked:?}"));
        }
        if sel(button).role("Label").find(&widgets).is_none() {
            problems.push(format!("{tool}: the options are not headed {button:?}"));
        }
        s.shot(&format!("tool-{tool}"));
    }
    // The strip's own buttons choose the tool too.
    for t in TOOLS.iter().filter(|t| **t != "Transform") {
        s.click(&sel(t).role("Button"));
        let w = s.expect(&sel(t).role("Button"));
        if w.toggled != Some(true) {
            problems.push(format!("clicking {t} does not mark it"));
        }
    }
    for p in &problems {
        s.step(&format!("PROBLEM {p}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Regression (found by the gate above): after the window moved away from
/// the cursor, the first click on a panel button reached the canvas and
/// painted, because the app asked egui whether the pointer was over the UI
/// using the pointer of the last frame. Panel clicks must never paint.
#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn a_click_on_a_panel_right_after_the_window_moved_never_paints() {
    let mut s = Session::start("ui-click-after-move");
    let mut problems = Vec::new();
    for (i, (x, y, w, h)) in [(0, 0, 1920, 1080), (100, 100, 1024, 700), (300, 50, 900, 650)]
        .into_iter()
        .enumerate()
    {
        tela_e2e::win::maximize(s.window, false);
        tela_e2e::win::place(s.window, x, y, w, h);
        std::thread::sleep(std::time::Duration::from_millis(300));
        for t in ["Brush", "Select", "Gradient", "Brush"] {
            s.click(&sel(t).role("Button"));
        }
        let st = s.settle();
        if st["document"]["undo"] != serde_json::Value::Null {
            problems.push(format!(
                "placement {i}: a panel click left the step {}",
                st["document"]["undo"]
            ));
            s.key(&[vk::CONTROL], vk::letter('z'));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

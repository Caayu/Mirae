//! Interface responsiveness, measured on the real executable (the 5F
//! quality gate, section 13): from the last OS input event to the app's
//! state showing its effect, read through the same file the tests read (so
//! each figure includes up to a few milliseconds of polling and the
//! frame that shows it). Generous limits catch regressions; the figures
//! go into the report and docs/performance.md.

#![cfg(windows)]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tela_e2e::win::{self, Button};
use tela_e2e::{Sel, Session, sel, vk};

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Milliseconds from `started` until `done` holds for the app's state.
fn until(s: &Session, started: Instant, done: impl Fn(&Value) -> bool) -> f64 {
    let deadline = started + Duration::from_secs(10);
    while Instant::now() < deadline {
        if done(&s.state()) {
            return started.elapsed().as_secs_f64() * 1000.0;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    f64::INFINITY
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// A click on a widget: the time from the button's release (when a button
/// acts) until `done`.
fn click(s: &mut Session, w: &Sel, done: impl Fn(&Value) -> bool) -> f64 {
    let c = s.reveal(w).unwrap_or_else(|e| panic!("{e}")).center();
    s.move_to(c[0], c[1]);
    s.press(Button::Left);
    let t0 = Instant::now();
    win::button(Button::Left, false);
    let ms = until(s, t0, done);
    s.settle();
    ms
}

/// A key: the time from its press until `done`.
fn key(s: &mut Session, k: u16, done: impl Fn(&Value) -> bool) -> f64 {
    s.front();
    let t0 = Instant::now();
    win::key(k, true);
    win::key(k, false);
    let ms = until(s, t0, done);
    s.settle();
    ms
}

/// A drag held at its end: the time from the last move until the app has
/// handled it and composed every tile it marked.
fn drag_held(s: &mut Session, a: [f32; 2], b: [f32; 2]) -> f64 {
    s.move_to(a[0], a[1]);
    s.press(Button::Left);
    let near = [b[0] - (b[0] - a[0]) * 0.05, b[1] - (b[1] - a[1]) * 0.05];
    s.drag_moves(a, near, 24);
    let (t0, wall) = (Instant::now(), wall_ms());
    s.move_to(b[0], b[1]);
    until(s, t0, |st| {
        st["input_ms"].as_u64().unwrap_or(0) >= wall && st["pending_tiles"] == 0
    })
}

#[test]
#[ignore = "desktop E2E: run scripts/e2e.ps1"]
fn interface_response_times() {
    let mut s = Session::start("performance");
    s.maximize();
    s.key(&[], vk::HOME);
    s.settle();
    let mut rows: Vec<(&str, f64)> = Vec::new();

    // Idle: the app draws nothing while nothing happens.
    let cpu0 = s.cpu_time();
    let frame0 = s.state()["frame"].as_u64().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    rows.push((
        "idle CPU, % of one core over 3 s",
        (s.cpu_time() - cpu0).as_secs_f64() / 3.0 * 100.0,
    ));
    rows.push((
        "idle frames drawn in 3 s",
        (s.state()["frame"].as_u64().unwrap() - frame0) as f64,
    ));

    let tools = [
        ("Gradient", "gradient"),
        ("Brush", "brush"),
        ("Select", "selection"),
        ("Fill", "fill"),
        ("Brush", "brush"),
    ];
    let runs = tools
        .iter()
        .map(|&(t, tool)| click(&mut s, &sel(t).role("Button"), |st| st["tool"] == tool))
        .collect();
    rows.push((
        "tool strip click → tool active, ms (median of 5)",
        median(runs),
    ));

    let keys = [
        ('m', "selection"),
        ('b', "brush"),
        ('g', "fill"),
        ('b', "brush"),
        ('k', "move"),
    ];
    let runs = keys
        .iter()
        .map(|&(k, tool)| key(&mut s, vk::letter(k), |st| st["tool"] == tool))
        .collect();
    rows.push(("shortcut → tool active, ms (median of 5)", median(runs)));
    s.key(&[], vk::letter('b'));

    let mut runs = Vec::new();
    for _ in 0..5 {
        let before = s.state()["colors"]["main"].clone();
        runs.push(click(&mut s, &sel("Swap"), |st| {
            st["colors"]["main"] != before
        }));
    }
    rows.push((
        "Swap click → colours shown swapped, ms (median of 5)",
        median(runs),
    ));

    // Gradient preview over the whole canvas.
    s.click(&sel("Gradient").role("Button"));
    let mut runs = Vec::new();
    for i in 0..5 {
        let a = s.doc_to_client(500.0, 1500.0);
        let b = s.doc_to_client(3500.0 - 200.0 * i as f32, 1600.0);
        runs.push(drag_held(&mut s, a, b));
        s.release(Button::Left);
        s.settle();
        s.key(&[vk::CONTROL], vk::letter('z'));
        s.settle();
    }
    rows.push((
        "gradient preview, last move → whole canvas shown, ms (median of 5)",
        median(runs),
    ));

    // Gradient commit of the whole 4000 × 3000 canvas.
    let mut runs = Vec::new();
    for _ in 0..3 {
        let a = s.doc_to_client(500.0, 1500.0);
        let b = s.doc_to_client(3500.0, 1600.0);
        s.move_to(a[0], a[1]);
        s.press(Button::Left);
        s.drag_moves(a, b, 10);
        s.settle();
        let t0 = Instant::now();
        win::button(Button::Left, false);
        runs.push(until(&s, t0, |st| {
            st["document"]["undo"] == "gradient" && st["pending_tiles"] == 0
        }));
        s.settle();
        s.key(&[vk::CONTROL], vk::letter('z'));
        s.settle();
    }
    rows.push((
        "gradient commit 4000×3000, release → shown, ms (median of 3)",
        median(runs),
    ));

    // Transform preview: a 1000² selection dragged with the Move tool.
    s.click(&sel("Select").role("Button"));
    s.drag_doc([1000.0, 1000.0], [2000.0, 2000.0], 8);
    s.key(&[vk::ALT], vk::DELETE);
    s.settle();
    s.click(&sel("Move").role("Button"));
    let mut runs = Vec::new();
    for i in 0..5 {
        let a = s.doc_to_client(1500.0, 1500.0);
        let b = s.doc_to_client(1500.0 + 100.0 * (i + 1) as f32, 1500.0);
        runs.push(drag_held(&mut s, a, b));
        s.release(Button::Left);
        s.settle();
        s.key(&[vk::CONTROL], vk::letter('z'));
        s.settle();
    }
    rows.push((
        "Move preview (1000² selection), last move → shown, ms (median of 5)",
        median(runs),
    ));
    s.key(&[vk::CONTROL], vk::letter('d'));

    // The layer panel with 40 layers.
    for _ in 0..39 {
        s.key(&[vk::CONTROL, vk::SHIFT], vk::letter('n'));
    }
    s.settle();
    let row = s.reveal(&sel("Layer 20  100 %")).unwrap();
    let t0 = Instant::now();
    s.wheel(row.center(), -3);
    rows.push((
        "layer list (40 layers), wheel → list moved, ms",
        until(&s, t0, |st| {
            st["widgets"].as_array().unwrap().iter().any(|w| {
                w["label"] == "Layer 20  100 %"
                    && w["rect"][1].as_f64() != Some(f64::from(row.rect[1]))
            })
        }),
    ));
    s.settle();
    rows.push((
        "layer row click → layer active, ms",
        click(&mut s, &sel("Layer 5  100 %"), |st| {
            st["document"]["layers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["name"] == "Layer 5" && l["active"] == true)
        }),
    ));

    // A resize: the window placed → the new layout drawn.
    let t0 = Instant::now();
    win::maximize(s.window, false);
    win::place(s.window, 50, 50, 1280, 800);
    rows.push((
        "resize → new layout drawn, ms",
        until(&s, t0, |st| {
            st["window"][0] == 1264 || st["window"][0] == 1280
        }),
    ));

    s.note("\n| Measure | Value |\n|---|---:|");
    for (name, v) in &rows {
        s.note(&format!("| {name} | {v:.1} |"));
    }
    // Limits far above the figures: they catch regressions, they do not
    // grade.
    let get = |n: &str| rows.iter().find(|r| r.0.starts_with(n)).unwrap().1;
    assert!(get("idle CPU") < 3.0, "the app works while idle");
    assert!(get("idle frames") < 10.0, "the app redraws while idle");
    assert!(get("tool strip click") < 100.0);
    assert!(get("shortcut") < 100.0);
    assert!(get("Swap click") < 100.0);
    assert!(get("gradient preview") < 600.0);
    assert!(get("layer row click") < 100.0);
}

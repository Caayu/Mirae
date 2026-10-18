//! Desktop end-to-end tests of the real Mirae executable (docs/testing.md,
//! "Desktop end-to-end tests"; docs/quality/QUALITY_GATE.md, layer L3).
//!
//! **Responsibility:** start `tela.exe` in its test mode (`TELA_E2E_DIR`),
//! drive it with OS input — `SendInput` mouse and keyboard, a synthetic
//! Windows Ink pen — find widgets by what they say (role and label, from
//! the app's AccessKit description) rather than by fixed coordinates,
//! wait until the app is idle, read its state and document (pixels,
//! digests), capture the screen and write a report per test into
//! `target/e2e/<test>/`. **Does not:** change the app or reach into it;
//! every action is input a user could make. **Depends on:** serde_json,
//! png, windows-sys. **Used by:** its own tests only (`cargo test -p
//! tela-e2e --release -- --ignored --test-threads=1`, or
//! `scripts/e2e.ps1`).
//!
//! Limits (recorded in docs/testing.md): Windows only; one desktop, so one
//! test at a time; the window must stay in front (another program taking
//! the focus fails a test rather than clicking elsewhere); egui's widgets
//! are not exposed to Windows UI Automation (the app has no AccessKit
//! adapter), so the app writes their description itself in test mode.

#[cfg(windows)]
pub mod win;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::Value;

#[cfg(windows)]
use win::{Button, Window};

/// One test at a time: they share the screen, the mouse and the keyboard.
static DESKTOP: Mutex<()> = Mutex::new(());

/// Virtual-key codes used by the tests.
pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const ENTER: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const ALT: u16 = 0x12;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const HOME: u16 = 0x24;
    pub const DELETE: u16 = 0x2E;
    /// `A`..`Z` are their ASCII capitals.
    pub const fn letter(c: char) -> u16 {
        c.to_ascii_uppercase() as u16
    }
}

/// The repository root (this crate is `crates/e2e`).
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A widget as the app described it in its last frame.
#[derive(Clone, Debug)]
pub struct Widget {
    pub role: String,
    pub label: String,
    pub value: Option<String>,
    pub number: Option<f64>,
    /// `[x0, y0, x1, y1]`, client pixels.
    pub rect: [f32; 4],
    pub enabled: bool,
    pub toggled: Option<bool>,
    pub focused: bool,
}

impl Widget {
    fn from_json(v: &Value) -> Self {
        let rect = v["rect"]
            .as_array()
            .map(|r| {
                let f = |i: usize| r.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                [f(0), f(1), f(2), f(3)]
            })
            .unwrap_or_default();
        Self {
            role: v["role"].as_str().unwrap_or_default().to_owned(),
            label: v["label"].as_str().unwrap_or_default().to_owned(),
            value: v["value"].as_str().map(str::to_owned),
            number: v["number"].as_f64(),
            rect,
            enabled: v["enabled"].as_bool().unwrap_or(false),
            toggled: v["toggled"].as_bool(),
            focused: v["focused"].as_bool().unwrap_or(false),
        }
    }

    /// The text a person reads on it: the label, or a label's value.
    pub fn text(&self) -> &str {
        if self.label.is_empty() {
            self.value.as_deref().unwrap_or_default()
        } else {
            &self.label
        }
    }

    pub fn center(&self) -> [f32; 2] {
        [
            (self.rect[0] + self.rect[2]) / 2.0,
            (self.rect[1] + self.rect[3]) / 2.0,
        ]
    }

    /// Whether the whole widget lies inside a client area of `size`.
    pub fn inside(&self, size: [i32; 2]) -> bool {
        self.rect[0] >= -0.5
            && self.rect[1] >= -0.5
            && self.rect[2] <= size[0] as f32 + 0.5
            && self.rect[3] <= size[1] as f32 + 0.5
    }
}

/// Which widget: by its text (and role), optionally the first one after
/// some anchors in layout order ("the `Linear` after `Mix`").
#[derive(Clone, Debug)]
pub struct Sel {
    text: Option<String>,
    role: Option<String>,
    after: Vec<String>,
    nth: usize,
}

/// A widget whose text is `text`.
pub fn sel(text: &str) -> Sel {
    Sel {
        text: Some(text.to_owned()),
        role: None,
        after: Vec::new(),
        nth: 0,
    }
}

/// A widget of `role` (e.g. the slider after a label).
pub fn role(role: &str) -> Sel {
    Sel {
        text: None,
        role: Some(role.to_owned()),
        after: Vec::new(),
        nth: 0,
    }
}

impl Sel {
    pub fn role(mut self, role: &str) -> Self {
        self.role = Some(role.to_owned());
        self
    }

    /// Only widgets after the first one reading `anchor` (anchors apply in
    /// order).
    pub fn after(mut self, anchor: &str) -> Self {
        self.after.push(anchor.to_owned());
        self
    }

    /// The `n`th match (0 is the first).
    pub fn nth(mut self, n: usize) -> Self {
        self.nth = n;
        self
    }

    fn describe(&self) -> String {
        let mut s = String::new();
        if let Some(r) = &self.role {
            let _ = write!(s, "{r} ");
        }
        if let Some(t) = &self.text {
            let _ = write!(s, "{t:?}");
        }
        for a in &self.after {
            let _ = write!(s, " after {a:?}");
        }
        if self.nth > 0 {
            let _ = write!(s, " #{}", self.nth);
        }
        s
    }

    /// The widget among `widgets`.
    pub fn find(&self, widgets: &[Widget]) -> Option<Widget> {
        let mut start = 0;
        for a in &self.after {
            let i = widgets[start..].iter().position(|w| w.text() == a)?;
            start += i + 1;
        }
        widgets[start..]
            .iter()
            .filter(|w| self.text.as_ref().is_none_or(|t| w.text() == t))
            .filter(|w| self.role.as_ref().is_none_or(|r| &w.role == r))
            .nth(self.nth)
            .cloned()
    }
}

/// One test's run of the app.
#[cfg(windows)]
pub struct Session {
    name: String,
    dir: PathBuf,
    child: Child,
    pub window: Window,
    report: String,
    shots: usize,
    request: u64,
    started: Instant,
    _desktop: MutexGuard<'static, ()>,
}

#[cfg(windows)]
impl Session {
    /// Starts the release executable in test mode with a fresh folder
    /// `target/e2e/<name>/` and waits for its first frame.
    pub fn start(name: &str) -> Self {
        let desktop = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
        win::dpi_aware();
        let root = workspace();
        let exe = std::env::var_os("TELA_EXE").map_or_else(
            || root.join("target").join("release").join("tela.exe"),
            PathBuf::from,
        );
        assert!(
            exe.exists(),
            "{} is missing: build it first (cargo build --release -p tela-app)",
            exe.display()
        );
        let dir = root.join("target").join("e2e").join(name);
        // Only this test's own folder under target/ is ever removed.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app")).expect("create the test folder");
        let log = std::fs::File::create(dir.join("app.log")).expect("create the log");
        let child = Command::new(&exe)
            .env("TELA_E2E_DIR", dir.join("app"))
            .env(
                "RUST_LOG",
                "info,tela::gradient=debug,tela::fill=debug,tela::e2e=debug",
            )
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("start tela.exe");
        let mut s = Self {
            name: name.to_owned(),
            dir,
            child,
            window: Window(0),
            report: String::new(),
            shots: 0,
            request: 0,
            started: Instant::now(),
            _desktop: desktop,
        };
        let _ = writeln!(
            s.report,
            "# E2E `{name}`\n\nExecutable: `{}`  \nRun folder: `target/e2e/{name}`\n",
            exe.display()
        );
        let pid = s.child.id();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(w) = win::windows_of(pid)
                .into_iter()
                .find(|w| w.title.starts_with("Mirae"))
                && s.dir.join("app").join("state.json").exists()
            {
                s.window = w.window;
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the app showed no window within 30 s"
            );
            sleep(Duration::from_millis(100));
        }
        s.front();
        s.settle();
        s.step("started");
        s
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Appends a line to the report (and the test's output).
    pub fn step(&mut self, text: &str) {
        let t = self.started.elapsed().as_secs_f32();
        eprintln!("[{}] {t:7.2}s {text}", self.name);
        let _ = writeln!(self.report, "- `{t:7.2}s` {text}");
    }

    /// Brings the window to the front and checks it is there: input goes to
    /// the foreground window, so a test never types into another program.
    pub fn front(&mut self) {
        for _ in 0..5 {
            if win::foreground() == self.window {
                return;
            }
            win::bring_to_front(self.window);
        }
        // A dialog of ours (a native file dialog) may be in front.
        let ours = win::windows_of(self.child.id());
        assert!(
            ours.iter().any(|w| w.window == win::foreground()),
            "another program holds the foreground; refusing to send input"
        );
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("app").join("state.json")
    }

    /// The app's state after its last frame.
    pub fn state(&self) -> Value {
        for _ in 0..50 {
            if let Ok(text) = std::fs::read_to_string(self.state_path())
                && let Ok(v) = serde_json::from_str::<Value>(&text)
            {
                return v;
            }
            sleep(Duration::from_millis(10));
        }
        panic!("cannot read the app's state");
    }

    /// Waits until the app is idle: no tile left to compose, no drag, and
    /// the same state (apart from the frame counter) for 150 ms.
    pub fn settle(&mut self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut last = String::new();
        let mut since = Instant::now();
        loop {
            self.check_alive();
            let mut v = self.state();
            v["frame"] = Value::Null;
            let text = v.to_string();
            let idle = v["pending_tiles"].as_u64() == Some(0);
            if text != last {
                last = text;
                since = Instant::now();
            } else if idle && since.elapsed() >= Duration::from_millis(150) {
                return self.state();
            }
            assert!(
                Instant::now() < deadline,
                "the app did not become idle within 20 s (pending tiles {})",
                v["pending_tiles"]
            );
            sleep(Duration::from_millis(25));
        }
    }

    fn check_alive(&mut self) {
        if let Ok(Some(status)) = self.child.try_wait() {
            panic!(
                "the app exited ({status}); see {}",
                self.dir.join("app.log").display()
            );
        }
    }

    pub fn widgets(&self) -> Vec<Widget> {
        self.state()["widgets"]
            .as_array()
            .map(|a| a.iter().map(Widget::from_json).collect())
            .unwrap_or_default()
    }

    pub fn client_size(&self) -> [i32; 2] {
        win::client(self.window).1
    }

    /// The widget, or why it cannot be used.
    pub fn widget(&self, s: &Sel) -> Result<Widget, String> {
        s.find(&self.widgets())
            .ok_or_else(|| format!("no widget {}", s.describe()))
    }

    /// The widget, which must be there.
    pub fn expect(&self, s: &Sel) -> Widget {
        self.widget(s).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Clicks a widget by what it says. It must lie entirely inside the
    /// window and be enabled: a control a user cannot reach fails the test.
    pub fn click(&mut self, s: &Sel) -> Widget {
        let w = self.expect(s);
        let size = self.client_size();
        assert!(
            w.inside(size),
            "{} is outside the window ({:?} in {size:?})",
            s.describe(),
            w.rect
        );
        assert!(w.enabled, "{} is disabled", s.describe());
        let c = w.center();
        self.click_at(c[0], c[1]);
        self.step(&format!("click {}", s.describe()));
        self.settle();
        w
    }

    fn to_screen(&self, x: f32, y: f32) -> [i32; 2] {
        let (origin, _) = win::client(self.window);
        [origin[0] + x.round() as i32, origin[1] + y.round() as i32]
    }

    /// Moves the mouse to client pixel `(x, y)`.
    pub fn move_to(&mut self, x: f32, y: f32) {
        let p = self.to_screen(x, y);
        win::move_cursor(p[0], p[1]);
        sleep(Duration::from_millis(12));
    }

    pub fn press(&mut self, b: Button) {
        self.front();
        win::button(b, true);
        sleep(Duration::from_millis(30));
    }

    pub fn release(&mut self, b: Button) {
        win::button(b, false);
        sleep(Duration::from_millis(30));
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.press(Button::Left);
        self.release(Button::Left);
    }

    /// A left drag between client points in `steps` moves, ~60 per second.
    pub fn drag(&mut self, from: [f32; 2], to: [f32; 2], steps: usize) {
        self.move_to(from[0], from[1]);
        self.press(Button::Left);
        self.drag_moves(from, to, steps);
        self.release(Button::Left);
    }

    /// Moves with the button held (the caller pressed it).
    pub fn drag_moves(&mut self, from: [f32; 2], to: [f32; 2], steps: usize) {
        let steps = steps.max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            self.move_to(
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
            );
            sleep(Duration::from_millis(4));
        }
    }

    /// Client pixel of document point `(x, y)` (through the app's view).
    pub fn doc_to_client(&self, x: f32, y: f32) -> [f32; 2] {
        let v = self.state();
        let m: Vec<f32> = v["view"]["to_screen"]
            .as_array()
            .map(|a| a.iter().map(|n| n.as_f64().unwrap_or(0.0) as f32).collect())
            .unwrap_or_default();
        assert_eq!(m.len(), 6, "no view in the state");
        [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
    }

    /// A left drag between document points.
    pub fn drag_doc(&mut self, from: [f32; 2], to: [f32; 2], steps: usize) {
        let a = self.doc_to_client(from[0], from[1]);
        let b = self.doc_to_client(to[0], to[1]);
        self.drag(a, b, steps);
        self.step(&format!("drag document {from:?} -> {to:?}"));
    }

    /// A synthetic-pen stroke between document points.
    pub fn pen_doc(&mut self, from: [f32; 2], to: [f32; 2], steps: usize, pressure: f32) {
        self.pen_doc_with(from, to, steps, pressure, false);
    }

    pub fn pen_doc_with(
        &mut self,
        from: [f32; 2],
        to: [f32; 2],
        steps: usize,
        pressure: f32,
        eraser: bool,
    ) {
        self.front();
        let pen = win::Pen::new().expect("create a synthetic pen");
        let points: Vec<[i32; 2]> = (0..=steps.max(1))
            .map(|i| {
                let t = i as f32 / steps.max(1) as f32;
                let p = self.doc_to_client(
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                );
                self.to_screen(p[0], p[1])
            })
            .collect();
        pen.stroke(&points, pressure, eraser);
        self.step(&format!(
            "pen{} document {from:?} -> {to:?}",
            if eraser { " (eraser end)" } else { "" }
        ));
    }

    /// Presses and releases a key, with modifiers held around it.
    pub fn key(&mut self, mods: &[u16], key: u16) {
        self.front();
        for &m in mods {
            win::key(m, true);
        }
        win::key(key, true);
        sleep(Duration::from_millis(20));
        win::key(key, false);
        for &m in mods.iter().rev() {
            win::key(m, false);
        }
        sleep(Duration::from_millis(40));
    }

    pub fn type_text(&mut self, text: &str) {
        win::type_text(text);
    }

    pub fn wheel(&mut self, at: [f32; 2], notches: i32) {
        self.move_to(at[0], at[1]);
        win::wheel(notches);
        sleep(Duration::from_millis(40));
    }

    /// Captures the client area into `NN-<label>.png` and links it in the
    /// report; returns the pixels (RGBA) and the size.
    pub fn shot(&mut self, label: &str) -> (Vec<u8>, [i32; 2]) {
        let (origin, size) = win::client(self.window);
        let rgba = win::capture(origin[0], origin[1], size[0], size[1]);
        self.shots += 1;
        let file = format!("{:02}-{label}.png", self.shots);
        write_png(&self.dir.join(&file), &rgba, size[0] as u32, size[1] as u32);
        let _ = writeln!(self.report, "  ![{label}]({file})");
        (rgba, size)
    }

    /// One read-only query to the app (`pixel x y`, `digest`).
    pub fn request(&mut self, query: &str) -> Value {
        self.request += 1;
        let id = self.request;
        let app = self.dir.join("app");
        let tmp = app.join("request.tmp");
        std::fs::write(&tmp, format!("{id} {query}")).expect("write a request");
        std::fs::rename(&tmp, app.join("request.txt")).expect("write a request");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(text) = std::fs::read_to_string(app.join("response.json"))
                && let Ok(v) = serde_json::from_str::<Value>(&text)
                && v["id"].as_u64() == Some(id)
            {
                return v["answer"].clone();
            }
            self.check_alive();
            assert!(Instant::now() < deadline, "no answer to {query:?}");
            sleep(Duration::from_millis(20));
        }
    }

    /// Straight RGBA8 of the composite over the paper and of the active
    /// layer at document pixel `(x, y)`.
    pub fn pixel(&mut self, x: i32, y: i32) -> ([u8; 4], Option<[u8; 4]>) {
        let v = self.request(&format!("pixel {x} {y}"));
        let rgba = |v: &Value| -> Option<[u8; 4]> {
            let a = v.as_array()?;
            let c = |i: usize| a.get(i).and_then(Value::as_u64).unwrap_or(0) as u8;
            Some([c(0), c(1), c(2), c(3)])
        };
        (
            rgba(&v["composite"]).unwrap_or_else(|| panic!("no composite pixel: {v}")),
            rgba(&v["layer"]),
        )
    }

    /// The document's content digest (per layer: pixels and mask).
    pub fn digest(&mut self) -> Value {
        self.request("digest")
    }

    /// The colour shown on screen at client pixel `(x, y)`.
    pub fn screen_rgb(&self, x: f32, y: f32) -> [u8; 3] {
        let p = self.to_screen(x, y);
        let c = win::capture(p[0], p[1], 1, 1);
        [c[0], c[1], c[2]]
    }

    /// The colour shown on screen at document point `(x, y)`.
    pub fn screen_rgb_doc(&self, x: f32, y: f32) -> [u8; 3] {
        let p = self.doc_to_client(x, y);
        self.screen_rgb(p[0], p[1])
    }

    /// Waits for a native dialog of the app (a file dialog) and answers it
    /// with `path` and Enter.
    pub fn file_dialog(&mut self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(15);
        let dialog = loop {
            if let Some(w) = win::windows_of(self.child.id())
                .into_iter()
                .find(|w| w.class == "#32770")
            {
                break w;
            }
            assert!(Instant::now() < deadline, "no file dialog appeared");
            sleep(Duration::from_millis(100));
        };
        sleep(Duration::from_millis(600));
        win::bring_to_front(dialog.window);
        assert_eq!(
            win::foreground(),
            dialog.window,
            "the file dialog is not in front"
        );
        // The file name box has the focus; replace its text.
        win::key(vk::CONTROL, true);
        win::key(vk::letter('a'), true);
        win::key(vk::letter('a'), false);
        win::key(vk::CONTROL, false);
        win::type_text(&path.display().to_string());
        sleep(Duration::from_millis(150));
        win::key(vk::ENTER, true);
        win::key(vk::ENTER, false);
        let deadline = Instant::now() + Duration::from_secs(15);
        while win::windows_of(self.child.id())
            .iter()
            .any(|w| w.window == dialog.window)
        {
            assert!(Instant::now() < deadline, "the file dialog stayed open");
            sleep(Duration::from_millis(100));
        }
        self.step(&format!("file dialog: {}", path.display()));
        self.front();
        self.settle();
    }

    /// Places the window (outer rectangle) and waits for the new layout.
    pub fn place(&mut self, x: i32, y: i32, width: i32, height: i32) {
        win::maximize(self.window, false);
        sleep(Duration::from_millis(200));
        win::place(self.window, x, y, width, height);
        sleep(Duration::from_millis(300));
        self.settle();
        let size = self.client_size();
        self.step(&format!("window placed: client {}×{}", size[0], size[1]));
    }

    pub fn maximize(&mut self) {
        win::maximize(self.window, true);
        sleep(Duration::from_millis(400));
        self.settle();
        let size = self.client_size();
        self.step(&format!("maximized: client {}×{}", size[0], size[1]));
    }

    /// Asks the window to close (the title bar's button) and answers the
    /// unsaved-changes question with `answer` if it is asked.
    pub fn close(&mut self, answer: &str) {
        win::close(self.window);
        sleep(Duration::from_millis(300));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                self.step("closed");
                return;
            }
            if self.state()["prompt"] == "unsaved"
                && let Ok(w) = self.widget(&sel(answer).role("Button"))
            {
                let c = w.center();
                self.click_at(c[0], c[1]);
                self.step(&format!("unsaved question: {answer}"));
            }
            assert!(Instant::now() < deadline, "the app did not close");
            sleep(Duration::from_millis(200));
        }
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

#[cfg(windows)]
impl Drop for Session {
    fn drop(&mut self) {
        let failed = std::thread::panicking();
        if failed && !self.exited() {
            self.shot("failure");
        }
        let _ = writeln!(
            self.report,
            "\nResult: **{}** after {:.1} s\n",
            if failed { "FAILED" } else { "passed" },
            self.started.elapsed().as_secs_f32()
        );
        let _ = std::fs::write(self.dir.join("report.md"), &self.report);
        // Only the process this session started is ever stopped.
        if !self.exited() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Writes straight RGBA8 pixels as a PNG.
pub fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("create a screenshot");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("PNG header");
    w.write_image_data(rgba).expect("PNG data");
}

/// Reads a PNG (8 or 16 bits, RGB or RGBA) as straight RGBA, 16-bit
/// channels kept: `(width, height, channels per pixel 4, bits, samples)`.
pub fn read_png(path: &Path) -> (u32, u32, u8, Vec<u16>) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::IDENTITY);
    let mut reader = dec.read_info().expect("PNG header");
    let mut buf = vec![0; reader.output_buffer_size().expect("PNG size")];
    let info = reader.next_frame(&mut buf).expect("PNG data");
    let bits = match info.bit_depth {
        png::BitDepth::Sixteen => 16,
        _ => 8,
    };
    let channels = info.color_type.samples();
    let samples: Vec<u16> = if bits == 16 {
        buf[..info.buffer_size()]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect()
    } else {
        buf[..info.buffer_size()]
            .iter()
            .map(|&b| u16::from(b))
            .collect()
    };
    let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
    let max = if bits == 16 { 65535 } else { 255 };
    for p in samples.chunks_exact(channels) {
        rgba.extend_from_slice(&p[..3.min(channels)]);
        rgba.push(if channels == 4 { p[3] } else { max });
    }
    (info.width, info.height, bits, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(role: &str, label: &str) -> Widget {
        Widget {
            role: role.into(),
            label: label.into(),
            value: None,
            number: None,
            rect: [0.0; 4],
            enabled: true,
            toggled: None,
            focused: false,
        }
    }

    #[test]
    fn selectors_find_by_text_role_and_anchor() {
        let list = [
            w("Label", "Gradient"),
            w("Button", "Linear"),
            w("Label", "Mix"),
            w("Button", "Linear"),
            w("Slider", ""),
        ];
        assert!(sel("Linear").find(&list).is_some());
        assert_eq!(
            sel("Linear").after("Mix").find(&list).unwrap().role,
            "Button"
        );
        assert_eq!(sel("Linear").nth(1).find(&list).unwrap().label, "Linear");
        assert!(sel("Linear").after("Mix").nth(1).find(&list).is_none());
        assert!(role("Slider").after("Mix").find(&list).is_some());
        assert!(sel("Missing").find(&list).is_none());
        assert!(sel("Linear").after("Nowhere").find(&list).is_none());
    }
}

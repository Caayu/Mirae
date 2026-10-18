//! The desktop end-to-end test mode (docs/testing.md, "Desktop end-to-end
//! tests"). Off unless the environment variable `TELA_E2E_DIR` names a
//! folder; production runs never reach this code.
//!
//! The tests drive the real executable with real (synthetic OS) mouse,
//! keyboard and pen input. This module only lets them *look*: after every
//! frame it writes `state.json` into the folder (the app's state, the
//! document's structure and egui's description of every widget, with its
//! rectangle in window pixels), and it answers read-only queries written
//! to `request.txt` (`<id> pixel x y`, `<id> digest`) in `response.json`.
//! Nothing here changes the document, the tools or the UI.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tela_canvas::TileCoord;
use tela_color::{PremulRgba, with_pixel_type};
use tela_document::{BlendMode, Document, LayerPart};
use tela_ui::WidgetInfo;

use super::{App, BLEND_LABELS, Drag};
use crate::colors::Slot;
use crate::tools::CanvasTool;

/// How often the app looks for a request while it is otherwise idle.
pub(super) const POLL: Duration = Duration::from_millis(40);

/// The test mode's folder and bookkeeping.
pub(super) struct E2e {
    dir: PathBuf,
    frame: u64,
    /// The last request answered (its id), so it is answered once.
    answered: Option<u64>,
    pub due: Instant,
    /// When the app last handled an input event (milliseconds since the
    /// Unix epoch), and whether the state written last shows it: the
    /// tests wait for their input to be handled before they look.
    input_ms: u64,
    written_ms: u64,
}

impl E2e {
    /// The test mode, if `TELA_E2E_DIR` asks for it.
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("TELA_E2E_DIR")?);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!(target: "tela::e2e", "cannot use {}: {e}", dir.display());
            return None;
        }
        log::info!(target: "tela::e2e", "end-to-end test mode: {}", dir.display());
        Some(Self {
            dir,
            frame: 0,
            answered: None,
            due: Instant::now(),
            input_ms: 0,
            written_ms: 0,
        })
    }

    /// An input event was handled now.
    pub fn input(&mut self) {
        self.input_ms = now_ms();
    }

    /// Replaces `name` atomically (a reader never sees half a file).
    fn write(&self, name: &str, text: &str) {
        let tmp = self.dir.join(format!("{name}.tmp"));
        let path = self.dir.join(name);
        let result = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &path));
        if let Err(e) = result {
            log::warn!(target: "tela::e2e", "cannot write {}: {e}", path.display());
        }
    }
}

impl App {
    /// After a frame: the state the tests read.
    pub(super) fn e2e_frame(&mut self) {
        self.e2e_poll();
        let Some(e2e) = &mut self.e2e else {
            return;
        };
        e2e.frame += 1;
        self.e2e_write_state();
    }

    /// Between frames: writes the state again if an input event was
    /// handled since it was last written (an event that needs no frame).
    pub(super) fn e2e_idle(&mut self) {
        self.e2e_poll();
        if self.e2e.as_ref().is_some_and(|e| e.input_ms != e.written_ms) {
            self.e2e_write_state();
        }
    }

    fn e2e_write_state(&mut self) {
        let state = self.e2e_state();
        if let Some(e2e) = &mut self.e2e {
            e2e.write("state.json", &state);
            e2e.written_ms = e2e.input_ms;
        }
    }

    /// Answers a new request, if there is one.
    pub(super) fn e2e_poll(&mut self) {
        let Some(e2e) = &self.e2e else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(e2e.dir.join("request.txt")) else {
            return;
        };
        let mut words = text.split_whitespace();
        let Some(id) = words.next().and_then(|w| w.parse::<u64>().ok()) else {
            return;
        };
        if e2e.answered == Some(id) {
            return;
        }
        let args: Vec<&str> = words.collect();
        let answer = match args.as_slice() {
            ["pixel", x, y] => match (x.parse::<i32>(), y.parse::<i32>()) {
                (Ok(x), Ok(y)) => self.e2e_pixel(x, y),
                _ => error("bad coordinates"),
            },
            ["digest"] => digest(&self.document),
            _ => error("unknown request"),
        };
        let response = format!("{{\"id\":{id},\"answer\":{answer}}}");
        if let Some(e2e) = &mut self.e2e {
            e2e.write("response.json", &response);
            e2e.answered = Some(id);
        }
    }

    /// The composite over the paper (what the canvas shows, at document
    /// precision) and the active layer's stored pixel at document pixel
    /// `(x, y)`, as 8-bit straight RGBA.
    fn e2e_pixel(&mut self, x: i32, y: i32) -> String {
        let composite = self.document.composite_pixel(x, y, &mut self.compose);
        let layer = self
            .document
            .layer(self.document.active_layer())
            .map(|l| l.pixels().pixel(x, y));
        format!(
            "{{\"composite\":{},\"layer\":{}}}",
            composite.map_or("null".into(), rgba8),
            layer.map_or("null".into(), rgba8)
        )
    }

    fn e2e_state(&self) -> String {
        let mut j = String::with_capacity(16 * 1024);
        let doc = &self.document;
        let frame = self.e2e.as_ref().map_or(0, |e| e.frame);
        let input_ms = self.e2e.as_ref().map_or(0, |e| e.input_ms);
        let (size, ppp, focused_ui) = self.gpu.as_ref().map_or(([0, 0], 1.0, false), |g| {
            let s = g.window.inner_size();
            (
                [s.width, s.height],
                g.window.scale_factor(),
                g.ui.has_focus(),
            )
        });
        let _ = write!(
            j,
            "{{\"frame\":{frame},\"input_ms\":{input_ms},\"window\":[{},{}],\"scale\":{ppp},\"ui_focus\":{focused_ui},",
            size[0], size[1]
        );
        let tool = match self.tool {
            CanvasTool::Brush if self.tools.eraser_mode => "eraser",
            CanvasTool::Brush => "brush",
            CanvasTool::Eyedropper => "eyedropper",
            CanvasTool::Selection => "selection",
            CanvasTool::Move => "move",
            CanvasTool::Fill => "fill",
            CanvasTool::Gradient => "gradient",
        };
        let drag = match self.drag {
            Drag::None => "none",
            Drag::Paint => "paint",
            Drag::Pan => "pan",
            Drag::Pick(_) => "pick",
            Drag::Select => "select",
            Drag::Transform => "transform",
            Drag::Gradient => "gradient",
        };
        let _ = write!(
            j,
            "\"tool\":{},\"drag\":{},\"preset\":{},\"pending_tiles\":{},",
            string(tool),
            string(drag),
            string(&self.tools.active().name),
            self.stale.len()
        );
        // The view: document point (x, y) is at screen pixel
        // (a·x + b·y + c, d·x + e·y + f).
        let m = self.view.canvas_to_screen();
        let (x, y, t) = (m.matrix2.x_axis, m.matrix2.y_axis, m.translation);
        let _ = write!(
            j,
            "\"view\":{{\"zoom\":{},\"rotation\":{},\"to_screen\":[{},{},{},{},{},{}]}},",
            self.view.zoom, self.view.rotation, x.x, y.x, t.x, x.y, y.y, t.y
        );
        let modified = self.is_modified();
        let _ = write!(
            j,
            "\"document\":{{\"name\":{},\"path\":{},\"width\":{},\"height\":{},\"bits\":{},\"modified\":{modified},\"revision\":{},\"undo\":{},\"redo\":{},\"selection\":{},\"active_part\":{},\"status\":{},\"layers\":[",
            string(&self.document_name()),
            self.file
                .path
                .as_ref()
                .map_or("null".into(), |p| string(&p.display().to_string())),
            doc.width(),
            doc.height(),
            doc.depth().bits(),
            string(&format!("{:?}", doc.revision())),
            doc.next_undo()
                .map_or("null".into(), |a| string(super::history_label(a))),
            doc.next_redo()
                .map_or("null".into(), |a| string(super::history_label(a))),
            doc.has_selection(),
            string(match doc.active_part() {
                LayerPart::Pixels => "pixels",
                LayerPart::Mask => "mask",
            }),
            self.file.status.as_deref().map_or("null".into(), string)
        );
        let active = doc.active_layer();
        for (i, l) in doc.layers().enumerate() {
            let blend = BlendMode::ALL
                .iter()
                .position(|&m| m == l.blend_mode())
                .map_or("?", |i| BLEND_LABELS[i]);
            let _ = write!(
                j,
                "{}{{\"name\":{},\"active\":{},\"visible\":{},\"locked\":{},\"opacity\":{},\"blend\":{},\"clipped\":{},\"alpha_locked\":{},\"mask\":{},\"tiles\":{}}}",
                if i > 0 { "," } else { "" },
                string(l.name()),
                l.id() == active,
                l.visible(),
                l.locked(),
                l.opacity(),
                string(blend),
                l.clipped(),
                l.alpha_locked(),
                l.mask().map_or("null".into(), |m| format!(
                    "{{\"enabled\":{}}}",
                    m.enabled()
                )),
                l.pixels().tile_count()
            );
        }
        j.push_str("]},");
        let c = &self.colors;
        let _ = write!(
            j,
            "\"colors\":{{\"main\":{},\"sub\":{},\"active\":{},\"recent\":{}}},",
            string(&c.main().to_hex()),
            string(&c.sub().to_hex()),
            string(if c.active_slot() == Slot::Main {
                "main"
            } else {
                "sub"
            }),
            c.recent().len()
        );
        let g = &self.gradient_tool;
        let _ = write!(
            j,
            "\"gradient\":{{\"kind\":{},\"edge\":{},\"mixing\":{},\"dither\":{},\"selected\":{},\"session\":{},\"stops\":[",
            string(&format!("{:?}", g.kind)),
            string(&format!("{:?}", g.edge)),
            string(&format!("{:?}", g.mixing)),
            g.dither,
            g.selected(),
            self.gradient.is_some()
        );
        for (i, s) in g.stops().iter().enumerate() {
            let color = match s.color {
                tela_ui::StopColor::Main => "main".to_owned(),
                tela_ui::StopColor::Sub => "sub".to_owned(),
                tela_ui::StopColor::Custom(c) => c.to_hex(),
            };
            let _ = write!(
                j,
                "{}{{\"position\":{},\"opacity\":{},\"color\":{}}}",
                if i > 0 { "," } else { "" },
                s.position,
                s.opacity,
                string(&color)
            );
        }
        j.push_str("]},");
        let _ = write!(
            j,
            "\"transform\":{},\"prompt\":{},",
            self.transform.is_some(),
            string(if self.file.asking.is_some() {
                "unsaved"
            } else if self.file.notice.is_some() {
                "notice"
            } else if self.merging.is_some() {
                "merge"
            } else if self.transform_exit.is_some() {
                "apply_transform"
            } else {
                ""
            })
        );
        // The areas the UI covers (physical pixels): the canvas shows
        // elsewhere.
        j.push_str("\"panels\":[");
        let panels: &[[f32; 4]] = self.gpu.as_ref().map_or(&[], |g| g.ui.occupied());
        for (i, r) in panels.iter().enumerate() {
            let _ = write!(
                j,
                "{}[{},{},{},{}]",
                if i > 0 { "," } else { "" },
                r[0],
                r[1],
                r[2],
                r[3]
            );
        }
        j.push_str("],\"widgets\":[");
        let widgets: &[WidgetInfo] = self.gpu.as_ref().map_or(&[], |g| g.ui.widgets());
        for (i, w) in widgets.iter().enumerate() {
            let _ = write!(
                j,
                "{}{{\"role\":{},\"label\":{},\"value\":{},\"number\":{},\"rect\":[{},{},{},{}],\"visible\":[{},{},{},{}],\"enabled\":{},\"toggled\":{},\"focused\":{}}}",
                if i > 0 { "," } else { "" },
                string(&w.role),
                string(&w.label),
                w.value.as_deref().map_or("null".into(), string),
                w.number
                    .filter(|n| n.is_finite())
                    .map_or("null".into(), |n| n.to_string()),
                w.rect[0],
                w.rect[1],
                w.rect[2],
                w.rect[3],
                w.visible[0],
                w.visible[1],
                w.visible[2],
                w.visible[3],
                w.enabled,
                w.toggled.map_or("null".into(), |t| t.to_string()),
                w.focused
            );
        }
        j.push_str("]}");
        j
    }
}

/// A content digest of the document: per layer, the stored pixels and the
/// mask (absent and blank tiles count as nothing), and the layer's
/// properties. Equal documents have equal digests.
fn digest(doc: &Document) -> String {
    let mut out = String::from("{\"layers\":[");
    for (i, l) in doc.layers().enumerate() {
        let mut h = Fnv::new();
        let mut coords: Vec<TileCoord> = l.pixels().tile_coords().collect();
        coords.sort_by_key(|c| (c.y, c.x));
        with_pixel_type!(doc.depth(), P => {
            if let Some(s) = l.pixels().surface::<P>() {
                for &c in &coords {
                    if let Some(t) = s.tile(c).filter(|t| !t.is_blank()) {
                        h.write(&c.x.to_le_bytes());
                        h.write(&c.y.to_le_bytes());
                        for p in t.pixels() {
                            h.write(bytemuck_pixel(p).as_slice());
                        }
                    }
                }
            }
        });
        let mut m = Fnv::new();
        if let Some(mask) = l.mask() {
            let mut coords: Vec<TileCoord> = mask.tile_coords().collect();
            coords.sort_by_key(|c| (c.y, c.x));
            for c in coords {
                if let Some(values) = mask.reveal_tile(c) {
                    m.write(&c.x.to_le_bytes());
                    m.write(&c.y.to_le_bytes());
                    for v in values {
                        m.write(&[v]);
                    }
                }
            }
        }
        let _ = write!(
            out,
            "{}{{\"name\":{},\"pixels\":\"{:016x}\",\"mask\":\"{:016x}\"}}",
            if i > 0 { "," } else { "" },
            string(l.name()),
            h.0,
            m.0
        );
    }
    out.push_str("]}");
    out
}

/// A stored pixel's bytes (8- or 16-bit channels, little-endian).
fn bytemuck_pixel<P: tela_color::StoragePixel>(p: &P) -> Vec<u8> {
    let c = (*p).to_premul();
    // The stored value, recovered exactly at either depth.
    let q = |v: f32| ((v * 65535.0).round() as u16).to_le_bytes();
    [c.r, c.g, c.b, c.a].iter().flat_map(|&v| q(v)).collect()
}

/// FNV-1a, 64 bits: enough to tell documents apart in a test.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

fn rgba8(p: PremulRgba) -> String {
    let c = p.unpremultiply();
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("[{},{},{},{}]", q(c.r), q(c.g), q(c.b), q(c.a))
}

/// Wall-clock milliseconds since the Unix epoch (the tests use the same
/// clock to know their input was handled).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn error(text: &str) -> String {
    format!("{{\"error\":{}}}", string(text))
}

/// `s` as a JSON string.
fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped_as_json() {
        assert_eq!(string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
        assert_eq!(string("Untitled — ã"), "\"Untitled — ã\"");
    }

    #[test]
    fn equal_documents_have_equal_digests() {
        let a = Document::new(300, 200);
        let b = Document::new(300, 200);
        assert_eq!(digest(&a), digest(&b));
    }
}

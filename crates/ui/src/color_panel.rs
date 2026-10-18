//! The colour panel: main and sub colour, an HSV picker (hue bar and
//! saturation/value square), RGB and HSV values, hex, where the eyedropper
//! picks, recent colours and swatches (phase 5A).
//!
//! It owns no authoritative state: the app lends a [`ColorModel`] each
//! frame and applies the [`ColorAction`] the panel returns. The panel
//! keeps only what a control needs between frames: the HSV value it is
//! editing (so the hue survives passing through grey or black, which have
//! none) and the hex text being typed.
//!
//! Synchronisation: every control shows the one active colour. An edit
//! produces a new colour, which the panel remembers as the colour it last
//! saw; when the app lends that same colour next frame nothing is
//! re-derived, so dragging in HSV never drifts through an RGB round trip.
//! Any other change (eyedropper, swatch, `X`, swap) re-derives the HSV
//! with [`Hsv::follow`]. Values are shown at 8 bits (0–255, integer
//! degrees and percent, `#RRGGBB`); the colour itself keeps `f32`
//! precision (a picked 16-bit colour is not rounded by being shown).

use egui::{
    Color32, DragValue, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, pos2,
    vec2,
};
use tela_color::{Color, Hsv};

use crate::style;

/// Where the eyedropper reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickSource {
    /// The composite as displayed, paper included (the reference's
    /// "displayed colours").
    #[default]
    Canvas,
    /// The active layer's own pixel (the reference's "layer colour").
    Layer,
}

/// What the colour panel shows for one frame.
#[derive(Clone, Copy, Debug)]
pub struct ColorModel<'a> {
    pub main: Color,
    pub sub: Color,
    /// The main colour paints (else the sub colour).
    pub main_active: bool,
    /// Newest first.
    pub recent: &'a [Color],
    pub swatches: &'a [Color],
    pub pick_source: PickSource,
}

impl ColorModel<'_> {
    fn active(&self) -> Color {
        if self.main_active {
            self.main
        } else {
            self.sub
        }
    }
}

/// What the colour panel asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorAction {
    /// New value of the active colour.
    Set(Color),
    /// Make the main (`true`) or sub colour the active one.
    SelectMain(bool),
    /// The other slot becomes active (`X`).
    Toggle,
    /// Exchange the two colours.
    Swap,
    /// Main black, sub white, main active.
    Reset,
    AddSwatch,
    RemoveSwatch(usize),
    SetPickSource(PickSource),
}

/// Between-frame state of the colour controls (never authoritative).
#[derive(Debug, Default)]
pub struct ColorPanel {
    hsv: Hsv,
    /// The active colour the controls last showed or produced.
    shown: Option<Color>,
    /// Hex text while its field has focus.
    hex: Option<String>,
}

fn color32(c: Color) -> Color32 {
    let [r, g, b] = c.to_rgb8();
    Color32::from_rgb(r, g, b)
}

const SQUARE: f32 = 200.0;
const BAR: f32 = 14.0;
const GRID: u32 = 8;

impl ColorPanel {
    /// Draws the panel; returns the action requested, if any.
    pub fn show(&mut self, ui: &mut Ui, m: &ColorModel) -> Option<ColorAction> {
        let active = m.active();
        if self.shown != Some(active) {
            self.hsv = Hsv::follow(active, self.hsv);
            self.shown = Some(active);
        }
        let mut action = None;
        ui.horizontal(|ui| {
            for (main, color, label) in [(true, m.main, "Main"), (false, m.sub, "Sub")] {
                let selected = main == m.main_active;
                let (rect, r) = ui.allocate_exact_size(vec2(40.0, 28.0), Sense::click());
                ui.painter().rect_filled(rect, 3.0, color32(color));
                let stroke = if selected {
                    Stroke::new(3.0, ui.visuals().selection.stroke.color)
                } else {
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                };
                ui.painter()
                    .rect_stroke(rect, 3.0, stroke, StrokeKind::Outside);
                if r.on_hover_text(format!("{label} colour ({})", color.to_hex()))
                    .clicked()
                {
                    action = Some(ColorAction::SelectMain(main));
                }
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button("Main/Sub")
                        .on_hover_text("Paint with the other colour (X)")
                        .clicked()
                    {
                        action = Some(ColorAction::Toggle);
                    }
                    if ui
                        .button("Swap")
                        .on_hover_text("Exchange main and sub colours")
                        .clicked()
                    {
                        action = Some(ColorAction::Swap);
                    }
                });
                if ui
                    .button("Reset")
                    .on_hover_text("Main black, sub white (D)")
                    .clicked()
                {
                    action = Some(ColorAction::Reset);
                }
            });
        });

        let mut edited: Option<(Color, bool)> = None;
        // Saturation/value square for the current hue.
        let (rect, r) =
            ui.allocate_exact_size(vec2(SQUARE, SQUARE * 0.75), Sense::click_and_drag());
        let h = self.hsv.h;
        paint_grid(ui, rect, |x, y| color32(Hsv::new(h, x, 1.0 - y).to_color()));
        if let Some(p) = r.interact_pointer_pos() {
            let s = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let v = 1.0 - ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            self.hsv = Hsv::new(h, s, v);
            edited = Some((self.hsv.to_color(), false));
        }
        let at = pos2(
            rect.left() + self.hsv.s * rect.width(),
            rect.top() + (1.0 - self.hsv.v) * rect.height(),
        );
        marker(ui, at);
        // Hue bar.
        let (bar, r) = ui.allocate_exact_size(vec2(SQUARE, BAR), Sense::click_and_drag());
        paint_grid(ui, bar, |x, _| {
            color32(Hsv::new(x * 360.0, 1.0, 1.0).to_color())
        });
        if let Some(p) = r.interact_pointer_pos() {
            let h = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0) * 360.0;
            self.hsv = Hsv::new(h.min(359.999), self.hsv.s, self.hsv.v);
            edited = Some((self.hsv.to_color(), false));
        }
        marker(
            ui,
            pos2(
                bar.left() + self.hsv.h / 360.0 * bar.width(),
                bar.center().y,
            ),
        );

        // Values: RGB at 8 bits, HSV in degrees and percent.
        let current = edited.map_or(active, |(c, _)| c);
        let mut rgb = current.to_rgb8();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(28.0, 18.0), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, color32(current));
            let mut changed = false;
            for (label, v) in ["R", "G", "B"].into_iter().zip(rgb.iter_mut()) {
                ui.label(label);
                changed |= ui.add(DragValue::new(v).range(0..=255)).changed();
            }
            if changed {
                edited = Some((Color::from_rgb8(rgb), true));
            }
        });
        let mut hsv = [self.hsv.h, self.hsv.s * 100.0, self.hsv.v * 100.0];
        ui.horizontal(|ui| {
            let mut changed = false;
            for ((label, v), max) in ["H", "S", "V"]
                .into_iter()
                .zip(hsv.iter_mut())
                .zip([359.0, 100.0, 100.0])
            {
                ui.label(label);
                changed |= ui
                    .add(DragValue::new(v).range(0.0..=max).fixed_decimals(0))
                    .changed();
            }
            if changed {
                self.hsv = Hsv::new(hsv[0], hsv[1] / 100.0, hsv[2] / 100.0);
                edited = Some((self.hsv.to_color(), false));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Hex");
            let mut text = self.hex.clone().unwrap_or_else(|| current.to_hex());
            let r = ui.add(
                TextEdit::singleline(&mut text)
                    .desired_width(70.0)
                    .char_limit(7),
            );
            if r.changed() {
                // Applied as soon as it is a valid colour, like the reference.
                if let Ok(c) = Color::from_hex(&text) {
                    edited = Some((c, true));
                }
            }
            self.hex = r.has_focus().then_some(text);
        });

        if let Some((color, rederive)) = edited {
            if rederive {
                self.hsv = Hsv::follow(color, self.hsv);
            }
            self.shown = Some(color);
            if color != active {
                action = Some(ColorAction::Set(color));
            }
        }

        style::row(ui, "Pick from", |ui| {
            let options = [
                (
                    PickSource::Canvas,
                    "Canvas",
                    "The eyedropper picks the colour shown (all layers over the paper)",
                ),
                (
                    PickSource::Layer,
                    "Layer",
                    "The eyedropper picks the active layer's own colour",
                ),
            ];
            if let Some(source) = style::segmented(ui, m.pick_source, &options) {
                action = Some(ColorAction::SetPickSource(source));
            }
        });

        // One row of the newest colours, always the same height, so
        // painting (which adds a recent colour) never moves what is below.
        style::row(ui, "Recent", |ui| {
            let per_row = ((ui.available_width() + 2.0) / 20.0).floor().max(1.0) as usize;
            let shown = &m.recent[..m.recent.len().min(per_row)];
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), style::CONTROL_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_height(style::CONTROL_HEIGHT);
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if shown.is_empty() {
                        style::hint(ui, "colours you paint with");
                    }
                    for (i, &c) in shown.iter().enumerate() {
                        if swatch(ui, c).clicked() {
                            action = Some(ColorAction::Set(shown[i]));
                        }
                    }
                },
            );
        });
        style::row(ui, "Swatches", |ui| {
            if ui
                .button("+")
                .on_hover_text("Add the current colour (right-click a swatch to remove it)")
                .clicked()
            {
                action = Some(ColorAction::AddSwatch);
            }
        });
        match swatch_grid(ui, m.swatches, "swatch") {
            (Some(i), _) => action = Some(ColorAction::Set(m.swatches[i])),
            (None, Some(i)) => action = Some(ColorAction::RemoveSwatch(i)),
            (None, None) => {}
        }
        action
    }
}

/// Fills `rect` with a bilinear grid of colours `f(x, y)` (`x`, `y` in
/// `0..=1`). Vertex colours, no texture: cheap to rebuild every frame.
fn paint_grid(ui: &Ui, rect: Rect, f: impl Fn(f32, f32) -> Color32) {
    let mut mesh = Mesh::default();
    for yi in 0..=GRID {
        for xi in 0..=GRID {
            let (x, y) = (xi as f32 / GRID as f32, yi as f32 / GRID as f32);
            mesh.colored_vertex(
                pos2(
                    rect.left() + x * rect.width(),
                    rect.top() + y * rect.height(),
                ),
                f(x, y),
            );
            if xi < GRID && yi < GRID {
                let tl = yi * (GRID + 1) + xi;
                mesh.add_triangle(tl, tl + 1, tl + GRID + 1);
                mesh.add_triangle(tl + 1, tl + GRID + 1, tl + GRID + 2);
            }
        }
    }
    ui.painter().add(Shape::mesh(mesh));
}

fn marker(ui: &Ui, at: Pos2) {
    ui.painter()
        .circle_stroke(at, 5.0, Stroke::new(2.5, Color32::BLACK));
    ui.painter()
        .circle_stroke(at, 5.0, Stroke::new(1.0, Color32::WHITE));
}

/// A wrapped grid of small swatches; returns the index clicked and the
/// index right-clicked.
fn swatch_grid(ui: &mut Ui, colors: &[Color], id: &str) -> (Option<usize>, Option<usize>) {
    let (mut clicked, mut removed) = (None, None);
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            for (i, &c) in colors.iter().enumerate() {
                let r = swatch(ui, c);
                if r.clicked() {
                    clicked = Some(i);
                }
                if r.secondary_clicked() {
                    removed = Some(i);
                }
            }
        });
    });
    (clicked, removed)
}

/// One colour swatch (18 points), clickable, with its hex as tooltip.
fn swatch(ui: &mut Ui, c: Color) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    ui.painter().rect_filled(rect, 2.0, color32(c));
    ui.painter().rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    r.on_hover_text(c.to_hex())
}

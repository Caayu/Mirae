//! Mirae's small design system (docs/architecture/ui.md, "Design
//! system"): the tokens every panel uses — spacing, control sizes, the
//! label column, colours, type sizes — the egui theme built from them, and
//! the few layout pieces that keep panels consistent (a section heading, a
//! labelled row, a segmented choice, a tool button). Panels use these
//! instead of their own numbers and colours, so the interface reads as one
//! application rather than a set of debug controls.

use egui::{Align, Color32, CornerRadius, Layout, Margin, RichText, Stroke, Ui, Vec2};

/// Spacing scale, in points.
pub mod space {
    pub const XS: f32 = 2.0;
    pub const S: f32 = 4.0;
    pub const M: f32 = 8.0;
    pub const L: f32 = 12.0;
    pub const XL: f32 = 16.0;
}

/// Height of a button, field or slider.
pub const CONTROL_HEIGHT: f32 = 22.0;
/// Width of the label column of a labelled row, so controls line up.
pub const LABEL_WIDTH: f32 = 76.0;
/// The tool strip at the far left.
pub const TOOL_STRIP_WIDTH: f32 = 100.0;
/// The tool options panel beside it.
pub const OPTIONS_WIDTH: f32 = 316.0;
/// The layers and document panel on the right.
pub const LAYERS_WIDTH: f32 = 288.0;
/// The bar at the top (file, history, document).
pub const TOP_BAR_HEIGHT: f32 = 34.0;
/// Height of a tool button in the strip.
pub const TOOL_HEIGHT: f32 = 30.0;
pub const RADIUS: u8 = 4;

/// Type sizes, in points.
pub mod text {
    pub const BODY: f32 = 13.0;
    pub const SMALL: f32 = 11.0;
    pub const HEADING: f32 = 15.0;
    pub const SECTION: f32 = 11.0;
}

/// Semantic colours (dark theme).
pub mod color {
    use egui::Color32;

    /// Panels.
    pub const PANEL: Color32 = Color32::from_rgb(0x23, 0x24, 0x28);
    /// The top bar and the tool strip, a step darker than panels.
    pub const CHROME: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x21);
    /// Buttons and fields at rest.
    pub const SURFACE: Color32 = Color32::from_rgb(0x30, 0x32, 0x38);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x3A, 0x3D, 0x45);
    pub const SURFACE_ACTIVE: Color32 = Color32::from_rgb(0x45, 0x49, 0x52);
    /// Text fields and slider rails.
    pub const FIELD: Color32 = Color32::from_rgb(0x18, 0x19, 0x1C);
    pub const BORDER: Color32 = Color32::from_rgb(0x3B, 0x3E, 0x46);
    pub const TEXT: Color32 = Color32::from_rgb(0xE4, 0xE5, 0xE8);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9C, 0xA0, 0xA8);
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x66, 0x6A, 0x72);
    /// Selection, the active tool, the active layer, focus.
    pub const ACCENT: Color32 = Color32::from_rgb(0x4F, 0x7F, 0xE0);
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x63, 0x90, 0xEA);
    pub const ON_ACCENT: Color32 = Color32::WHITE;
    /// The mask is the edit target, warnings.
    pub const WARNING: Color32 = Color32::from_rgb(0xE0, 0xA4, 0x3C);
    pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x53, 0x4B);
}

/// Installs the theme (once, at start-up).
pub fn apply(ctx: &egui::Context) {
    use egui::{FontId, TextStyle};
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Body, FontId::proportional(text::BODY)),
            (TextStyle::Button, FontId::proportional(text::BODY)),
            (TextStyle::Small, FontId::proportional(text::SMALL)),
            (TextStyle::Heading, FontId::proportional(text::HEADING)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        let sp = &mut s.spacing;
        sp.item_spacing = Vec2::new(space::S + 2.0, space::S + 1.0);
        sp.button_padding = Vec2::new(space::M, space::XS + 1.0);
        sp.interact_size = Vec2::new(36.0, CONTROL_HEIGHT);
        sp.slider_width = 120.0;
        sp.combo_width = 140.0;
        sp.text_edit_width = 120.0;
        sp.indent = space::L;
        sp.menu_margin = Margin::same(space::M as i8);
        sp.window_margin = Margin::same(space::L as i8);
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = color::PANEL;
        v.window_fill = color::PANEL;
        v.window_stroke = Stroke::new(1.0, color::BORDER);
        v.window_corner_radius = CornerRadius::same(RADIUS + 2);
        v.menu_corner_radius = CornerRadius::same(RADIUS);
        v.extreme_bg_color = color::FIELD;
        v.faint_bg_color = color::SURFACE;
        v.override_text_color = None;
        v.selection.bg_fill = color::ACCENT;
        v.selection.stroke = Stroke::new(1.0, color::ON_ACCENT);
        v.hyperlink_color = color::ACCENT_HOVER;
        let radius = CornerRadius::same(RADIUS);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = color::PANEL;
        w.noninteractive.weak_bg_fill = color::PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0, color::BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, color::TEXT);
        w.noninteractive.corner_radius = radius;
        w.inactive.bg_fill = color::SURFACE;
        w.inactive.weak_bg_fill = color::SURFACE;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, color::TEXT);
        w.inactive.corner_radius = radius;
        w.hovered.bg_fill = color::SURFACE_HOVER;
        w.hovered.weak_bg_fill = color::SURFACE_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, color::BORDER);
        w.hovered.fg_stroke = Stroke::new(1.5, color::TEXT);
        w.hovered.corner_radius = radius;
        w.active.bg_fill = color::SURFACE_ACTIVE;
        w.active.weak_bg_fill = color::SURFACE_ACTIVE;
        w.active.bg_stroke = Stroke::new(1.0, color::ACCENT);
        w.active.fg_stroke = Stroke::new(2.0, color::TEXT);
        w.active.corner_radius = radius;
        w.open.bg_fill = color::SURFACE_ACTIVE;
        w.open.weak_bg_fill = color::SURFACE_ACTIVE;
        w.open.bg_stroke = Stroke::new(1.0, color::BORDER);
        w.open.fg_stroke = Stroke::new(1.0, color::TEXT);
        w.open.corner_radius = radius;
    });
}

/// A section heading: small capitals in the muted colour, with space
/// above, so sections read as groups without boxes.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(space::M);
    ui.label(
        RichText::new(title.to_uppercase())
            .size(text::SECTION)
            .color(color::TEXT_MUTED)
            .strong(),
    );
    ui.add_space(space::XS);
}

/// A row with a label in the fixed label column and the controls after it.
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(LABEL_WIDTH, CONTROL_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(LABEL_WIDTH);
                ui.set_max_width(LABEL_WIDTH);
                ui.add(egui::Label::new(RichText::new(label).color(color::TEXT_MUTED)).truncate());
            },
        );
        add(ui)
    })
    .inner
}

/// Muted explanatory text.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(text::SMALL)
            .color(color::TEXT_MUTED),
    );
}

/// One choice among a few, shown side by side; returns the one clicked.
/// Each option is `(value, label, tooltip)`.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: T,
    options: &[(T, &str, &str)],
) -> Option<T> {
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::XS;
        for &(value, label, tip) in options {
            let mut r = ui.add(egui::Button::selectable(current == value, label));
            if !tip.is_empty() {
                r = r.on_hover_text(tip);
            }
            if r.clicked() {
                chosen = Some(value);
            }
        }
    });
    chosen
}

/// The Draw / Erase choice of a tool (gradient, fill). Erase is shown in
/// the warning colour with a hint while it is on, so a tool that removes
/// paint never looks like one that adds it. Returns the new choice.
pub fn draw_or_erase(ui: &mut Ui, erase: bool, what: &str) -> Option<bool> {
    let chosen = row(ui, "Mode", |ui| {
        let options = [
            (false, "Draw", "Paint with the colours"),
            (true, "Erase", "Remove paint (as the eraser does)"),
        ];
        segmented(ui, erase, &options)
    });
    if erase {
        row(ui, "", |ui| {
            ui.label(
                RichText::new(format!("Erasing: {what} removes paint"))
                    .size(text::SMALL)
                    .color(color::WARNING),
            );
        });
    }
    chosen
}

/// A tool button of the strip: full width, the accent fill when it is the
/// active tool.
pub fn tool_button(ui: &mut Ui, selected: bool, label: &str, tip: &str) -> egui::Response {
    let text = if selected {
        RichText::new(label).color(color::ON_ACCENT).strong()
    } else {
        RichText::new(label)
    };
    let button = egui::Button::selectable(selected, text)
        .min_size(Vec2::new(ui.available_width(), TOOL_HEIGHT));
    ui.add(button).on_hover_text(tip)
}

/// The frame of a side panel.
pub fn panel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(space::M as i8, space::S as i8))
}

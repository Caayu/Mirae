//! The fill panel: the bucket tool's settings, and Edit › Fill. It edits
//! nothing: it shows what the app lends it ([`FillModel`]) and returns a
//! [`FillAction`]. The tool itself is chosen in the tool strip.

use egui::Ui;

use crate::style;

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct FillModel {
    /// The region is decided on the visible canvas (all layers, without the
    /// paper) rather than on the edited layer alone.
    pub canvas: bool,
    /// Tolerance in percent, `0..=100`, one decimal.
    pub tolerance: f32,
    pub anti_alias: bool,
    /// The document has a selection (Fill then fills it, else the canvas).
    pub has_selection: bool,
}

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillAction {
    /// Decide the region on the canvas (`true`) or the edited layer.
    Canvas(bool),
    Tolerance(f32),
    AntiAlias(bool),
    /// Fill the selection, or the whole layer without one, now.
    FillSelection,
}

/// Draws the fill tool's options.
pub fn fill_panel(ui: &mut Ui, m: &FillModel) -> Option<FillAction> {
    let mut action = None;
    style::row(ui, "Refer to", |ui| {
        let options = [
            (false, "Layer", "Find the area on the edited layer only"),
            (
                true,
                "Canvas",
                "Find the area on every visible layer (without the paper)",
            ),
        ];
        if let Some(canvas) = style::segmented(ui, m.canvas, &options) {
            action = Some(FillAction::Canvas(canvas));
        }
    });
    style::row(ui, "Tolerance", |ui| {
        let mut t = m.tolerance;
        if ui
            .add(
                egui::Slider::new(&mut t, 0.0..=100.0)
                    .step_by(0.1)
                    .fixed_decimals(1)
                    .suffix(" %"),
            )
            .on_hover_text("How different a colour may be from the clicked one")
            .changed()
        {
            action = Some(FillAction::Tolerance(t));
        }
    });
    style::row(ui, "Edges", |ui| {
        let mut aa = m.anti_alias;
        if ui
            .checkbox(&mut aa, "Anti-aliased")
            .on_hover_text("Partly cover the pixels just outside the area")
            .changed()
        {
            action = Some(FillAction::AntiAlias(aa));
        }
    });
    style::row(ui, "", |ui| {
        let label = if m.has_selection {
            "Fill selection"
        } else {
            "Fill layer"
        };
        if ui
            .button(label)
            .on_hover_text("Fill the selection, or the whole layer, with the colour (Alt+Delete)")
            .clicked()
        {
            action = Some(FillAction::FillSelection);
        }
    });
    action
}

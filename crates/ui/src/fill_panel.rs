//! The fill panel: the bucket tool and its settings, and Edit › Fill. It
//! edits nothing: it shows what the app lends it ([`FillModel`]) and
//! returns a [`FillAction`].

use egui::Ui;

/// What the panel shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct FillModel {
    /// The fill (bucket) tool is the canvas tool.
    pub active: bool,
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
    /// Use the fill tool on the canvas.
    Tool,
    /// Decide the region on the canvas (`true`) or the edited layer.
    Canvas(bool),
    Tolerance(f32),
    AntiAlias(bool),
    /// Fill the selection, or the whole layer without one, now.
    FillSelection,
}

/// Draws the fill panel.
pub fn fill_panel(ui: &mut Ui, m: &FillModel) -> Option<FillAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label("Fill");
        if ui
            .selectable_label(m.active, "Bucket")
            .on_hover_text(
                "Fill a connected area with the colour (G); the eraser end or E clears it",
            )
            .clicked()
        {
            action = Some(FillAction::Tool);
        }
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
    ui.horizontal(|ui| {
        ui.label("Refer to");
        if ui
            .selectable_label(!m.canvas, "Layer")
            .on_hover_text("Find the area on the edited layer only")
            .clicked()
        {
            action = Some(FillAction::Canvas(false));
        }
        if ui
            .selectable_label(m.canvas, "Canvas")
            .on_hover_text("Find the area on every visible layer (without the paper)")
            .clicked()
        {
            action = Some(FillAction::Canvas(true));
        }
        let mut aa = m.anti_alias;
        if ui
            .checkbox(&mut aa, "AA")
            .on_hover_text("Partly cover the pixels just outside the area")
            .changed()
        {
            action = Some(FillAction::AntiAlias(aa));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Tolerance");
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
    action
}

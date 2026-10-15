//! The layer panel: the stack (top first), visibility and lock per layer,
//! and new/delete/move/blend mode/opacity, clipping, transparency lock and
//! the layer mask (add, delete, enable, edit pixels or mask) for the active
//! layer. It never sees the
//! document: the app lends one [`LayerRow`] per layer and applies the
//! [`LayerAction`] the panel returns. Controls the document would refuse
//! are greyed out (as the reference application does); the document still
//! enforces every rule, so a stale click is simply refused.

use egui::{Slider, Ui};

/// One layer as the panel shows it.
#[derive(Clone, Copy, Debug)]
pub struct LayerRow<'a> {
    /// Opaque key the app maps back to the layer.
    pub key: u32,
    pub name: &'a str,
    pub visible: bool,
    pub locked: bool,
    /// `0..=1`.
    pub opacity: f32,
    /// Index into the blend-mode names given to [`layer_panel`]; 0 is the
    /// default mode.
    pub blend: usize,
    pub active: bool,
    /// Clipped to the layer below.
    pub clipped: bool,
    /// "Lock transparent pixels".
    pub alpha_locked: bool,
    /// `Some(enabled)` when the layer has a mask.
    pub mask: Option<bool>,
    /// Strokes edit the mask rather than the pixels (active row only).
    pub editing_mask: bool,
}

/// What the layer panel asks the app to do. Commands without a key act on
/// the active layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayerAction {
    Add,
    Remove,
    MoveUp,
    MoveDown,
    Select(u32),
    SetVisible(u32, bool),
    SetLocked(u32, bool),
    /// `0..=1`.
    SetOpacity(f32),
    /// Index into the blend-mode names.
    SetBlendMode(usize),
    SetClipped(bool),
    SetAlphaLocked(bool),
    AddMask,
    RemoveMask,
    SetMaskEnabled(bool),
    /// `true`: strokes edit the mask; `false`: the pixels.
    EditMask(bool),
}

/// Draws the panel for `rows` (bottom to top, as the document stores
/// them); `blend_names` labels the blend modes the app offers. Returns the
/// action requested, if any.
pub fn layer_panel(ui: &mut Ui, rows: &[LayerRow], blend_names: &[&str]) -> Option<LayerAction> {
    let mut action = None;
    let active = rows.iter().find(|r| r.active);
    let unlocked = active.is_some_and(|r| !r.locked);
    let index = rows.iter().position(|r| r.active);

    ui.heading("Layers");
    ui.horizontal(|ui| {
        if ui
            .button("New")
            .on_hover_text("New layer (Ctrl+Shift+N)")
            .clicked()
        {
            action = Some(LayerAction::Add);
        }
        let removable = unlocked && rows.len() > 1;
        if ui
            .add_enabled(removable, egui::Button::new("Delete"))
            .clicked()
        {
            action = Some(LayerAction::Remove);
        }
        let up = index.is_some_and(|i| i + 1 < rows.len());
        if ui.add_enabled(up, egui::Button::new("Up")).clicked() {
            action = Some(LayerAction::MoveUp);
        }
        let down = index.is_some_and(|i| i > 0);
        if ui.add_enabled(down, egui::Button::new("Down")).clicked() {
            action = Some(LayerAction::MoveDown);
        }
    });

    if let Some(row) = active {
        // Blend mode and whole-percent opacity, like the reference; both
        // disabled while the layer is locked or hidden.
        let editable = !row.locked && row.visible;
        ui.add_enabled_ui(editable, |ui| {
            egui::ComboBox::from_id_salt("blend-mode")
                .selected_text(blend_names.get(row.blend).copied().unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, name) in blend_names.iter().enumerate() {
                        if ui.selectable_label(i == row.blend, *name).clicked() && i != row.blend {
                            action = Some(LayerAction::SetBlendMode(i));
                        }
                    }
                });
        });
        let mut percent = (row.opacity * 100.0).round() as i32;
        let slider = Slider::new(&mut percent, 0..=100)
            .suffix(" %")
            .text("opacity");
        if ui.add_enabled(editable, slider).changed() {
            action = Some(LayerAction::SetOpacity(percent as f32 / 100.0));
        }
        // Clipping and transparency lock: refused on locked layers.
        ui.horizontal(|ui| {
            let toggle = |ui: &mut Ui, on: bool, text: &str, tip: &str| {
                ui.add_enabled(!row.locked, egui::Button::selectable(on, text))
                    .on_hover_text(tip)
                    .clicked()
            };
            if toggle(ui, row.clipped, "Clip", "Clip to the layer below") {
                action = Some(LayerAction::SetClipped(!row.clipped));
            }
            if toggle(
                ui,
                row.alpha_locked,
                "Lock alpha",
                "Lock transparent pixels: paint changes colour only (the eraser still erases)",
            ) {
                action = Some(LayerAction::SetAlphaLocked(!row.alpha_locked));
            }
        });
        // The mask: add/delete (refused on locked layers), enable (allowed
        // there, like the reference), and which part strokes edit.
        ui.horizontal(|ui| match row.mask {
            None => {
                if ui
                    .add_enabled(!row.locked, egui::Button::new("Add mask"))
                    .on_hover_text("A mask that shows everything; paint reveals, erase hides")
                    .clicked()
                {
                    action = Some(LayerAction::AddMask);
                }
            }
            Some(enabled) => {
                if ui
                    .selectable_label(!row.editing_mask, "Pixels")
                    .on_hover_text("Strokes paint the layer")
                    .clicked()
                {
                    action = Some(LayerAction::EditMask(false));
                }
                if ui
                    .selectable_label(row.editing_mask, "Mask")
                    .on_hover_text("Strokes paint the mask: paint reveals, erase hides")
                    .clicked()
                {
                    action = Some(LayerAction::EditMask(true));
                }
                let mut on = enabled;
                if ui
                    .checkbox(&mut on, "on")
                    .on_hover_text("Apply the mask")
                    .changed()
                {
                    action = Some(LayerAction::SetMaskEnabled(on));
                }
                if ui
                    .add_enabled(!row.locked, egui::Button::new("Delete mask"))
                    .clicked()
                {
                    action = Some(LayerAction::RemoveMask);
                }
            }
        });
    }
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in rows.iter().rev() {
            ui.horizontal(|ui| {
                let mut visible = row.visible;
                if ui
                    .checkbox(&mut visible, "")
                    .on_hover_text("Visible")
                    .changed()
                {
                    action = Some(LayerAction::SetVisible(row.key, visible));
                }
                if ui
                    .selectable_label(row.locked, "Lock")
                    .on_hover_text("Lock: no painting, opacity or deletion")
                    .clicked()
                {
                    action = Some(LayerAction::SetLocked(row.key, !row.locked));
                }
                // Clipped layers are indented under their base.
                let indent = if row.clipped { "   > " } else { "" };
                let mut label = format!("{indent}{}  {:.0} %", row.name, row.opacity * 100.0);
                if row.blend != 0 {
                    label.push_str("  ");
                    label.push_str(blend_names.get(row.blend).copied().unwrap_or("?"));
                }
                if row.alpha_locked {
                    label.push_str("  \u{3b1}");
                }
                match (row.mask, row.active && row.editing_mask) {
                    (Some(true), false) => label.push_str("  [mask]"),
                    (Some(true), true) => label.push_str("  [editing mask]"),
                    (Some(false), _) => label.push_str("  [mask off]"),
                    (None, _) => {}
                }
                if ui.selectable_label(row.active, label).clicked() {
                    action = Some(LayerAction::Select(row.key));
                }
            });
        }
    });
    action
}

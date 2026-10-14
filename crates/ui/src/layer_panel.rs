//! The layer panel: the stack (top first), visibility and lock per layer,
//! and new/delete/move/opacity for the active layer. It never sees the
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
    pub active: bool,
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
}

/// Draws the panel for `rows` (bottom to top, as the document stores
/// them). Returns the action requested, if any.
pub fn layer_panel(ui: &mut Ui, rows: &[LayerRow]) -> Option<LayerAction> {
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
        // Whole percent, like the reference; disabled while the layer is
        // locked or hidden.
        let mut percent = (row.opacity * 100.0).round() as i32;
        let slider = Slider::new(&mut percent, 0..=100)
            .suffix(" %")
            .text("opacity");
        let editable = !row.locked && row.visible;
        if ui.add_enabled(editable, slider).changed() {
            action = Some(LayerAction::SetOpacity(percent as f32 / 100.0));
        }
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
                let label = format!("{}  {:.0} %", row.name, row.opacity * 100.0);
                if ui.selectable_label(row.active, label).clicked() {
                    action = Some(LayerAction::Select(row.key));
                }
            });
        }
    });
    action
}

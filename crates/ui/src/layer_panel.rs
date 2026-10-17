//! The layer panel: the stack (top first), visibility and lock per layer,
//! and new/delete/move/merge down/blend mode/opacity, clipping,
//! transparency lock and the layer mask (add, delete, enable, edit pixels
//! or mask) for the active layer; renaming by double-clicking a name and
//! reordering by dragging a row (phase 5B, observed in the reference:
//! parity record 5B, parts 3–4). It never sees the document: the app
//! lends one [`LayerRow`] per layer and applies the [`LayerAction`] the
//! panel returns. Controls the document would refuse are greyed out (as
//! the reference application does); the document still enforces every
//! rule, so a stale click is simply refused.
//!
//! The only state the panel keeps is presentation state in egui's memory:
//! the name being typed while a row is renamed, and egui's drag payload
//! while a row is dragged.

use egui::{Color32, CursorIcon, DragAndDrop, Id, Key, RichText, Sense, Slider, Stroke, Ui};

/// Longest layer name the rename field takes, as the reference's (64
/// characters; parity record 5B, RN5).
pub const MAX_NAME_CHARS: usize = 64;

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
#[derive(Clone, Debug, PartialEq)]
pub enum LayerAction {
    Add,
    Remove,
    MoveUp,
    MoveDown,
    /// Move layer `key` directly above (`above`) or below the layer
    /// `target` (drag and drop).
    MoveTo {
        key: u32,
        target: u32,
        above: bool,
    },
    /// Merge the active layer into the one below.
    MergeDown,
    Select(u32),
    /// A new, non-empty name for the layer.
    Rename(u32, String),
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

/// egui drag payload: the key of the row being dragged.
struct DraggedLayer(u32);

/// The row being renamed and the text typed so far (egui memory).
#[derive(Clone)]
struct Renaming {
    key: u32,
    text: String,
    /// The field has been focused once (focus it on the first frame only,
    /// so a click elsewhere can take the focus and commit).
    focused: bool,
}

/// What a finished rename does: a new name, or nothing (Escape, an empty
/// or unchanged name; reference RN2–RN4).
fn rename_result(old: &str, typed: &str, escaped: bool) -> Option<String> {
    let name: String = typed.chars().take(MAX_NAME_CHARS).collect();
    (!escaped && !name.is_empty() && name != old).then_some(name)
}

/// Where a row dragged over `target` goes: above it when the pointer is in
/// its upper half (rows are shown top first), else below it.
fn drop_above(pointer_y: f32, row: egui::Rect) -> bool {
    pointer_y < row.center().y
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
        // Like the reference's command: needs a layer below, and both
        // layers visible and unlocked (parity record 5B).
        let mergeable = index.is_some_and(|i| {
            i > 0
                && [&rows[i], &rows[i - 1]]
                    .iter()
                    .all(|r| r.visible && !r.locked)
        });
        if ui
            .add_enabled(mergeable, egui::Button::new("Merge down"))
            .on_hover_text("Merge into the layer below (Ctrl+E)")
            .clicked()
        {
            action = Some(LayerAction::MergeDown);
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
        // Which part strokes edit, said once and plainly (the row keeps
        // only a short marker, so the panel width does not change).
        let target = if row.editing_mask {
            "Strokes edit: mask"
        } else {
            "Strokes edit: pixels"
        };
        ui.label(RichText::new(target).small().weak());
    }
    ui.separator();

    // Escape cancels a drag (reference DR6).
    if DragAndDrop::has_payload_of_type::<DraggedLayer>(ui.ctx())
        && ui.input(|i| i.key_pressed(Key::Escape))
    {
        DragAndDrop::clear_payload(ui.ctx());
    }
    let rename_id = Id::new("layer-rename");
    let mut renaming: Option<Renaming> = ui.data(|d| d.get_temp(rename_id));
    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in rows.iter().rev() {
            let line = ui.horizontal(|ui| {
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
                    .on_hover_text("Lock: no painting, opacity, blend mode, merging or deletion")
                    .clicked()
                {
                    action = Some(LayerAction::SetLocked(row.key, !row.locked));
                }
                if let Some(r) = renaming.as_mut().filter(|r| r.key == row.key) {
                    let output = egui::TextEdit::singleline(&mut r.text)
                        .char_limit(MAX_NAME_CHARS)
                        .desired_width(f32::INFINITY)
                        .show(ui);
                    let edit = &output.response.response;
                    if !r.focused {
                        // The whole name starts selected, as in the
                        // reference: typing replaces it.
                        let mut state = output.state.clone();
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(r.text.chars().count()),
                        );
                        state.cursor.set_char_range(Some(all));
                        state.store(ui.ctx(), edit.id);
                        edit.request_focus();
                        r.focused = true;
                    } else if edit.lost_focus() {
                        // Enter and a click elsewhere commit; Escape cancels.
                        let escaped = ui.input(|i| i.key_pressed(Key::Escape));
                        if let Some(name) = rename_result(row.name, &r.text, escaped) {
                            action = Some(LayerAction::Rename(row.key, name));
                        }
                        r.key = 0;
                    }
                    return;
                }
                let label = row_label(row, blend_names);
                let name = ui
                    .add(
                        egui::Button::selectable(row.active, label)
                            .sense(Sense::click_and_drag())
                            .truncate(),
                    )
                    .on_hover_text(format!(
                        "{}\nDouble-click to rename; drag to move",
                        row.name
                    ));
                if name.double_clicked() {
                    renaming = Some(Renaming {
                        key: row.key,
                        text: row.name.to_owned(),
                        focused: false,
                    });
                } else if name.clicked() {
                    action = Some(LayerAction::Select(row.key));
                }
                if name.drag_started() {
                    // The dragged layer becomes active, as pressing its row
                    // does in the reference (DR3).
                    name.dnd_set_drag_payload(DraggedLayer(row.key));
                    action = Some(LayerAction::Select(row.key));
                }
            });
            // Drop target: the whole row. A line shows where the layer
            // goes (reference DR1: a horizontal line at the insertion).
            let rect = line.response.rect;
            if let Some(dragged) = line.response.dnd_hover_payload::<DraggedLayer>() {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                let pointer = ui.input(|i| i.pointer.interact_pos());
                if let Some(p) = pointer.filter(|_| dragged.0 != row.key) {
                    let y = if drop_above(p.y, rect) {
                        rect.top()
                    } else {
                        rect.bottom()
                    };
                    ui.painter().hline(
                        rect.x_range(),
                        y,
                        Stroke::new(2.0, Color32::from_rgb(230, 60, 60)),
                    );
                }
            }
            if let Some(dragged) = line.response.dnd_release_payload::<DraggedLayer>()
                && dragged.0 != row.key
                && let Some(p) = ui.input(|i| i.pointer.interact_pos())
            {
                action = Some(LayerAction::MoveTo {
                    key: dragged.0,
                    target: row.key,
                    above: drop_above(p.y, rect),
                });
            }
        }
    });
    ui.data_mut(|d| match renaming.filter(|r| r.key != 0) {
        Some(r) => {
            d.insert_temp(rename_id, r);
        }
        None => d.remove::<Renaming>(rename_id),
    });
    action
}

/// The text of a row: clipped layers indented under their base, then the
/// name, opacity, a non-default mode and short markers. Hidden layers are
/// dimmed.
fn row_label(row: &LayerRow, blend_names: &[&str]) -> RichText {
    // egui's default fonts have no arrow glyphs: plain text it is.
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
        (Some(true), false) => label.push_str("  [m]"),
        (Some(true), true) => label.push_str("  [M]"),
        (Some(false), _) => label.push_str("  [m off]"),
        (None, _) => {}
    }
    let text = RichText::new(label);
    if row.visible { text } else { text.weak() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_follow_the_reference_field() {
        // Enter or focus loss commits; Escape cancels; empty or unchanged
        // names change nothing; 64 characters at most.
        assert_eq!(rename_result("a", "b", false), Some("b".into()));
        assert_eq!(rename_result("a", "b", true), None);
        assert_eq!(rename_result("a", "", false), None);
        assert_eq!(rename_result("a", "a", false), None);
        let long = "x".repeat(300);
        assert_eq!(
            rename_result("a", &long, false).unwrap().chars().count(),
            64
        );
        let accented = "ç".repeat(70);
        assert_eq!(
            rename_result("a", &accented, false)
                .unwrap()
                .chars()
                .count(),
            64,
            "characters, not bytes"
        );
    }

    #[test]
    fn the_upper_half_of_a_row_drops_above_it() {
        let row = egui::Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(200.0, 120.0));
        assert!(drop_above(101.0, row));
        assert!(drop_above(109.9, row));
        assert!(!drop_above(110.0, row));
        assert!(!drop_above(119.0, row));
    }
}

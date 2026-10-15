//! Modal questions the app asks: the unsaved-changes prompt (Save /
//! Don't Save / Cancel, Save the default, as in the reference's close
//! prompt, parity record 4F F7) and notices (errors the user must see).
//! They are drawn by egui rather than as native message boxes: a native
//! message box opened inside the event loop's handler is never shown
//! (it waits for an idle queue the paused event loop cannot give it).
//! Like the panels they hold no state: the app decides when they are open.

use egui::{Align, Context, Id, Key, Layout, Modal, Rect, RichText};

/// The answer to the unsaved-changes prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptChoice {
    /// Save, then go on (stop if saving fails or is cancelled).
    Save,
    /// Go on without saving.
    DontSave,
    /// Keep working; nothing happens.
    Cancel,
}

/// Asks whether to save `name` before `doing` ("closing", …). Returns the
/// answer once given, and the screen area the prompt blocks. Enter
/// answers Save, Escape or a click beside the prompt Cancel.
pub fn unsaved_prompt(ctx: &Context, name: &str, doing: &str) -> (Option<PromptChoice>, Rect) {
    let mut choice = None;
    let modal = Modal::new(Id::new("unsaved-prompt")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new(format!("\u{201c}{name}\u{201d} has unsaved changes.")).strong());
        ui.label(format!("Save them before {doing}?"));
        ui.add_space(8.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                choice = Some(PromptChoice::Cancel);
            }
            if ui.button("Don't Save").clicked() {
                choice = Some(PromptChoice::DontSave);
            }
            let save = ui.button(RichText::new("Save").strong());
            if save.clicked() {
                choice = Some(PromptChoice::Save);
            }
        });
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            choice = Some(PromptChoice::Save);
        }
    });
    if choice.is_none() && modal.should_close() {
        choice = Some(PromptChoice::Cancel);
    }
    (choice, modal.backdrop_response.rect)
}

/// Shows `text` until dismissed (OK, Enter, Escape). Returns whether it
/// was dismissed this frame, and the screen area it blocks.
pub fn notice(ctx: &Context, text: &str) -> (bool, Rect) {
    let mut done = false;
    let modal = Modal::new(Id::new("notice")).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(text);
        ui.add_space(8.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            done |= ui.button("OK").clicked();
        });
        done |= ui.input(|i| i.key_pressed(Key::Enter));
    });
    (done || modal.should_close(), modal.backdrop_response.rect)
}

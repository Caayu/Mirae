//! Undo/redo buttons. The panel knows only what the app tells it: a label
//! for the step each button would revert or re-apply, or `None` when there
//! is none (the button is then disabled).

use egui::{Button, Ui};

/// What the history buttons show this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct HistoryButtons<'a> {
    /// The step Undo would revert, e.g. "Paint".
    pub undo: Option<&'a str>,
    /// The step Redo would re-apply.
    pub redo: Option<&'a str>,
}

/// Which button was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryCommand {
    Undo,
    Redo,
}

/// Draws the Undo / Redo buttons in a row.
pub fn history_bar(ui: &mut Ui, buttons: &HistoryButtons) -> Option<HistoryCommand> {
    let mut command = None;
    ui.horizontal(|ui| {
        let undo = ui
            .add_enabled(buttons.undo.is_some(), Button::new("Undo"))
            .on_hover_text(format!("Undo {} (Ctrl+Z)", buttons.undo.unwrap_or("")))
            .on_disabled_hover_text("Nothing to undo");
        if undo.clicked() {
            command = Some(HistoryCommand::Undo);
        }
        let redo = ui
            .add_enabled(buttons.redo.is_some(), Button::new("Redo"))
            .on_hover_text(format!(
                "Redo {} (Ctrl+Y, Ctrl+Shift+Z)",
                buttons.redo.unwrap_or("")
            ))
            .on_disabled_hover_text("Nothing to redo");
        if redo.clicked() {
            command = Some(HistoryCommand::Redo);
        }
    });
    command
}

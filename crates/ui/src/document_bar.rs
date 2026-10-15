//! Document controls: the storage depth (bits per channel), new documents
//! of either depth, and depth conversion. Like the other panels it never
//! sees the document: the app lends a [`DocumentInfo`] and applies the
//! [`DocumentCommand`] returned.

use egui::{Button, Ui};

/// What the document bar shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct DocumentInfo {
    pub width: u32,
    pub height: u32,
    /// Bits per channel of the layers' pixels (8 or 16).
    pub bits: u32,
}

/// What the document bar asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentCommand {
    /// Replace the document with an empty one of the same size at this
    /// depth (bits per channel).
    New(u32),
    /// Convert the document to this depth (one undoable step).
    Convert(u32),
}

/// Draws the document line and its buttons.
pub fn document_bar(ui: &mut Ui, info: &DocumentInfo) -> Option<DocumentCommand> {
    let mut command = None;
    ui.label(format!(
        "Document {} × {}, {}-bit",
        info.width, info.height, info.bits
    ))
    .on_hover_text(
        "Bits per channel of the layers' pixels. 16-bit keeps faint paint \
         and smooth build-up that 8-bit rounds away; the screen shows 8 bits \
         either way. Masks are 8-bit in every document.",
    );
    ui.horizontal(|ui| {
        for bits in [8, 16] {
            if ui
                .button(format!("New {bits}-bit"))
                .on_hover_text("Replace the document with an empty one (nothing is saved yet)")
                .clicked()
            {
                command = Some(DocumentCommand::New(bits));
            }
        }
        let other = if info.bits == 16 { 8 } else { 16 };
        let hint = if other == 8 {
            "Round every layer to 8 bits per channel (undoable)"
        } else {
            "Store every layer at 16 bits per channel; nothing changes on screen (undoable)"
        };
        if ui
            .add(Button::new(format!("To {other}-bit")))
            .on_hover_text(hint)
            .clicked()
        {
            command = Some(DocumentCommand::Convert(other));
        }
    });
    command
}

//! Document controls: the file commands (new, open, save, save as, PNG import
//! and export), the storage depth (bits per channel) and depth
//! conversion. Like the other panels it never sees the document: the app
//! lends a [`DocumentInfo`] and the [`ExportChoice`] it keeps, and applies
//! the [`DocumentCommand`] returned.

use egui::{Button, Ui};

use crate::style;

/// What the document bar shows this frame.
#[derive(Clone, Copy, Debug)]
pub struct DocumentInfo<'a> {
    /// File name without extension, or "Untitled".
    pub name: &'a str,
    /// Changed since it was created, opened or saved.
    pub modified: bool,
    pub width: u32,
    pub height: u32,
    /// Bits per channel of the layers' pixels (8 or 16).
    pub bits: u32,
    /// The outcome of the last file command (errors, warnings), if any.
    pub status: Option<&'a str>,
}

/// How the next PNG export is written; owned by the app, edited here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportChoice {
    /// Transparent background instead of the paper.
    pub transparent: bool,
    /// 16 bits per channel instead of 8.
    pub sixteen: bool,
}

/// What the document bar asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentCommand {
    /// Replace the document with an empty one of the same size at this
    /// depth (bits per channel), after asking about unsaved changes.
    New(u32),
    /// Convert the document to this depth (one undoable step).
    Convert(u32),
    /// Open a document file (asks about unsaved changes first).
    Open,
    /// Save to the document's file (asks for one if it has none).
    Save,
    /// Save to a new file.
    SaveAs,
    /// Add a PNG image as a new layer.
    ImportPng,
    /// Write the composite as a PNG, as [`ExportChoice`] says.
    ExportPng,
}

/// The Document section of the right panel: the storage depth and its
/// conversion, how PNGs are exported, and the last file command's outcome.
/// The file commands themselves are in the top bar
/// ([`crate::toolbar::top_bar`]).
pub fn document_panel(
    ui: &mut Ui,
    info: &DocumentInfo,
    export: &mut ExportChoice,
) -> Option<DocumentCommand> {
    let mut command = None;
    style::row(ui, "Size", |ui| {
        ui.label(format!("{} × {} px", info.width, info.height));
    });
    style::row(ui, "Depth", |ui| {
        ui.label(format!("{}-bit", info.bits)).on_hover_text(
            "Bits per channel of the layers' pixels. 16-bit keeps faint paint \
             and smooth build-up that 8-bit rounds away; the screen shows 8 bits \
             either way. Masks are 8-bit in every document.",
        );
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
    style::row(ui, "Export", |ui| {
        ui.checkbox(&mut export.transparent, "Transparent")
            .on_hover_text("Export without the paper (layers keep their blend modes)");
        ui.checkbox(&mut export.sixteen, "16-bit")
            .on_hover_text("16 bits per channel (keeps a 16-bit document's precision)");
    });
    if let Some(status) = info.status {
        style::hint(ui, status);
    }
    command
}

//! Document controls: file commands (new, open, save, save as, PNG import
//! and export), the storage depth (bits per channel) and depth
//! conversion. Like the other panels it never sees the document: the app
//! lends a [`DocumentInfo`] and the [`ExportChoice`] it keeps, and applies
//! the [`DocumentCommand`] returned.

use egui::{Button, RichText, Ui};

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

/// Draws the document lines and their buttons.
pub fn document_bar(
    ui: &mut Ui,
    info: &DocumentInfo,
    export: &mut ExportChoice,
) -> Option<DocumentCommand> {
    let mut command = None;
    let marker = if info.modified { " *" } else { "" };
    ui.label(RichText::new(format!("{}{marker}", info.name)).strong())
        .on_hover_text(if info.modified {
            "Changed since it was opened or saved"
        } else {
            "No unsaved changes"
        });
    ui.horizontal_wrapped(|ui| {
        let mut button = |ui: &mut Ui, label: &str, hint: &str, c: DocumentCommand| {
            if ui.button(label).on_hover_text(hint).clicked() {
                command = Some(c);
            }
        };
        button(
            ui,
            "Open…",
            "Open a Mirae document (Ctrl+O)",
            DocumentCommand::Open,
        );
        button(
            ui,
            "Save",
            "Save the document (Ctrl+S)",
            DocumentCommand::Save,
        );
        button(
            ui,
            "Save As…",
            "Save to a new file (Ctrl+Shift+S, Shift+Alt+S)",
            DocumentCommand::SaveAs,
        );
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Import PNG…")
            .on_hover_text("Add an image as a new layer, centred (also: drop a .png on the window)")
            .clicked()
        {
            command = Some(DocumentCommand::ImportPng);
        }
        if ui
            .button("Export PNG…")
            .on_hover_text("Write what the canvas shows as a PNG image")
            .clicked()
        {
            command = Some(DocumentCommand::ExportPng);
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut export.transparent, "transparent")
            .on_hover_text(
                "Export without the paper. Not available yet for layers in blend modes \
                 other than Normal (how they combine without paper is still being measured).",
            );
        ui.checkbox(&mut export.sixteen, "16-bit")
            .on_hover_text("16 bits per channel (keeps a 16-bit document's precision)");
    });
    ui.label(format!(
        "{} × {}, {}-bit",
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
                .on_hover_text("Replace the document with an empty one (Ctrl+N: 8-bit)")
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
    if let Some(status) = info.status {
        ui.label(RichText::new(status).small().weak());
    }
    command
}

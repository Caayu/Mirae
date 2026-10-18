//! The tool strip (one button per canvas tool, the active one filled with
//! the accent colour) and the top bar (file commands, undo and redo, the
//! document's name). Like the panels they edit nothing: the app
//! says which tool is active and applies what is returned.

use egui::{Button, RichText, Ui};

use crate::style::{self, color, space};

/// Width of the document name in the top bar.
const NAME_WIDTH: f32 = 150.0;
use crate::{DocumentCommand, DocumentInfo, HistoryButtons, HistoryCommand};

/// What the tool strip offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolChoice {
    Brush,
    Eraser,
    Eyedropper,
    Select,
    Move,
    Fill,
    Gradient,
    /// Opens a transform of the active layer (or its selected part).
    Transform,
}

impl ToolChoice {
    pub const ALL: [ToolChoice; 8] = [
        ToolChoice::Brush,
        ToolChoice::Eraser,
        ToolChoice::Eyedropper,
        ToolChoice::Select,
        ToolChoice::Move,
        ToolChoice::Fill,
        ToolChoice::Gradient,
        ToolChoice::Transform,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolChoice::Brush => "Brush",
            ToolChoice::Eraser => "Eraser",
            ToolChoice::Eyedropper => "Eyedropper",
            ToolChoice::Select => "Select",
            ToolChoice::Move => "Move",
            ToolChoice::Fill => "Fill",
            ToolChoice::Gradient => "Gradient",
            ToolChoice::Transform => "Transform",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            ToolChoice::Brush => "Paint with the brush (B)",
            ToolChoice::Eraser => "Erase with the eraser (E toggles)",
            ToolChoice::Eyedropper => {
                "Pick a colour from the canvas (also: right-click or Alt with any tool)"
            }
            ToolChoice::Select => "Select a rectangle, ellipse or lasso (M)",
            ToolChoice::Move => "Move the layer or its selected part (K)",
            ToolChoice::Fill => "Fill a connected area (G switches Fill and Gradient)",
            ToolChoice::Gradient => "Drag a gradient (G switches Fill and Gradient)",
            ToolChoice::Transform => {
                "Scale, rotate or flip the layer or its selected part (Ctrl+T); Enter applies"
            }
        }
    }
}

/// The vertical strip of tools; `active` is the tool in use.
pub fn tool_strip(ui: &mut Ui, active: ToolChoice) -> Option<ToolChoice> {
    let mut chosen = None;
    ui.add_space(space::S);
    ui.spacing_mut().item_spacing.y = space::XS + 1.0;
    for tool in ToolChoice::ALL {
        if tool == ToolChoice::Transform {
            ui.add_space(space::M);
        }
        if style::tool_button(ui, tool == active, tool.label(), tool.tip()).clicked() {
            chosen = Some(tool);
        }
    }
    chosen
}

/// The top bar: the document's name (with a dot while it has unsaved
/// changes), file commands, undo and redo.
pub fn top_bar(
    ui: &mut Ui,
    info: &DocumentInfo,
    history: &HistoryButtons,
) -> (Option<DocumentCommand>, Option<HistoryCommand>) {
    let mut command = None;
    let mut step = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = space::S;
        // The name in a fixed width (truncated), the modified dot beside
        // it in a fixed place: nothing after them moves when the document
        // changes or is renamed.
        ui.allocate_ui_with_layout(
            egui::Vec2::new(NAME_WIDTH, style::CONTROL_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(NAME_WIDTH);
                ui.set_max_width(NAME_WIDTH);
                ui.add(egui::Label::new(RichText::new(info.name).strong()).truncate())
                    .on_hover_text(info.name);
            },
        );
        let dot = RichText::new("•").color(if info.modified {
            color::WARNING
        } else {
            color::CHROME
        });
        ui.label(dot).on_hover_text(if info.modified {
            "Changed since it was opened or saved"
        } else {
            "No unsaved changes"
        });
        ui.separator();
        ui.menu_button("New", |ui| {
            if ui
                .button("New 8-bit document")
                .on_hover_text("An empty document, 8 bits per channel (Ctrl+N)")
                .clicked()
            {
                command = Some(DocumentCommand::New(8));
                ui.close();
            }
            if ui
                .button("New 16-bit document")
                .on_hover_text("An empty document, 16 bits per channel")
                .clicked()
            {
                command = Some(DocumentCommand::New(16));
                ui.close();
            }
        });
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
        ui.separator();
        button(
            ui,
            "Import PNG…",
            "Add an image as a new layer, centred (also: drop a .png on the window)",
            DocumentCommand::ImportPng,
        );
        button(
            ui,
            "Export PNG…",
            "Write the picture as a PNG (options in the Document section)",
            DocumentCommand::ExportPng,
        );
        ui.separator();
        let undo = ui
            .add_enabled(history.undo.is_some(), Button::new("Undo"))
            .on_hover_text(format!("Undo {} (Ctrl+Z)", history.undo.unwrap_or("")))
            .on_disabled_hover_text("Nothing to undo");
        if undo.clicked() {
            step = Some(HistoryCommand::Undo);
        }
        let redo = ui
            .add_enabled(history.redo.is_some(), Button::new("Redo"))
            .on_hover_text(format!(
                "Redo {} (Ctrl+Y, Ctrl+Shift+Z)",
                history.redo.unwrap_or("")
            ))
            .on_disabled_hover_text("Nothing to redo");
        if redo.clicked() {
            step = Some(HistoryCommand::Redo);
        }
    });
    (command, step)
}

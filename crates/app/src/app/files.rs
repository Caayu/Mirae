//! The file workflow: new, open, save, save as, PNG import and export,
//! dropped files, and the unsaved-changes question. Behaviour follows the
//! reference where it applies to a single-document app
//! (docs/parity/4F-file-workflow.md): Save / Don't Save / Cancel with
//! Save as the default before anything replaces or closes a modified
//! document; native file dialogs; the title shows the name and `*`.
//! Unlike the reference (F4), undoing back to the saved state is
//! unmodified again.
//!
//! The question is asked by an egui prompt (`tela_ui::unsaved_prompt`):
//! the action waits in [`FileState::asking`] until it is answered.
//! Errors are shown as egui notices.
//!
//! Files are read and written by `tela-io`; this module only decides
//! when, asks the user, and swaps the document in after a load succeeded,
//! so a failed open never loses the open document.

use std::path::{Path, PathBuf};

use rfd::FileDialog;
use tela_color::PixelDepth;
use tela_document::{Document, Placement, Revision};
use tela_io::{IoError, Limits, PngBackground, PngBits, PngExport, SaveOptions, Warning};
use tela_ui::{ExportChoice, PromptChoice};

use super::App;

const DOCUMENT_EXT: &str = "mirae";

/// The document's relation to a file.
pub(super) struct FileState {
    /// Where the document is saved, once it has been saved or opened.
    pub path: Option<PathBuf>,
    /// The document's revision when it was created, opened or last saved:
    /// it is modified while its revision differs.
    pub saved: Revision,
    /// Folder of the last file dialog, offered by the next one.
    pub folder: Option<PathBuf>,
    /// Outcome of the last file command, shown in the document bar.
    pub status: Option<String>,
    pub export: ExportChoice,
    /// An action waiting for the answer to the unsaved-changes prompt.
    pub asking: Option<Pending>,
    /// A message the user must see (an error), until dismissed.
    pub notice: Option<String>,
    /// The window may close now.
    pub quit: bool,
}

/// An action that replaces or closes the document.
#[derive(Clone, Debug)]
pub(super) enum Pending {
    New(PixelDepth),
    Open,
    OpenPath(PathBuf),
    Close,
}

impl Pending {
    /// How the prompt names the action ("Save them before …?").
    pub fn doing(&self) -> &'static str {
        match self {
            Pending::New(_) => "creating a new document",
            Pending::Open | Pending::OpenPath(_) => "opening another document",
            Pending::Close => "closing",
        }
    }
}

impl FileState {
    pub fn new(doc: &Document) -> Self {
        Self {
            path: None,
            saved: doc.revision(),
            folder: None,
            status: None,
            export: ExportChoice::default(),
            asking: None,
            notice: None,
            quit: false,
        }
    }
}

/// `path` with `ext` appended unless it already ends with it (any case).
fn with_extension(path: PathBuf, ext: &str) -> PathBuf {
    let has = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext));
    if has {
        return path;
    }
    let mut s = path.into_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn summary(warnings: &[Warning]) -> String {
    match warnings {
        [] => String::new(),
        [one] => format!(" (note: {one})"),
        [first, rest @ ..] => format!(" ({} notes, first: {first})", rest.len() + 1),
    }
}

impl App {
    /// Changed since it was created, opened or saved.
    pub(super) fn is_modified(&self) -> bool {
        self.document.revision() != self.file.saved
    }

    /// The file name without extension, or "Untitled".
    pub(super) fn document_name(&self) -> String {
        self.file
            .path
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(
                || "Untitled".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            )
    }

    fn file_dialog(&self, title: &str) -> FileDialog {
        let mut d = FileDialog::new().set_title(title);
        if let Some(dir) = &self.file.folder {
            d = d.set_directory(dir);
        }
        if let Some(gpu) = &self.gpu {
            d = d.set_parent(&*gpu.window);
        }
        d
    }

    /// Reports a failed file command: a notice (it must be seen), the
    /// status line and the log.
    fn report(&mut self, what: &str, e: &IoError) {
        log::error!(target: "tela::io", "{what}: {e}");
        self.file.notice = Some(format!("{what}.\n\n{e}"));
        self.file.status = Some(format!("{what}: {e}"));
    }

    fn remember_folder(&mut self, path: &Path) {
        self.file.folder = path.parent().map(Path::to_path_buf);
    }

    /// Runs an action that replaces or closes the document: at once if
    /// nothing is unsaved, otherwise after the unsaved-changes prompt
    /// (Save / Don't Save / Cancel, like the reference's close prompt,
    /// F7). Ignored while another question is open.
    pub(super) fn request(&mut self, action: Pending) {
        if self.file.asking.is_some() || self.file.notice.is_some() {
            return;
        }
        if self.is_modified() {
            self.file.asking = Some(action);
        } else {
            self.perform(action);
        }
    }

    /// The prompt's answer: Save goes on only if the document was saved
    /// (a cancelled or failed save keeps it open and unsaved).
    pub(super) fn answer(&mut self, choice: PromptChoice) {
        let Some(action) = self.file.asking.take() else {
            return;
        };
        match choice {
            PromptChoice::Save => {
                if self.save() {
                    self.perform(action);
                }
            }
            PromptChoice::DontSave => self.perform(action),
            PromptChoice::Cancel => {}
        }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New(depth) => {
                let (w, h) = (self.document.width(), self.document.height());
                if self.replace_document(Document::with_depth(w, h, depth), None) {
                    self.file.status = Some(format!("New {}-bit document", depth.bits()));
                }
            }
            Pending::Open => {
                let picked = self
                    .file_dialog("Open document")
                    .add_filter("Mirae document", &[DOCUMENT_EXT])
                    .pick_file();
                if let Some(path) = picked {
                    self.open_path(path);
                }
            }
            Pending::OpenPath(path) => self.open_path(path),
            Pending::Close => self.file.quit = true,
        }
    }

    /// Saves to the document's file, or asks for one (F5, F6). `true` when
    /// the document is saved.
    pub(super) fn save(&mut self) -> bool {
        match self.file.path.clone() {
            Some(path) => self.save_to(path),
            None => self.save_as(),
        }
    }

    /// Asks for a file and saves there.
    pub(super) fn save_as(&mut self) -> bool {
        let Some(path) = self
            .file_dialog("Save document")
            .add_filter("Mirae document", &[DOCUMENT_EXT])
            .set_file_name(format!("{}.{DOCUMENT_EXT}", self.document_name()))
            .save_file()
        else {
            return false;
        };
        self.save_to(with_extension(path, DOCUMENT_EXT))
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        self.remember_folder(&path);
        match tela_io::save_document(&self.document, &path, &SaveOptions::default()) {
            Ok(()) => {
                log::info!(target: "tela::io", "saved {}", path.display());
                self.file.status = Some(format!("Saved {}", file_name(&path)));
                self.file.saved = self.document.revision();
                self.file.path = Some(path);
                true
            }
            Err(e) => {
                self.report(
                    &format!("Could not save \u{201c}{}\u{201d}", file_name(&path)),
                    &e,
                );
                false
            }
        }
    }

    /// Makes `doc` the document, if the display can show it; the old one
    /// stays otherwise. Resets the view and the file state.
    fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) -> bool {
        if let Some(gpu) = &mut self.gpu
            && let Err(e) = gpu.renderer.set_document_size(doc.width(), doc.height())
        {
            log::error!("{e}");
            self.file.notice = Some(format!(
                "This document cannot be shown on this display.\n\n{e}"
            ));
            return false;
        }
        // What the old document showed must be cleared on screen.
        self.stale.extend(&self.document.content_tiles());
        self.document = doc;
        self.stale.extend(&self.document.content_tiles());
        self.stroke_target = None;
        self.file.saved = self.document.revision();
        self.file.path = path;
        self.fit_view();
        self.update_title();
        true
    }

    /// Loads `path` and, only if that fully succeeded, replaces the
    /// document (unsaved changes were already settled).
    fn open_path(&mut self, path: PathBuf) {
        self.remember_folder(&path);
        match tela_io::load_document(&path, &Limits::default()) {
            Ok(loaded) => {
                for w in &loaded.warnings {
                    log::warn!(target: "tela::io", "{}: {w}", path.display());
                }
                let status = format!("Opened {}{}", file_name(&path), summary(&loaded.warnings));
                if self.replace_document(loaded.document, Some(path)) {
                    self.file.status = Some(status);
                }
            }
            Err(e) => self.report(
                &format!("Could not open \u{201c}{}\u{201d}", file_name(&path)),
                &e,
            ),
        }
    }

    /// Asks for a PNG and adds it as a layer.
    pub(super) fn import_png(&mut self) {
        let picked = self
            .file_dialog("Import image as a layer")
            .add_filter("PNG image", &["png"])
            .pick_file();
        if let Some(path) = picked {
            self.import_path(&path);
        }
    }

    /// Adds the PNG at `path` as a new layer above the active one, named
    /// after the file and centred (reference F15); one undo step.
    fn import_path(&mut self, path: &Path) {
        self.remember_folder(path);
        let doc = &self.document;
        let canvas = (doc.width(), doc.height());
        let image = match tela_io::import_png(path, canvas, doc.depth(), &Limits::default()) {
            Ok(image) => image,
            Err(e) => {
                self.report(
                    &format!("Could not import \u{201c}{}\u{201d}", file_name(path)),
                    &e,
                );
                return;
            }
        };
        for w in &image.warnings {
            log::warn!(target: "tela::io", "{}: {w}", path.display());
        }
        let name = path
            .file_stem()
            .map_or_else(|| "Image".to_owned(), |s| s.to_string_lossy().into_owned());
        let at = Placement::Above(self.document.active_layer());
        match self.document.add_layer_with_pixels(at, name, image.pixels) {
            Ok((_, dirty)) => {
                self.stale.extend(&dirty);
                self.file.status = Some(format!(
                    "Imported {} ({} × {}){}",
                    file_name(path),
                    image.width,
                    image.height,
                    summary(&image.warnings)
                ));
            }
            Err(e) => {
                log::error!(target: "tela::io", "import refused: {e}");
                self.file.status = Some(format!("Could not import: {e}"));
            }
        }
    }

    /// Asks for a file and writes the composite as PNG.
    pub(super) fn export_png(&mut self) {
        let choice = self.file.export;
        let picked = self
            .file_dialog("Export PNG")
            .add_filter("PNG image", &["png"])
            .set_file_name(format!("{}.png", self.document_name()))
            .save_file();
        let Some(path) = picked else {
            return;
        };
        let path = with_extension(path, "png");
        self.remember_folder(&path);
        let options = PngExport {
            background: if choice.transparent {
                PngBackground::Transparent
            } else {
                PngBackground::Paper
            },
            bits: if choice.sixteen {
                PngBits::Sixteen
            } else {
                PngBits::Eight
            },
        };
        match tela_io::export_png(&self.document, &path, &options) {
            Ok(()) => {
                log::info!(target: "tela::io", "exported {}", path.display());
                self.file.status = Some(format!("Exported {}", file_name(&path)));
            }
            Err(e) => self.report(
                &format!("Could not export \u{201c}{}\u{201d}", file_name(&path)),
                &e,
            ),
        }
    }

    /// A file dropped on the window: a document opens (after the
    /// unsaved-changes question), an image becomes a layer.
    pub(super) fn dropped(&mut self, path: PathBuf) {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        match ext.as_deref() {
            Some(DOCUMENT_EXT) => self.request(Pending::OpenPath(path)),
            Some("png") => self.import_path(&path),
            _ => {
                self.file.status = Some(format!(
                    "{} is not a Mirae document or a PNG image",
                    file_name(&path)
                ));
            }
        }
    }
}

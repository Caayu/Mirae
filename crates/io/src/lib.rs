//! Document files and image exchange.
//!
//! **Responsibility:** the `.mirae` document format (v1: a ZIP container
//! with a JSON manifest and raw tiles; [`format`](mod@format), docs/formats/mirae-v1.md,
//! ADR 0019), saving it atomically and loading it with explicit
//! [`Limits`], PNG export of the composite and PNG import as a new layer's
//! pixels. **Does not:** know about the UI, dialogs or the app's state; it
//! never changes a document — loading builds a new one through
//! [`tela_document::Document::from_parts`], so it cannot construct an
//! invalid document. **Depends on:** document, canvas, color, serde,
//! serde_json, png, zip. **Used by:** app.
//!
//! Every input file is untrusted: malformed data gives an [`IoError`],
//! never a panic. The public reading functions also catch panics from the
//! decoders they use and report them as [`IoError::Internal`] (defence in
//! depth; the tests drive the unguarded paths with hostile input).

mod atomic;
mod document;
mod error;
pub mod format;
mod limits;
mod manifest;
mod png_export;
mod png_import;
mod zipcheck;

pub use document::{
    Compression, Loaded, SaveOptions, load_document, read_document, save_document, write_document,
};
pub use error::{IoError, Warning};
pub use format::{FORMAT_VERSION, MIMETYPE};
pub use limits::Limits;
pub use png_export::{PngBackground, PngBits, PngExport, export_png, write_png};
pub use png_import::{ImportedImage, import_png, read_png};

/// Runs `f`, turning a panic inside it into [`IoError::Internal`].
fn guarded<T>(f: impl FnOnce() -> Result<T, IoError>) -> Result<T, IoError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown".to_owned());
        Err(IoError::Internal(what))
    })
}

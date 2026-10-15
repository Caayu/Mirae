use std::fmt;

use tela_document::PartsError;

/// Why a file could not be read or written. Nothing is half-done when an
/// operation returns one: a failed load builds no document, a failed save
/// leaves the previous file as it was.
#[derive(Debug)]
pub enum IoError {
    /// The operating system refused a read or write (missing file,
    /// permissions, disk full, …).
    Filesystem(std::io::Error),
    /// The file is not a Mirae document: not a ZIP archive, or no Mirae
    /// identification (`mimetype`) in it.
    NotADocument,
    /// A ZIP archive Mirae does not accept: damaged structure, duplicate
    /// or unsafe entry names, encryption, unknown compression, entries the
    /// manifest does not account for.
    MalformedArchive(String),
    /// The document was written by a newer format version (`found`) than
    /// this build reads (`supported`). Nothing is loaded rather than
    /// loading part of it.
    UnsupportedVersion { found: u64, supported: u32 },
    /// The manifest is missing, not JSON, or a field is missing or has the
    /// wrong type or an impossible value. The message names the field.
    InvalidManifest(String),
    /// The manifest is well formed but describes a document that cannot
    /// exist (duplicate ids, missing active layer, …).
    InvalidDocument(PartsError),
    /// A tile's entry is missing, has the wrong size or fails its checksum.
    CorruptTile { entry: String, problem: String },
    /// The file declares more than the configured [`Limits`](crate::Limits)
    /// allow (checked before the data is read).
    LimitExceeded {
        what: &'static str,
        limit: u64,
        found: u64,
    },
    /// Valid input this version cannot represent faithfully (e.g. a blend
    /// mode it does not know, a transparent export it cannot compose).
    Unsupported(String),
    /// A PNG image that cannot be decoded or encoded.
    InvalidImage(String),
    /// A decoder stopped unexpectedly (a bug, reported instead of
    /// crashing); nothing was loaded.
    Internal(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::Filesystem(e) => write!(f, "file system error: {e}"),
            IoError::NotADocument => f.write_str("the file is not a Mirae document"),
            IoError::MalformedArchive(m) => write!(f, "the document file is damaged: {m}"),
            IoError::UnsupportedVersion { found, supported } => write!(
                f,
                "the document was saved by a newer version of Mirae (format {found}; \
                 this version reads up to {supported})"
            ),
            IoError::InvalidManifest(m) => write!(f, "the document description is invalid: {m}"),
            IoError::InvalidDocument(e) => write!(f, "the document is inconsistent: {e}"),
            IoError::CorruptTile { entry, problem } => {
                write!(
                    f,
                    "the document's pixel data is damaged ({entry}: {problem})"
                )
            }
            IoError::LimitExceeded { what, limit, found } => {
                write!(f, "too large: {found} {what} (the limit is {limit})")
            }
            IoError::Unsupported(m) => write!(f, "not supported: {m}"),
            IoError::InvalidImage(m) => write!(f, "invalid image: {m}"),
            IoError::Internal(m) => write!(f, "internal error while reading the file: {m}"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IoError::Filesystem(e) => Some(e),
            IoError::InvalidDocument(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::Filesystem(e)
    }
}

/// Something a successful load or import did not take over exactly, for
/// the app to show or log. A warning never hides a change of appearance:
/// those are errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// A manifest field this version does not know (written by a newer
    /// compatible writer), ignored. The path names it, e.g. `layers[2].tag`.
    UnknownField(String),
    /// An archive entry outside the document's data, ignored.
    UnknownEntry(String),
    /// The image carries colour-space information (an ICC profile, gamma
    /// or chromaticities); Mirae has no colour management and reads its
    /// values as sRGB.
    ColourSpaceIgnored,
    /// A 16-bit image was rounded to the nearest 8-bit level for an 8-bit
    /// document (a 16-bit document keeps all 16 bits).
    ReducedTo8Bits,
    /// An animated image: only its first frame was imported.
    FirstFrameOnly,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::UnknownField(p) => write!(f, "unknown field `{p}` ignored"),
            Warning::UnknownEntry(n) => write!(f, "unknown entry `{n}` ignored"),
            Warning::ColourSpaceIgnored => {
                f.write_str("the image's colour profile was ignored (read as sRGB)")
            }
            Warning::ReducedTo8Bits => f.write_str(
                "the 16-bit image was rounded to 8 bits; convert the document to 16-bit \
                 before importing to keep them",
            ),
            Warning::FirstFrameOnly => f.write_str("only the first frame of the animation"),
        }
    }
}

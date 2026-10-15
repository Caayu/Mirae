use std::fmt;

use tela_color::PixelDepth;

use crate::LayerId;

/// Why a layer operation was refused. A refused operation leaves the
/// document unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerError {
    /// No layer with this id exists (any more) in this document.
    NotFound(LayerId),
    /// The document's only layer cannot be removed.
    LastLayer,
    /// The layer is locked against this change.
    Locked(LayerId),
    /// Hidden layers cannot be painted on.
    Hidden(LayerId),
    /// Opacity must be a finite number.
    InvalidOpacity,
    /// Every layer id has been used once (ids are never reused).
    IdsExhausted,
    /// The operation needs a layer mask and the layer has none.
    NoMask(LayerId),
    /// The layer already has a mask (one per layer).
    HasMask(LayerId),
    /// A disabled mask cannot be painted on (reference behaviour).
    MaskDisabled(LayerId),
    /// A pixel edit was written for another depth than the document's
    /// (`document` is the depth the document stores).
    WrongDepth {
        document: PixelDepth,
        edit: PixelDepth,
    },
}

impl fmt::Display for LayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayerError::NotFound(id) => write!(f, "{id} does not exist"),
            LayerError::LastLayer => f.write_str("the last layer cannot be removed"),
            LayerError::Locked(id) => write!(f, "{id} is locked"),
            LayerError::Hidden(id) => write!(f, "{id} is hidden"),
            LayerError::InvalidOpacity => f.write_str("opacity must be a finite number"),
            LayerError::IdsExhausted => f.write_str("no layer ids left in this document"),
            LayerError::NoMask(id) => write!(f, "{id} has no mask"),
            LayerError::HasMask(id) => write!(f, "{id} already has a mask"),
            LayerError::MaskDisabled(id) => write!(f, "the mask of {id} is disabled"),
            LayerError::WrongDepth { document, edit } => write!(
                f,
                "the document stores {}-bit pixels, not {}-bit",
                document.bits(),
                edit.bits()
            ),
        }
    }
}

impl std::error::Error for LayerError {}

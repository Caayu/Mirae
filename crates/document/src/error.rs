use std::fmt;

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
        }
    }
}

impl std::error::Error for LayerError {}

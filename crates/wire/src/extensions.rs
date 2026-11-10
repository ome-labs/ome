//! Room for additions: a list of extensions at the end of the envelope and of every record that is stored or re-sent
//! on its own, inside the signed bytes. A later release adds a field as an extension instead of a new protocol
//! version: a reader ignores an extension it doesn't know unless the writer marked it critical, in which case the
//! record is refused (it can't be understood without it).
//!
//! The extensions are canonical like everything else: ids strictly increasing, at most [`MAX_EXTENSIONS`] of at most
//! [`MAX_EXTENSION_BYTES`] each.

use serde::{Deserialize, Serialize};

use crate::{
    Error,
    limits::{MAX_EXTENSION_BYTES, MAX_EXTENSIONS},
};

/// One extension: an id assigned in `ome-wire`, whether a reader that doesn't know it must refuse the record, and its
/// canonical bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Extension {
    /// Assigned in this crate, never reused.
    pub id: u16,
    /// A reader that doesn't know `id` refuses the record instead of ignoring the extension.
    pub critical: bool,
    /// The extension's own canonical encoding.
    pub bytes: Vec<u8>,
}

/// The extensions of one record. Empty in everything this release writes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Extensions(pub Vec<Extension>);

/// The extension ids this build understands. None yet.
pub const KNOWN: &[u16] = &[];

impl Extensions {
    /// No extensions.
    #[must_use]
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    /// The extension `id`, if present.
    #[must_use]
    pub fn get(&self, id: u16) -> Option<&Extension> {
        self.0.iter().find(|e| e.id == id)
    }

    /// Bounds, canonical order, and no critical extension this build doesn't know.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] (`extensions`), [`Error::UnknownCriticalExtension`].
    pub fn validate(&self) -> Result<(), Error> {
        let out = Error::OutOfBounds {
            field: "extensions",
        };
        if self.0.len() > MAX_EXTENSIONS
            || self.0.iter().any(|e| e.bytes.len() > MAX_EXTENSION_BYTES)
            || self.0.windows(2).any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(out);
        }
        match self.0.iter().find(|e| e.critical && !KNOWN.contains(&e.id)) {
            Some(unknown) => Err(Error::UnknownCriticalExtension(unknown.id)),
            None => Ok(()),
        }
    }
}

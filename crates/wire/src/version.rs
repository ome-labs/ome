//! Protocol versions and how parties agree on one.
//!
//! Every signed message carries the protocol version it was written in, and decoding accepts any version in
//! [`SUPPORTED`]. A vault's manifest pins the version its members agreed on when it was founded; founders agree with
//! [`negotiate`] over the ranges their keepers advertise. A release that changes a wire format or a domain tag adds a
//! version, keeps speaking the old one while members upgrade, and moves a vault only once every member supports it.

use serde::{Deserialize, Serialize};

use crate::Error;

/// The version this build writes when nothing else is agreed.
pub const PROTOCOL_VERSION: u16 = 0;

/// Every version this build reads and writes. The crate's own tests add a test-only version 1 (`encoding`).
pub const SUPPORTED: VersionRange = VersionRange {
    min: 0,
    max: if cfg!(test) { 1 } else { PROTOCOL_VERSION },
};

/// An inclusive range of protocol versions a keeper speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionRange {
    /// Oldest version.
    pub min: u16,
    /// Newest version.
    pub max: u16,
}

impl VersionRange {
    /// Whether `version` is in the range.
    #[must_use]
    pub const fn contains(&self, version: u16) -> bool {
        self.min <= version && version <= self.max
    }

    /// Structural check: `min <= max`.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] (`versions`).
    pub const fn validate(&self) -> Result<(), Error> {
        if self.min <= self.max {
            Ok(())
        } else {
            Err(Error::OutOfBounds { field: "versions" })
        }
    }
}

/// Accepts `version` only if this build speaks it.
///
/// # Errors
/// [`Error::UnsupportedVersion`].
pub const fn require_supported(version: u16) -> Result<(), Error> {
    if SUPPORTED.contains(version) {
        Ok(())
    } else {
        Err(Error::UnsupportedVersion(version))
    }
}

/// The newest version every range contains. Pass this build's [`SUPPORTED`] among the ranges.
///
/// # Errors
/// [`Error::NoCommonVersion`] if the ranges do not overlap (or there are none), [`Error::OutOfBounds`] for an
/// inverted range.
pub fn negotiate<'a>(ranges: impl IntoIterator<Item = &'a VersionRange>) -> Result<u16, Error> {
    let mut common: Option<VersionRange> = None;
    for range in ranges {
        range.validate()?;
        common = Some(match common {
            None => *range,
            Some(c) => VersionRange {
                min: c.min.max(range.min),
                max: c.max.min(range.max),
            },
        });
    }
    match common {
        Some(c) if c.min <= c.max => Ok(c.max),
        _ => Err(Error::NoCommonVersion),
    }
}

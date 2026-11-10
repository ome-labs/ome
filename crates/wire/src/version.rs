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

/// What a keeper that never sent its capabilities speaks: the first version.
pub const FIRST: VersionRange = VersionRange { min: 0, max: 0 };

/// The version a vault's keepers write in: the newest that this build and every member speaks. A member whose
/// capabilities haven't arrived counts as [`FIRST`], so the vault moves only once every member can read the new
/// version.
///
/// # Errors
/// [`Error::NoCommonVersion`] when no version is shared (a member is past this build's oldest version).
pub fn vault_version(
    members: impl IntoIterator<Item = Option<VersionRange>>,
) -> Result<u16, Error> {
    let ranges: Vec<VersionRange> = members
        .into_iter()
        .map(|range| range.unwrap_or(FIRST))
        .chain([SUPPORTED])
        .collect();
    negotiate(&ranges)
}

/// The members, by `key`, that keep a vault from writing `needed`: those that don't speak it (or never said).
pub fn holding_back<K: Clone>(needed: u16, members: &[(K, Option<VersionRange>)]) -> Vec<K> {
    members
        .iter()
        .filter(|(_, range)| !range.unwrap_or(FIRST).contains(needed))
        .map(|(key, _)| key.clone())
        .collect()
}

/// The body of a pasted string `text` with `prefix` (`ome-invite-v0:`). A string of the same kind in a newer format
/// (`ome-invite-v1:`) is refused as needing a newer release, anything else as malformed.
///
/// # Errors
/// [`Error::NewerFormat`] or [`Error::Malformed`].
pub fn strip_prefix<'a>(text: &'a str, prefix: &str, what: &'static str) -> Result<&'a str, Error> {
    if let Some(body) = text.strip_prefix(prefix) {
        return Ok(body);
    }
    // `ome-invite-v0:` → `ome-invite-v`, then the version up to `:`.
    let stem = prefix
        .trim_end_matches(':')
        .trim_end_matches(|c: char| c.is_ascii_digit());
    let newer = text
        .strip_prefix(stem)
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(version, _)| version.parse::<u16>().ok());
    match newer {
        Some(version) => Err(Error::NewerFormat { what, version }),
        None => Err(Error::Malformed),
    }
}

use ome_crypto::{Base, Point, poseidon2};

use super::{check_ordered, constants::SIGNER_SET_TAG, count, padded, tag};
use crate::Error;

/// The ordered signer keys. Slot `i` is member `i + 1`; approvals are matched to keys by slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignerSet {
    keys: Vec<Point>,
}

impl SignerSet {
    /// A signer set of `1..=8` distinct keys, in slot order.
    ///
    /// # Errors
    /// [`Error::MemberCount`] or [`Error::DuplicateKey`].
    pub fn new(keys: Vec<Point>) -> Result<Self, Error> {
        check_ordered(&keys)?;
        Ok(Self { keys })
    }

    /// Keys in slot order.
    #[must_use]
    pub fn keys(&self) -> &[Point] {
        &self.keys
    }

    /// Number of signers `n`.
    #[must_use]
    pub fn count(&self) -> u32 {
        count(&self.keys)
    }

    /// The key in `slot`.
    #[must_use]
    pub fn key(&self, slot: u32) -> Option<&Point> {
        self.keys.get(usize::try_from(slot).ok()?)
    }

    /// `Poseidon2("ome.signers.v0", [x_0, y_0, ..., x_7, y_7])` with empty slots zero.
    #[must_use]
    pub fn commitment(&self) -> Base {
        poseidon2::hash_with_separator(tag(SIGNER_SET_TAG), &padded(&self.keys))
    }

    /// Members of `self` whose key is absent from `next`.
    #[must_use]
    pub fn removed_in(&self, next: &Self) -> u32 {
        let removed = self
            .keys
            .iter()
            .filter(|key| !next.keys.contains(key))
            .count();
        u32::try_from(removed).unwrap_or(u32::MAX)
    }
}

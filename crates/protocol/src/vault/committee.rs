use ome_crypto::{Base, Point, poseidon2, threshold::shamir::MemberIndex};

use super::{
    check_ordered,
    constants::{COMMITTEE_TAG, MIN_VIEWING_THRESHOLD},
    count, padded, tag,
};
use crate::Error;

/// The viewing committee: threshold `t_v` and each member's verification share `Y_i`, in member order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Committee {
    viewing_threshold: u32,
    shares: Vec<Point>,
}

impl Committee {
    /// A committee of `1..=8` distinct shares with `2 <= t_v <= n`.
    ///
    /// # Errors
    /// [`Error::MemberCount`], [`Error::DuplicateKey`] or [`Error::ViewingThreshold`].
    pub fn new(viewing_threshold: u32, shares: Vec<Point>) -> Result<Self, Error> {
        check_ordered(&shares)?;
        let members = count(&shares);
        if !(MIN_VIEWING_THRESHOLD..=members).contains(&viewing_threshold) {
            return Err(Error::ViewingThreshold {
                threshold: viewing_threshold,
                members,
            });
        }
        Ok(Self {
            viewing_threshold,
            shares,
        })
    }

    /// Viewing threshold `t_v`.
    #[must_use]
    pub fn viewing_threshold(&self) -> u32 {
        self.viewing_threshold
    }

    /// Verification shares in member order.
    #[must_use]
    pub fn shares(&self) -> &[Point] {
        &self.shares
    }

    /// Number of members.
    #[must_use]
    pub fn count(&self) -> u32 {
        count(&self.shares)
    }

    /// `(index, Y_index)` for every member, as interpolation takes them.
    #[must_use]
    pub fn evaluations(&self) -> Vec<(MemberIndex, Point)> {
        (1..).zip(self.shares.iter().copied()).collect()
    }

    /// `Poseidon2("ome.committee.v0", [t_v, Y_0.x, Y_0.y, ..., Y_7.y])` with empty slots zero.
    #[must_use]
    pub fn commitment(&self) -> Base {
        let mut inputs = vec![Base::from(self.viewing_threshold)];
        inputs.extend_from_slice(&padded(&self.shares));
        poseidon2::hash_with_separator(tag(COMMITTEE_TAG), &inputs)
    }
}

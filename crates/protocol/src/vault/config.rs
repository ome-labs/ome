use ome_crypto::Base;

use super::{Committee, SignerSet};
use crate::Error;

/// The vault's config note and the transitions the contract allows (`governance.nr`). Checking a transition here
/// before proposing it gives a member the contract's answer without paying for a failed transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Spending threshold `k`.
    pub threshold: u32,
    /// Signer count `n`.
    pub signer_count: u32,
    /// Increases on every change; bound into every approval.
    pub version: u32,
    /// Commitment to the ordered signer keys.
    pub signer_set_commitment: Base,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// Commitment to the viewing committee.
    pub committee_commitment: Base,
}

impl Config {
    /// The config the constructor stores, at version 0.
    ///
    /// # Errors
    /// [`Error::Threshold`], or [`Error::MemberCount`] if the committee and signer set differ in size.
    pub fn genesis(
        signers: &SignerSet,
        threshold: u32,
        committee: &Committee,
    ) -> Result<Self, Error> {
        Self::checked(signers, threshold, committee, 0)
    }

    /// Same policy, next version: voids every approval collected so far (`invalidate_pending`).
    #[must_use]
    pub fn bump_version(self) -> Self {
        Self {
            version: self.version + 1,
            ..self
        }
    }

    /// New spending threshold, next version (`set_threshold`).
    ///
    /// # Errors
    /// [`Error::Threshold`].
    pub fn with_threshold(self, threshold: u32) -> Result<Self, Error> {
        check_threshold(threshold, self.signer_count)?;
        Ok(Self {
            threshold,
            version: self.version + 1,
            ..self
        })
    }

    /// New signer set, thresholds and committee as one change, next version (`rotate_membership`).
    ///
    /// # Errors
    /// [`Error::SignerSetMismatch`] if `current` is not the stored set, [`Error::TooManyRemoved`] if `t_v` or more
    /// current members leave, threshold and size errors.
    pub fn rotate(
        self,
        current: &SignerSet,
        next: &SignerSet,
        threshold: u32,
        committee: &Committee,
    ) -> Result<Self, Error> {
        if current.commitment() != self.signer_set_commitment
            || current.count() != self.signer_count
        {
            return Err(Error::SignerSetMismatch);
        }
        let removed = current.removed_in(next);
        if removed >= self.viewing_threshold {
            return Err(Error::TooManyRemoved {
                removed,
                allowed: self.viewing_threshold - 1,
            });
        }
        Self::checked(next, threshold, committee, self.version + 1)
    }

    fn checked(
        signers: &SignerSet,
        threshold: u32,
        committee: &Committee,
        version: u32,
    ) -> Result<Self, Error> {
        check_threshold(threshold, signers.count())?;
        if committee.count() != signers.count() {
            return Err(Error::MemberCount(
                usize::try_from(committee.count()).unwrap_or(usize::MAX),
            ));
        }
        Ok(Self {
            threshold,
            signer_count: signers.count(),
            version,
            signer_set_commitment: signers.commitment(),
            viewing_threshold: committee.viewing_threshold(),
            committee_commitment: committee.commitment(),
        })
    }
}

fn check_threshold(threshold: u32, members: u32) -> Result<(), Error> {
    if (1..=members).contains(&threshold) {
        Ok(())
    } else {
        Err(Error::Threshold { threshold, members })
    }
}

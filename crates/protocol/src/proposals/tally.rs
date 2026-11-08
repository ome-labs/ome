use std::collections::{BTreeMap, BTreeSet};

use ome_crypto::schnorr;
use ome_wire::messages::proposals::{Approval, Proposal, Rejection};

use super::{verify_approval, verify_rejection};
use crate::{Error, membership::Vault, vault::Witness};

/// Where a proposal stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Still collecting votes.
    Pending {
        /// Valid approvals so far.
        approvals: u32,
        /// Valid rejections so far.
        rejections: u32,
        /// The spending threshold `k`.
        needed: u32,
    },
    /// At least `k` members approved; the witness is ready.
    Approved,
    /// More than `n - k` members rejected, so `k` approvals can no longer be reached. It stays blocked until it is
    /// cancelled or the config changes.
    Blocked,
    /// The deadline passed before execution.
    Expired,
}

/// The votes on one proposal. Each slot has one vote: the first valid approval or rejection from it counts, and later
/// votes from the same slot are ignored.
pub struct Tally<'a> {
    vault: &'a Vault,
    proposal: &'a Proposal,
    id: [u8; 32],
    approvals: BTreeMap<u32, schnorr::Signature>,
    rejections: BTreeSet<u32>,
}

impl<'a> Tally<'a> {
    /// An empty tally for `proposal` on `vault`.
    ///
    /// # Errors
    /// [`ome_wire::Error::Malformed`] if the proposal cannot be encoded.
    pub fn new(vault: &'a Vault, proposal: &'a Proposal) -> Result<Self, Error> {
        Ok(Self {
            vault,
            proposal,
            id: proposal.id()?,
            approvals: BTreeMap::new(),
            rejections: BTreeSet::new(),
        })
    }

    /// Verifies and records an approval. Returns whether it counted (`false` if the slot already voted).
    ///
    /// # Errors
    /// Verification errors from [`verify_approval`].
    pub fn add_approval(&mut self, approval: &Approval) -> Result<bool, Error> {
        verify_approval(self.vault, self.proposal, approval)?;
        if self.voted(approval.slot) {
            return Ok(false);
        }
        let signature = schnorr::Signature::try_from(&approval.signature)?;
        self.approvals.insert(approval.slot, signature);
        Ok(true)
    }

    /// Verifies and records a rejection. Returns whether it counted (`false` if the slot already voted).
    ///
    /// # Errors
    /// Verification errors from [`verify_rejection`].
    pub fn add_rejection(&mut self, rejection: &Rejection) -> Result<bool, Error> {
        verify_rejection(self.vault, &self.id, rejection)?;
        if self.voted(rejection.slot) {
            return Ok(false);
        }
        self.rejections.insert(rejection.slot);
        Ok(true)
    }

    /// The proposal's status at unix time `now`.
    #[must_use]
    pub fn status(&self, now: u64) -> Status {
        let needed = self.vault.config().threshold;
        let members = self.vault.signers().count();
        let approvals = count(self.approvals.len());
        let rejections = count(self.rejections.len());
        if now > self.proposal.expires_at {
            Status::Expired
        } else if approvals >= needed {
            Status::Approved
        } else if rejections > members - needed {
            Status::Blocked
        } else {
            Status::Pending {
                approvals,
                rejections,
                needed,
            }
        }
    }

    /// The auth witness once approved: the `k` lowest-slot approvals (enough for the contract, and the same witness
    /// whichever member builds it).
    #[must_use]
    pub fn witness(&self) -> Option<Witness> {
        let needed = usize::try_from(self.vault.config().threshold).ok()?;
        if self.approvals.len() < needed {
            return None;
        }
        let signers = self.vault.signers();
        let mut signatures = vec![None; signers.keys().len()];
        for (slot, signature) in self.approvals.iter().take(needed) {
            signatures[usize::try_from(*slot).ok()?] = Some(*signature);
        }
        Some(Witness::encode(
            signers,
            &signatures,
            self.proposal.expires_at,
        ))
    }

    fn voted(&self, slot: u32) -> bool {
        self.approvals.contains_key(&slot) || self.rejections.contains(&slot)
    }
}

fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

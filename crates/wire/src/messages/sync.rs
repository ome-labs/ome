//! Catching up after being offline.

use serde::{Deserialize, Serialize};

use super::bounded;
use crate::{Error, limits::MAX_SYNC_ITEMS};

/// An approval or rejection a member holds, by proposal and slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VoteRef {
    /// Proposal id.
    pub proposal: [u8; 32],
    /// Signer slot.
    pub slot: u32,
}

/// What a member already holds for a vault; peers answer with the records it lacks. Missing payloads are fetched
/// separately by hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Proposal ids.
    pub proposals: Vec<[u8; 32]>,
    /// Approvals.
    pub approvals: Vec<VoteRef>,
    /// Rejections.
    pub rejections: Vec<VoteRef>,
    /// Executed proposal ids.
    pub receipts: Vec<[u8; 32]>,
}

impl SyncRequest {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("proposals", self.proposals.len(), 0, MAX_SYNC_ITEMS)?;
        bounded("approvals", self.approvals.len(), 0, MAX_SYNC_ITEMS)?;
        bounded("rejections", self.rejections.len(), 0, MAX_SYNC_ITEMS)?;
        bounded("receipts", self.receipts.len(), 0, MAX_SYNC_ITEMS)
    }
}

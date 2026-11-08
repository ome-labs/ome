//! Spending proposals and the votes on them.
//!
//! A [`Proposal`] carries what members sign and review, plus a [`PayloadRef`]: the hash and length of the Aztec
//! execution request it would run. The payload itself travels on its own as a [`ProposalPayload`], fetched by hash
//! with a [`PayloadRequest`]. Proposals stay small enough to gossip, sync and relay cheaply, and a member that already
//! holds a payload never downloads it twice. Before signing, a member's daemon fetches the payload, checks it against
//! the reference, and recomputes `outer_hash` from it.
//!
//! Checking an approval or rejection signature needs the approval message, which is a protocol rule; it lives in
//! `ome-protocol`.

use ome_crypto::{schnorr, threshold::shamir::MemberIndex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::bounded;
use crate::{
    Error,
    codec::canonical,
    limits::{MAX_PAYLOAD_BYTES, MAX_PAYLOAD_REQUESTS, MAX_REASON_BYTES, MAX_SUMMARY_BYTES},
    types::{WireField, WireSignature},
};

/// Domain tag of proposal ids.
pub const PROPOSAL_DOMAIN: &[u8] = b"ome.proposal.v0";
/// Domain tag of payload hashes.
pub const PAYLOAD_DOMAIN: &[u8] = b"ome.proposal.payload.v0";

/// A proposal's payload, by hash and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PayloadRef {
    /// [`ProposalPayload::hash`].
    pub hash: [u8; 32],
    /// Payload length in bytes, so a receiver can refuse an oversized payload before fetching it.
    pub len: u32,
}

impl PayloadRef {
    /// Whether `payload` is the one referenced.
    #[must_use]
    pub fn matches(&self, payload: &ProposalPayload) -> bool {
        u32::try_from(payload.bytes.len()).ok() == Some(self.len) && payload.hash() == self.hash
    }
}

/// A spending proposal. Its id is derived from its content, so an approval cannot point at a different proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// Config version the approvals are collected under.
    pub config_version: u32,
    /// The hash members approve: the entrypoint payload hash, which each approver recomputes from the payload.
    pub outer_hash: WireField,
    /// Approval deadline (unix seconds).
    pub expires_at: u64,
    /// Entrypoint `tx_nonce`; cancelling the proposal burns it.
    pub tx_nonce: WireField,
    /// Fee payment method passed to `entrypoint`.
    pub fee_method: u8,
    /// The execution request this proposal would run.
    pub payload: PayloadRef,
    /// Human-readable summary of the calls.
    pub summary: String,
    /// Proposing member.
    pub proposer: MemberIndex,
}

impl Proposal {
    /// `SHA-256("ome.proposal.v0" ‖ canonical(proposal))`.
    ///
    /// # Errors
    /// [`Error::Malformed`] if the proposal cannot be encoded.
    pub fn id(&self) -> Result<[u8; 32], Error> {
        Ok(tagged_hash(PROPOSAL_DOMAIN, &canonical(self)?))
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "payload.len",
            usize::try_from(self.payload.len).unwrap_or(usize::MAX),
            1,
            MAX_PAYLOAD_BYTES,
        )?;
        bounded("summary", self.summary.len(), 0, MAX_SUMMARY_BYTES)
    }
}

/// The opaque Aztec execution request of a proposal, serialised by the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalPayload {
    /// Payload bytes.
    pub bytes: Vec<u8>,
}

impl ProposalPayload {
    /// `SHA-256("ome.proposal.payload.v0" ‖ bytes)`.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        tagged_hash(PAYLOAD_DOMAIN, &self.bytes)
    }

    /// The reference a proposal carries for this payload.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] above [`MAX_PAYLOAD_BYTES`].
    pub fn reference(&self) -> Result<PayloadRef, Error> {
        self.validate()?;
        let len =
            u32::try_from(self.bytes.len()).map_err(|_| Error::OutOfBounds { field: "payload" })?;
        Ok(PayloadRef {
            hash: self.hash(),
            len,
        })
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("payload", self.bytes.len(), 1, MAX_PAYLOAD_BYTES)
    }
}

/// Asks a peer for payloads by hash; it answers with one [`ProposalPayload`] message per payload it holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadRequest {
    /// Payload hashes.
    pub hashes: Vec<[u8; 32]>,
}

impl PayloadRequest {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("hashes", self.hashes.len(), 1, MAX_PAYLOAD_REQUESTS)
    }
}

/// A member's approval: its Schnorr signature over the proposal's approval message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    /// Approved proposal id.
    pub proposal: [u8; 32],
    /// Signer slot (member index - 1).
    pub slot: u32,
    /// The signature as contract limbs.
    pub signature: WireSignature,
}

impl Approval {
    /// Wraps a signature.
    #[must_use]
    pub fn new(proposal: [u8; 32], slot: u32, signature: &schnorr::Signature) -> Self {
        Self {
            proposal,
            slot,
            signature: WireSignature::from(signature),
        }
    }
}

/// A member's vote against a proposal, signed by the slot's signer. Not a veto (the contract counts approvals only):
/// once more than `n - k` members reject, the threshold can no longer be reached and the proposal is blocked until it
/// is cancelled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    /// Rejected proposal id.
    pub proposal: [u8; 32],
    /// Signer slot (member index - 1).
    pub slot: u32,
    /// The member's reason, shown to the other members.
    pub reason: String,
    /// The signature as contract limbs.
    pub signature: WireSignature,
}

impl Rejection {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("reason", self.reason.len(), 0, MAX_REASON_BYTES)
    }
}

/// A proposal was executed in `tx_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Executed proposal id.
    pub proposal: [u8; 32],
    /// Aztec transaction hash.
    pub tx_hash: WireField,
}

fn tagged_hash(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

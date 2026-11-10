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
    Error, Extensions, IdentityPublic, KeeperIdentity,
    codec::canonical,
    limits::{
        MAX_AUTHWITS, MAX_PAYLOAD_BYTES, MAX_PAYLOAD_REQUESTS, MAX_REASON_BYTES, MAX_SUMMARY_BYTES,
    },
    types::{Bytes64, WireField, WireSignature},
};

/// Domain tag of proposal ids.
pub const PROPOSAL_DOMAIN: &[u8] = b"ome.proposal.v0";
/// Domain tag of payload hashes.
pub const PAYLOAD_DOMAIN: &[u8] = b"ome.proposal.payload.v0";
/// Domain tag of execution receipt signatures.
pub const RECEIPT_DOMAIN: &str = "ome.receipt.v0";

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
    /// The message hashes of the private authwits the vault grants with this transaction, in payload order. Each
    /// approver recomputes them from the payload, and an approval signs each one.
    pub authwits: Vec<WireField>,
    /// When the proposer made it (unix seconds, the clock `expires_at` uses). Part of the id, so every member shows
    /// the same time.
    pub created_at: u64,
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
    /// Additions a later release may make (none yet).
    pub extensions: Extensions,
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
        if self.created_at > self.expires_at {
            return Err(Error::OutOfBounds {
                field: "created_at",
            });
        }
        bounded(
            "payload.len",
            usize::try_from(self.payload.len).unwrap_or(usize::MAX),
            1,
            MAX_PAYLOAD_BYTES,
        )?;
        self.extensions.validate()?;
        bounded("authwits", self.authwits.len(), 0, MAX_AUTHWITS)?;
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

/// A member's approval: its Schnorr signatures over the proposal's approval messages, the transaction's first and
/// then one per authwit the proposal grants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    /// Approved proposal id.
    pub proposal: [u8; 32],
    /// Signer slot (member index - 1).
    pub slot: u32,
    /// The signatures as contract limbs: the transaction's, then each authwit's in the proposal's order.
    pub signatures: Vec<WireSignature>,
    /// Additions a later release may make (none yet).
    pub extensions: Extensions,
}

impl Approval {
    /// Wraps the signatures, the transaction's first.
    #[must_use]
    pub fn new(proposal: [u8; 32], slot: u32, signatures: &[schnorr::Signature]) -> Self {
        Self {
            proposal,
            slot,
            signatures: signatures.iter().map(WireSignature::from).collect(),
            extensions: Extensions::none(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        self.extensions.validate()?;
        bounded("signatures", self.signatures.len(), 1, 1 + MAX_AUTHWITS)
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
    /// Additions a later release may make (none yet).
    pub extensions: Extensions,
}

impl Rejection {
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.extensions.validate()?;
        bounded("reason", self.reason.len(), 0, MAX_REASON_BYTES)
    }
}

/// A proposal was executed in `tx_hash`, by the member whose keeper is `executor`. The executor's keeper signs it, so
/// a receipt names its executor however it travels (sync re-sends other members' records under its own frames).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Executed proposal id.
    pub proposal: [u8; 32],
    /// Aztec transaction hash.
    pub tx_hash: WireField,
    /// Block that included it.
    pub block: u64,
    /// The executing member's keeper.
    pub executor: IdentityPublic,
    /// Additions a later release may make (none yet), signed with the rest.
    pub extensions: Extensions,
    /// Ed25519 signature by `executor` over the domain, the vault and the fields above.
    pub signature: Bytes64,
}

impl ExecutionReceipt {
    /// A receipt for `vault`, signed by `identity`, the executing member's keeper.
    ///
    /// # Errors
    /// [`Error::Malformed`] if it cannot be encoded.
    pub fn sign(
        identity: &KeeperIdentity,
        vault: [u8; 32],
        proposal: [u8; 32],
        tx_hash: WireField,
        block: u64,
    ) -> Result<Self, Error> {
        let executor = identity.public();
        let extensions = Extensions::none();
        let signature = identity.sign(&canonical(&(
            RECEIPT_DOMAIN,
            vault,
            proposal,
            tx_hash,
            block,
            executor,
            &extensions,
        ))?);
        Ok(Self {
            proposal,
            tx_hash,
            block,
            executor,
            extensions,
            signature,
        })
    }

    /// Checks that `executor` signed this receipt for `vault`.
    ///
    /// # Errors
    /// [`Error::InvalidSignature`].
    pub fn verify(&self, vault: [u8; 32]) -> Result<(), Error> {
        self.executor.verify(
            &canonical(&(
                RECEIPT_DOMAIN,
                vault,
                self.proposal,
                self.tx_hash,
                self.block,
                self.executor,
                &self.extensions,
            ))?,
            &self.signature,
        )
    }
}

/// How far a member's execution of a proposal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaimPhase {
    /// Rebuilding the transaction.
    Rebuilding,
    /// Proving.
    Proving,
    /// Sending.
    Sending,
    /// Included in a block: the claim ends.
    Mined,
    /// Failed: the claim ends.
    Failed,
}

impl ClaimPhase {
    /// Whether the execution is over, so the claim no longer holds.
    #[must_use]
    pub fn is_final(self) -> bool {
        matches!(self, Self::Mined | Self::Failed)
    }
}

/// A member is executing a proposal: other members wait instead of proving the same transaction. Sent by the
/// claimant's keeper on each phase change and never synced; a claim not refreshed for a while lapses. It is a
/// courtesy, not a lock: the proposal's nonce is what lets only one execution land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionClaim {
    /// Proposal being executed.
    pub proposal: [u8; 32],
    /// How far it is.
    pub phase: ClaimPhase,
    /// Unix seconds the execution started.
    pub started_at: u64,
}

fn tagged_hash(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

//! The message catalogue, grouped by conversation. [`Message`] variants are only ever appended within a protocol
//! version: their position is their encoding. Each body has structural bounds checked on encode and decode, and
//! conversions to `ome-crypto` types that validate the cryptographic content.

pub mod artifacts;
pub mod capabilities;
pub mod ceremony;
pub mod device;
pub mod ecdh;
pub mod lobby;
pub mod mailbox;
pub mod names;
pub mod proposals;
pub mod sync;

use serde::{Deserialize, Serialize};

use crate::Error;
use artifacts::{ArtifactChunk, ArtifactManifest, ArtifactRequest};
use capabilities::Capabilities;
use ceremony::{
    Abort, Complaint, Confirm, DkgRound1, Echo, Justification, Manifest, ReshareDeal, SealedShare,
};
use device::{DevicePair, DevicePaired, DeviceReview, DeviceUnpair, DeviceVote, DeviceVoted};
use ecdh::{EcdhRequest, EcdhResponse};
use lobby::{LobbyJoin, LobbyLeave, LobbyPlan, LobbyRemoved, LobbyState, SealedSecrets};
use names::NameChange;
use proposals::{
    Approval, ExecutionClaim, ExecutionReceipt, PayloadRequest, Proposal, ProposalPayload,
    Rejection,
};
use sync::SyncRequest;

/// Every message kind. Append new variants at the end only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// The committee as members know it.
    Manifest(Manifest),
    /// DKG round 1 broadcast.
    DkgRound1(DkgRound1),
    /// DKG round 2 private share.
    DkgRound2(SealedShare),
    /// DKG transcript confirmation.
    DkgConfirm(Confirm),
    /// Reshare dealer commitments.
    ReshareDeal(ReshareDeal),
    /// Reshare private sub-share.
    ReshareSubShare(SealedShare),
    /// Reshare transcript confirmation.
    ReshareConfirm(Confirm),
    /// A member accuses a dealer of a bad share.
    Complaint(Complaint),
    /// The accused reveals the disputed share.
    Justification(Justification),
    /// A ceremony is abandoned.
    Abort(Abort),
    /// A dealer's broadcast forwarded by another participant.
    Echo(Echo),
    /// Threshold ECDH request.
    EcdhRequest(EcdhRequest),
    /// Threshold ECDH partials.
    EcdhResponse(EcdhResponse),
    /// A spending proposal; its payload travels separately.
    Proposal(Proposal),
    /// A request for proposal payloads by hash.
    PayloadRequest(PayloadRequest),
    /// A proposal payload.
    Payload(ProposalPayload),
    /// A member's approval of a proposal.
    Approval(Approval),
    /// A member's signed vote against a proposal.
    Rejection(Rejection),
    /// A proposal was executed.
    ExecutionReceipt(ExecutionReceipt),
    /// What a member holds, so peers send what it lacks.
    SyncRequest(SyncRequest),
    /// A guest asks to join a lobby.
    LobbyJoin(LobbyJoin),
    /// A lobby's roster, from its host.
    LobbyState(LobbyState),
    /// A started lobby's plan, from its host.
    LobbyPlan(LobbyPlan),
    /// A guest leaves a lobby, or the host closes it.
    LobbyLeave(LobbyLeave),
    /// A reshare dealer hands a joining member the vault's shared protocol secrets, sealed to its keeper.
    ReshareSecrets(SealedSecrets),
    /// A signing device asks its member's keeper to pair, answering a pairing offer.
    DevicePair(DevicePair),
    /// The keeper accepted a device's pairing.
    DevicePaired(DevicePaired),
    /// The keeper asks its paired device to review a proposal.
    DeviceReview(Box<DeviceReview>),
    /// A paired device votes on a proposal.
    DeviceVote(DeviceVote),
    /// The keeper's answer to a device's vote.
    DeviceVoted(DeviceVoted),
    /// A device or its keeper ends the pairing.
    DeviceUnpair(DeviceUnpair),
    /// A request for a contract artifact's manifest or chunks.
    ArtifactRequest(ArtifactRequest),
    /// A contract artifact's chunk hashes.
    ArtifactManifest(ArtifactManifest),
    /// One chunk of a contract artifact.
    ArtifactChunk(ArtifactChunk),
    /// A member is executing a proposal.
    ExecutionClaim(ExecutionClaim),
    /// The host removed a guest from its lobby.
    LobbyRemoved(LobbyRemoved),
    /// A member renamed the vault, or themselves.
    NameChange(NameChange),
    /// What a member's keeper speaks in a vault: its protocol versions and release.
    Capabilities(Capabilities),
}

impl Message {
    /// Structural bounds: counts, lengths and ranges. Cryptographic validity is checked by the conversions.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] naming the field, or [`Error::UnsupportedVersion`] for a manifest or plan.
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Manifest(m) => m.validate(),
            Self::DkgRound1(m) => m.validate(),
            Self::ReshareDeal(m) => m.validate(),
            Self::DkgRound2(m) | Self::ReshareSubShare(m) => m.validate(),
            Self::Echo(m) => m.validate(),
            Self::EcdhRequest(m) => m.validate(),
            Self::EcdhResponse(m) => m.validate(),
            Self::Proposal(m) => m.validate(),
            Self::PayloadRequest(m) => m.validate(),
            Self::Payload(m) => m.validate(),
            Self::Rejection(m) => m.validate(),
            Self::SyncRequest(m) => m.validate(),
            Self::LobbyJoin(m) => m.validate(),
            Self::LobbyState(m) => m.validate(),
            Self::LobbyPlan(m) => m.validate(),
            Self::ReshareSecrets(m) => m.validate(),
            Self::DevicePair(m) => m.validate(),
            Self::DevicePaired(m) => m.validate(),
            Self::DeviceReview(m) => m.validate(),
            Self::DeviceVote(m) => m.validate(),
            Self::DeviceVoted(m) => m.validate(),
            Self::DeviceUnpair(m) => m.validate(),
            Self::ArtifactRequest(m) => m.validate(),
            Self::ArtifactManifest(m) => m.validate(),
            Self::ArtifactChunk(m) => m.validate(),
            Self::NameChange(m) => m.validate(),
            Self::Capabilities(m) => m.validate(),
            Self::Approval(m) => m.validate(),
            Self::ExecutionReceipt(m) => m.extensions.validate(),
            Self::DkgConfirm(_)
            | Self::ReshareConfirm(_)
            | Self::Complaint(_)
            | Self::Justification(_)
            | Self::Abort(_)
            | Self::ExecutionClaim(_)
            | Self::LobbyRemoved(_)
            | Self::LobbyLeave(_) => Ok(()),
        }
    }
}

/// `len` must lie in `min..=max`.
pub(crate) fn bounded(
    field: &'static str,
    len: usize,
    min: usize,
    max: usize,
) -> Result<(), Error> {
    if (min..=max).contains(&len) {
        Ok(())
    } else {
        Err(Error::OutOfBounds { field })
    }
}

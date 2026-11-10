//! A member's paired signing device (the phone) and its keeper. The keeper hands out a one-time [`PairOffer`] (shown as
//! a QR code); the device answers with a [`DevicePair`] sealed under the offer's secret, then the two exchange reviews
//! and votes through their pairwise relay mailbox. Every device message is an ordinary signed message: the device
//! signs with its own Ed25519 identity, the keeper with its keeper identity. See `architecture/mobile.md`.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{
    bounded,
    ceremony::{Manifest, addresses},
    proposals::{Approval, Proposal, ProposalPayload, Rejection},
};
use crate::{
    Error, IdentityPublic,
    codec::{from_canonical, to_canonical},
    limits::{
        MAX_ADDRESSES, MAX_INVITE_CHARS, MAX_MEMBERS, MAX_NAME_BYTES, MAX_REASON_BYTES,
        MAX_SUMMARY_BYTES,
    },
    types::{WireField, WirePoint},
    version::{self, VersionRange},
};

/// Prefix of a pairing offer's string form.
pub const PAIR_OFFER_PREFIX: &str = "ome-pair-v0:";

/// Most contract labels a review carries.
pub const MAX_LABELS: usize = 16;

/// Most fee payer contracts a pairing names.
pub const MAX_FEE_PAYERS: usize = 8;

/// The header scope of a pairing's messages: the offer id zero-padded to 32 bytes.
#[must_use]
pub fn pairing_scope(offer: &[u8; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(offer);
    out
}

/// The one-time secret of a pairing offer. Zeroized on drop.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PairSecret(pub [u8; 32]);

/// A keeper's offer to pair a signing device, passed to the device out of band (a QR code). Whoever holds it can pair
/// until it expires or is used. `Debug` omits the secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairOffer {
    /// Offer id.
    pub offer: [u8; 16],
    /// The keeper's identity.
    pub keeper: IdentityPublic,
    /// The keeper's X25519 transport key.
    pub transport: [u8; 32],
    /// Relays both sides use, each ending in `/p2p/<peer id>`.
    pub relays: Vec<String>,
    /// The one-time secret that keys the pairing mailbox.
    pub secret: PairSecret,
    /// Unix time after which the keeper refuses the offer.
    pub expires_at: u64,
    /// The member's display name.
    pub member: String,
    /// Network label set by the daemon (opaque to keepers).
    pub network: String,
}

impl core::fmt::Debug for PairOffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PairOffer")
            .field("offer", &self.offer)
            .field("keeper", &self.keeper)
            .field("relays", &self.relays)
            .field("expires_at", &self.expires_at)
            .field("member", &self.member)
            .field("network", &self.network)
            .finish_non_exhaustive()
    }
}

impl PairOffer {
    /// Structural bounds: at least one relay, names.
    ///
    /// # Errors
    /// [`Error::OutOfBounds`] naming the field.
    pub fn validate(&self) -> Result<(), Error> {
        bounded("relays", self.relays.len(), 1, MAX_ADDRESSES)?;
        addresses(&self.relays)?;
        name("member", &self.member)?;
        name("network", &self.network)
    }

    /// The string form: [`PAIR_OFFER_PREFIX`] followed by base64url (no padding) of the canonical bytes.
    ///
    /// # Errors
    /// Bounds errors or [`Error::TooLarge`].
    pub fn encode(&self) -> Result<String, Error> {
        self.validate()?;
        let text = format!(
            "{PAIR_OFFER_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(to_canonical(self, MAX_INVITE_CHARS)?)
        );
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        Ok(text)
    }

    /// Parses the string form; surrounding whitespace is ignored and only the canonical form is accepted.
    ///
    /// # Errors
    /// [`Error::TooLarge`], [`Error::Malformed`] or bounds errors.
    pub fn decode(text: &str) -> Result<Self, Error> {
        let text = text.trim();
        if text.len() > MAX_INVITE_CHARS {
            return Err(Error::TooLarge(text.len()));
        }
        let body = version::strip_prefix(text, PAIR_OFFER_PREFIX, "pairing offer")?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::Malformed)?;
        let offer: Self = from_canonical(&bytes, MAX_INVITE_CHARS)?;
        offer.validate()?;
        Ok(offer)
    }
}

/// A device asks to pair, sealed into the offer's pairing mailbox and signed by the device's identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePair {
    /// The offer it answers.
    pub offer: [u8; 16],
    /// The device's display name.
    pub name: String,
    /// The device's X25519 transport key.
    pub transport: [u8; 32],
    /// The Grumpkin key the device signs approvals with.
    pub signer_key: WirePoint,
    /// The protocol versions the device's app reads and writes, so its keeper writes to it in one it reads.
    pub versions: VersionRange,
}

impl DevicePair {
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.versions.validate()?;
        name("name", &self.name)
    }
}

/// The keeper accepted a pairing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePaired {
    /// The offer it accepted.
    pub offer: [u8; 16],
    /// The member's display name.
    pub member: String,
    /// Network label.
    pub network: String,
    /// Fee payer contracts the member's own configuration trusts: a rotation may also call these.
    pub fee_payers: Vec<WireField>,
}

impl DevicePaired {
    pub(super) fn validate(&self) -> Result<(), Error> {
        name("member", &self.member)?;
        name("network", &self.network)?;
        bounded("fee_payers", self.fee_payers.len(), 0, MAX_FEE_PAYERS)
    }
}

/// The vault's config note, as the member's daemon read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Spending threshold `k`.
    pub threshold: u32,
    /// Signer count `n`.
    pub signer_count: u32,
    /// Config version.
    pub version: u32,
    /// Commitment to the ordered signer keys.
    pub signer_set_commitment: WireField,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// Commitment to the viewing committee.
    pub committee_commitment: WireField,
}

/// The chain a vault lives on, as its outer hashes bind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceChain {
    /// L1 chain id.
    pub chain_id: WireField,
    /// Rollup protocol version.
    pub version: WireField,
}

/// A contract or account name the member's daemon knows. The device shows it as claimed unless it pinned the same one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLabel {
    /// Address.
    pub address: WireField,
    /// Label.
    pub label: String,
}

/// The rotation the member's keeper reshared, for a `rotate_membership` proposal that changes the committee.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRotation {
    /// New signer keys in slot order.
    pub signers: Vec<WirePoint>,
    /// New spending threshold.
    pub threshold: u32,
    /// New viewing threshold.
    pub viewing_threshold: u32,
    /// The reshared committee's verification shares in member order.
    pub verification_shares: Vec<WirePoint>,
}

/// A proposal for the device to review, with everything it needs to check it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceReview {
    /// Vault address.
    pub vault: WireField,
    /// The vault's display name.
    pub vault_name: String,
    /// The vault's current committee.
    pub manifest: Manifest,
    /// The config note it was read with.
    pub config: DeviceConfig,
    /// The vault's chain.
    pub chain: DeviceChain,
    /// The proposal.
    pub proposal: Proposal,
    /// Its payload.
    pub payload: ProposalPayload,
    /// Names the daemon knows for the addresses involved.
    pub labels: Vec<DeviceLabel>,
    /// The daemon's simulation outcome, when it simulated.
    pub simulation: Option<String>,
    /// For a membership rotation, the rotation this member's keeper reshared.
    pub rotation: Option<DeviceRotation>,
}

impl DeviceReview {
    pub(super) fn validate(&self) -> Result<(), Error> {
        name("vault_name", &self.vault_name)?;
        self.manifest.validate()?;
        self.proposal.validate()?;
        self.payload.validate()?;
        bounded("labels", self.labels.len(), 0, MAX_LABELS)?;
        self.labels
            .iter()
            .try_for_each(|label| name("labels.label", &label.label))?;
        if let Some(simulation) = &self.simulation {
            bounded("simulation", simulation.len(), 0, MAX_SUMMARY_BYTES)?;
        }
        if let Some(rotation) = &self.rotation {
            bounded("rotation.signers", rotation.signers.len(), 1, MAX_MEMBERS)?;
            bounded(
                "rotation.verification_shares",
                rotation.verification_shares.len(),
                1,
                MAX_MEMBERS,
            )?;
        }
        Ok(())
    }
}

/// A device's vote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ballot {
    /// An approval.
    Approval(Approval),
    /// A rejection.
    Rejection(Rejection),
}

impl Ballot {
    /// The proposal voted on.
    #[must_use]
    pub fn proposal(&self) -> [u8; 32] {
        match self {
            Self::Approval(approval) => approval.proposal,
            Self::Rejection(rejection) => rejection.proposal,
        }
    }
}

/// A device votes on one of its member's vault's proposals; the keeper stores and replicates the vote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceVote {
    /// Vault address.
    pub vault: [u8; 32],
    /// The vote.
    pub ballot: Ballot,
}

impl DeviceVote {
    pub(super) fn validate(&self) -> Result<(), Error> {
        match &self.ballot {
            Ballot::Approval(approval) => approval.validate(),
            Ballot::Rejection(rejection) => rejection.validate(),
        }
    }
}

/// The keeper's answer to a vote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceVoted {
    /// The proposal voted on.
    pub proposal: [u8; 32],
    /// Why the keeper refused the vote; `None` when it stored and replicated it.
    pub refused: Option<String>,
}

impl DeviceVoted {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if let Some(reason) = &self.refused {
            bounded("refused", reason.len(), 0, MAX_REASON_BYTES)?;
        }
        Ok(())
    }
}

/// Either side ends the pairing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceUnpair {
    /// Why.
    pub reason: String,
}

impl DeviceUnpair {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded("reason", self.reason.len(), 0, MAX_REASON_BYTES)
    }
}

fn name(field: &'static str, value: &str) -> Result<(), Error> {
    bounded(field, value.len(), 0, MAX_NAME_BYTES)
}

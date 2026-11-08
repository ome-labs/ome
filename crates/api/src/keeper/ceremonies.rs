//! DKG and reshare ceremonies, and threshold ECDH.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{KeeperContact, Manifest};
use crate::{Hex32, Hex64, Point};

/// A member of a reshare's new committee.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NewMember {
    /// Keeper identity.
    pub identity: Hex32,
    /// Keeper transport key.
    pub transport: Hex32,
    /// Keeper addresses.
    pub addresses: Vec<String>,
    /// Approval signing key.
    pub signer_key: Point,
}

/// Which ceremony.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CeremonyKind {
    /// Founding DKG.
    Dkg,
    /// Membership reshare.
    Reshare,
}

/// Where a ceremony is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CeremonyStatus {
    /// Still running.
    Running,
    /// Finished; a reshare's committee is pending until the chain shows it.
    Completed,
    /// Stopped; see the failure.
    Aborted,
    /// A reshare's committee became active.
    Activated,
    /// A reshare's committee never became active; its pending share was destroyed.
    Expired,
}

/// Why a ceremony stopped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Failure {
    /// Reason.
    pub reason: String,
    /// Identities of the participants held responsible, if any.
    pub blamed: Vec<String>,
}

/// A finished DKG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DkgResult {
    /// This keeper's member index.
    pub index: u32,
    /// Viewing threshold.
    pub threshold: u32,
    /// Group key `Ivpk`.
    pub group_key: Point,
    /// Verification shares in member order.
    pub verification_shares: Vec<Point>,
    /// Transcript hash every member must agree on.
    pub transcript: Hex64,
}

/// A finished reshare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReshareResult {
    /// The new committee's commitment, for `rotate_membership`.
    pub committee_commitment: Hex32,
    /// This keeper's index in the new committee, if it stays.
    pub new_index: Option<u32>,
    /// The new manifest, if this keeper stays.
    pub manifest: Option<Manifest>,
    /// Transcript hash, if this keeper stays.
    pub transcript: Option<Hex64>,
}

/// A ceremony's outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum CeremonyResult {
    /// Of a DKG.
    Dkg(DkgResult),
    /// Of a reshare.
    Reshare(ReshareResult),
}

/// A ceremony as `ceremony.status` reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Ceremony {
    /// Which ceremony.
    pub kind: CeremonyKind,
    /// Where it is.
    pub status: CeremonyStatus,
    /// Why it stopped, if aborted.
    pub failure: Option<Failure>,
    /// Its outcome, once completed.
    pub result: Option<CeremonyResult>,
}

/// Params of `ceremony.dkg`. Every founding member's daemon calls it with the same values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DkgParams {
    /// Session id.
    pub session: Hex32,
    /// Viewing threshold.
    pub threshold: u32,
    /// Every member's keeper, in member order.
    pub members: Vec<KeeperContact>,
}

/// Params of `ceremony.reshare`. Every dealer's and new member's daemon calls it with the same values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReshareParams {
    /// Session id.
    pub session: Hex32,
    /// Vault.
    pub vault: Hex32,
    /// The current manifest: required from a joining member's keeper, which does not know the vault.
    pub current: Option<Manifest>,
    /// Dealing members' indices.
    pub dealers: Vec<u32>,
    /// New viewing threshold.
    pub new_threshold: u32,
    /// New spending threshold.
    pub new_spending_threshold: u32,
    /// The new committee, in member order.
    pub new_members: Vec<NewMember>,
}

/// What an ECDH batch is for. Recorded in the audit log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum EcdhPurpose {
    /// Note decryption.
    Decryption,
    /// Recipient tags.
    Tagging,
    /// Sender tags.
    SenderTag,
    /// Handshakes.
    Handshake,
}

/// Params of `ecdh.batch`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EcdhParams {
    /// Vault.
    pub vault: Hex32,
    /// Committee epoch the PXE expects.
    pub epoch: u32,
    /// What the points are for.
    pub purpose: EcdhPurpose,
    /// Ephemeral points.
    pub points: Vec<Point>,
}

/// One partial `D_i = s_i · P` with its DLEQ proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Partial {
    /// `D_i`.
    pub point: Point,
    /// DLEQ challenge.
    pub challenge: Hex32,
    /// DLEQ response.
    pub response: Hex32,
}

/// One member's partials, in input order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemberPartials {
    /// Member index.
    pub member: u32,
    /// Partials.
    pub partials: Vec<Partial>,
}

/// The result of `ecdh.batch`: at least `t_v` members' verified partials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EcdhResult {
    /// Committee epoch.
    pub epoch: u32,
    /// Verified partials, this keeper's included.
    pub responses: Vec<MemberPartials>,
}

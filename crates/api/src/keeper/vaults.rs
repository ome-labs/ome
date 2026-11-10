//! Vaults, committees and their members as the keeper serves them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, Point};

/// What `keeper.info` reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KeeperInfo {
    /// The API version this keeper serves, [`crate::API_VERSION`].
    pub api_version: u32,
    /// Keeper release.
    pub version: String,
    /// Keeper identity: its Ed25519 public key, also its libp2p peer id.
    pub identity: Hex32,
    /// X25519 key mailbox envelopes to this keeper are sealed to.
    pub transport: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Addresses the keeper advertises.
    pub addresses: Vec<String>,
}

/// How to reach a keeper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct KeeperContact {
    /// Keeper identity.
    pub identity: Hex32,
    /// Keeper transport key.
    pub transport: Hex32,
    /// Advertised addresses.
    pub addresses: Vec<String>,
}

/// A committee member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// 1-based index.
    pub index: u32,
    /// Approval signing key.
    pub signer_key: Point,
    /// Public verification share of the member's viewing share.
    pub verification_share: Point,
    /// Keeper identity.
    pub identity: Hex32,
    /// Keeper transport key.
    pub transport: Hex32,
    /// Keeper addresses.
    pub addresses: Vec<String>,
}

/// A committee manifest: one vault epoch's members, thresholds and group key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Protocol version the members agreed on.
    pub protocol_version: u16,
    /// Vault address.
    pub vault: Hex32,
    /// Committee epoch.
    pub epoch: u32,
    /// Spending threshold `k`.
    pub threshold: u32,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// Preaddress.
    pub preaddress: Hex32,
    /// Group key `Ivpk`.
    pub group_key: Point,
    /// Members in slot order.
    pub members: Vec<Member>,
}

/// Whether the keeper still serves a vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum VaultStatus {
    /// Serving.
    Active,
    /// Superseded by a later committee epoch.
    Retired,
    /// This keeper is no longer in the vault's committee: a verified rotation left it out, or its member retired the
    /// share it kept. The share is destroyed.
    Removed,
}

/// A vault the keeper knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Vault {
    /// Current manifest.
    pub manifest: Manifest,
    /// Status.
    pub status: VaultStatus,
    /// This keeper's member index, if a member.
    pub index: Option<u32>,
    /// Whether the keeper holds its share for the current epoch.
    pub serving: bool,
}

/// The protocol a vault's keepers write in, and what each member's keeper speaks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VaultProtocol {
    /// The version every member speaks, which the vault's messages are written in.
    pub version: u16,
    /// Each current member, in member order.
    pub members: Vec<MemberProtocol>,
}

/// What one member's keeper said it speaks; `None` until its record arrives (it then counts as the first version).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemberProtocol {
    /// Member index.
    pub member: u32,
    /// Its keeper's release.
    pub release: Option<String>,
    /// The protocol versions it reads and writes.
    pub versions: Option<ProtocolRange>,
}

/// An inclusive range of protocol versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProtocolRange {
    /// Oldest.
    pub min: u16,
    /// Newest.
    pub max: u16,
}

/// The vault's config note, as the daemon read it from the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct VaultConfig {
    /// Spending threshold `k`.
    pub threshold: u32,
    /// Registered signers.
    pub signer_count: u32,
    /// Config version.
    pub version: u32,
    /// Commitment to the signer set.
    pub signer_set_commitment: Hex32,
    /// Viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// Commitment to the viewing committee.
    pub committee_commitment: Hex32,
}

/// What an observed config meant to the keeper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Observation {
    /// It names the committee the keeper serves.
    Current,
    /// It is older than the committee the keeper serves; nothing changes.
    Outdated,
    /// The committee of the keeper's completed reshare became active; the new share replaced the old one.
    Activated,
    /// The committee of the keeper's completed reshare became active without it; its share was destroyed.
    Removed,
    /// It names a committee the keeper cannot verify. The keeper keeps its share and raises `configMismatch`.
    Unknown,
}

/// The result of `vault.observeConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Observed {
    /// What the config meant.
    pub observation: Observation,
}

/// How reachable a committee member is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Reachability {
    /// Member index.
    pub index: u32,
    /// Keeper identity.
    pub identity: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Whether the member is this keeper.
    pub you: bool,
    /// There is an established connection.
    pub connected: bool,
    /// Every connection is a relay circuit.
    pub relayed: bool,
    /// Unix seconds of the last connection or frame.
    pub last_seen: Option<u64>,
    /// Last round trip in milliseconds.
    pub rtt_ms: Option<u64>,
}

/// One served or refused ECDH request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuditEntry {
    /// Unix seconds.
    pub at: u64,
    /// `local`, or the requesting keeper's identity.
    pub requester: String,
    /// What the points were for.
    pub purpose: String,
    /// Number of points.
    pub points: u32,
    /// `served`, or why it was refused.
    pub outcome: String,
}

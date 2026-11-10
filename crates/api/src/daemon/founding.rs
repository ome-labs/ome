//! Founding a vault: lobbies as the member sees them, and the founding steps after the lobby starts.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Hex16, Hex32, Point,
    keeper::{LobbyRole, LobbyStatus},
};

/// A lobby's host, as an invite names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InviteHost {
    /// Keeper identity.
    pub identity: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Keeper addresses.
    pub addresses: Vec<String>,
}

/// What an invite says, checked against this daemon's network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InvitePreview {
    /// Lobby id.
    pub lobby: Hex16,
    /// Vault name.
    pub name: String,
    /// Host's name.
    pub host_name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included.
    pub members: u8,
    /// Network.
    pub network: String,
    /// Whether it is this daemon's network.
    pub same_network: bool,
    /// The host.
    pub host: InviteHost,
}

/// A member on a lobby's roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyViewMember {
    /// Future member index.
    pub index: u32,
    /// Name.
    pub name: String,
    /// The member's own Aztec account.
    pub account: Hex32,
    /// Approval signing key.
    pub signer_key: Point,
    /// Keeper identity.
    pub identity: Hex32,
    /// libp2p peer id.
    pub peer_id: String,
    /// Keeper addresses.
    pub addresses: Vec<String>,
    /// Whether it is this member.
    pub you: bool,
}

/// A lobby.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyView {
    /// Lobby id.
    pub lobby: Hex16,
    /// This member's part.
    pub role: LobbyRole,
    /// Where it is.
    pub status: LobbyStatus,
    /// Vault name.
    pub name: String,
    /// Spending threshold.
    pub threshold: u8,
    /// Viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included; the host starts once that many have joined.
    pub expected: u8,
    /// Network.
    pub network: String,
    /// Roster.
    pub members: Vec<LobbyViewMember>,
    /// The host removed this member (the lobby is then `closed` for them).
    pub removed: bool,
    /// The invite, on the host; a new one after a removal.
    pub invite: Option<String>,
    /// The DKG session, once started.
    pub session: Option<Hex32>,
}

/// Params of `lobby.create`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyCreateParams {
    /// Vault name, 1 to 48 characters.
    pub name: String,
    /// Proposed spending threshold, 1 to 8.
    pub threshold: u8,
    /// Proposed viewing threshold, 2 to 8.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included, 2 to 8.
    pub members: u8,
}

/// Params of `lobby.start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartLobbyParams {
    /// Lobby id.
    pub lobby: Hex16,
    /// Final spending threshold.
    pub threshold: u8,
    /// Final viewing threshold.
    pub viewing_threshold: u8,
}

/// A founding step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FoundingStepId {
    /// Every member agreed on the plan.
    Agree,
    /// The DKG ran.
    Dkg,
    /// The vault's keys and address were derived.
    Derive,
    /// The keeper registered the manifest.
    Register,
    /// The vault was deployed.
    Deploy,
    /// The deployment was checked against the plan.
    Verify,
}

/// Where a step is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    /// Not started.
    Pending,
    /// Running.
    Active,
    /// Done.
    Done,
    /// Failed.
    Failed,
}

/// One founding step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FoundingStep {
    /// Which.
    pub id: FoundingStepId,
    /// Where it is.
    pub status: StepStatus,
    /// Unix seconds it started.
    pub started_at: Option<u64>,
    /// Unix seconds it finished.
    pub finished_at: Option<u64>,
    /// Short detail, e.g. "round 2 of 3" or the deployment transaction.
    pub detail: Option<String>,
}

/// Why a founding failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FoundingError {
    /// Failed step.
    pub step: FoundingStepId,
    /// Message.
    pub message: String,
    /// Identities held responsible.
    pub blamed: Vec<String>,
}

/// A founding in progress or finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FoundingState {
    /// Lobby id.
    pub lobby: Hex16,
    /// Steps in order.
    pub steps: Vec<FoundingStep>,
    /// The vault, once derived.
    pub vault: Option<Hex32>,
    /// Failure.
    pub error: Option<FoundingError>,
    /// Finished, successfully or not.
    pub done: bool,
}

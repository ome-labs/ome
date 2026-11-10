//! Founding lobbies: a host opens one and shares an invite, guests join, and the host fixes the plan the DKG runs on.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::KeeperContact;
use crate::{Hex16, Hex32, HexBytes, Point};

/// The member a keeper represents in a lobby.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyMemberParams {
    /// Display name.
    pub name: String,
    /// The member's own Aztec account, shown to the other members.
    pub account: Hex32,
    /// Approval signing key.
    pub signer_key: Point,
}

/// Params of `lobby.open`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyOpenParams {
    /// Vault name.
    pub name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included (2 to 8).
    pub members: u8,
    /// Network the vault will live on, as the daemon names it.
    pub network: String,
    /// The host member.
    pub member: LobbyMemberParams,
}

/// The result of `lobby.open`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyOpened {
    /// Lobby id.
    pub lobby: Hex16,
    /// Invite to share with the other members.
    pub invite: String,
}

/// A join request for an existing vault, as `keeper.joinRequest` and `membership.request` return it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct JoinRequestText {
    /// `ome-join-v0:…`, signed by the newcomer's keeper; passed out of band to a member.
    pub request: String,
}

/// Params of `lobby.join`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyJoinParams {
    /// Invite from the host.
    pub invite: String,
    /// The joining member.
    pub member: LobbyMemberParams,
}

/// The result of `lobby.join`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyJoined {
    /// Lobby id.
    pub lobby: Hex16,
}

/// Params of `lobby.inspect`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InviteParams {
    /// Invite.
    pub invite: String,
}

/// What an invite says, decoded without any network traffic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    /// Lobby id.
    pub lobby: Hex16,
    /// Vault name.
    pub name: String,
    /// Host's display name.
    pub host_name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included.
    pub members: u8,
    /// Network.
    pub network: String,
    /// The host's keeper.
    pub host: KeeperContact,
}

/// Params naming a lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyParams {
    /// Lobby id.
    pub lobby: Hex16,
}

/// Params of `lobby.start`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyStartParams {
    /// Lobby id.
    pub lobby: Hex16,
    /// Final spending threshold.
    pub threshold: u8,
    /// Final viewing threshold.
    pub viewing_threshold: u8,
    /// Deployment salt.
    pub salt: Hex32,
    /// The host's sealed founding secrets, handed to every guest with the plan.
    pub secrets: HexBytes,
}

/// This keeper's part in a lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LobbyRole {
    /// Opened it.
    Host,
    /// Joined it.
    Guest,
}

/// Where a lobby is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LobbyStatus {
    /// Accepting members, or waiting for the plan.
    Open,
    /// The plan is fixed.
    Started,
    /// Cancelled by the host, or left by this guest.
    Closed,
}

/// A member on a lobby's roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyMember {
    /// 1-based position, the future member index.
    pub index: u32,
    /// Display name.
    pub name: String,
    /// The member's own Aztec account.
    pub account: Hex32,
    /// Approval signing key.
    pub signer_key: Point,
    /// Keeper identity.
    pub identity: Hex32,
    /// Keeper transport key.
    pub transport: Hex32,
    /// Keeper addresses.
    pub addresses: Vec<String>,
    /// Whether the member is this keeper's.
    pub you: bool,
}

/// Params of `lobby.remove`: the host drops a joiner before starting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyRemoveParams {
    /// Lobby id.
    pub lobby: Hex16,
    /// The joiner's keeper identity.
    pub identity: Hex32,
}

/// Params of `lobby.update`: the host changes the plan before starting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyUpdateParams {
    /// Lobby id.
    pub lobby: Hex16,
    /// How many members the vault will have.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub members: Option<u8>,
    /// Spending threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub threshold: Option<u8>,
    /// Viewing threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewing_threshold: Option<u8>,
}

/// What changed in a lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LobbyChangeKind {
    /// Opened, or this keeper joined it.
    Opened,
    /// A member joined.
    Joined,
    /// A member left.
    Left,
    /// The host removed a member.
    Removed,
    /// The host changed its size or thresholds.
    Updated,
    /// The host started it.
    Started,
    /// The host closed it, or this guest left.
    Closed,
}

/// A change to a lobby, and the member it was about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LobbyChange {
    /// What changed.
    pub kind: LobbyChangeKind,
    /// The member who joined, left or was removed (keeper identity).
    pub member: Option<Hex32>,
}

/// The plan a started lobby fixed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LobbyPlan {
    /// DKG session id.
    pub session: Hex32,
    /// Spending threshold.
    pub threshold: u8,
    /// Viewing threshold.
    pub viewing_threshold: u8,
    /// Deployment salt.
    pub salt: Hex32,
    /// The host's sealed founding secrets, until the founding finishes.
    pub secrets: Option<HexBytes>,
    /// Protocol version the vault will speak.
    pub protocol_version: u16,
}

/// A lobby.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Lobby {
    /// Lobby id.
    pub lobby: Hex16,
    /// This keeper's part.
    pub role: LobbyRole,
    /// Where it is.
    pub status: LobbyStatus,
    /// Vault name.
    pub name: String,
    /// Host's display name.
    pub host_name: String,
    /// Proposed spending threshold.
    pub threshold: u8,
    /// Proposed viewing threshold.
    pub viewing_threshold: u8,
    /// How many members the vault will have, host included.
    pub expected: u8,
    /// Network.
    pub network: String,
    /// The host's keeper.
    pub host: KeeperContact,
    /// Roster in join order.
    pub members: Vec<LobbyMember>,
    /// This keeper's member was removed by the host (the lobby is then `closed` for it).
    pub removed: bool,
    /// The plan, once started.
    pub plan: Option<LobbyPlan>,
    /// The invite, on the host.
    pub invite: Option<String>,
}
